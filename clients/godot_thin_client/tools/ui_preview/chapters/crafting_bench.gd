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

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 258

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

## **THE TWO SHIPPED TIERS.** Every item opens at `plain` — bone, hide and fibre work — and `spears`,
## `clubs` and `hoes` carry a second, `flint` (knapped stone and wood). The ledger no longer heads a
## section with a tier: one row per ITEM carries both of an item's recipes, so the tier a recipe makes
## is named by its `recipeLabel`, in the popup and the picker.
const TIER_PLAIN := "plain"
const TIER_FLINT := "flint"

## The ledger's three section heads, as the panel names them and keys their fold state.
const HEAD_KIT := "Kit"
const HEAD_TOOLS := "Bench tools"
const HEAD_MATERIALS := "Materials"

## Two stock recipes making ONE material, for the one-row-per-thing-made pair. The sim requires a
## label on sibling recipes, so both carry one.
const HURDLES_NAME := "Hurdles"
const HURDLES_MATERIAL := "hurdles"
const HURDLES_WOOD_RECIPE := "hurdles_wood"
const HURDLES_WOOD_LABEL := "Wood"
const HURDLES_WITHY_RECIPE := "hurdles_withy"
const HURDLES_WITHY_LABEL := "Withy"
## **THE TIER WORDS THAT MAY NOT REACH AN OWNED CELL AT ALL.** The popup and the picker name tiers by
## design, so this claim is scoped to the CELLS; the fixtures publish a `tier_id` on every batch they
## own, which is what stops the negative being vacuous.
const TIER_WORDS: Array[String] = [TIER_PLAIN, TIER_FLINT]

## **THE RECIPES OF A TWO-RECIPE ITEM, spelled as the sim publishes them.** Spears are pointed with
## bone at the `plain` tier or knapped at `flint`; baskets are woven of reed or withy at ONE tier,
## which is what makes them the substitute case — the popup's Owned column is absent there. The ids
## are the recipe book's own, and the labels are what the popup and the picker must render verbatim.
const SPEARS_BONE_RECIPE := "spears"
const SPEARS_FLINT_RECIPE := "spears_flint"
const SPEARS_BONE_LABEL := "Bone"
const SPEARS_FLINT_LABEL := "Flint"
const BASKETS_REED_RECIPE := "baskets"
const BASKETS_WITHY_RECIPE := "baskets_withy"
const BASKETS_REED_LABEL := "Reed"
const BASKETS_WITHY_LABEL := "Withy"
## The second Traps recipe the shrug-with-a-link state adds — offer-only, never suggested, so the row
## still describes the reference `traps` recipe and only its item cell changes shape.
const TRAPS_SINEW_RECIPE := "traps_sinew"
const TRAPS_SINEW_LABEL := "Sinew"
const CLUBS_BONE_RECIPE := "clubs"
const CLUBS_STONE_RECIPE := "clubs_stone"
const CLUBS_BONE_LABEL := "Bone"
const CLUBS_STONE_LABEL := "Stone"
## What each spear recipe makes and how long a fresh one lasts — resolved sim-side, rendered verbatim.
## The flint spear hits harder and breaks sooner, which is the whole of why it is a choice.
const SPEARS_BONE_MAKES := "20 attack"
const SPEARS_FLINT_MAKES := "26 attack"
const SPEARS_BONE_LASTS := "250 blows"
const SPEARS_FLINT_LASTS := "175 blows"
## The reference band's spears, all at the `plain` tier — six, which is the popup's Owned figure on the
## Bone line and the zero on the Flint one.
const SPEARS_PLAIN_OWNED := 6
## **THE BENCH'S FULL NAME, as the sim publishes it for a job on a two-recipe item.** Rendered VERBATIM;
## the client composes no `item · recipe` of its own.
const BENCH_TWO_RECIPE_NAME := "Baskets (Reed)"
## …and what a client composing the recipe label onto the bench title itself would print instead.
const BENCH_COMPOSED_NAME := "Baskets · Reed"

## The band's two-tier stock, batch by batch. Spears sit at two GRADES of `flint` — and the `good`
## pair at different wear, which the cell must sum into one line rather than list twice — while the
## clubs are a single grade at the older `plain` tier, so the cell has a tier word to leak.
const TWO_TIER_SPEARS_GOOD_A := 3
const TWO_TIER_SPEARS_GOOD_B := 2
const TWO_TIER_SPEARS_EXCELLENT := 1
const TWO_TIER_CLUBS_POOR := 4

## **THE CRAFTED PILE THE STOCK ROW REPORTS, AND WHAT ONE PASS OF IT YIELDS.** Named because the
## claims below are arithmetic about these three and a literal typed twice is a claim nobody can
## re-derive. **Both amounts are deliberately un-whole, and so is their sum**: a material is measured,
## not counted, so the Owned cell states `9.3` and an equipment-column `×n` would round it to `×9` —
## the assertion reads the total back through the rail's own `BATCH_AMOUNT_FORMAT`, so a cell that
## counts fails it. **TWO batches for one total** is what makes the summing visible at all: with a
## single batch, a cell printing the first amount it finds passes every claim about the pile.
const STOCK_CORDAGE_STOUT := 6.5
const STOCK_CORDAGE_SLACK := 2.8
## The yield is WHOLE, so the cost cell's `_amount_text` prints `6` rather than `6.0` — the same
## whole-where-it-is-whole rule the input clauses beside it follow.
const STOCK_CORDAGE_YIELD := 6.0

## Where each of the three crafts stands. Weaving is DONE (this band gathers), Tanning is climbing (it
## hunts deer) and Bone-working is stalled at 12% — which is not an error state but a band standing in
## country with no bone game. The land decides what you are good at.
## The crew weaving baskets on the running bench — named because the head-count claims below are
## arithmetic ABOUT it, and a literal repeated in three places is a claim nobody can re-derive.
const BENCH_CREW := 2

## The workforce of the bench-bound band: all but one worker is at the bench, which is the only shape
## in which "idle" and "how many could be at the bench" produce a visibly different stepper.
const BENCH_BOUND_WORKING_AGE := 3

## **THE RUNNING BENCH'S OWN THREE NUMBERS.** Named because every claim about the finish estimate is
## arithmetic ABOUT them: 3.0 of the 6 `work` a pass costs, and a rate of 1.0 a turn — which is the
## playtest's own shape, two crafters delivering 1.0 because bare-handed `craft_speed` is 0.5. **The
## fixture is chosen so a re-derivation fails visibly**: `remaining / rate` is 3 turns while
## `remaining / crew` is 2, so a panel that guessed the rate off the head count renders a different
## sentence rather than the same one.
const BENCH_WORK := 6.0
const BENCH_PROGRESS := 3.0
const BENCH_RATE := 1.0

## What the bench has already withdrawn for the pass in flight — the amounts the STORE lost, which is
## what a clear destroys and what its tooltip has to name. Deliberately unlike the recipe's own input
## row, since the two differ the moment a bench tool's material efficiency applies.
const BENCH_DRAWN_FIBRE := 5.0
const BENCH_DRAWN_HIDE := 1.0

## **THE RUNNING BENCH'S HEAD ORDER** — two baskets asked for, one made, which is what the head row's
## `1/2` reads. Its `−` is DEAD (a count of one would finish the order); the queue state below stages
## a head with room to come down.
const BENCH_ORDER_COUNT := 2
const BENCH_ORDER_MADE := 1

## **THE PROGRESS LINE, SPELLED OUT RATHER THAN RECOMPOSED** through the vocab formats the panel
## builds it with — an expectation borrowed from the code under test can only agree with itself. It
## is the running fixture's own reading: 3.0 of the 6 work a pass costs, what a turn adds and the
## turns that implies, and the grade the pile in flight fixed. What the order has already delivered
## is NOT on it — that is the head queue row's `made/count`, the one home that fact has.
const BENCH_PROGRESS_LINE := "3.0 of 6 work · +1.0/turn · done in 3 turns · this pile → good"

## **THE SAME BENCH WITH THE ESTIMATE WITHHELD**, which is what a stopped bench reads: the progress
## and the grade, and nothing about turns. Spelled out for the same reason as the line
## above, and it is what makes "shows neither" a claim about the WHOLE line rather than about two
## needles that could each be missing for their own reason.
const BENCH_STOPPED_PROGRESS_LINE := "3.0 of 6 work · this pile → good"

## The tooltip the ✕ carries on a DRAWN bench — the withdrawal, named material by material.
const BENCH_CLEAR_TOOLTIP := "Take this order off the bench — 5 fibre · 1 hide already cut are lost"

## **THE CHEAPEST GENUINE REFUSAL THERE IS**, and the sim's own wording for it
## (`core_sim/src/snapshot/crafting.rs`): the crew walked off. It is also the reason that SURVIVES the
## rule that nothing short stops a pile already drawn, so a fixture built on it stages a bench the sim
## would really publish stopped.
const BLOCKED_BENCH_CREW := 0
const BLOCKED_BENCH_REASON := "No one at the bench"
## A bench with nobody on it accrues nothing, so the sim publishes a rate of ZERO beside that reason —
## the "there is nothing to compute" half of the estimate's gate.
const BLOCKED_BENCH_RATE := 0.0
## **AND THE SIM CALLS IT A PROMPT RATHER THAN A FAULT**, which is the severity a crewless bench really
## resolves to: the player staffs the bench, so this is the ordinary state one click after **Make** and
## not an error the player has to undo.
const BLOCKED_BENCH_SEVERITY := HudCraftingVocab.SEVERITY_NEUTRAL

## **THE OTHER STOPPED BENCH, AND IT IS STOPPED FOR A COMPLETELY DIFFERENT REASON.** A bench short of
## material publishes its REAL, non-zero rate: the crew is standing there and the tool is fine, it
## simply has not drawn. That is what makes the pair discriminating — a panel gating the estimate on
## the rate alone would quote *"done in 5 turns"* beside *"Short 0.6 fibre"*, promising progress that
## is not happening. Its pile is undrawn too, which is the other tooltip this chapter has to see.
const SHORT_BENCH_REASON := "Short 0.6 fibre"
const SHORT_BENCH_CREW := 2
const SHORT_BENCH_RATE := 1.0
const SHORT_BENCH_PROGRESS := 1.0
const SHORT_BENCH_PROGRESS_LINE := "1.0 of 6 work"
## **AND THE SIM CALLS THIS ONE A FAULT.** The band cannot cover the next draw and nothing but the
## player finding more fibre will move it, so it keeps the alarm the crewless bench gives up — which is
## the whole reason the severity rides beside the reason rather than being read off the wording.
const SHORT_BENCH_SEVERITY := HudCraftingVocab.SEVERITY_DANGER

## **THE WIRE'S ANSWER, DELIBERATELY DISAGREEING WITH THE WORDING** — a shortfall sentence published at
## `neutral`. No sim resolves this; it exists so the tint can be shown to come from the published field
## rather than from a client re-reading the string. A panel that inferred severity from the words would
## render it in the alarm ink, and this is the only fixture that can tell those two implementations
## apart.
const MISMATCHED_BENCH_REASON := "Short 2.5 hide"
const MISMATCHED_BENCH_SEVERITY := HudCraftingVocab.SEVERITY_NEUTRAL

## **THE TWO CASES THE ROUND NUMBERS ABOVE CANNOT SHOW**, asserted with no frame of their own: a
## remainder that does not divide (`4.5 / 1.3` is 3.46, and only a CEILING answers 4 — a floor or a
## round both say 3) and a bench inside one turn of done, which reads *"done next turn"* rather than
## the *"done in 1 turns"* the plural format would have produced.
const CEIL_BENCH_PROGRESS := 1.5
const CEIL_BENCH_RATE := 1.3
const CEIL_BENCH_PROGRESS_LINE := "1.5 of 6 work · +1.3/turn · done in 4 turns · this pile → good"
const NEXT_TURN_BENCH_PROGRESS := 5.6
const NEXT_TURN_BENCH_PROGRESS_LINE := "5.6 of 6 work · +1.0/turn · done next turn · this pile → good"

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
	_assert_a_stock_row_reads_its_pile_and_its_yield()
	_assert_a_material_row_states_a_spend_order_not_a_grade()
	await h._save("crafting_panel")

	# **THE EMPTY-STORE HALF OF THAT PILE CLAIM — PNG-less, and between two saves on purpose.** It
	# re-opens the same band with its store emptied, which is a payload no picture needs.
	await _assert_a_material_the_band_lacks_says_so()

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
	await _short_bench_state()
	await _severity_follows_the_wire_state()
	await _estimate_arithmetic_states()
	await _clear_bench_command_state()
	await _map_gesture_state()
	await _bench_priority_states()
	await _recipe_states()
	await _shrug_with_a_link_state()
	await _queue_and_suggestion_states()
	await _short_head_state()
	await _material_shortage_state()
	await _worked_forecast_state()
	await _auto_craft_states()

	# Hand everything back: the panel closed, the roster restored to the reference band.
	h._hud.close_crafting_panel()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await h._settle()

# ---- the QUEUE and the SUGGESTIONS (issue #776, §7 "The queue" / "Suggestions") -----------------

## The queued band's three orders: the head working baskets (1 of 3), a PAUSED flint-spear order that
## was raised over and still holds its cut pile, and a sled order that merely waits.
const QUEUE_HEAD_COUNT := 3
const QUEUE_HEAD_MADE := 1
const QUEUE_PAUSED_INDEX := 1
const QUEUE_PAUSED_COUNT := 2
const QUEUE_PAUSED_PROGRESS := 2.0
const QUEUE_WAITING_INDEX := 2
const QUEUE_WAITING_COUNT := 1
## The paused order's name, as the recipe book composes it — the sim's own `full_name` shape.
const QUEUE_PAUSED_NAME := "Spears (Flint)"
const QUEUE_WAITING_NAME := "Sled"

## **THE THREE SUGGESTIONS, IN THE SIM'S RANK ORDER** — workers going without, descending:
## clubs (4 warriors, REFUSED — the only recipe is short of bone), spears (3 hunters on TAKE rows,
## two recipes so its Queue opens the picker), sled (2 workers whose shortage costs WORK a turn).
const SUGGEST_CLUBS_COUNT := 4
const SUGGEST_SPEARS_COUNT := 3
const SUGGEST_SLED_COUNT := 2
const SUGGEST_SLED_WORK := 1.6
## The rendered titles and consequence lines, SPELLED OUT rather than recomposed through the formats.
const SUGGEST_CLUBS_TITLE := "Clubs ×4"
const SUGGEST_CLUBS_LINE := "4 workers without"
const SUGGEST_SPEARS_TITLE := "Spears ×3"
const SUGGEST_SPEARS_LINE := "3 hunters without"
const SUGGEST_SLED_TITLE := "Sled ×2"
const SUGGEST_SLED_LINE := "+1.6 work a turn"
## The spears suggestion's hover, one line per source.
const SUGGEST_SPEARS_TOOLTIP := "Hunters on Red Deer: 2 missing, 2 without\nHunters on Aurochs: 1 missing, 1 without"

## **THE QUEUE AND THE SUGGESTIONS, ON ONE BAND.** One frame stages every queue state the spec names —
## the head being worked (the WELL, with its own `made/count` stepper), a PAUSED order (drawn, not the
## head), one that merely waits — and every
## suggestion shape: a refused one (its offer's own words, a dead Queue), a take-row one in the job's
## crew noun, and one whose shortage costs WORK. The claims a picture cannot carry are asserted beside
## it, and then each control is PRESSED through the real relay and the line it sends is checked.
func _queue_and_suggestion_states() -> void:
	var band := _queued_band()
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/queue — the queued panel is open", false)
		return
	_assert_the_queue_reads(panel)
	_assert_the_suggestions_read(panel)
	await h._save("crafting_queue_suggestions")

	# --- THE COMMANDS. Each asserted as the LINE the socket would see, off the real relay.
	var faction := HudConst.PLAYER_FACTION_ID
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	var lines := await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_INCREMENT_META,
		HudCraftingVocab.ORDER_HEAD_INDEX))
	h._assert_hud("crafting/queue — the head's + asks for one more (%s)" % [lines],
		lines == ["bench_order_count %d %d order 0 count %d" % [faction, band_id, QUEUE_HEAD_COUNT + 1]])
	lines = await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_RAISE_META,
		QUEUE_WAITING_INDEX))
	h._assert_hud("crafting/queue — a waiting order's ↑ raises it by its own index (%s)" % [lines],
		lines == ["bench_raise %d %d order %d" % [faction, band_id, QUEUE_WAITING_INDEX]])
	lines = await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_REMOVE_META,
		QUEUE_PAUSED_INDEX))
	h._assert_hud("crafting/queue — the paused order's ✕ removes it by its own index (%s)" % [lines],
		lines == ["bench_remove %d %d order %d" % [faction, band_id, QUEUE_PAUSED_INDEX]])
	lines = await _lines_from(_suggestion_queue_button(panel, "sled"))
	h._assert_hud("crafting/suggest — a single-recipe suggestion's Queue enqueues its WHOLE count (%s)"
			% [lines],
		lines == ["bench_enqueue %d %d recipe sled count %d" % [faction, band_id, SUGGEST_SLED_COUNT]])
	# The two-recipe suggestion: Queue opens the SAME picker Make opens, under the suggestion, and
	# Start queues the chosen recipe at the suggestion's count — not Make's one.
	lines = await _lines_from(_suggestion_queue_button(panel, "spears"))
	var picker := _picker(panel)
	h._assert_hud("crafting/suggest — a two-recipe suggestion's Queue opens the picker and sends nothing (%s)"
			% [lines],
		lines.is_empty() and picker != null
			and String(picker.get_meta(HudCraftingVocab.PICKER_META)) == "spears")
	h._assert_hud("crafting/suggest — …drawn under the SUGGESTION, before the bench, not under the ledger row",
		picker != null and _is_above_the_bench(panel, picker))
	await h._save("crafting_suggestion_picker")
	lines = await _lines_from(_picker_control(panel, HudCraftingVocab.PICKER_START_META))
	h._assert_hud("crafting/suggest — Start queues the chosen recipe at the suggestion's count (%s)"
			% [lines],
		lines == ["bench_enqueue %d %d recipe %s count %d" % [faction, band_id, SPEARS_FLINT_RECIPE,
			SUGGEST_SPEARS_COUNT]])
	lines = await _lines_from(_make_button(panel, "crook"))
	h._assert_hud("crafting/queue — Make queues an order of ONE (%s)" % [lines],
		lines == ["bench_enqueue %d %d recipe crook count %d" % [faction, band_id,
			HudCraftingVocab.MAKE_ORDER_COUNT]])
	h._hud.close_crafting_panel()
	await h._settle()

# ---- the SHORT HEAD: the bench works the first order it can (issue #776 follow-up) ---------------

## The head is short of fibre, so the bench skips it and works order 1, the flint spears.
const SHORT_HEAD_WORKED := 1
const SHORT_HEAD_REASON := "Short 3.0 fibre"
const SHORT_HEAD_COUNT := 2
const SHORT_HEAD_WORKED_COUNT := 3
const SHORT_HEAD_WORKED_MADE := 1

## **A SHORT HEAD NO LONGER STALLS THE QUEUE, AND THE PANEL FOLLOWS `worked`.** Every bench scalar
## describes `orders[worked]`, so the well is order 1 here — its title, its `made/count`, its ✕ — and
## the skipped head is a queue row ABOVE the rest, reading WAITING with the sim's reason in the danger
## ink. The claims are PAIRS on the two places an index can go wrong: the head row's ✕ and `+` send 0
## while the well's send 1, since a panel still hard-wired to the head would send 0 from both.
func _short_head_state() -> void:
	var band := _short_head_band()
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/short-head — the panel is open", false)
		return
	var texts := _label_texts(panel)
	h._assert_hud("crafting/short-head — the well describes the WORKED order, not the head",
		_has_prefix(texts, QUEUE_PAUSED_NAME)
			and _label_with_text(panel, "%d/%d" % [SHORT_HEAD_WORKED_MADE, SHORT_HEAD_WORKED_COUNT]) != null
			and not _has_prefix(texts, BENCH_TWO_RECIPE_NAME + HudCraftingVocab.BENCH_SUB_SEPARATOR))
	var rows := _queue_rows(panel)
	var indices: Array = []
	for row in rows:
		indices.append(int(row.get_meta(HudCraftingVocab.QUEUE_ROW_META)))
	h._assert_hud("crafting/short-head — the skipped head is a queue row ABOVE the others (%s)" % [indices],
		indices == [HudCraftingVocab.ORDER_HEAD_INDEX, QUEUE_WAITING_INDEX])
	if rows.is_empty():
		return
	var head_texts := _label_texts(rows[0])
	h._assert_hud("crafting/short-head — the head row reads WAITING with the sim's reason (%s)" % [head_texts],
		head_texts.has(HudCraftingVocab.ORDER_STATUS_WAITING.to_upper())
			and head_texts.has(SHORT_HEAD_REASON) and head_texts.has(BENCH_TWO_RECIPE_NAME))
	var reason := _reason_label(rows[0])
	h._assert_hud("crafting/short-head — …the reason line is MUTED and the colour rides the WAITING status word",
		reason != null and reason.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_FAINT
			and _forecast_label(panel, HudCraftingVocab.ORDER_STATUS_META,
				HudCraftingVocab.ORDER_HEAD_INDEX).get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.DANGER)
	h._assert_hud("crafting/short-head — the head row still has no ↑",
		_queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, HudCraftingVocab.ORDER_HEAD_INDEX) == null)
	await h._save("crafting_queue_short_head")

	var faction := HudConst.PLAYER_FACTION_ID
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	var lines := await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_INCREMENT_META,
		HudCraftingVocab.ORDER_HEAD_INDEX))
	h._assert_hud("crafting/short-head — the head ROW's + edits order 0 (%s)" % [lines],
		lines == ["bench_order_count %d %d order 0 count %d" % [faction, band_id, SHORT_HEAD_COUNT + 1]])
	lines = await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_INCREMENT_META,
		SHORT_HEAD_WORKED))
	h._assert_hud("crafting/short-head — the WELL's + edits order 1 (%s)" % [lines],
		lines == ["bench_order_count %d %d order %d count %d" % [faction, band_id, SHORT_HEAD_WORKED,
			SHORT_HEAD_WORKED_COUNT + 1]])
	lines = await _lines_from(_queue_control(panel, HudCraftingVocab.ORDER_REMOVE_META,
		HudCraftingVocab.ORDER_HEAD_INDEX))
	h._assert_hud("crafting/short-head — the head ROW's ✕ removes order 0 (%s)" % [lines],
		lines == ["bench_remove %d %d order 0" % [faction, band_id]])
	lines = await _lines_from(_clear_button(panel))
	h._assert_hud("crafting/short-head — the WELL's ✕ removes order 1 (%s)" % [lines],
		lines == ["bench_remove %d %d order %d" % [faction, band_id, SHORT_HEAD_WORKED]])

	# **THE PAUSING-RAISE HOVER IS A PAIR.** The sled sits straight under the worked spears, whose pile
	# is cut, so raising it pauses them and its ↑ says so; the SAME sled carrying a `blocked_reason`
	# would still be skipped once above, so raising it pauses nothing and its ↑ must not claim it.
	panel = h._hud.crafting_panel().panel()
	var sled_raise := _queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, QUEUE_WAITING_INDEX)
	h._assert_hud("crafting/short-head — raising a workable order over the drawn worked one warns it pauses it",
		sled_raise != null and sled_raise.tooltip_text == HudCraftingVocab.ORDER_RAISE_PAUSES_TOOLTIP)
	var blocked_band := _short_head_band()
	var blocked_bench: Dictionary = blocked_band["bench"]
	var blocked_sled: Dictionary = (blocked_bench["orders"] as Array)[QUEUE_WAITING_INDEX]
	blocked_sled["blocked_reason"] = SHORT_HEAD_REASON
	blocked_sled["blocked_severity"] = HudCraftingVocab.SEVERITY_DANGER
	h._hud.update_band_alerts([blocked_band])
	h._hud.crafting_panel().refresh_snapshot()
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	sled_raise = _queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, QUEUE_WAITING_INDEX) \
		if panel != null else null
	h._assert_hud("crafting/short-head — …while a WAITING order's ↑ claims no pause, it would still be skipped",
		sled_raise != null and sled_raise.tooltip_text == HudCraftingVocab.ORDER_RAISE_TOOLTIP)
	h._hud.close_crafting_panel()
	await h._settle()

# ---- the MATERIAL SHORTAGE forecast (issue #777) -------------------------------------------------

const FORECAST_ONE_SHORT := 6.0
const FORECAST_TWO_SHORT_A := 3.0
const FORECAST_TWO_SHORT_B := 1.5
const FORECAST_SUGGEST_SHORT := 12.0
const FORECAST_SUGGEST_ITEM := "clubs"
## The stock a shortfall row says is on hand; `required` is stock plus the shortfall.
const FORECAST_HELD := 1.0

func _shortfall_row(material_id: String, short: float) -> Dictionary:
	return {"material_id": material_id, "required": short + FORECAST_HELD, "held": FORECAST_HELD,
		"short": short}

## **A FORECAST IS NOT A BLOCK.** The paused spears read one material, the waiting sled two; the Clubs
## suggestion carries its whole-count shortfall. The sim's `short` is rendered verbatim to one decimal,
## muted (`INK_FAINT`), and never on an order that also carries a blocked reason.
func _material_shortage_state() -> void:
	var band := _queued_band()
	var bench: Dictionary = band["bench"]
	var orders: Array = bench["orders"]
	(orders[QUEUE_PAUSED_INDEX] as Dictionary)["short_to_finish"] = [
		_shortfall_row("wood", FORECAST_ONE_SHORT)]
	(orders[QUEUE_WAITING_INDEX] as Dictionary)["short_to_finish"] = [
		_shortfall_row("wood", FORECAST_TWO_SHORT_A), _shortfall_row("fibre", FORECAST_TWO_SHORT_B)]
	var suggestions: Array = band["craft_suggestions"]
	for suggestion_variant in suggestions:
		var suggestion: Dictionary = suggestion_variant
		if String(suggestion["item_id"]) == FORECAST_SUGGEST_ITEM:
			suggestion["shortfalls"] = [_shortfall_row("wood", FORECAST_SUGGEST_SHORT)]
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/forecast — the panel is open", false)
		return
	var one := _forecast_label(panel, HudCraftingVocab.ORDER_SHORT_TO_FINISH_META, QUEUE_PAUSED_INDEX)
	h._assert_hud("crafting/forecast — one material reads the sentence (%s)" % [one.text if one != null else "none"],
		one != null and one.text == "Short 6.0 wood"
			and one.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_FAINT
			and one.tooltip_text == one.text)
	h._assert_hud("crafting/forecast — the status words carry the colour: PAUSED amber, QUEUED a step brighter than faint",
		_forecast_label(panel, HudCraftingVocab.ORDER_STATUS_META, QUEUE_PAUSED_INDEX)
				.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.WARN
			and _forecast_label(panel, HudCraftingVocab.ORDER_STATUS_META, QUEUE_WAITING_INDEX)
				.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_DIM)
	var two := _forecast_label(panel, HudCraftingVocab.ORDER_SHORT_TO_FINISH_META, QUEUE_WAITING_INDEX)
	h._assert_hud("crafting/forecast — two materials join with a middle dot (%s)" % [two.text if two != null else "none"],
		two != null and two.text == "Short 3.0 wood · Short 1.5 fibre")
	var sug := _forecast_label(panel, HudCraftingVocab.SUGGESTION_SHORTFALL_META, FORECAST_SUGGEST_ITEM)
	h._assert_hud("crafting/forecast — the suggestion says it for the whole count (%s)" % [sug.text if sug != null else "none"],
		sug != null and sug.text == "Short 12.0 wood for all %d" % SUGGEST_CLUBS_COUNT
			and sug.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_FAINT)
	h._assert_hud("crafting/forecast — a covered suggestion carries no line",
		_forecast_label(panel, HudCraftingVocab.SUGGESTION_SHORTFALL_META, "spears") == null)
	await h._save("crafting_material_forecast")

	# Exclusive with the blocked reason: a skipped order says why it is skipped, not the forecast.
	(orders[QUEUE_WAITING_INDEX] as Dictionary)["blocked_reason"] = SHORT_HEAD_REASON
	h._hud.update_band_alerts([band])
	h._hud.crafting_panel().refresh_snapshot()
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	h._assert_hud("crafting/forecast — a blocked order shows its reason and NO forecast line",
		panel != null and _forecast_label(panel, HudCraftingVocab.ORDER_SHORT_TO_FINISH_META, QUEUE_WAITING_INDEX) == null
			and _reason_label(panel) != null)
	h._hud.close_crafting_panel()
	await h._settle()

const WORKED_FORECAST_SHORT := 10.0

## **THE WORKED ORDER HAS NO QUEUE ROW, SO THE WELL CARRIES ITS FORECAST** — a lone order, affordable for
## one pass, short for the whole run. Paired with the same bench BLOCKED: the well's blocked line already
## quotes the queue-aware numbers, so the forecast line must not appear beside it.
func _worked_forecast_state() -> void:
	var band := _crafting_band()
	var bench: Dictionary = band["bench"]
	var order := _order(BASKETS_REED_RECIPE, QUEUE_HEAD_COUNT, QUEUE_HEAD_MADE, BENCH_PROGRESS, true)
	order["short_to_finish"] = [_shortfall_row("fibre", WORKED_FORECAST_SHORT)]
	bench["orders"] = [order]
	bench["worked"] = 0
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/worked-forecast — the panel is open", false)
		return
	var line := _forecast_label(panel, HudCraftingVocab.BENCH_SHORT_TO_FINISH_META, true)
	h._assert_hud("crafting/worked-forecast — the well names the lone worked order's shortage (%s)"
			% [line.text if line != null else "none"],
		line != null and line.text == "Short 10.0 fibre"
			and line.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_FAINT
			and line.tooltip_text == line.text)
	await h._save("crafting_worked_forecast")

	bench["blocked_reason"] = SHORT_HEAD_REASON
	bench["blocked_severity"] = HudCraftingVocab.SEVERITY_DANGER
	h._hud.update_band_alerts([band])
	h._hud.crafting_panel().refresh_snapshot()
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	h._assert_hud("crafting/worked-forecast — a BLOCKED well shows its refusal and NO forecast line",
		panel != null and _forecast_label(panel, HudCraftingVocab.BENCH_SHORT_TO_FINISH_META, true) == null
			and _blocked_line(panel) != null)
	h._hud.close_crafting_panel()
	await h._settle()

# ---- AUTO-CRAFT (issue #779) ----------------------------------------------------------------------

const AUTO_HEAD_RECIPE := "clubs"
const AUTO_HEAD_NAME := "Clubs"
const AUTO_HEAD_COUNT := SUGGEST_CLUBS_COUNT
const AUTO_SKIPPED_ITEM := "clubs"
const AUTO_SHORT_BONE := 4.9
const AUTO_SKIPPED_LINE := "Auto skipped — back when bone is in"
const AUTO_FREE_LINE := "Auto takes it back when the bench is free"

## An auto head waiting short of bone: the bench is blocked, nothing is workable.
func _auto_waiting_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["recipe_id"] = AUTO_HEAD_RECIPE
	bench["display_name"] = AUTO_HEAD_NAME
	bench["teaches"] = ""
	bench["auto"] = true
	bench["auto_skipped"] = []
	bench["drawn"] = false
	bench["output_grade"] = ""
	bench["progress"] = 0.0
	bench["rate_per_turn"] = BENCH_RATE
	bench["blocked_reason"] = "Short %.1f bone" % AUTO_SHORT_BONE
	bench["work"] = BENCH_WORK
	bench["blocked_severity"] = HudCraftingVocab.SEVERITY_DANGER
	var head := _order(AUTO_HEAD_RECIPE, AUTO_HEAD_COUNT, 0, 0.0, false, bench["blocked_reason"],
		HudCraftingVocab.SEVERITY_DANGER)
	head["auto"] = true
	bench["orders"] = [head]
	bench["worked"] = 0
	band["bench"] = bench
	band["craft_suggestions"] = _suggestions_without(["clubs"])
	return band

## The sim nets queued work out of Make next, so an item on the bench is not listed.
func _suggestions_without(item_ids: Array) -> Array:
	return _craft_suggestions().filter(func(s: Dictionary) -> bool: return not item_ids.has(String(s["item_id"])))

## An auto order being worked, with the clubs suggestion skipped.
func _auto_working_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["auto"] = true
	bench["auto_skipped"] = [AUTO_SKIPPED_ITEM]
	bench["recipe_id"] = SPEARS_FLINT_RECIPE
	bench["display_name"] = QUEUE_PAUSED_NAME
	bench["teaches"] = ""
	(bench["orders"] as Array)[0]["recipe_id"] = SPEARS_FLINT_RECIPE
	(bench["orders"] as Array)[0]["auto"] = true
	band["bench"] = bench
	band["craft_suggestions"] = _suggestions_without(["spears"])
	return band

func _auto_idle_band() -> Dictionary:
	var band := _bare_band()
	var bench: Dictionary = band["bench"]
	bench["auto"] = true
	bench["auto_skipped"] = []
	band["bench"] = bench
	return band

func _auto_meta_node(node: Node, meta: String) -> Node:
	if node.has_meta(meta):
		return node
	for child in node.get_children():
		var found := _auto_meta_node(child, meta)
		if found != null:
			return found
	return null

func _auto_tag_nodes(node: Node, into: Array) -> void:
	if node.has_meta(HudCraftingVocab.AUTO_TAG_META):
		into.append(node)
	for child in node.get_children():
		_auto_tag_nodes(child, into)

func _auto_show(band: Dictionary) -> CraftingPanel:
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	return h._hud.crafting_panel().panel()

func _auto_craft_states() -> void:
	var band_id := int(_crafting_band().get("band_id", HudConst.NO_BAND_ID))
	var faction := HudConst.PLAYER_FACTION_ID

	# (a) an auto head waiting short of bone: the tag in the well title, the Skip link under the red line.
	var panel: CraftingPanel = await _auto_show(_auto_waiting_band())
	if panel == null:
		h._assert_hud("crafting/auto — the panel opens", false)
		return
	var tags: Array = []
	_auto_tag_nodes(panel, tags)
	h._assert_hud("crafting/auto — the waiting auto head wears ONE AUTO tag, on the well title (%s)"
			% [tags.map(func(t): return t.get_meta(HudCraftingVocab.AUTO_TAG_META))],
		tags.size() == 1 and int(tags[0].get_meta(HudCraftingVocab.AUTO_TAG_META))
			== HudCraftingVocab.AUTO_TAG_WELL_INDEX)
	var skip := _auto_meta_node(panel, HudCraftingVocab.AUTO_SKIP_META)
	var blocked := _blocked_line(panel)
	h._assert_hud("crafting/auto — the Skip link sits under the red blocked line",
		skip != null and blocked != null and skip is Button
			and (skip as Button).text == HudCraftingVocab.AUTO_SKIP_LINK
			and (skip as Control).get_global_rect().position.y >= blocked.get_global_rect().end.y)
	var switch := _auto_meta_node(panel, HudCraftingVocab.BENCH_AUTO_SWITCH_META)
	h._assert_hud("crafting/auto — the switch reads ON",
		switch != null and bool(switch.get_meta(HudCraftingVocab.BENCH_AUTO_SWITCH_META)))
	await h._save("crafting_auto_waiting")

	# The two commands, as the socket would see them.
	var sent: Array = []
	var on_auto := func(p: Dictionary) -> void:
		sent.append(String(MAIN_SCRIPT.format_bench_auto(p).get("line", "")))
	var on_skip := func(p: Dictionary) -> void:
		sent.append(String(MAIN_SCRIPT.format_bench_auto_skip(p).get("line", "")))
	h._hud.bench_auto_requested.connect(on_auto)
	h._hud.bench_auto_skip_requested.connect(on_skip)
	await _press_control(switch as Control)
	panel = h._hud.crafting_panel().panel()
	await _press_control(_auto_meta_node(panel, HudCraftingVocab.AUTO_SKIP_META) as Control)
	h._hud.bench_auto_requested.disconnect(on_auto)
	h._hud.bench_auto_skip_requested.disconnect(on_skip)
	h._assert_hud("crafting/auto — a REAL press on the switch sends `bench_auto … off`, Skip sends `bench_auto_skip` (%s)" % [sent],
		sent == ["bench_auto %d %d off" % [faction, band_id], "bench_auto_skip %d %d" % [faction, band_id]])
	h._hud.close_crafting_panel()
	await h._settle()

	# An auto head that is NOT waiting shows no Skip; a player-queued waiting head shows none either.
	var player_wait := _auto_waiting_band()
	((player_wait["bench"] as Dictionary)["orders"] as Array)[0]["auto"] = false
	panel = await _auto_show(player_wait)
	h._assert_hud("crafting/auto — a waiting head the PLAYER queued gets no Skip and no tag",
		panel != null and _auto_meta_node(panel, HudCraftingVocab.AUTO_SKIP_META) == null
			and _auto_meta_node(panel, HudCraftingVocab.AUTO_TAG_META) == null)
	var switch_off_band := _auto_waiting_band()
	(switch_off_band["bench"] as Dictionary)["auto"] = false
	panel = await _auto_show(switch_off_band)
	h._assert_hud("crafting/auto — Auto off hides the Skip link even on an auto-tagged head, and the switch reads OFF",
		panel != null and _auto_meta_node(panel, HudCraftingVocab.AUTO_SKIP_META) == null
			and not bool(_auto_meta_node(panel, HudCraftingVocab.BENCH_AUTO_SWITCH_META)
				.get_meta(HudCraftingVocab.BENCH_AUTO_SWITCH_META)))
	h._hud.close_crafting_panel()
	await h._settle()

	# (b) an auto order being worked, a skipped suggestion dimmed beneath.
	panel = await _auto_show(_auto_working_band())
	var skipped_note := _auto_meta_node(panel, HudCraftingVocab.AUTO_SKIPPED_NOTE_META)
	h._assert_hud("crafting/auto — the skipped clubs row says it was skipped, naming bone (%s)"
			% [(skipped_note as Label).text if skipped_note != null else "none"],
		skipped_note is Label and (skipped_note as Label).text == AUTO_SKIPPED_LINE
			and (skipped_note as Label).get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_FAINT)
	h._assert_hud("crafting/auto — …in place of its red shortfall line",
		_forecast_label(panel, HudCraftingVocab.SUGGESTION_SHORTFALL_META, AUTO_SKIPPED_ITEM) == null)
	var queue_button := _suggestion_queue_button(panel, AUTO_SKIPPED_ITEM)
	h._assert_hud("crafting/auto — …and its Queue stays live",
		queue_button != null and not queue_button.disabled)
	tags.clear()
	_auto_tag_nodes(panel, tags)
	h._assert_hud("crafting/auto — the worked auto order's AUTO tag rides the well title",
		tags.size() == 1 and int(tags[0].get_meta(HudCraftingVocab.AUTO_TAG_META))
			== HudCraftingVocab.AUTO_TAG_WELL_INDEX)
	h._assert_hud("crafting/auto — a worked bench shows no Skip link",
		_auto_meta_node(panel, HudCraftingVocab.AUTO_SKIP_META) == null)
	await h._save("crafting_auto_working")

	# A skipped item nothing is short of any more reads the free-bench line; a queued auto order
	# behind the worked one wears the tag on its ROW.
	var free_band := _auto_working_band()
	for offer_variant in free_band["craft_offers"]:
		var offer: Dictionary = offer_variant
		if String(offer.get("output_item_id", "")) == AUTO_SKIPPED_ITEM:
			offer["shortfalls"] = []
	var queued := _order("sled", QUEUE_WAITING_COUNT, 0, 0.0, false)
	queued["auto"] = true
	((free_band["bench"] as Dictionary)["orders"] as Array).append(queued)
	panel = await _auto_show(free_band)
	skipped_note = _auto_meta_node(panel, HudCraftingVocab.AUTO_SKIPPED_NOTE_META)
	h._assert_hud("crafting/auto — a skipped item with nothing short reads the free-bench line",
		skipped_note is Label and (skipped_note as Label).text == AUTO_FREE_LINE)
	tags.clear()
	_auto_tag_nodes(panel, tags)
	var row_tag := _queue_rows(panel)
	h._assert_hud("crafting/auto — an auto order on a queue ROW wears the tag (%d tags)" % tags.size(),
		tags.size() == 2 and row_tag.size() == 1 and _auto_meta_node(row_tag[0], HudCraftingVocab.AUTO_TAG_META) != null)
	h._hud.close_crafting_panel()
	await h._settle()

	# (c) an idle bench with Auto on.
	panel = await _auto_show(_auto_idle_band())
	h._assert_hud("crafting/auto — an idle bench with Auto on reads `%s`" % HudCraftingVocab.AUTO_IDLE_SUB,
		panel != null and _label_with_text(panel, HudCraftingVocab.AUTO_IDLE_SUB) != null
			and _label_with_text(panel, HudCraftingVocab.BENCH_IDLE_SUB) == null)
	await h._save("crafting_auto_idle")
	h._hud.close_crafting_panel()
	await h._settle()

func _forecast_label(node: Node, meta: String, value: Variant) -> Label:
	if node is Label and node.has_meta(meta) and node.get_meta(meta) == value:
		return node as Label
	for child in node.get_children():
		var found := _forecast_label(child, meta, value)
		if found != null:
			return found
	return null

func _reason_label(node: Node) -> Label:
	if node is Label and node.has_meta(HudCraftingVocab.ORDER_REASON_META):
		return node as Label
	for child in node.get_children():
		var found := _reason_label(child)
		if found != null:
			return found
	return null

## The reference band with a short head: baskets short of fibre (skipped), flint spears WORKED with a
## cut pile, a sled waiting. Every bench scalar is the spears order's, and `on_bench` follows it.
func _short_head_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["recipe_id"] = SPEARS_FLINT_RECIPE
	bench["display_name"] = QUEUE_PAUSED_NAME
	# The fixture's knowledge roster names no knapping track, so the spears order teaches nothing here.
	bench["teaches"] = ""
	bench["worked"] = SHORT_HEAD_WORKED
	bench["orders"] = [
		_order(BASKETS_REED_RECIPE, SHORT_HEAD_COUNT, 0, 0.0, false, SHORT_HEAD_REASON,
			HudCraftingVocab.SEVERITY_DANGER),
		_order(SPEARS_FLINT_RECIPE, SHORT_HEAD_WORKED_COUNT, SHORT_HEAD_WORKED_MADE, BENCH_PROGRESS, true),
		_order("sled", QUEUE_WAITING_COUNT, 0, 0.0, false),
	]
	band["bench"] = bench
	var offers: Array = []
	for offer_variant in _craft_offers():
		var offer: Dictionary = offer_variant
		offer["on_bench"] = String(offer.get("recipe_id", "")) == SPEARS_FLINT_RECIPE
		offers.append(offer)
	band["craft_offers"] = offers
	return band

## Every queue claim no picture can carry: the head's `made/count` in the WELL and no second row for
## it, one row per order BEHIND it keeping its published index, the status word each reads, the `−`
## dead exactly at `made + 1`, no ↑ on the head, and the ✕ tooltips.
func _assert_the_queue_reads(panel: CraftingPanel) -> void:
	var rows := _queue_rows(panel)
	h._assert_hud("crafting/queue — one row per order BEHIND the head, each at its published index (%d rows)"
			% rows.size(),
		rows.size() == 2 and int(rows[0].get_meta(HudCraftingVocab.QUEUE_ROW_META)) == QUEUE_PAUSED_INDEX
			and int(rows[1].get_meta(HudCraftingVocab.QUEUE_ROW_META)) == QUEUE_WAITING_INDEX)
	if rows.size() != 2:
		return
	var paused_texts := _label_texts(rows[0])
	var waiting_texts := _label_texts(rows[1])
	# **THE WELL IS THE HEAD'S ROW**: its `made/count` face and the head's count stepper live there,
	# under the sim's own name in the title — never a second row repeating them.
	var head_minus := _queue_control(panel, HudCraftingVocab.ORDER_DECREMENT_META, 0)
	h._assert_hud("crafting/queue — the head's made/count rides the well, beside its own stepper",
		_label_with_text(panel, "%d/%d" % [QUEUE_HEAD_MADE, QUEUE_HEAD_COUNT]) != null
			and head_minus != null and _queue_control(panel, HudCraftingVocab.ORDER_INCREMENT_META, 0) != null
			and _label_texts(panel).has(HudCraftingVocab.HEAD_COUNT_CAPTION.to_upper()))
	h._assert_hud("crafting/queue — a drawn order that is not the head reads PAUSED (%s)" % [paused_texts],
		paused_texts.has(HudCraftingVocab.ORDER_STATUS_PAUSED.to_upper())
			and paused_texts.has(QUEUE_PAUSED_NAME))
	h._assert_hud("crafting/queue — an undrawn waiting order reads as queued, not paused (%s)"
			% [waiting_texts],
		waiting_texts.has(HudCraftingVocab.ORDER_STATUS_QUEUED.to_upper())
			and not waiting_texts.has(HudCraftingVocab.ORDER_STATUS_PAUSED.to_upper())
			and waiting_texts.has(QUEUE_WAITING_NAME))
	h._assert_hud("crafting/queue — each row reads made/count (%s · %s)" % [paused_texts, waiting_texts],
		paused_texts.has("%d/%d" % [0, QUEUE_PAUSED_COUNT])
			and waiting_texts.has("%d/%d" % [0, QUEUE_WAITING_COUNT]))
	# `−` is live while the count can come down without finishing the order, and dead at `made + 1`.
	var waiting_minus := _queue_control(panel, HudCraftingVocab.ORDER_DECREMENT_META, QUEUE_WAITING_INDEX)
	h._assert_hud("crafting/queue — − is live above made + 1 and dead at it",
		head_minus != null and not head_minus.disabled
			and waiting_minus != null and waiting_minus.disabled)
	h._assert_hud("crafting/queue — the head has no ↑ and every other order has one",
		_queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, 0) == null
			and _queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, QUEUE_PAUSED_INDEX) != null
			and _queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, QUEUE_WAITING_INDEX) != null)
	var paused_remove := _queue_control(panel, HudCraftingVocab.ORDER_REMOVE_META, QUEUE_PAUSED_INDEX)
	var waiting_remove := _queue_control(panel, HudCraftingVocab.ORDER_REMOVE_META, QUEUE_WAITING_INDEX)
	var head_remove := _clear_button(panel)
	h._assert_hud("crafting/queue — each ✕ says whether a cut pile is lost, the head's being the well's own",
		paused_remove != null and paused_remove.tooltip_text == HudCraftingVocab.ORDER_REMOVE_TOOLTIP_DRAWN
			and waiting_remove != null
			and waiting_remove.tooltip_text == HudCraftingVocab.ORDER_REMOVE_TOOLTIP_UNDRAWN
			and head_remove != null and head_remove.tooltip_text == BENCH_CLEAR_TOOLTIP
			and _queue_control(panel, HudCraftingVocab.ORDER_REMOVE_META, 0) == null)
	# Raising the order straight under a drawn head pauses that head, and its ↑ says so first.
	var paused_raise := _queue_control(panel, HudCraftingVocab.ORDER_RAISE_META, QUEUE_PAUSED_INDEX)
	h._assert_hud("crafting/queue — the ↑ that would pause the drawn head says so",
		paused_raise != null and paused_raise.tooltip_text == HudCraftingVocab.ORDER_RAISE_PAUSES_TOOLTIP)
	# The well still describes the HEAD: the crew stepper, the progress line, no "finished" clause.
	h._assert_hud("crafting/queue — the well's progress line still describes the head",
		_label_with_text(panel, BENCH_PROGRESS_LINE) != null)

## The suggestion claims: published order, each row's two lines, the refused one's reason and dead
## button, the live ones' live buttons, and the sources on the hover.
func _assert_the_suggestions_read(panel: CraftingPanel) -> void:
	var rows := _suggestion_rows(panel)
	var order: Array = []
	for row in rows:
		order.append(String(row.get_meta(HudCraftingVocab.SUGGESTION_META)))
	h._assert_hud("crafting/suggest — one row per suggestion, in the sim's published order (%s)" % [order],
		order == ["clubs", "spears", "sled"])
	if rows.size() != 3:
		return
	var texts := [_label_texts(rows[0]), _label_texts(rows[1]), _label_texts(rows[2])]
	h._assert_hud("crafting/suggest — name ×count over ONE consequence line (%s)" % [texts],
		texts[0].has(SUGGEST_CLUBS_TITLE) and texts[0].has(SUGGEST_CLUBS_LINE)
			and texts[1].has(SUGGEST_SPEARS_TITLE) and texts[1].has(SUGGEST_SPEARS_LINE)
			and texts[2].has(SUGGEST_SLED_TITLE) and texts[2].has(SUGGEST_SLED_LINE))
	var clubs := _suggestion_queue_button(panel, "clubs")
	h._assert_hud("crafting/suggest — a KNOWN item short of material has a LIVE Queue and no refusal under it",
		clubs != null and not clubs.disabled and not texts[0].has("Short 6.9 bone"))
	var spears := _suggestion_queue_button(panel, "spears")
	var sled := _suggestion_queue_button(panel, "sled")
	h._assert_hud("crafting/suggest — a makeable item's Queue is live",
		spears != null and not spears.disabled and sled != null and not sled.disabled)
	h._assert_hud("crafting/suggest — the sources ride the row's hover, one line each (%s)"
			% [rows[1].tooltip_text],
		rows[1].tooltip_text == SUGGEST_SPEARS_TOOLTIP)
	h._assert_hud("crafting/suggest — the list opens the main column, above the bench",
		_is_above_the_bench(panel, rows[0]))

## Press `control` for real and return the command LINES `Main` would build from what the HUD emitted
## across every bench verb — so a press that sent the wrong verb, or two, is visible in one array.
func _lines_from(control: Control) -> Array:
	var lines: Array = []
	var on_enqueue := func(p: Dictionary) -> void:
		lines.append(String(MAIN_SCRIPT.format_bench_enqueue(p).get("line", "")))
	var on_count := func(p: Dictionary) -> void:
		lines.append(String(MAIN_SCRIPT.format_bench_order_count(p).get("line", "")))
	var on_remove := func(p: Dictionary) -> void:
		lines.append(String(MAIN_SCRIPT.format_bench_remove(p).get("line", "")))
	var on_raise := func(p: Dictionary) -> void:
		lines.append(String(MAIN_SCRIPT.format_bench_raise(p).get("line", "")))
	var on_crew := func(p: Dictionary) -> void:
		lines.append(String(MAIN_SCRIPT.format_bench_crew(p).get("line", "")))
	h._hud.bench_enqueue_requested.connect(on_enqueue)
	h._hud.bench_order_count_requested.connect(on_count)
	h._hud.bench_remove_requested.connect(on_remove)
	h._hud.bench_raise_requested.connect(on_raise)
	h._hud.bench_crew_requested.connect(on_crew)
	await _press_control(control)
	h._hud.bench_enqueue_requested.disconnect(on_enqueue)
	h._hud.bench_order_count_requested.disconnect(on_count)
	h._hud.bench_remove_requested.disconnect(on_remove)
	h._hud.bench_raise_requested.disconnect(on_raise)
	h._hud.bench_crew_requested.disconnect(on_crew)
	return lines

## Whether `node` sits ABOVE the bench well on screen — the suggestions open the main column.
func _is_above_the_bench(panel: CraftingPanel, node: Control) -> bool:
	var progress := _label_with_text(panel, BENCH_PROGRESS_LINE)
	return progress != null and node.get_global_rect().position.y < progress.get_global_rect().position.y

func _queue_rows(node: Node) -> Array:
	var found: Array = []
	if node is Control and node.has_meta(HudCraftingVocab.QUEUE_ROW_META):
		found.append(node)
	for child in node.get_children():
		found.append_array(_queue_rows(child))
	return found

func _suggestion_rows(node: Node) -> Array:
	var found: Array = []
	if node is Control and node.has_meta(HudCraftingVocab.SUGGESTION_META):
		found.append(node)
	for child in node.get_children():
		found.append_array(_suggestion_rows(child))
	return found

## One queue control by its meta and the ORDER INDEX it is valued with; `null` when not drawn.
func _queue_control(node: Node, meta: String, index: int) -> Button:
	if node is Button and node.has_meta(meta) and int(node.get_meta(meta)) == index:
		return node as Button
	for child in node.get_children():
		var found := _queue_control(child, meta, index)
		if found != null:
			return found
	return null

func _suggestion_queue_button(node: Node, item_id: String) -> Button:
	if node is Button and String(node.get_meta(HudCraftingVocab.SUGGESTION_QUEUE_META, "")) == item_id:
		return node as Button
	for child in node.get_children():
		var found := _suggestion_queue_button(child, item_id)
		if found != null:
			return found
	return null

## The reference band with a three-order queue and three suggestions on it.
func _queued_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["orders"] = [
		_order(BASKETS_REED_RECIPE, QUEUE_HEAD_COUNT, QUEUE_HEAD_MADE, BENCH_PROGRESS, true),
		_order(SPEARS_FLINT_RECIPE, QUEUE_PAUSED_COUNT, 0, QUEUE_PAUSED_PROGRESS, true),
		_order("sled", QUEUE_WAITING_COUNT, 0, 0.0, false),
	]
	band["bench"] = bench
	band["craft_suggestions"] = _craft_suggestions()
	return band

## **THE SIM'S LIST, IN THE SIM'S SHAPE** (`dict/population.rs`), ranked by `workers_without`.
func _craft_suggestions() -> Array:
	return [
		_suggestion("clubs", SUGGEST_CLUBS_COUNT, 4.0, 0.0, [
			_source(HudCraftingVocab.SOURCE_KIND_TAKE, "warrior", -1, -1, "", "", 4.0, 4.0, 0.0)]),
		_suggestion("spears", SUGGEST_SPEARS_COUNT, 3.0, 0.0, [
			_source(HudCraftingVocab.SOURCE_KIND_TAKE, "hunt", 0, 0, "red_deer", "", 2.0, 2.0, 0.0),
			_source(HudCraftingVocab.SOURCE_KIND_TAKE, "hunt", 0, 0, "aurochs", "", 1.0, 1.0, 0.0)]),
		_suggestion("sled", SUGGEST_SLED_COUNT, 2.0, SUGGEST_SLED_WORK, [
			_source(HudCraftingVocab.SOURCE_KIND_POOL, "builders", 0, 0, "", "", 1.5, 1.5, 1.2),
			_source(HudCraftingVocab.SOURCE_KIND_SITE, "extract", 12, 7, "", "wood", 0.5, 0.5, 0.4)]),
	]

func _suggestion(item_id: String, count: int, workers_without: float, work_per_turn: float,
		sources: Array) -> Dictionary:
	return {"item_id": item_id, "count": count, "workers_without": workers_without,
		"work_per_turn": work_per_turn, "sources": sources}

func _source(kind: String, job: String, x: int, y: int, fauna_id: String, material: String,
		missing: float, workers_without: float, work_per_turn: float) -> Dictionary:
	return {"kind": kind, "job": job, "target_x": maxi(x, 0), "target_y": maxi(y, 0),
		"fauna_id": fauna_id, "material": material, "missing_units": missing,
		"workers_without": workers_without, "work_per_turn": work_per_turn}

# ---- states 12-13: TWO TIERS, which is the only shape the readout can be judged on ---------------

## **THE SUGGESTED RECIPE IS WHAT A ROW WOULD BE MADE AT; THE CELL IS WHAT THE BAND HAS.** A band that
## can knap flint but still carries plain clubs is the shape in which the two disagree — the Clubs row
## suggests its `Stone` recipe (flint) over four plain clubs. The cell renders count and grade and no
## tier word; which tier the band holds is the recipe popup's `ownedAtTier` column to say.
func _two_tier_states() -> void:
	h._hud.update_band_alerts([_two_tier_band()])
	h._hud.open_crafting_panel(_two_tier_band())
	await h._settle()
	_assert_owned_cell_reads_what_the_band_has()
	_assert_no_tier_word_reaches_a_cell(_two_tier_equipment_batches(), "two-tier")
	await h._save("crafting_panel_two_tiers")

	await _assert_folding_a_head_hides_only_its_own_rows()

	h._hud.close_crafting_panel()
	await h._settle()

## **A MATERIAL ROW REPORTS THE BAND'S TOTAL OF IT, AND THE YIELD SITS WITH THE COST.** Four claims,
## three of them halves of a pair, because every one-sided version passes on a panel that lost the
## join entirely:
##
## - **The pile as ONE total, summed across the batches** — asserted as the cell's whole contents, so
##   a cell that kept the rail's per-batch lines or its rating chips fails here rather than passing on
##   the presence of a number. Paired with the row the band banks NONE of, which is what says the cell
##   joined on the material rather than printing amounts unconditionally.
## - **It is an AMOUNT, not a count** — nothing in the cell begins with `×`. A cell reaching for the
##   equipment column's format renders 9.3 as `×9`, which is the specific defect a material measured
##   rather than counted invites, and it is named separately so the failure says which mistake it was.
## - **The yield is in the COST cell and NOT in the Owned cell** — asserted as both, since a panel that
##   never moved it satisfies the first and one that simply dropped it satisfies the second.
func _assert_a_stock_row_reads_its_pile_and_its_yield() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the stock-row panel is open", false)
		return
	var cordage := _owned_cell_texts(panel, "Cordage")
	var held := HudCraftingVocab.BATCH_AMOUNT_FORMAT % (STOCK_CORDAGE_STOUT + STOCK_CORDAGE_SLACK)
	h._assert_hud("crafting — a material row reports the band's whole pile of it, %s + %s = %s (%s)"
			% [HudCraftingVocab.BATCH_AMOUNT_FORMAT % STOCK_CORDAGE_STOUT,
				HudCraftingVocab.BATCH_AMOUNT_FORMAT % STOCK_CORDAGE_SLACK, held, cordage],
		cordage == [held])
	# …and it is an AMOUNT: the equipment column's `×n` would render this pile as `×9`.
	h._assert_hud("crafting — …as an amount and not a count, so nothing in the cell counts (%s)"
			% [cordage],
		_count_starting_with(cordage, HudCraftingVocab.OWNED_COUNT_FORMAT.substr(0, 1)) == 0)
	# **THE YIELD MOVED TO THE COST CELL, AND LEFT THE OWNED ONE.** Both halves: a panel that never
	# moved it passes the first, and one that dropped it on the floor passes the second.
	var yielded := HudCraftingVocab.COST_YIELD_FORMAT % [
		str(int(STOCK_CORDAGE_YIELD)), "cordage"]
	var row := _ledger_row(panel, "Cordage")
	h._assert_hud("crafting — a pass's yield closes the rebuild-costs cell (%s)" % [yielded],
		row != null and _label_texts(row).has(yielded))
	h._assert_hud("crafting — …and no arrow is left in the Owned cell (%s)" % [cordage],
		_count_starting_with(cordage, "→") == 0)

# ---- the material-only row's two sentences -------------------------------------------------------

## The row names the claim below is made on. The stock row is the ONE material-only recipe in the
## fixture book; `Baskets` is the equipment row that shares its bench material and its missing tool,
## which is what makes the pair an A/B on the grade rather than on the wording; `Loom` is a bench tool,
## the third `group` and the one whose second line the stock format must not have disturbed.
const STOCK_ROW_NAME := "Cordage"
const KIT_ROW_NAME := "Baskets"
const TOOL_ROW_NAME := "Loom"

## **EVERY EXPECTED SENTENCE IS SPELLED OUT, never recomposed through `HudCraftingVocab`.** A claim
## built from the format under test can only agree with itself — the harness rule
## `_assert_horizon_floor_is_the_whole_trip` records — and here it would be worse than usual: the
## retired wording is the whole point of the negative, so composing it from the LIVE const would make
## it un-nameable the moment the const changes, which is exactly the change being guarded.
const STOCK_ROLE_LINE := "Weaving · worst strong spent first"
## ⛔ **THE DEAD CLAIM, KEPT AS A LITERAL SO IT CAN BE DENIED BY NAME.** The row read
## `Weaving · quality from strong` (shipped: `Tanning · quality from suppleness`) and no quality of a
## material-only recipe's output comes from its input's axis — `MaterialBatch` has no grade field and
## `RecipeDef::grade_for` resolves none, so every set of hurdles carries the recipe's own
## `stoutness 0.6 / span 0.6` whatever hide went in. The axis survives on the line because `reads`
## ALSO picks which batches are spent, worst-first (`systems/crafting.rs::spend_axis`).
const RETIRED_STOCK_ROLE_LINE := "Weaving · quality from strong"
## The word the retired sentence turned on, denied over the whole row rather than over that one
## string: a rewording that kept the quality claim in other words is the same lie.
const RETIRED_QUALITY_WORD := "quality"
## The two refusals, which are the sim's half of the same A/B and arrive on the wire resolved. They
## share every word up to the tail, so the tail is the only thing the pair can be reading.
const STOCK_REASON := "Reed, no loom"
const KIT_REASON := "Reed, no loom → fair"
## The bench tool's own second line, untouched by any of this — `materials[].tool_item_id` names the
## loom as fibre's tool, so the row states the material it stretches.
const TOOL_ROLE_LINE := "Bench tool — fibre"

## **A MATERIAL-ONLY ROW STATES A SPEND ORDER; THE EQUIPMENT ROW BESIDE IT STILL STATES A GRADE.**
## Reported from play against the shipped `hurdles` row, which said `Hide, no tanning frame → poor`
## under a second line reading `Tanning · quality from suppleness` — and neither the arrow nor the
## quality was ever true of it. A grade is a property of EQUIPMENT: it is stamped on an equipment
## batch and read back off it, while a `MaterialBatch` carries its own `characteristics` and has
## nowhere to put one, so `RecipeDef::grade_for` resolves nothing here and `outputGrade` publishes
## `""`.
##
## **EVERY CLAIM IS AN EQUALITY, and that is not fussiness** — the whole defect was a sentence that
## was individually plausible, so `contains` would have passed on it. The one exception is the
## `quality` negative, which is deliberately looser than the retired string it backs up: a rewording
## that kept the claim in other words is the same lie.
##
## **AND EVERY CLAIM IS HALF OF A PAIR**, because a one-sided one passes on a panel that lost the
## thing entirely:
##
## - The stock row's second line reads the spend-order sentence **and** the retired quality one is
##   nowhere in the panel. The first alone passes on a panel printing both; the second alone on a
##   panel that dropped line two from every stock row.
## - The stock row's refusal has no `→ grade` tail **and** the KIT row's — same bench material, same
##   missing loom, so the two differ in the tail and in nothing else — still has one. Without the
##   second, the fix is indistinguishable from a build that stripped grade words everywhere.
## - The TOOL row still names the material it bounds, which is the third `group` and says the stock
##   branch's edit did not reach the ones beside it.
func _assert_a_material_row_states_a_spend_order_not_a_grade() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the spend-order panel is open", false)
		return
	var stock_row := _ledger_row(panel, STOCK_ROW_NAME)
	var kit_row := _ledger_row(panel, KIT_ROW_NAME)
	var tool_row := _ledger_row(panel, TOOL_ROW_NAME)
	if stock_row == null or kit_row == null or tool_row == null:
		h._assert_hud("crafting — the ledger carries a stock, a kit and a tool row", false)
		return
	var stock_texts := _label_texts(stock_row)
	h._assert_hud("crafting — a material row's second line states the SPEND ORDER (%s) — got %s"
			% [STOCK_ROLE_LINE, stock_texts],
		stock_texts.has(STOCK_ROLE_LINE))
	h._assert_hud("crafting — …and the retired quality claim (%s) is nowhere in the panel"
			% [RETIRED_STOCK_ROLE_LINE],
		not _label_texts(panel).has(RETIRED_STOCK_ROLE_LINE))
	h._assert_hud("crafting — …and the word `%s` appears nowhere on that row (%s)"
			% [RETIRED_QUALITY_WORD, stock_texts],
		_count_containing_ci(stock_texts, RETIRED_QUALITY_WORD) == 0)
	h._assert_hud("crafting — a material row's refusal carries no grade tail (%s) — got %s"
			% [STOCK_REASON, stock_texts],
		stock_texts.has(STOCK_REASON))
	# **THE VACUITY GUARD.** Same material, same absent loom, one word of difference — so a build that
	# stripped grade language everywhere fails HERE while every claim above stays green.
	var kit_texts := _label_texts(kit_row)
	h._assert_hud("crafting — …while an EQUIPMENT row's refusal still names the grade (%s) — got %s"
			% [KIT_REASON, kit_texts],
		kit_texts.has(KIT_REASON))
	var tool_texts := _label_texts(tool_row)
	h._assert_hud("crafting — …and a bench tool still names the material it bounds (%s) — got %s"
			% [TOOL_ROLE_LINE, tool_texts],
		tool_texts.has(TOOL_ROLE_LINE))

## How many of `texts` contain `needle`, case-insensitively — a COUNT rather than a bool so a failure
## says how many lines leaked the word rather than only that one did.
func _count_containing_ci(texts: Array, needle: String) -> int:
	var found := 0
	var wanted := needle.to_lower()
	for text in texts:
		if String(text).to_lower().contains(wanted):
			found += 1
	return found

## **THE OTHER HALF OF THE PILE CLAIM, PNG-LESS: a material the band banks NONE of says so.** Without
## it every claim in `_assert_a_stock_row_reads_its_pile_and_its_yield` is satisfied by a cell that
## prints amounts for whatever row it is handed, and with a second stock RECIPE it would be satisfied
## by a fixture — so the pairing is made by emptying the STORE under the same recipe instead. It earns
## no frame twice over: the picture is one chip, and a second stock row in the ledger would put the
## `crafting_panel_band_dock_collapsed` state (which stages the ledger FITTING its room) one row over
## the line it exists to sit under.
func _assert_a_material_the_band_lacks_says_so() -> void:
	var emptied := _crafting_band()
	emptied["material_batches"] = []
	h._hud.update_band_alerts([emptied])
	h._hud.open_crafting_panel(emptied)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the emptied-store panel is open", false)
		return
	var cordage := _owned_cell_texts(panel, "Cordage")
	h._assert_hud("crafting — a material the band holds none of says so in the ledger's one "
			+ "none-wording, rather than 0.0 (%s)" % [cordage],
		cordage == [HudCraftingVocab.OWNED_NONE])
	# …and the row still states what a pass would YIELD, which is the fact the cost cell now owns: an
	# empty store is a reason to make the thing, so the arrow must survive having nothing to report.
	var row := _ledger_row(panel, "Cordage")
	h._assert_hud("crafting — …while the cost cell still names the yield on an empty store",
		row != null and _label_texts(row).has(
			HudCraftingVocab.COST_YIELD_FORMAT % [str(int(STOCK_CORDAGE_YIELD)), "cordage"]))

## **THE GRADE LINES AS A PAIR, AND NOTHING ELSE IN THE CELL.** "Two lines" is satisfied by a cell that
## lists every batch, so the single-grade row is asserted beside it. Which tier the band holds is the
## recipe popup's per-tier Owned column (`_popup_states`), never the cell's.
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
	# **THE CELL IS COUNT AND GRADE, AND NOTHING ELSE.** Asked as "everything that is not a count or a
	# legend word", so a line composed client-side — a tier word, a sentence — fails it.
	h._assert_hud("crafting — an Owned cell carries nothing beside its grades (%s)"
			% [_non_grade_texts(spears)],
		_non_grade_texts(spears).is_empty())

## **NO TIER WORD REACHES AN OWNED CELL AT ALL.** Scoped to the Owned CELLS rather than to the ledger,
## because the popup and the picker name a recipe's tier by design and a panel-wide scan cannot tell
## those apart from a cell. Non-vacuous by construction, and the precondition says so: every batch the
## band owns publishes a `tier_id`, so a tier word sits one field away from every cell asserted about.
## Asked of whichever band is open, `batches` being that band's own. Its positive half is the recipe
## popup's per-tier Owned column (`_popup_states`).
func _assert_no_tier_word_reaches_a_cell(batches: Array, which: String) -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the tier-word panel is open (%s)" % which, false)
		return
	var cells := _all_owned_cell_texts(panel)
	h._assert_hud("crafting — precondition: the %s fixture publishes tier ids" % which,
		_batches_carrying_a_tier(batches) > 0 and not cells.is_empty())
	var leaked_words: Array = []
	for text_variant in cells:
		var text := String(text_variant)
		for word in TIER_WORDS:
			if text.to_lower().contains(word):
				leaked_words.append(text)
	h._assert_hud("crafting — no Owned cell carries any tier word (%s: %s)" % [which, leaked_words],
		leaked_words.is_empty())

## **FOLDING A HEAD HIDES ITS OWN ROWS AND NOTHING ELSE, AND THE HEAD STAYS.** Both halves, and the
## reverse toggle: a panel that hid the whole table satisfies the first alone, and one that never
## restored the rows satisfies the pair on the way down.
func _assert_folding_a_head_hides_only_its_own_rows() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var head := _group_head(panel, HEAD_KIT) if panel != null else null
	if panel == null or head == null:
		h._assert_hud("crafting — the two-tier panel offers a %s head" % HEAD_KIT, false)
		return
	h._assert_hud("crafting — precondition: both groups render their rows open",
		_label_texts(panel).has("Clubs") and _label_texts(panel).has("Loom"))

	head.pressed.emit()
	await h._settle()
	var folded := _label_texts(panel)
	h._assert_hud("crafting — folding a head hides ITS rows (%s under %s)" % ["Clubs", HEAD_KIT],
		not folded.has("Clubs") and not folded.has("Spears") and not folded.has("Traps"))
	h._assert_hud("crafting — …while another group's rows stay visible (%s under %s)"
			% ["Loom", HEAD_TOOLS],
		folded.has("Loom"))
	h._assert_hud("crafting — …and the folded head itself remains, dimmed and carrying its caret",
		folded.has(_head_face(HEAD_KIT, true)))
	await h._save("crafting_panel_group_folded")

	var reopen := _group_head(panel, HEAD_KIT)
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

# ---- state 15: THE BENCH THAT IS STOPPED FOR THE OTHER REASON ------------------------------------

## **THE HALF OF THE PAIR THAT SEPARATES THE TWO GATES.** State 14's bench cannot accrue at all, so
## `rate > 0` and `blockedReason == ""` would both have withheld the estimate there and the state
## cannot say which of them did it. This bench's rate is FINE and its store is not, which is the shape
## the gate exists for — and it is the chapter's only undrawn job, so the ✕'s no-pile wording is
## asked here too.
func _short_bench_state() -> void:
	h._hud.update_band_alerts([_short_bench_band()])
	h._hud.open_crafting_panel(_short_bench_band())
	await h._settle()
	_assert_a_bench_short_of_material_quotes_no_finish()
	await h._save("crafting_bench_short")

## **WHICH OF THE TWO IMPLEMENTATIONS THE PANEL HAS, and no picture can show it.** States 14 and 15
## pair a prompt against a fault, but both of their severities agree with their wording, so a panel
## that had re-derived the tint from the string would render both frames correctly. This bench
## publishes a shortfall SENTENCE at `neutral` — a payload no sim produces — and the well must follow
## the wire. No frame, because the claim is about where a colour came from and the picture is the same
## either way.
func _severity_follows_the_wire_state() -> void:
	h._hud.update_band_alerts([_mismatched_severity_bench_band()])
	h._hud.open_crafting_panel(_mismatched_severity_bench_band())
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var reason := _blocked_line(panel) if panel != null else null
	h._assert_hud("crafting — precondition: the wording reads like the shortage state 15 tinted DANGER (%s)"
			% [reason.text if reason != null else "no line at all"],
		reason != null and reason.text == MISMATCHED_BENCH_REASON
			and MISMATCHED_BENCH_SEVERITY != SHORT_BENCH_SEVERITY)
	h._assert_hud("crafting — …and the tint is the PUBLISHED severity's, not the wording's (%s)"
			% [reason.get_theme_color(FONT_COLOR_THEME_ITEM) if reason != null else "no line at all"],
		reason != null
			and reason.get_theme_color(FONT_COLOR_THEME_ITEM) == HudCraftingVocab.REASON_COLOR_QUIET)

## **THE TWO ESTIMATE READINGS THE FRAMES ABOVE CANNOT STAGE**, and neither is worth a frame: a
## remainder that only a ceiling rounds up, and the singular wording. Rendered through the real panel
## and read off the real label, since the arithmetic and the wording are both the panel's.
func _estimate_arithmetic_states() -> void:
	h._hud.update_band_alerts([_ceil_bench_band()])
	h._hud.open_crafting_panel(_ceil_bench_band())
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	# `4.5 / 1.3` is 3.46: a floor or a round both answer 3, and only the ceiling answers 4.
	h._assert_hud("crafting — a remainder that does not divide rounds UP (%s)"
			% CEIL_BENCH_PROGRESS_LINE,
		panel != null and _label_with_text(panel, CEIL_BENCH_PROGRESS_LINE) != null)

	h._hud.update_band_alerts([_next_turn_bench_band()])
	h._hud.open_crafting_panel(_next_turn_bench_band())
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	h._assert_hud("crafting — a bench inside one turn of done reads `done next turn` (%s)"
			% NEXT_TURN_BENCH_PROGRESS_LINE,
		panel != null and _label_with_text(panel, NEXT_TURN_BENCH_PROGRESS_LINE) != null)

## **WHICH VERB THE ✕ ACTUALLY EMITS, asserted as a PAIR.** It is `bench_remove … order 0` — the
## retired `clear_bench`'s control — and a mis-wired button that ENQUEUED would satisfy a bare
## "something was emitted" while putting a job on the bench the player never chose. Driven through
## the REAL relay (panel → controller → `HudLayer`), because the panel's own signal says nothing about
## whether the seam carries it or what band it names.
func _clear_bench_command_state() -> void:
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var button := _clear_button(panel) if panel != null else null
	if button == null:
		h._assert_hud("crafting — the bench carries a ✕ to press", false)
		return
	var cleared: Array = []
	var benched: Array = []
	var on_clear := func(payload: Dictionary) -> void: cleared.append(payload)
	var on_bench := func(payload: Dictionary) -> void: benched.append(payload)
	h._hud.bench_remove_requested.connect(on_clear)
	h._hud.bench_enqueue_requested.connect(on_bench)
	button.pressed.emit()
	await h._settle()
	h._hud.bench_remove_requested.disconnect(on_clear)
	h._hud.bench_enqueue_requested.disconnect(on_bench)
	var band := _crafting_band()
	h._assert_hud("crafting — pressing the bench's ✕ removes the HEAD order, naming the band (%s)"
			% [cleared],
		cleared.size() == 1
			and int((cleared[0] as Dictionary).get("band_id", -1)) == int(band.get("band_id", -2))
			and int((cleared[0] as Dictionary).get("order", -1)) == HudCraftingVocab.ORDER_HEAD_INDEX)
	h._assert_hud("crafting — …and asks for no new job on it (%s)" % [benched],
		benched.is_empty())

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
	# The refusal is rendered VERBATIM, with its number. "Short 6.9 bone", never "cannot craft".
	h._assert_hud("crafting — a refusal names its number",
		texts.has("Short 6.9 bone") and not texts.has("cannot craft"))
	# …and the shrug reads differently from the shortage, which is the whole reason the reason is
	# published rather than derived from `available`.
	h._assert_hud("crafting — the shrug is its own string", texts.has("Not needed yet"))
	# **OWNERSHIP IS STATED AND CONDITION IS NOT — and the two claims are a PAIR.** The condition
	# readout has one home (the Band panel's role cards), so no `life` wording may reach this table;
	# but a table that had also stopped saying whether the band OWNS the thing would satisfy that
	# negative on its own, and the Owned cell is what has to carry the ownership reading alone.
	#
	# **ONE CONSEQUENCE WORDING, FOR EVERY GROUP.** This asserted a PAIR — `Bare hands` on a kit and
	# `Not made` on a tool — and the kit half was nonsense under a column head reading `Owned`: it told
	# the player they owned their own hands. The assertion could not catch it, because it only ever
	# checked that both strings were drawn somewhere. The wording is now single, and the kit/tool
	# distinction lives in the chip's ink instead, which this text-only sweep cannot see.
	h._assert_hud("crafting — owning none reads Not made, on a kit and a tool alike",
		texts.has(HudCraftingVocab.OWNED_NONE))
	h._assert_hud("crafting — no condition wording reaches the ledger",
		_missing_from(texts, RETIRED_CONDITION_WORDINGS) == RETIRED_CONDITION_WORDINGS.size())
	# The ledger is FOUR columns — Item · Owned · Rebuild costs · action — and the action head is blank
	# by design, so the three named ones are what the claim can name.
	h._assert_hud("crafting — the ledger heads its four columns",
		texts.has(HudCraftingVocab.LEDGER_COLUMN_ITEM.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_OWNED.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_COST.to_upper()))
	# **THE THREE SECTIONS ARE THE THREE GROUPS** — `Kit`, `Bench tools`, `Materials`, all carrying the
	# open caret.
	h._assert_hud("crafting — the three group heads read as one foldable family",
		texts.has(_head_face(HEAD_KIT, false)) and texts.has(_head_face(HEAD_TOOLS, false))
			and texts.has(_head_face(HEAD_MATERIALS, false)))
	# **THE RUNNING ROW'S MAKE STAYS LIVE** — the bench holds a queue, so another press is another
	# order. It was spent (*On the bench*) while the bench held one job; a Make still dead here would be
	# that rule surviving the queue.
	var running_make := _make_button(panel, "baskets")
	h._assert_hud("crafting — the running row's Make stays live, another press being another order",
		running_make != null and not running_make.disabled
			and running_make.text == HudCraftingVocab.MAKE_LABEL)
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
	# **…AND THE SHRUG DIMS THE ROW'S INFORMATION, NEVER ITS CONTROL.** A LIVE Make on the untouched
	# row must render at full strength — a faded button reads as a disabled one, which is how a
	# player read an untouched Hoes row as unmakeable. Asserted as a PAIR on the SAME row: the button's
	# alpha is the modulate product up the whole tree, so a dim on the row or any ancestor fails it,
	# and the reason beside it still dims, so a fix that simply dropped the shrug passes neither half.
	var shrug_button := _row_make_button(panel, "Traps")
	var shrug_reason := _label_with_text(panel, "Not needed yet")
	h._assert_hud("crafting — the shrug row's Make is live and full-strength while its reason dims"
			+ " (button %.2f, reason %.2f)" % [_effective_alpha(shrug_button),
				_effective_alpha(shrug_reason)],
		shrug_button != null and not shrug_button.disabled
			and is_equal_approx(_effective_alpha(shrug_button), 1.0)
			and shrug_reason != null and _effective_alpha(shrug_reason) < 1.0)
	_assert_no_control_on_the_row_is_dimmed(panel, "Traps", 1)
	# **THE UNBLOCKED HALF OF THE BENCH PAIR** (states 14 and 15 are the others): this band's bench is
	# running, so the well states its progress and carries no refusal line under it at all — no empty
	# label, no reserved gap. A one-sided claim on the blocked frame alone would pass on a panel that
	# had simply grown a permanent third line.
	#
	# **AND THE LINE SAYS WHAT A TURN ADDS AND WHEN THAT FINISHES IT.** The unit is `work` because a
	# worker-turn is not what a worker does in a turn, which is the arithmetic that produced the
	# playtest error this state pins.
	h._assert_hud("crafting — a running bench says how far along it is and nothing beneath it",
		_label_with_text(panel, BENCH_PROGRESS_LINE) != null and _blocked_line(panel) == null)
	# **THE ESTIMATE IS `ceil((work − progress) / rate)`, and the fixture is what makes the claim
	# discriminating**: 3 remaining at 1.0 a turn is three turns, where a panel dividing by the CREW
	# would say two. The expected number is computed from this chapter's own constants rather than
	# through the panel's format, an expectation borrowed from the code under test agreeing only with
	# itself.
	var expected_turns := int(ceil((BENCH_WORK - BENCH_PROGRESS) / BENCH_RATE))
	var sub := _label_with_text(panel, BENCH_PROGRESS_LINE)
	h._assert_hud("crafting — the finish estimate is the remaining work over the PUBLISHED rate (%d turns)"
			% expected_turns,
		sub != null and sub.text.contains(HudCraftingVocab.BENCH_ESTIMATE_FORMAT % expected_turns))
	# …and the rate itself is the wire's number rendered verbatim, never one composed from the crew.
	h._assert_hud("crafting — the rate is the wire's own value (%s)"
			% [HudCraftingVocab.BENCH_RATE_FORMAT % BENCH_RATE],
		sub != null and sub.text.contains(HudCraftingVocab.BENCH_RATE_FORMAT % BENCH_RATE)
			and not sub.text.contains(HudCraftingVocab.BENCH_RATE_FORMAT % float(BENCH_CREW)))
	# **THE WAY OFF THE BENCH — present on a job, and its tooltip names what clearing it destroys.**
	# The pile is read off the published withdrawal, so a tooltip composed from the recipe's inputs
	# would name a different number the moment a bench tool's efficiency applies. Paired with the idle
	# bench's own claim, which is where the button must be absent entirely.
	var clear := _clear_button(panel)
	h._assert_hud("crafting — a bench with a job carries a ✕ to clear it", clear != null)
	h._assert_hud("crafting — …and its tooltip names the pile already cut (%s)"
			% [clear.tooltip_text if clear != null else "no button at all"],
		clear != null and clear.tooltip_text == BENCH_CLEAR_TOOLTIP)
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
	h._assert_hud("crafting — a stopped bench states its reason verbatim (%s)"
		% [reason.text if reason != null else "no line at all"],
		reason != null and reason.text == BLOCKED_BENCH_REASON)
	# **THE CREWLESS HALF OF THE TINT PAIR.** This is the state one click after Make — the player is
	# who staffs the bench — so the sim marks it `neutral` and the well reads it in the QUIET ink. The
	# `not DANGER` clause is what makes the claim bite: a panel with one tint for every refusal renders
	# an ordinary prompt as a fault, which is what shipped. State 15's shortage is the other half, and
	# a panel that had simply gone quiet everywhere fails there.
	h._assert_hud("crafting — …and a bench merely waiting for its crew reads as a PROMPT, not a fault (%s)"
		% [reason.get_theme_color(FONT_COLOR_THEME_ITEM) if reason != null else "no line at all"],
		reason != null
			and reason.get_theme_color(FONT_COLOR_THEME_ITEM) == HudCraftingVocab.REASON_COLOR_QUIET
			and reason.get_theme_color(FONT_COLOR_THEME_ITEM) != HudStyle.DANGER)
	var progress := _label_with_text(panel, BENCH_STOPPED_PROGRESS_LINE)
	h._assert_hud("crafting — …AND still says how much of the job is banked (%s)"
		% BENCH_STOPPED_PROGRESS_LINE,
		progress != null and progress.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_DIM)
	# **…AND PROMISES NOTHING ABOUT WHEN IT WILL FINISH.** Asserted as the WHOLE line rather than as
	# two absent needles: this bench has no crew, so its published rate is zero and there is nothing to
	# compute — the other half of the gate is state 15's, whose rate is perfectly good.
	h._assert_hud("crafting — …and a bench that cannot accrue quotes no rate and no finish (%s)"
			% [progress.text if progress != null else "no line at all"],
		progress != null and progress.text == BENCH_STOPPED_PROGRESS_LINE)

## **THE GATE'S OTHER HALF, AND THE ONE THAT NEEDS SAYING.** This bench's crew and tool are fine — the
## sim publishes a real rate for it — and it is stopped anyway, because the store cannot cover the next
## draw. Quoting a finish here would promise progress that is not happening, so the refusal is what
## withholds it, and the fixture's own non-zero rate is asserted so the claim cannot be satisfied by a
## panel that merely gated on the rate.
func _assert_a_bench_short_of_material_quotes_no_finish() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the short-of-material panel is open", false)
		return
	h._assert_hud("crafting — the short bench's own rate is non-zero, so the refusal is what withholds the finish",
		SHORT_BENCH_RATE > 0.0)
	var progress := _label_with_text(panel, SHORT_BENCH_PROGRESS_LINE)
	h._assert_hud("crafting — a bench short of material still says how far along it is (%s)"
			% SHORT_BENCH_PROGRESS_LINE,
		progress != null)
	var reason := _blocked_line(panel)
	h._assert_hud("crafting — …and states the shortfall verbatim (%s)"
			% [reason.text if reason != null else "no line at all"],
		reason != null and reason.text == SHORT_BENCH_REASON)
	# **THE OTHER HALF OF THE TINT PAIR.** Nothing the player does at the bench moves this one, so the
	# sim marks it `danger` and the alarm stays. Asserted beside state 14's quiet prompt because either
	# claim alone passes on a panel that tints every refusal one colour — which is the defect the
	# published severity exists to end, in whichever of the two directions it is made.
	h._assert_hud("crafting — …in the DANGER ink, because a shortage really is a fault (%s)"
			% [reason.get_theme_color(FONT_COLOR_THEME_ITEM) if reason != null else "no line at all"],
		reason != null and reason.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.DANGER)
	h._assert_hud("crafting — …while quoting neither a rate nor a finish (%s)"
			% [progress.text if progress != null else "no line at all"],
		progress != null and progress.text == SHORT_BENCH_PROGRESS_LINE)
	# The undrawn half of the tooltip pair: nothing has been cut, so the ✕ says so rather than naming
	# an empty list.
	var clear := _clear_button(panel)
	h._assert_hud("crafting — an undrawn bench's ✕ says nothing has been cut yet (%s)"
			% [clear.tooltip_text if clear != null else "no button at all"],
		clear != null and clear.tooltip_text == HudCraftingVocab.CLEAR_BENCH_TOOLTIP_NOTHING)

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
	# **THE OTHER HALF OF THE ✕'s PAIR.** There is nothing on this bench to clear, so the control is
	# absent rather than present and dead — and asking it here is what stops the presence claim on
	# state 1 passing on a panel that draws the button unconditionally.
	h._assert_hud("crafting — an idle bench carries no ✕ to clear",
		_clear_button(panel) == null)

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
	# The queue rows wear the same two glyphs on their COUNT steppers, and those are no claim about
	# the crew — skip them by the meta they carry rather than by face.
	if node.has_meta(HudCraftingVocab.ORDER_DECREMENT_META) \
			or node.has_meta(HudCraftingVocab.ORDER_INCREMENT_META):
		return true
	if node is Button and (node as Button).text == face:
		return (node as Button).disabled
	for child in node.get_children():
		if not _crew_button_disabled(child, face):
			return false
	return true

## The bench's ✕, found by the meta the panel stamps on it. **A search by face could not work here**:
## the card header's close button wears the same glyph, so a face match finds two buttons and cannot
## say which of them destroys the drawn pile. `null` when the well renders none, which is what the
## idle bench's half of the pair asks for.
func _clear_button(node: Node) -> Button:
	if node is Button and node.has_meta(HudCraftingVocab.CLEAR_BENCH_META):
		return node as Button
	for child in node.get_children():
		var found := _clear_button(child)
		if found != null:
			return found
	return null

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

## **EVERY CONTROL ON A SHRUG ROW RENDERS AT FULL STRENGTH WHILE ITS NAME DIMS** — the general form of
## the Make claim, asked of every `BaseButton` descendant (Make, and the `N recipes` link on a
## multi-recipe row) so a new control on the row is covered the day it lands. `min_controls` is the
## vacuity guard: a row with fewer controls than the fixture stages proves nothing about them.
func _assert_no_control_on_the_row_is_dimmed(panel: Node, item_name: String, min_controls: int) -> void:
	var row := _ledger_row(panel, item_name)
	var controls: Array = [] if row == null else _row_controls(row)
	var faded: Array = []
	for control: BaseButton in controls:
		if not is_equal_approx(_effective_alpha(control), 1.0):
			faded.append("%s %.2f" % [control.get_class(), _effective_alpha(control)])
	var name_alpha := _row_alpha(panel, item_name)
	h._assert_hud("crafting — no control on the %s shrug row is dimmed while its name is (%d controls,"
			% [item_name, controls.size()] + " faded %s, name %.2f)" % [faded, name_alpha],
		controls.size() >= min_controls and faded.is_empty()
			and name_alpha >= 0.0 and name_alpha < 1.0)

func _row_controls(node: Node) -> Array:
	var found: Array = []
	if node is BaseButton:
		found.append(node)
	for child in node.get_children():
		found.append_array(_row_controls(child))
	return found

## The RENDERED alpha of the item name on the ledger row naming `item_name` — the modulate product up
## the tree, since the shrug dims the row's information cells rather than the row itself, and a claim
## reading only the row's own `modulate` would see `1.0` on a correctly dimmed row. `-1.0` when no
## such row is found, which fails the dimmed half honestly.
##
## **IT LOOKS FOR THE INNERMOST MATCHING `HBoxContainer`, and that is not fussiness.** The zones row
## is an `HBoxContainer` too and every ledger row is a descendant of it, so a walk that took the first
## match from the top answered the ZONES row's alpha — a flat `1.0` — for every item in the table, and
## the dimming claim failed against a frame that was rendering it correctly.
func _row_alpha(node: Node, item_name: String) -> float:
	var row := _ledger_row(node, item_name)
	if row == null:
		return -1.0
	return _effective_alpha(_label_with_text(row, item_name))

## The alpha `node` actually renders at: its own `modulate`/`self_modulate` times every ancestor
## `CanvasItem`'s `modulate`. `-1.0` for a missing node, which fails either direction of a claim.
func _effective_alpha(node: Node) -> float:
	if node == null:
		return -1.0
	var alpha := 1.0
	if node is CanvasItem:
		alpha *= (node as CanvasItem).self_modulate.a
	var walk := node
	while walk != null:
		if walk is CanvasItem:
			alpha *= (walk as CanvasItem).modulate.a
		walk = walk.get_parent()
	return alpha

## The Make button on the ledger row naming `item_name`, found by IDENTITY inside that row.
func _row_make_button(panel: Node, item_name: String) -> Button:
	var row := _ledger_row(panel, item_name)
	return null if row == null else _any_make_button(row)

func _any_make_button(node: Node) -> Button:
	if node is Button and node.has_meta(HudCraftingVocab.MAKE_BUTTON_META):
		return node as Button
	for child in node.get_children():
		var found := _any_make_button(child)
		if found != null:
			return found
	return null

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
## the published legend's band words, which on every row is empty. Derived rather than listed, so a cell growing a line nobody
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
		# **A CRAFTED MATERIAL IS STILL A MATERIAL** — it heads a rail group and is banked in
		# `material_batches` exactly as a gathered one is, which is the fact the stock row's Owned cell
		# totals. TWO axes on purpose: the rail is where a pile's ratings are drawn, and a one-axis
		# material would let a rail row that dropped an axis pass.
		# The axis NAMES are the shipped `hurdles` ones (`recipes.json`) rather than short invented
		# ones: the wrap question is asked in pixels, and `stout` is narrow enough to answer it
		# wrongly for every real material.
		{"id": "cordage", "craft": "weaving", "axes": ["stoutness", "span"],
			"hand_workable": true, "tool_item_id": ""},
		# **WOOD AND STONE, BECAUSE A FLINT RECIPE IS MADE OF THEM.** The spear's knapped recipe is
		# published AVAILABLE, and a band holding no stone could not make it — so the store holds both,
		# and the offer's availability is one a server could send.
		{"id": "wood", "craft": "shaping", "axes": ["hard", "pliant"],
			"hand_workable": true, "tool_item_id": ""},
		{"id": "stone", "craft": "knapping", "axes": ["hard", "workable"],
			"hand_workable": true, "tool_item_id": ""},
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
		# **TWO RECIPES FOR ONE ITEM** carry the item as their output and a `label` apiece — the
		# recipe's own short name among its siblings, `""` on a sole recipe.
		_kit_recipe(BASKETS_REED_RECIPE, "Baskets", "weaving", [["fibre", 20.0, "strong"]],
			"baskets", BASKETS_REED_LABEL),
		_kit_recipe(BASKETS_WITHY_RECIPE, "Baskets", "weaving", [["wood", 3.0, ""], ["fibre", 2.0, ""]],
			"baskets", BASKETS_WITHY_LABEL),
		_kit_recipe(SPEARS_BONE_RECIPE, "Spears", "bone_working",
			[["fibre", 12.0, ""], ["bone", 8.0, "dense"]], "spears", SPEARS_BONE_LABEL),
		_kit_recipe(SPEARS_FLINT_RECIPE, "Spears", "knapping",
			[["stone", 1.0, ""], ["wood", 2.0, ""], ["fibre", 1.0, ""]], "spears", SPEARS_FLINT_LABEL),
		_kit_recipe(CLUBS_STONE_RECIPE, "Clubs", "knapping", [["stone", 1.0, ""], ["wood", 2.0, ""]],
			"clubs", CLUBS_STONE_LABEL),
		# **THE ANIMAL WEB'S KIT ITEM IS THE CROOK** (`recipes.json`) — a long bone hafted with fibre,
		# so it reads bone's LENGTH where the spears above read its density. It stands where `hurdles`
		# used to: hurdles are a crafted MATERIAL now (`docs/plan_standing_upkeep.md` §4.9 item 12) and
		# their recipe declares no `grades`, so a kit-shaped fixture keyed on them staged a row the sim
		# cannot produce — a material output carries no tier, no grade and no durability.
		_kit_recipe("crook", "Crook", "bone_working",
			[["bone", 1.0, "long"], ["fibre", 2.0, ""]]),
		_kit_recipe("sled", "Sled", "tanning", [["hide", 18.0, "tough"], ["fibre", 10.0, ""]]),
		_kit_recipe(CLUBS_BONE_RECIPE, "Clubs", "bone_working", [["bone", 10.0, "dense"]], "clubs",
			CLUBS_BONE_LABEL),
		_kit_recipe("traps", "Traps", "weaving", [["fibre", 14.0, ""], ["hide", 6.0, ""]]),
		_tool_recipe("loom", "Loom", "tanning", [["hide", 14.0, ""]]),
		_tool_recipe("bone_awl", "Bone awl", "weaving", [["fibre", 12.0, ""], ["hide", 6.0, ""]]),
		{
			"id": "cordage", "display_name": "Cordage", "craft": "weaving",
			"group": HudCraftingVocab.GROUP_STOCK, "work": 3.0, "requires_knowledge": [],
			# TWO inputs, because the shipped stock recipe has two (`hurdles` = 4 wood · 2 hide) and a
			# one-input fixture cannot show whether the cost cell fits `a · b` PLUS the yield. `fibre`
			# stays first and keeps the `reads_axis` the row's role line reads; `hide` carries none,
			# and is a material the fixture already publishes and the band already banks, so no
			# other row in the chapter moves.
			"inputs": [
				{"material_id": "fibre", "amount": 12.0, "reads_axis": "strong"},
				{"material_id": "hide", "amount": 6.0, "reads_axis": ""},
			],
			"outputs": [{"equipment_id": "", "material_id": "cordage",
				"amount": STOCK_CORDAGE_YIELD}],
		},
	]

## A kit recipe. `item_id` is the equipment it makes — its own id unless it is one of several recipes
## for one item — and `label` its short name among those, `""` on a sole recipe.
func _kit_recipe(id: String, display_name: String, craft: String, inputs: Array,
		item_id: String = "", label: String = "") -> Dictionary:
	return _equipment_recipe(id, display_name, craft, HudCraftingVocab.GROUP_KIT, inputs, item_id,
		label)

func _tool_recipe(id: String, display_name: String, craft: String, inputs: Array) -> Dictionary:
	return _equipment_recipe(id, display_name, craft, HudCraftingVocab.GROUP_TOOL, inputs)

func _equipment_recipe(id: String, display_name: String, craft: String, group: String,
		inputs: Array, item_id: String = "", label: String = "") -> Dictionary:
	var rows: Array = []
	for input in inputs:
		rows.append({
			"material_id": String(input[0]), "amount": float(input[1]),
			"reads_axis": String(input[2]),
		})
	return {
		"id": id, "display_name": display_name, "craft": craft, "group": group,
		"work": 5.0, "requires_knowledge": [], "inputs": rows, "label": label,
		"outputs": [{"equipment_id": item_id if item_id != "" else id, "material_id": "",
			"amount": 1.0}],
	}

## The band the panel is composed for — the prototype's own, carrying every state the ledger has to
## tell apart at once.
func _crafting_band() -> Dictionary:
	var band := BandFx.with_band_id({
		"name": "Pinewold", "id": "Pinewold",
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
	# …and the sim's own reading of that reason: a bench waiting for its crew is a PROMPT, because the
	# player is who staffs it.
	bench["blocked_severity"] = BLOCKED_BENCH_SEVERITY
	# Nobody on it accrues nothing, and the sim says so as a rate rather than leaving it to be
	# inferred from the crew — the "nothing to compute" half of the estimate's gate.
	bench["rate_per_turn"] = BLOCKED_BENCH_RATE
	band["bench"] = bench
	return band

## **THE STOPPED BENCH WHOSE RATE IS FINE.** The crew is standing there and the tool is live, so the
## sim publishes a real rate; what stops it is that the store cannot cover the next draw, which is a
## `blockedReason` and nothing else. It is also the chapter's only UNDRAWN job, so the ✕'s no-pile
## wording is asked of a bench that genuinely has nothing cut rather than of an idle one with no ✕ at
## all.
func _short_bench_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["workers"] = SHORT_BENCH_CREW
	bench["progress"] = SHORT_BENCH_PROGRESS
	bench["rate_per_turn"] = SHORT_BENCH_RATE
	bench["blocked_reason"] = SHORT_BENCH_REASON
	bench["blocked_severity"] = SHORT_BENCH_SEVERITY
	bench["shortfalls"] = [{"material_id": "fibre", "required": 4.0, "held": 3.4, "short": 0.6}]
	bench["drawn"] = false
	# The ONE order is short too, so nothing can be worked: `worked` stays 0 and the well reads the
	# blocked head exactly as before — the order's own reason is the bench's.
	bench["orders"] = [_order(BASKETS_REED_RECIPE, BENCH_ORDER_COUNT, 0, SHORT_BENCH_PROGRESS, false,
		SHORT_BENCH_REASON, SHORT_BENCH_SEVERITY)]
	bench["output_grade"] = ""
	bench["drawn_inputs"] = []
	band["bench"] = bench
	return band

## **THE BENCH WHOSE WIRE AND WORDING DISAGREE.** The short bench's shape with a shortfall sentence
## stamped `neutral` — a payload no sim produces, which is exactly its value: the tint has to come from
## the field beside the reason, and nothing else in this chapter can tell a panel reading the field
## from one reading the string.
func _mismatched_severity_bench_band() -> Dictionary:
	var band := _short_bench_band()
	var bench: Dictionary = band["bench"]
	bench["blocked_reason"] = MISMATCHED_BENCH_REASON
	bench["blocked_severity"] = MISMATCHED_BENCH_SEVERITY
	band["bench"] = bench
	return band

## The same running bench at a progress the round numbers cannot reach — a remainder that does not
## divide by the rate, so only a CEILING renders the fourth turn.
func _ceil_bench_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["progress"] = CEIL_BENCH_PROGRESS
	bench["rate_per_turn"] = CEIL_BENCH_RATE
	band["bench"] = bench
	return band

## …and the same bench inside one turn of finishing, which is where the wording changes.
func _next_turn_bench_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["progress"] = NEXT_TURN_BENCH_PROGRESS
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
		# **THE CRAFTED PILE, AT TWO RATINGS.** Two batches rather than one because the rail draws a
		# pile rating by rating, and because the ledger's Owned cell SUMS them — a cell printing the
		# first amount it finds passes every one-batch claim. The amounts are FRACTIONAL and so is the
		# 9.3 they make: an Owned cell reading `×n` renders that as `×9`, which is the defect this
		# shape is chosen to expose.
		_batch("cordage", STOCK_CORDAGE_STOUT, [["stoutness", 0.62, "good"], ["span", 0.20, "poor"]]),
		_batch("cordage", STOCK_CORDAGE_SLACK, [["stoutness", 0.28, "poor"], ["span", 0.71, "good"]]),
		_batch("wood", 11.0, [["hard", 0.40, "fair"], ["pliant", 0.62, "good"]]),
		_batch("stone", 6.0, [["hard", 0.70, "good"], ["workable", 0.40, "fair"]]),
	]

func _batch(material_id: String, amount: float, readings: Array) -> Dictionary:
	var rows: Array = []
	for reading in readings:
		rows.append({"axis": String(reading[0]), "value": float(reading[1]),
			"band_name": String(reading[2])})
	return {"material_id": material_id, "amount": amount, "readings": rows, "variety_name": ""}

## **THE RATE IS PUBLISHED, NOT DERIVED**, so the fixture states it as the sim would: two crafters at
## the bare-handed 0.5 deliver 1.0 a turn, which is exactly the reading a client multiplying the crew
## by anything of its own would get wrong. The withdrawal rides beside it, because `drawn: true` says
## a pile exists and cannot say what is in it.
func _bench() -> Dictionary:
	return {
		"recipe_id": BASKETS_REED_RECIPE, "display_name": BENCH_TWO_RECIPE_NAME, "workers": BENCH_CREW,
		"progress": BENCH_PROGRESS, "work": BENCH_WORK, "teaches": "weaving", "blocked_reason": "",
		"shortfalls": [], "drawn": true, "output_grade": "good",
		"rate_per_turn": BENCH_RATE, "drawn_inputs": _drawn_inputs(),
		# The order every scalar here describes — the head, which can be worked.
		"worked": 0,
		# **A RUNNING BENCH ALWAYS PUBLISHES ITS QUEUE**, the head first — every scalar above is that
		# order's. One basket of two already made, which is what the head row's `1/2` reads.
		"orders": [_order(BASKETS_REED_RECIPE, BENCH_ORDER_COUNT, BENCH_ORDER_MADE, BENCH_PROGRESS,
			true)],
	}

## One `BenchState.orders` row, in the decoder's own shape.
func _order(recipe_id: String, count: int, made: int, progress: float, drawn: bool,
		blocked_reason: String = "", blocked_severity: String = "") -> Dictionary:
	return {"recipe_id": recipe_id, "count": count, "made": made, "progress": progress, "drawn": drawn,
		"blocked_reason": blocked_reason, "blocked_severity": blocked_severity}

## The pile already cut, in the recipe's own input order.
func _drawn_inputs() -> Array:
	return [
		{"material_id": "fibre", "amount": BENCH_DRAWN_FIBRE},
		{"material_id": "hide", "amount": BENCH_DRAWN_HIDE},
	]

func _idle_bench() -> Dictionary:
	return {
		"recipe_id": "", "display_name": "", "workers": 0, "progress": 0.0, "work": 0.0,
		"teaches": "", "blocked_reason": "", "shortfalls": [],
		"drawn": false, "output_grade": "", "rate_per_turn": 0.0, "drawn_inputs": [],
		# An IDLE bench is an empty queue, and `worked` reads 0.
		"orders": [], "worked": 0,
	}

## **ONE ROW PER RECIPE, ALWAYS**, each carrying the reason and the severity the SIM resolved. Every
## string here is one the sim's own vocabulary produces (`.claude/rules/core_sim/crafting.md`).
func _craft_offers() -> Array:
	return [
		_offer("wayfinding", "Wayfinding gear", HudCraftingVocab.GROUP_KIT, "wayfinding", true,
			"Scouts see 1 tile, not 2", HudCraftingVocab.SEVERITY_DANGER),
		# **BASKETS ARE THE SUBSTITUTE CASE** — two recipes, ONE tier — so both publish
		# `owned_at_tier` -1 and the popup has no Owned column. The reed recipe is the running job and
		# the suggested one: the sim suggests the recipe last started while it can still be made.
		_offer(BASKETS_REED_RECIPE, "Baskets", HudCraftingVocab.GROUP_KIT, "baskets", true,
			"Reed, no loom → fair", HudCraftingVocab.SEVERITY_NEUTRAL, [], true,
			{"recipe_label": BASKETS_REED_LABEL, "output_grade": "fair", "makes": "6.8 carry",
				"lasts": "2500 gathered"}),
		_offer(BASKETS_WITHY_RECIPE, "Baskets", HudCraftingVocab.GROUP_KIT, "baskets", true,
			"Withy → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": BASKETS_WITHY_LABEL, "output_grade": "good", "makes": "8.0 carry",
				"lasts": "2500 gathered", "suggested": false}),
		# **SPEARS ARE THE TIER-DISTINCT CASE** — bone makes `plain`, knapping makes `flint` — so each
		# recipe states a count of its own. The bone recipe is SHORT and the flint one can be made, so
		# the sim suggests flint.
		_offer(SPEARS_BONE_RECIPE, "Spears", HudCraftingVocab.GROUP_KIT, "spears", false,
			"Short 4.9 bone", HudCraftingVocab.SEVERITY_DANGER,
			[{"material_id": "bone", "required": 8.0, "held": 3.1, "short": 4.9}], false,
			{"recipe_label": SPEARS_BONE_LABEL, "makes": SPEARS_BONE_MAKES,
				"lasts": SPEARS_BONE_LASTS, "suggested": false, "owned_at_tier": SPEARS_PLAIN_OWNED}),
		_offer(SPEARS_FLINT_RECIPE, "Spears", HudCraftingVocab.GROUP_KIT, "spears", true,
			"Flint → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": SPEARS_FLINT_LABEL, "output_grade": "good", "makes": SPEARS_FLINT_MAKES,
				"lasts": SPEARS_FLINT_LASTS, "owned_at_tier": 0}),
		_offer("crook", "Crook", HudCraftingVocab.GROUP_KIT, "crook", true,
			"Long bone → good", HudCraftingVocab.SEVERITY_NEUTRAL),
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
		# **THE ONE MATERIAL-ONLY ROW, AND ITS REFUSAL CARRIES NO `→ grade` TAIL.** It read
		# `Reed .70 → strong`, which is a payload no sim can resolve: `RecipeDef::grade_for` returns
		# `None` for a recipe whose outputs are all materials, so `outputGrade` is `""` and
		# `invitation` appends nothing. The `baskets` row above stages the SAME left-hand clause WITH
		# its tail (`Reed, no loom → fair`), which is what makes the pair a claim rather than a
		# panel that lost grade words everywhere — the band owns no loom, so both read `no loom`.
		_offer("cordage", "Cordage", HudCraftingVocab.GROUP_STOCK, "", true,
			"Reed, no loom", HudCraftingVocab.SEVERITY_NEUTRAL),
	]

## **ONE OFFER PER RECIPE, IN THE WIRE'S OWN SHAPE.** The defaults are a SOLE recipe's: suggested (the
## only one in its row), no label, nothing it `makes` or `lasts` worth stating, `owned_at_tier` -1.
## `recipe` overrides any of those for a recipe that is one of several — the label, the headline
## stats and the count at its own tier.
func _offer(recipe_id: String, display_name: String, group: String, output_item_id: String,
		available: bool, reason: String, severity: String, shortfalls: Array = [],
		on_bench: bool = false, recipe: Dictionary = {}) -> Dictionary:
	var offer := {
		"recipe_id": recipe_id, "display_name": display_name, "group": group,
		"output_item_id": output_item_id, "available": available, "queueable": true, "reason": reason,
		"severity": severity, "shortfalls": shortfalls, "output_grade": "", "on_bench": on_bench,
		"recipe_label": "", "makes": "", "lasts": "", "suggested": true,
		"owned_at_tier": HudCraftingVocab.OWNED_AT_TIER_UNATTRIBUTED,
	}
	offer.merge(recipe, true)
	return offer

## What the band OWNS, and how much life is in it. **`count == 0` means it owns none**, and `life` is
## what tells a worn-out item from one that was never made.
func _equipment_batches() -> Array:
	return [
		_batch_row("wayfinding", "", "", 0, 0.0, "Worn out", HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("baskets", TIER_PLAIN, "good", 4, 8.0, "~1 turn left",
			HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("spears", TIER_PLAIN, "good", SPEARS_PLAIN_OWNED, 34.0, "~15 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("crook", TIER_PLAIN, "good", 2, 62.0, "~28 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("sled", TIER_PLAIN, "fair", 1, 71.0, "~42 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("clubs", TIER_PLAIN, "excellent", 5, 96.0, "48 raids left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("traps", TIER_PLAIN, "good", 8, 100.0, "Untouched",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("loom", "", "", 0, 0.0, "Never made", HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("bone_awl", TIER_PLAIN, "poor", 1, 47.0, "~19 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
	]

# ---- the SECOND-TIER fixture --------------------------------------------------------------------

## **A BAND THAT CAN KNAP FLINT AND IS STILL CARRYING PLAIN CLUBS.** The one shape in which the
## suggested recipe and the cell disagree — the band holds four plain clubs while the row would make
## flint — and the popup's per-recipe `owned_at_tier` counts are what say so. Everything in it is what
## the sim would have resolved: the tier words its own, the grades the shared `characteristic_bands`
## words. **Both spear recipes can be made here**, which is what lets the picker
## choose the NON-suggested one — the only way Start's claim can be a claim about the choice.
func _two_tier_band() -> Dictionary:
	var band := _crafting_band()
	band["craft_offers"] = _two_tier_offers()
	band["equipment_batches"] = _two_tier_equipment_batches()
	return band

## Spears and clubs carry a bone and a knapped recipe each, the knapped one suggested; traps and the
## loom are single-recipe rows, the loom under `Bench tools`, which is what makes the fold claim a
## statement about ONE group rather than about the table.
func _two_tier_offers() -> Array:
	return [
		_offer(SPEARS_BONE_RECIPE, "Spears", HudCraftingVocab.GROUP_KIT, "spears", true,
			"Bone + bone awl → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": SPEARS_BONE_LABEL, "output_grade": "good", "makes": SPEARS_BONE_MAKES,
				"lasts": SPEARS_BONE_LASTS, "suggested": false, "owned_at_tier": 0}),
		_offer(SPEARS_FLINT_RECIPE, "Spears", HudCraftingVocab.GROUP_KIT, "spears", true,
			"Flint → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": SPEARS_FLINT_LABEL, "output_grade": "good", "makes": SPEARS_FLINT_MAKES,
				"lasts": SPEARS_FLINT_LASTS,
				"owned_at_tier": TWO_TIER_SPEARS_GOOD_A + TWO_TIER_SPEARS_GOOD_B + TWO_TIER_SPEARS_EXCELLENT}),
		_offer(CLUBS_BONE_RECIPE, "Clubs", HudCraftingVocab.GROUP_KIT, "clubs", true,
			"Bone → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": CLUBS_BONE_LABEL, "output_grade": "good", "makes": "6.0 attack",
				"lasts": "50 blows", "suggested": false, "owned_at_tier": TWO_TIER_CLUBS_POOR}),
		_offer(CLUBS_STONE_RECIPE, "Clubs", HudCraftingVocab.GROUP_KIT, "clubs", true,
			"Stone → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			{"recipe_label": CLUBS_STONE_LABEL, "output_grade": "good", "makes": "9.0 attack",
				"lasts": "35 blows", "owned_at_tier": 0}),
		_offer("traps", "Traps", HudCraftingVocab.GROUP_KIT, "traps", true,
			"Reed → fair", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("loom", "Loom", HudCraftingVocab.GROUP_TOOL, "loom", true,
			"Unlocks excellent fibre work", HudCraftingVocab.SEVERITY_GOOD),
	]

## **THE SPEARS ARE THREE FLINT BATCHES AND TWO LINES.** Two of them are `good` at different wear and
## merge into one `×5`; the third is `excellent` and gets its own. The clubs are one batch at the OLDER
## `plain` tier,
## so the band carries two tiers at once. Every owned batch states its `tier_id`, so the negative that no
## tier word reaches a cell is asked over data that could leak one.
func _two_tier_equipment_batches() -> Array:
	return [
		_batch_row("spears", TIER_FLINT, "good", TWO_TIER_SPEARS_GOOD_A, 71.0, "~42 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("spears", TIER_FLINT, "good", TWO_TIER_SPEARS_GOOD_B, 34.0, "~15 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("spears", TIER_FLINT, "excellent", TWO_TIER_SPEARS_EXCELLENT, 96.0,
			"48 raids left", HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("clubs", TIER_PLAIN, "poor", TWO_TIER_CLUBS_POOR, 47.0, "~19 turns left",
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

# ---- the bench's RANK (`docs/plan_standing_upkeep.md` §4.9 item 9b) -----------------------------

## **THE ONE FRAME COMBINES EVERY NEW THING AT ONCE** — an IDLE bench (no job, no ✕), carrying a HIGH
## mark on its line two, with the rank picker OPEN beneath it and its `High` rung lit against two
## unlit ones. A frame per state would put the defect in the gap between them, and this arc has paid
## for that three times.
##
## **THE CARD RENDERS ONE BAND'S BENCH, so a marked bench and an unmarked one cannot share a PNG.**
## The unmarked half is therefore a claim rather than a picture, made on the reference band in this
## same block and paired with the marked one: *"prints a prefix"* alone passes on a panel that prints
## one always, and *"prints nothing"* alone passes on a panel that lost the mark entirely.
##
## The picker itself is `HudWidgets.build_work_priority_picker`, the work inspector's own control, so
## nothing here re-asserts its shape — what is under test is that this well MOUNTS it, on an idle
## bench, and that the press reaches the socket as `bench_priority`.
func _bench_priority_states() -> void:
	# --- the UNMARKED half, PNG-less. The reference bench carries no `priority` key at all, which is
	# what a `Normal` bench looks like on the wire, and its line two must be what it always was.
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the rank probe opens the panel", false)
		return
	h._assert_hud("crafting — a NORMAL bench prints no rank prefix (%s)"
			% [_priority_prefix(panel).text if _priority_prefix(panel) != null else "none"],
		_priority_prefix(panel) == null)
	h._assert_hud("crafting — …and still offers the `%s` link" % HudWorkVocab.WORK_INSPECT_PRIORITY,
		_priority_link(panel) != null)
	h._assert_hud("crafting — the picker is closed until the link is pressed (%d rungs)"
			% _priority_rungs(panel).size(),
		_priority_rungs(panel).is_empty())

	# --- the LINK, driven as real pointer input. `pressed.emit()` cannot see a covered, disabled,
	# zero-size or IGNORE-filtered control, which is how a dead control ships green.
	await _press_priority_link()
	panel = h._hud.crafting_panel().panel()
	var rungs := _priority_rungs(panel) if panel != null else []
	h._assert_hud("crafting — pressing the link opens the three-rung picker (%d)" % rungs.size(),
		rungs.size() == HudWorkVocab.WORK_PRIORITY_LEVELS.size())
	h._assert_hud("crafting — …lit on the level the bench is actually at (%s)"
			% _lit_priority_rung(panel),
		_lit_priority_rung(panel) == HudWorkVocab.WORK_PRIORITY_NORMAL)
	h._assert_hud("crafting — …under the hint that names no resource (%s)"
			% HudWorkVocab.WORK_PRIORITY_HINT,
		panel != null and _label_with_text(panel, HudWorkVocab.WORK_PRIORITY_HINT) != null)

	# --- THE COMMAND. Each rung asserted on its OWN payload, off the REAL relay (panel → controller →
	# `HudLayer`), because the panel's own signal says nothing about whether the seam carries it or
	# what band it ends up naming. **Every level, not just one**: a commit hard-wired to `normal` — the
	# shape a `_ =>` catch-all produces — satisfies any single-level claim.
	for level in HudWorkVocab.WORK_PRIORITY_LEVELS:
		await _assert_priority_rung_commits(String(level))

	# --- THE FRAME. The bare band's IDLE bench, marked HIGH — *the axes go first*, which is exactly
	# the moment a player states a rank with nothing on the bench — and the picker opened over it.
	h._hud.update_band_alerts([_marked_idle_band(HudWorkVocab.WORK_PRIORITY_HIGH)])
	h._hud.open_crafting_panel(_marked_idle_band(HudWorkVocab.WORK_PRIORITY_HIGH))
	await h._settle()
	await _press_priority_link()
	panel = h._hud.crafting_panel().panel()
	var prefix := _priority_prefix(panel) if panel != null else null
	h._assert_hud("crafting — a HIGH bench leads its line two with the mark (%s)"
			% [prefix.text if prefix != null else "no prefix at all"],
		prefix != null
			and prefix.text == HudWorkVocab.work_row_priority_prefix(HudWorkVocab.WORK_PRIORITY_HIGH))
	h._assert_hud("crafting — …in the tier's own ink (%s)"
			% [prefix.get_theme_color(FONT_COLOR_THEME_ITEM) if prefix != null else "none"],
		prefix != null and prefix.get_theme_color(FONT_COLOR_THEME_ITEM)
			== HudWorkVocab.work_priority_ink(HudWorkVocab.WORK_PRIORITY_HIGH))
	# The ✕ is what says this bench is IDLE — it is absent exactly when there is no job — so the
	# picker standing beside a well with no ✕ is the claim that the control renders on an empty bench.
	h._assert_hud("crafting — the picker renders on an IDLE bench (no ✕ beside it: %s)"
			% [_clear_button(panel) == null],
		panel != null and _clear_button(panel) == null
			and _priority_rungs(panel).size() == HudWorkVocab.WORK_PRIORITY_LEVELS.size())
	h._assert_hud("crafting — …opened on HIGH rather than on the default (%s)"
			% _lit_priority_rung(panel),
		_lit_priority_rung(panel) == HudWorkVocab.WORK_PRIORITY_HIGH)
	await h._save("crafting_bench_priority")

	# The other tier's ink, PNG-less — one frame can carry one bench, and DANGER-for-Low is the half
	# that says the ink is resolved from the level rather than pinned to the one colour above.
	h._hud.update_band_alerts([_marked_idle_band(HudWorkVocab.WORK_PRIORITY_LOW)])
	h._hud.open_crafting_panel(_marked_idle_band(HudWorkVocab.WORK_PRIORITY_LOW))
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	prefix = _priority_prefix(panel) if panel != null else null
	h._assert_hud("crafting — a LOW bench leads with its own face in its own ink (%s)"
			% [prefix.text if prefix != null else "no prefix at all"],
		prefix != null
			and prefix.text == HudWorkVocab.work_row_priority_prefix(HudWorkVocab.WORK_PRIORITY_LOW)
			and prefix.get_theme_color(FONT_COLOR_THEME_ITEM)
				== HudWorkVocab.work_priority_ink(HudWorkVocab.WORK_PRIORITY_LOW))

## Press one rung and assert what left the HUD. The picker CLOSES on a pick, so the link is re-pressed
## each time round — which is itself the claim that the control survives its own commit.
##
## **AND IT ASSERTS WHAT DID NOT GO OUT.** A mis-wired rung emitting `bench_crew` or `bench_remove`
## would satisfy a bare *"something was emitted"* and would silently re-crew or destroy the pile.
func _assert_priority_rung_commits(level: String) -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel != null and _priority_rungs(panel).is_empty():
		await _press_priority_link()
		panel = h._hud.crafting_panel().panel()
	var rung := _priority_rung(panel, level) if panel != null else null
	if rung == null:
		h._assert_hud("crafting — the picker offers a `%s` rung" % level, false)
		return
	var ranked: Array = []
	var others: Array = []
	var on_rank := func(payload: Dictionary) -> void: ranked.append(payload)
	var on_other := func(payload: Dictionary) -> void: others.append(payload)
	h._hud.bench_priority_requested.connect(on_rank)
	h._hud.bench_crew_requested.connect(on_other)
	h._hud.bench_remove_requested.connect(on_other)
	await _press_control(rung)
	h._hud.bench_priority_requested.disconnect(on_rank)
	h._hud.bench_crew_requested.disconnect(on_other)
	h._hud.bench_remove_requested.disconnect(on_other)
	if ranked.size() != 1:
		h._assert_hud("crafting — a REAL press on `%s` emitted exactly one rank (%s)" % [level, ranked],
			false)
		return
	# **ASSERTED ON THE COMMAND LINE, not on the payload dict** — the work board's own idiom — so the
	# verb, the token order and the level word are judged as the SOCKET would see them. That is also
	# what pins `bench_priority` as a sibling verb rather than a `work_priority` token.
	var band := _crafting_band()
	var line := String(MAIN_SCRIPT.format_bench_priority(ranked[0] as Dictionary).get("line", ""))
	var want := "bench_priority %d %d %s" % [HudConst.PLAYER_FACTION_ID,
		int(band.get("band_id", HudConst.NO_BAND_ID)), level]
	h._assert_hud("crafting — a REAL press on `%s` sends `%s` (got \"%s\")" % [level, want, line],
		line == want)
	h._assert_hud("crafting — …and touches neither the crew nor the job (%s)" % [others],
		others.is_empty())
	h._assert_hud("crafting — …and the picker closes on the pick",
		_priority_rungs(h._hud.crafting_panel().panel()).is_empty())

## The `Priority` link, driven. Separate from `_press_control` so the FIND is asserted rather than
## silently skipped — a link that stopped rendering would otherwise read as a picker that never opened.
func _press_priority_link() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var link := _priority_link(panel) if panel != null else null
	if link == null:
		h._assert_hud("crafting — the bench well carries a `%s` link to press"
			% HudWorkVocab.WORK_INSPECT_PRIORITY, false)
		return
	await _press_control(link)

## **REAL POINTER INPUT THROUGH THE REAL DISPATCH** — hover, press, release, at the control's own rect
## centre, the `selective_gather` chapter's idiom. The press rebuilds the bench well and frees the
## control, so no caller may touch it afterwards; `_settle` is what lets the freed generation leave the
## tree before anything is counted.
func _press_control(control: Control) -> void:
	# A control that did not render is a FAILED claim, not a script error: a null here would abort the
	# chapter and surface only as the checkpoint guard's short count.
	if control == null:
		h._assert_hud("crafting — the control to press was rendered", false)
		return
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		control.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h._settle()

## The `Priority` link, found by the meta the panel stamps on it. **A face match would be ambiguous
## across panels** — the work inspector's link wears the same word — and it would be asserting the
## string this chapter had already composed.
func _priority_link(node: Node) -> Button:
	if node is Button and node.has_meta(HudCraftingVocab.BENCH_PRIORITY_LINK_META):
		return node as Button
	for child in node.get_children():
		var found := _priority_link(child)
		if found != null:
			return found
	return null

## The rank's line-two prefix, found by IDENTITY: its face is a `HudWorkVocab` string this panel does
## not compose, so matching on the wording would test the other module's spelling. `null` on a Normal
## bench, which is the unmarked half of every pairing here.
func _priority_prefix(node: Node) -> Label:
	if node is Label and node.has_meta(HudCraftingVocab.BENCH_PRIORITY_META):
		return node as Label
	for child in node.get_children():
		var found := _priority_prefix(child)
		if found != null:
			return found
	return null

## Every rung of the open picker, by the meta the shared builder stamps — valued the LEVEL it would
## send, never the face, so a claim reads the button by what it would DO. Empty while the picker is
## closed, which is what the closed-by-default and closes-on-pick claims are made of.
func _priority_rungs(node: Node) -> Array:
	var found: Array = []
	if node is Button and node.has_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META):
		found.append(node)
	for child in node.get_children():
		found.append_array(_priority_rungs(child))
	return found

func _priority_rung(node: Node, level: String) -> Button:
	for rung in _priority_rungs(node):
		if String((rung as Button).get_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META)) == level:
			return rung as Button
	return null

## Which rung is LIT — **the SAME test `band_panel_preview._assert_work_priority_picker_lit` makes of
## this control on the work board**, the primary variant's own background, so the two surfaces cannot
## be judged by two different notions of "selected". `""` when the picker is closed or when nothing is
## lit, which is a real failure rather than a level.
func _lit_priority_rung(node: Node) -> String:
	if node == null:
		return ""
	for rung in _priority_rungs(node):
		var button := rung as Button
		var box := button.get_theme_stylebox(NORMAL_STYLEBOX_THEME_ITEM)
		if box is StyleBoxFlat \
				and (box as StyleBoxFlat).bg_color.is_equal_approx(HudStyle.BUTTON_PRIMARY_BG):
			return String(button.get_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META))
	return ""

## The `StyleBox` slot a `Button` draws at rest. Read back rather than eyeballed: the lit rung is what
## says which rank the bench is AT, and a frame cannot carry that claim.
const NORMAL_STYLEBOX_THEME_ITEM := "normal"

## The bare band's IDLE bench with a rank on it. **The rank rides an EMPTY bench deliberately** — it is
## a standing statement about the bench rather than about the job on it, and the sim publishes it on an
## idle bench for exactly that reason.
func _marked_idle_band(level: String) -> Dictionary:
	var band := _bare_band()
	var bench: Dictionary = band["bench"]
	bench[HudCraftingVocab.BENCH_PRIORITY_KEY] = level
	band["bench"] = bench
	return band

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

# ---- the last states: ONE ROW PER ITEM, ITS RECIPES BEHIND A LINK --------------------------------

## The Make picker's heading on the Spears row, spelled out rather than composed through
## `PICKER_HEADING_FORMAT` — an expectation built from the format under test only agrees with itself.
const SPEARS_PICKER_HEADING := "Make Spears with which recipe?"
## The bench title for a job on a two-recipe item: the sim's full name VERBATIM, then the craft.
const BENCH_TWO_RECIPE_TITLE := "Baskets (Reed) · Weaving"
## …and for a job on a single-recipe item, where the sim's name is the item's alone.
const BENCH_ONE_RECIPE_TITLE := "Crook · Bone-working"
## The first clause of each spear recipe's cost, as the cost cell renders it — the suggested flint
## recipe opens on stone, the bone one on fibre, so the Costs cell names which recipe it describes.
const SPEARS_FLINT_COST_LEAD := "1 stone · "
const SPEARS_BONE_COST_LEAD := "12 fibre · "

## **THE LAST STATES, APPENDED so no frame before them moves.** One short row per ITEM, an `N recipes`
## link on an item with a choice, a read-only popup comparing its recipes, and a picker under the row
## when Make has a choice to offer. Two frames — the popup open on Spears and the picker open on
## Spears — beside `crafting_panel`, which is the ledger at rest.
##
## **EVERY CLAIM IS A PAIR**, the chapter's discipline: a one-sided assertion passes on a panel that
## lost the feature. And every press is REAL POINTER INPUT through `_press_control`, the chapter's own
## idiom, because an emitted signal passes on a control that is covered or zero-size.
func _recipe_states() -> void:
	# Closed first, so the card opens as a fresh reading: the rank picker the state above left open is
	# VIEW state the card carries until it is dismissed, and it would sit over these frames.
	h._hud.close_crafting_panel()
	await h._settle()
	var band := _crafting_band()
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the recipes panel is open", false)
		return
	_assert_the_link_marks_a_choice(panel)
	_assert_the_row_describes_its_suggested_recipe(panel)
	# Six plain spears under a row that would make flint: which tier the band holds is the popup's
	# per-tier Owned column below, and no Owned cell may say it.
	_assert_no_tier_word_reaches_a_cell(band["equipment_batches"], "reference")
	_assert_the_bench_names_the_recipe_verbatim(panel)
	await _popup_states(panel)
	await _make_states(panel)
	await _start_sends_the_chosen_recipe()
	await _make_is_live_when_any_recipe_is()
	await _assert_a_single_recipe_bench_names_no_recipe()
	await _assert_one_row_per_material_made()
	h._hud.close_crafting_panel()
	await h._settle()

## **THE LINK IS THE NEWS THAT THERE IS A CHOICE.** On the two-recipe Spears row it reads `2 recipes`;
## on the single-recipe Crook row there is none — and that row keeps the role line the link replaces,
## which is what says the single-recipe row was left exactly as it was.
func _assert_the_link_marks_a_choice(panel: CraftingPanel) -> void:
	var spears_link := _recipes_link(_ledger_row(panel, "Spears"))
	h._assert_hud("crafting/recipes — a two-recipe row carries a `2 recipes` link (%s)"
			% [spears_link.text if spears_link != null else "no link"],
		spears_link != null and spears_link.text == "2 recipes")
	var crook_row := _ledger_row(panel, "Crook")
	h._assert_hud("crafting/recipes — …and a single-recipe row carries none, keeping its role line",
		crook_row != null and _recipes_link(crook_row) == null
			and _label_texts(crook_row).has("Bone-working"))

## **THE ROW DESCRIBES ITS SUGGESTED RECIPE.** Spears' bone recipe is short of bone and its flint one can
## be made, so the sim suggested flint — the Costs cell opens on stone, the refusal under Make is the
## flint recipe's, and the bone recipe's `Short 4.9 bone` is nowhere on the row. A row that described
## its FIRST offer instead would open on fibre and carry the shortage.
func _assert_the_row_describes_its_suggested_recipe(panel: CraftingPanel) -> void:
	var row := _ledger_row(panel, "Spears")
	var texts := _label_texts(row) if row != null else []
	h._assert_hud("crafting/recipes — the Spears row's Costs cell is the SUGGESTED flint recipe's (%s)"
			% [texts],
		texts.has(SPEARS_FLINT_COST_LEAD) and not texts.has(SPEARS_BONE_COST_LEAD))
	h._assert_hud("crafting/recipes — …and so is its refusal line, with the bone recipe's nowhere",
		texts.has("Flint → good") and not texts.has("Short 4.9 bone"))

## **THE BENCH NAMES THE RECIPE BY RENDERING THE SIM'S NAME, NOT BY COMPOSING ONE.** The running job is
## the reed basket, published as `Baskets (Reed)`; the title is that name and the craft, and the
## client-composed `Baskets · Reed` the prototype sketched is nowhere.
func _assert_the_bench_names_the_recipe_verbatim(panel: CraftingPanel) -> void:
	var texts := _label_texts(panel)
	h._assert_hud("crafting/recipes — a two-recipe job's bench title is the sim's name verbatim (%s)"
			% BENCH_TWO_RECIPE_TITLE,
		texts.has(BENCH_TWO_RECIPE_TITLE))
	h._assert_hud("crafting/recipes — …and the client composes no `%s` of its own" % BENCH_COMPOSED_NAME,
		_count_containing_ci(texts, BENCH_COMPOSED_NAME) == 0)

## **THE POPUP: one line per recipe, its labels verbatim, and an Owned column only where a count per
## recipe exists.** Spears' recipes make two tiers, so each carries its own count — `×6` on bone and
## `—` on flint; baskets' make one tier, so their popup has NO Owned column rather than a column of
## dashes. Then its three ways out: a second click of its own link, `Esc`, and a click outside it.
func _popup_states(panel: CraftingPanel) -> void:
	await _press_control(_recipes_link(_ledger_row(panel, "Spears")))
	var popup := panel.recipes_popup()
	h._assert_hud("crafting/recipes — pressing the link opens the popup",
		popup != null and popup.visible)
	if popup == null:
		return
	h._assert_hud("crafting/recipes — the popup lists one line per recipe, labels verbatim (%s)"
			% [_popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_RECIPE)],
		_popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_RECIPE)
			== [SPEARS_BONE_LABEL, SPEARS_FLINT_LABEL])
	h._assert_hud("crafting/recipes — a tier-distinct item's popup carries an Owned column (%s)"
			% [_popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_OWNED)],
		_popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_OWNED)
			== [HudCraftingVocab.OWNED_COUNT_FORMAT % SPEARS_PLAIN_OWNED, HudCraftingVocab.RECIPES_OWNED_NONE])
	h._assert_hud("crafting/recipes — …and each recipe's Makes and Lasts, verbatim",
		_label_texts(popup).has(SPEARS_FLINT_MAKES) and _label_texts(popup).has(SPEARS_BONE_LASTS))
	await h._save("crafting_recipes_popup")

	await _press_control(_recipes_link(_ledger_row(panel, "Spears")))
	h._assert_hud("crafting/recipes — a second click of its link closes the popup",
		not popup.visible)

	await _press_control(_recipes_link(_ledger_row(panel, "Baskets")))
	h._assert_hud("crafting/recipes — the substitute item's popup lists its recipes (%s)"
			% [_popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_RECIPE)],
		popup.visible and _popup_column(popup, HudCraftingVocab.RECIPES_COLUMN_RECIPE)
			== [BASKETS_REED_LABEL, BASKETS_WITHY_LABEL])
	h._assert_hud("crafting/recipes — …and carries NO Owned column, where every count is unattributed",
		not _popup_has_column(popup, HudCraftingVocab.RECIPES_COLUMN_OWNED))
	await _press_escape()
	h._assert_hud("crafting/recipes — Esc closes the popup", not popup.visible)

	await _press_control(_recipes_link(_ledger_row(panel, "Spears")))
	h._assert_hud("crafting/recipes — precondition: the popup is open again", popup.visible)
	await _press_card_chrome(panel)
	h._assert_hud("crafting/recipes — a click outside the popup closes it", not popup.visible)

## **MAKE ON ONE RECIPE STARTS IT; MAKE ON SEVERAL OPENS THE PICKER.** Each half is a pair — the single
## recipe is SENT and opens no picker, the two-recipe row opens the picker and sends NOTHING — because
## a Make that always picked, or never did, satisfies either half alone. In the picker the short bone
## recipe's radio is disabled and the flint one live, preselected because it is the suggestion.
func _make_states(panel: CraftingPanel) -> void:
	var sent: Array = []
	var on_bench := func(payload: Dictionary) -> void: sent.append(String(payload.get("recipe_id", "")))
	h._hud.bench_enqueue_requested.connect(on_bench)
	await _press_control(_make_button(panel, "crook"))
	h._assert_hud("crafting/recipes — Make on a single-recipe row sends that recipe (%s)" % [sent],
		sent == ["crook"])
	h._assert_hud("crafting/recipes — …and opens no picker", _picker(panel) == null)

	sent.clear()
	await _press_control(_make_button(panel, "spears"))
	var picker := _picker(panel)
	h._assert_hud("crafting/recipes — Make on a two-recipe row opens the picker under it",
		picker != null and String(picker.get_meta(HudCraftingVocab.PICKER_META)) == "spears")
	h._assert_hud("crafting/recipes — …and sends nothing (%s)" % [sent], sent.is_empty())
	h._assert_hud("crafting/recipes — the picker asks which recipe, naming the item",
		picker != null and _label_texts(picker).has(SPEARS_PICKER_HEADING))
	var bone := _picker_option(panel, SPEARS_BONE_RECIPE)
	var flint := _picker_option(panel, SPEARS_FLINT_RECIPE)
	h._assert_hud("crafting/recipes — a known recipe short of material has a LIVE radio (queueable, not available)",
		bone != null and not bone.disabled)
	h._assert_hud("crafting/recipes — …and the suggested one is live and chosen",
		flint != null and not flint.disabled and flint.button_pressed)
	h._assert_hud("crafting/recipes — …the short recipe states no refusal line",
		picker != null and not _label_texts(picker).has("Short 4.9 bone"))
	await h._save("crafting_make_picker")

	await _press_control(_make_button(panel, "spears"))
	h._assert_hud("crafting/recipes — pressing Make again closes the picker", _picker(panel) == null)
	h._hud.bench_enqueue_requested.disconnect(on_bench)

## **START SENDS THE CHOSEN RECIPE, AND NOT THE OTHER.** Staged where BOTH spear recipes can be made and
## the sim suggests flint, so choosing bone is a choice against the default — the only shape in which
## "sends the chosen one" and "sends the suggested one" answer differently. Cancel is the other way out,
## and sends nothing.
func _start_sends_the_chosen_recipe() -> void:
	var band := _two_tier_band()
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/recipes — the choice panel is open", false)
		return
	var sent: Array = []
	var on_bench := func(payload: Dictionary) -> void: sent.append(String(payload.get("recipe_id", "")))
	h._hud.bench_enqueue_requested.connect(on_bench)
	await _press_control(_make_button(panel, "spears"))
	var flint := _picker_option(panel, SPEARS_FLINT_RECIPE)
	h._assert_hud("crafting/recipes — precondition: the picker opens on the suggested flint recipe",
		flint != null and flint.button_pressed)
	await _press_control(_picker_option(panel, SPEARS_BONE_RECIPE))
	await _press_control(_picker_control(panel, HudCraftingVocab.PICKER_START_META))
	h._assert_hud("crafting/recipes — Start sends the CHOSEN recipe and not the suggested one (%s)"
			% [sent],
		sent == [SPEARS_BONE_RECIPE])
	h._assert_hud("crafting/recipes — …and closes the picker", _picker(panel) == null)

	sent.clear()
	await _press_control(_make_button(panel, "spears"))
	await _press_control(_picker_control(panel, HudCraftingVocab.PICKER_CANCEL_META))
	h._assert_hud("crafting/recipes — Cancel closes the picker and sends nothing (%s)" % [sent],
		_picker(panel) == null and sent.is_empty())
	h._hud.bench_enqueue_requested.disconnect(on_bench)

## **MAKE IS LIVE WHEN ANY RECIPE IS QUEUEABLE**, and the Costs cell still describes the suggestion. The
## fixture suggests the bone recipe (short of material) while the flint one can run — a shape the shipped
## sim's own rule does not produce, staged because the client's rule is "any", not "the suggested one".
## The clubs row, whose only recipe is short but KNOWN, keeps a live Make; with its craft unlearned
## (`queueable=false`) the button goes dead and the reason shows.
func _make_is_live_when_any_recipe_is() -> void:
	var band := _crafting_band()
	var offers: Array = band["craft_offers"]
	for offer in offers:
		var candidate: Dictionary = offer
		if String(candidate.get("output_item_id", "")) == "spears":
			candidate["suggested"] = String(candidate.get("recipe_id", "")) == SPEARS_BONE_RECIPE
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/recipes — the any-recipe panel is open", false)
		return
	var make := _make_button(panel, "spears")
	h._assert_hud("crafting/recipes — Make is LIVE when only the non-suggested recipe can be made",
		make != null and not make.disabled)
	var texts := _label_texts(_ledger_row(panel, "Spears"))
	h._assert_hud("crafting/recipes — …while the Costs cell still describes the suggested one (%s)"
			% [texts],
		texts.has(SPEARS_BONE_COST_LEAD))
	var clubs := _make_button(panel, "clubs")
	h._assert_hud("crafting/recipes — …and a row whose only recipe is short but KNOWN has Make LIVE",
		clubs != null and not clubs.disabled)
	# An UNLEARNED craft is the one thing that gates the queue now: Make dead, the sim's reason under it.
	var unlearned := _crafting_band()
	for offer in unlearned["craft_offers"]:
		var candidate_offer: Dictionary = offer
		if String(candidate_offer.get("recipe_id", "")) == "clubs":
			candidate_offer["queueable"] = false
	h._hud.update_band_alerts([unlearned])
	h._hud.crafting_panel().refresh_snapshot()
	await h._settle()
	panel = h._hud.crafting_panel().panel()
	clubs = _make_button(panel, "clubs") if panel != null else null
	h._assert_hud("crafting/recipes — an UNLEARNED recipe keeps Make disabled, with its reason under it",
		clubs != null and clubs.disabled
			and _label_texts(_ledger_row(panel, "Clubs")).has("Short 6.9 bone"))

## **A SINGLE-RECIPE JOB'S BENCH NAMES THE ITEM ALONE**, the other half of the verbatim pair: the sim
## publishes `Crook`, and the title is that and the craft, with nothing appended.
func _assert_a_single_recipe_bench_names_no_recipe() -> void:
	var band := _crafting_band()
	var bench: Dictionary = band["bench"]
	bench["recipe_id"] = "crook"
	bench["display_name"] = "Crook"
	bench["teaches"] = "bone_working"
	band["bench"] = bench
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	h._assert_hud("crafting/recipes — a single-recipe job's bench title is the item and the craft (%s)"
			% BENCH_ONE_RECIPE_TITLE,
		panel != null and _label_texts(panel).has(BENCH_ONE_RECIPE_TITLE))

## **ONE ROW PER THING MADE, AND A MATERIAL IS A THING.** The sim keys a row by what it makes — the
## equipment, or else the first material — and marks one `suggested` offer per such row, so two stock
## recipes twisting one material are ONE row under a `2 recipes` link, exactly like Spears. A client
## keying a stock row by its recipe draws two Hurdles rows, one of them describing an offer the sim did
## not suggest. Paired with the reference band's single-recipe Cordage row, which must stay one row with
## NO link — without it the first claim passes on a panel that links every stock row. PNG-less, and the
## recipe book is handed back afterwards, so no frame after it moves.
func _assert_one_row_per_material_made() -> void:
	var recipes := _recipes()
	recipes.append(_stock_recipe(HURDLES_WOOD_RECIPE, HURDLES_WOOD_LABEL, [["wood", 4.0, ""],
		["hide", 2.0, ""]]))
	recipes.append(_stock_recipe(HURDLES_WITHY_RECIPE, HURDLES_WITHY_LABEL, [["fibre", 6.0, ""],
		["hide", 2.0, ""]]))
	h._hud.update_crafting_catalogues(_materials(), _characteristic_bands(), recipes,
		_craft_knowledge())
	var band := _crafting_band()
	var offers: Array = band["craft_offers"]
	offers.append(_offer(HURDLES_WOOD_RECIPE, HURDLES_NAME, HudCraftingVocab.GROUP_STOCK, "", true,
		"Wood", HudCraftingVocab.SEVERITY_NEUTRAL, [], false, {"recipe_label": HURDLES_WOOD_LABEL}))
	offers.append(_offer(HURDLES_WITHY_RECIPE, HURDLES_NAME, HudCraftingVocab.GROUP_STOCK, "", true,
		"Withy", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
		{"recipe_label": HURDLES_WITHY_LABEL, "suggested": false}))
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting/recipes — the material-rows panel is open", false)
	else:
		var hurdles_rows := _rows_named(panel, HURDLES_NAME)
		var link := _recipes_link(hurdles_rows[0]) if hurdles_rows.size() == 1 else null
		h._assert_hud("crafting/recipes — two stock recipes making one material are ONE row with a "
				+ "`2 recipes` link (%d rows, %s)" % [hurdles_rows.size(),
					link.text if link != null else "no link"],
			hurdles_rows.size() == 1 and link != null and link.text == "2 recipes")
		var cordage_rows := _rows_named(panel, "Cordage")
		h._assert_hud("crafting/recipes — …while the single-recipe Cordage row is one row with no link "
				+ "(%d rows)" % cordage_rows.size(),
			cordage_rows.size() == 1 and _recipes_link(cordage_rows[0]) == null)
	h._hud.update_crafting_catalogues(_materials(), _characteristic_bands(), _recipes(),
		_craft_knowledge())
	h._hud.update_band_alerts([_crafting_band()])

## A material recipe: its output is `material_id`, which is what the ledger keys its row by.
func _stock_recipe(id: String, label: String, inputs: Array) -> Dictionary:
	var rows: Array = []
	for input in inputs:
		rows.append({"material_id": String(input[0]), "amount": float(input[1]),
			"reads_axis": String(input[2])})
	return {
		"id": id, "display_name": HURDLES_NAME, "craft": "shaping",
		"group": HudCraftingVocab.GROUP_STOCK, "work": 3.0, "requires_knowledge": [],
		"inputs": rows, "label": label,
		"outputs": [{"equipment_id": "", "material_id": HURDLES_MATERIAL, "amount": 1.0}],
	}

## Every ledger row whose Item cell carries `item_name` — the rows, not the first match, since the
## claim is how MANY there are. A row is an `HBoxContainer` whose own direct children include the
## Item cell, so a match is kept only where no descendant row already matched.
func _rows_named(node: Node, item_name: String) -> Array:
	var found: Array = []
	for child in node.get_children():
		found.append_array(_rows_named(child, item_name))
	if found.is_empty() and node is HBoxContainer and _label_texts(node).has(item_name):
		found.append(node)
	return found

## The `N recipes` link under ONE row, found by the meta the panel stamps — `null` on a row without one.
func _recipes_link(node: Node) -> LinkButton:
	if node == null:
		return null
	if node is LinkButton and node.has_meta(HudCraftingVocab.RECIPES_LINK_META):
		return node as LinkButton
	for child in node.get_children():
		var found := _recipes_link(child)
		if found != null:
			return found
	return null

## A row's Make button, by the row key the panel stamps on it.
func _make_button(node: Node, key: String) -> Button:
	if node is Button and String(node.get_meta(HudCraftingVocab.MAKE_BUTTON_META, "")) == key:
		return node as Button
	for child in node.get_children():
		var found := _make_button(child, key)
		if found != null:
			return found
	return null

## The open Make picker — `null` when none is open, which is half of several claims.
func _picker(node: Node) -> Control:
	if node is Control and node.has_meta(HudCraftingVocab.PICKER_META):
		return node as Control
	for child in node.get_children():
		var found := _picker(child)
		if found != null:
			return found
	return null

## One recipe's radio in the open picker, by the recipe id it would send.
func _picker_option(node: Node, recipe_id: String) -> CheckBox:
	if node is CheckBox and String(node.get_meta(HudCraftingVocab.PICKER_OPTION_META, "")) == recipe_id:
		return node as CheckBox
	for child in node.get_children():
		var found := _picker_option(child, recipe_id)
		if found != null:
			return found
	return null

## One of the picker's footer buttons, by its meta.
func _picker_control(node: Node, meta: String) -> Button:
	if node is Button and node.has_meta(meta):
		return node as Button
	for child in node.get_children():
		var found := _picker_control(child, meta)
		if found != null:
			return found
	return null

## The texts of ONE popup column's cells, in order — the column's own head excluded. Each cell carries
## its column head as a meta, so a column is read by what it IS rather than by where its text lines up.
func _popup_column(popup: Node, head: String) -> Array:
	var texts: Array = []
	_collect_popup_column(popup, head, texts)
	return texts

func _collect_popup_column(node: Node, head: String, into: Array) -> void:
	if String(node.get_meta(HudCraftingVocab.RECIPES_POPUP_COLUMN_META, "")) == head:
		var cell_text := " ".join(_label_texts(node))
		if cell_text != head.to_upper():
			into.append(cell_text)
		return
	for child in node.get_children():
		_collect_popup_column(child, head, into)

func _popup_has_column(popup: Node, head: String) -> bool:
	if String(popup.get_meta(HudCraftingVocab.RECIPES_POPUP_COLUMN_META, "")) == head:
		return true
	for child in popup.get_children():
		if _popup_has_column(child, head):
			return true
	return false

## `Esc`, as a real key event through the real dispatch — the popup is a Window, and a Window closes
## itself on the cancel action; this is what says the panel's view state follows it.
func _press_escape() -> void:
	var viewport: Viewport = h.get_viewport()
	var down := InputEventKey.new()
	down.keycode = KEY_ESCAPE
	down.physical_keycode = KEY_ESCAPE
	down.pressed = true
	viewport.push_input(down)
	await h.get_tree().process_frame
	var up := down.duplicate() as InputEventKey
	up.pressed = false
	viewport.push_input(up)
	await h._settle()

## A press on the card's own chrome, just inside its top-left corner — outside the popup, and on a
## surface with nothing to fire.
func _press_card_chrome(panel: CraftingPanel) -> void:
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		panel.card().get_global_rect().position + Vector2(CARD_CHROME_PROBE_INSET, CARD_CHROME_PROBE_INSET))
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h._settle()

# ---- the shrug with a link: a control IN the item cell ------------------------------------------

## **A SHRUG ROW WHOSE ITEM CELL HOLDS A CONTROL.** Traps gain a second recipe, so their item cell
## carries the `N recipes` link in place of the role line — a live control inside the very cell the
## shrug dims. The first cut of the fix faded that whole cell, link included; this frame is where a
## player would see it, and the claim is that the link and Make stay full-strength beside a dimmed name.
func _shrug_with_a_link_state() -> void:
	var band := _crafting_band()
	var offers: Array = band["craft_offers"]
	offers.append(_offer(TRAPS_SINEW_RECIPE, "Traps", HudCraftingVocab.GROUP_KIT, "traps", true,
		"Sinew → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
		{"recipe_label": TRAPS_SINEW_LABEL, "output_grade": "good", "suggested": false}))
	band["craft_offers"] = offers
	h._hud.update_band_alerts([band])
	h._hud.open_crafting_panel(band)
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the shrug-with-a-link panel is open", false)
		return
	# Precondition: the row really is a multi-recipe row, else the link half is vacuous.
	h._assert_hud("crafting — the multi-recipe shrug row carries its recipes link",
		_recipes_link(_ledger_row(panel, "Traps")) != null)
	_assert_no_control_on_the_row_is_dimmed(panel, "Traps", 2)
	await h._save("crafting_panel_shrug_recipes")
	h._hud.close_crafting_panel()
	await h._settle()
