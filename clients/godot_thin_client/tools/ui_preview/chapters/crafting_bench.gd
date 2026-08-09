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

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The band this chapter composes for. Its own entity, so it cannot be confused with the reference
## band the rest of the run uses.
const CRAFTING_BAND_ENTITY := 971

## The ladder each craft's `progress` is measured against — the sim's published
## `completion_threshold`, which is what stops the client inventing a scale of its own.
const CRAFT_THRESHOLD := 100.0

## Where each of the three crafts stands. Weaving is DONE (this band gathers), Tanning is climbing (it
## hunts deer) and Bone-working is stalled at 12% — which is not an error state but a band standing in
## country with no bone game. The land decides what you are good at.
## The crew weaving baskets on the running bench — named because the head-count claims below are
## arithmetic ABOUT it, and a literal repeated in three places is a claim nobody can re-derive.
const BENCH_CREW := 2

## The workforce of the bench-bound band: all but one worker is at the bench, which is the only shape
## in which "idle" and "how many could be at the bench" produce a visibly different stepper.
const BENCH_BOUND_WORKING_AGE := 3

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

	# Hand everything back: the panel closed, the roster restored to the reference band.
	h._hud.close_crafting_panel()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await h._settle()

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
	# negative on its own, and the tier chip is what has to carry the ownership reading alone.
	h._assert_hud("crafting — the tier chip states ownership",
		texts.has(HudCraftingVocab.TIER_CHIP_KIT_NONE) and texts.has(HudCraftingVocab.TIER_CHIP_TOOL_NONE))
	h._assert_hud("crafting — no condition wording reaches the ledger",
		_missing_from(texts, RETIRED_CONDITION_WORDINGS) == RETIRED_CONDITION_WORDINGS.size())
	# The ledger is FOUR columns — Item · Tier · Rebuild costs · action — and the action head is blank
	# by design, so the three named ones are what the claim can name.
	h._assert_hud("crafting — the ledger heads its four columns",
		texts.has(HudCraftingVocab.LEDGER_COLUMN_ITEM.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_TIER.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_COST.to_upper()))
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
	# The reading state 4 measures its own against: nothing is docked here, so this is the tallest the
	# card ever wants to be.
	_unreserved_card_height = panel.size.y

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
	if node is ScrollContainer:
		return (node as ScrollContainer).vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED
	for child in node.get_children():
		if _scroll_is_live(child):
			return true
	return false

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

## **A BAND WHOSE BENCH HOLDS ALL BUT ONE OF ITS WORKERS.** `working_age` is what is lowered, never
## `idle_workers`: `effective_idle` derives idle from the workforce, its assignments and the bench, so
## writing the published field alone would leave every claim below reading the reference band's 16.
func _bench_bound_band() -> Dictionary:
	var band := _crafting_band()
	band["working_age"] = BENCH_BOUND_WORKING_AGE
	band["idle_workers"] = BENCH_BOUND_WORKING_AGE - BENCH_CREW
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
		"shortfalls": [], "items_completed": 1, "drawn": true, "output_grade": "standard",
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
			"Reed, no loom → standard", HudCraftingVocab.SEVERITY_NEUTRAL, [], true),
		_offer("spears", "Spears", HudCraftingVocab.GROUP_KIT, "spears", false,
			"Short 4.9 bone", HudCraftingVocab.SEVERITY_DANGER,
			[{"material_id": "bone", "required": 8.0, "held": 3.1, "short": 4.9}]),
		_offer("husbandry_gear", "Husbandry gear", HudCraftingVocab.GROUP_KIT, "husbandry_gear", true,
			"Hide + tanning frame → standard", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("sled", "Sled", HudCraftingVocab.GROUP_KIT, "sled", true,
			"Mammoth hide → fine", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("clubs", "Clubs", HudCraftingVocab.GROUP_KIT, "clubs", false,
			"Short 6.9 bone", HudCraftingVocab.SEVERITY_DANGER,
			[{"material_id": "bone", "required": 10.0, "held": 3.1, "short": 6.9}]),
		_offer("traps", "Traps", HudCraftingVocab.GROUP_KIT, "traps", true,
			"Not needed yet", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("loom", "Loom", HudCraftingVocab.GROUP_TOOL, "loom", true,
			"Unlocks fine fibre work", HudCraftingVocab.SEVERITY_GOOD),
		_offer("bone_awl", "Bone awl", HudCraftingVocab.GROUP_TOOL, "bone_awl", true,
			"Bone costs −25%", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("cordage", "Cordage", HudCraftingVocab.GROUP_STOCK, "", true,
			"Reed .70 → strong", HudCraftingVocab.SEVERITY_NEUTRAL),
	]

func _offer(recipe_id: String, display_name: String, group: String, output_item_id: String,
		available: bool, reason: String, severity: String, shortfalls: Array = [],
		on_bench: bool = false) -> Dictionary:
	return {
		"recipe_id": recipe_id, "display_name": display_name, "group": group,
		"output_item_id": output_item_id, "available": available, "reason": reason,
		"severity": severity, "shortfalls": shortfalls, "output_grade": "", "on_bench": on_bench,
	}

## What the band OWNS, and how much life is in it. **`count == 0` means it owns none**, and `life` is
## what tells a worn-out item from one that was never made.
func _equipment_batches() -> Array:
	return [
		_batch_row("wayfinding", "", "", 0, 0.0, "Worn out", HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("baskets", "flint", "standard", 4, 8.0, "~1 turn left",
			HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("spears", "flint", "standard", 6, 34.0, "~15 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("husbandry_gear", "flint", "standard", 2, 62.0, "~28 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("sled", "flint", "standard", 1, 71.0, "~42 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("clubs", "flint", "fine", 5, 96.0, "48 raids left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("traps", "flint", "standard", 8, 100.0, "Untouched",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("loom", "", "", 0, 0.0, "Never made", HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("bone_awl", "flint", "standard", 1, 47.0, "~19 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
	]

func _batch_row(item_id: String, tier_id: String, grade: String, count: int, remaining: float,
		life: String, life_severity: String) -> Dictionary:
	return {
		"item_id": item_id, "tier_id": tier_id, "grade": grade, "count": count,
		"remaining": remaining, "quanta_left": remaining, "quantum_noun": "uses",
		"life": life, "life_severity": life_severity,
	}
