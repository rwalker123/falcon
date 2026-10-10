class_name CraftingPanelController
extends RefCounted

## The MATERIALS & CRAFTING cluster (`docs/plan_crafting_and_materials.md` §7) — the controller half
## of `CraftingPanel`: it owns the panel node, holds the per-world crafting catalogues, resolves which
## band the panel is showing, and turns the panel's signals into the six commands the bench takes —
## `bench_enqueue`, `bench_crew`, `bench_order_count`, `bench_remove`, `bench_raise`, `bench_priority`.
##
## Built on the `TurnOrbController` / `DisclosureController` idiom: `HudLayer` holds one as
## `_crafting`, hands it the shared `HudBandLaborState` BY REFERENCE and a HOST `Node` (a `RefCounted`
## cannot `add_child`), keeps thin delegators for the entry points reached BY NAME, and RELAYS this
## controller's signals onto its own so `Main` can format the commands.
##
## **THE CATALOGUES LIVE HERE, NOT ON A STATE MODEL.** `hud-modules.md`'s test is whether two or more
## clusters read a field; exactly one reads these, so they are this controller's own state — the same
## call `BandPanelController` makes for its zone state. If a second surface ever wants the recipe book
## they move to `HudBandLaborState` beside the kit roster.
##
## **THE PANEL'S SUBJECT IS A BAND, RE-RESOLVED BY ENTITY EVERY SNAPSHOT.** Holding the dict would
## freeze the bench's progress and the ledger's life the moment a turn ticked; holding the entity and
## looking it up keeps the panel a live surface, exactly as `BandPanelController._resolve_panel_band`
## keeps the dock one. A band that leaves the roster closes the panel rather than stranding it on a
## band that no longer exists.

## Add an order to the back of the band's bench queue — `bench_enqueue <faction> <band> recipe <id>
## count <n>`: Make sends a count of one, a suggestion's Queue its whole count. **The player staffs
## the bench**: no crew rides here, so an idle bench takes the order at zero and waits for the `− n +`
## stepper (the crew stays with the bench across orders). The bench is staffed like a worked source
## rather than through a standing role, which is why there is no Crafter role card anywhere.
signal bench_enqueue_requested(payload: Dictionary)
## Re-crew the bench, leaving its queue and progress alone — `bench_crew <faction> <band> workers <n>`.
signal bench_crew_requested(payload: Dictionary)
## Change one order's count — `bench_order_count <faction> <band> order <i> count <n>`.
signal bench_order_count_requested(payload: Dictionary)
## Take one order off the queue — `bench_remove <faction> <band> order <i>`. A drawn pile is lost,
## which is what the control's tooltip names before it is pressed. The well's ✕ is `order <worked>`.
signal bench_remove_requested(payload: Dictionary)
## Move one order up a place — `bench_raise <faction> <band> order <i>`, never index 0.
signal bench_raise_requested(payload: Dictionary)
## Rank the bench against the band's other work — `bench_priority <faction> <band> high|normal|low`
## (`docs/plan_standing_upkeep.md` §4.9 item 9b). **A SIBLING VERB, not a `work_priority` token**:
## that grammar reads a lone trailing token as a herd id, so `work_priority … bench low` would be
## ambiguous with a herd named `bench`. It names the band and nothing else, one bench per band meaning
## there is no order argument to disambiguate — and it is legal on an IDLE bench, a rank being a standing
## statement about the bench rather than about the job on it.
signal bench_priority_requested(payload: Dictionary)
## Switch auto-craft on or off for the band's bench - `bench_auto <faction> <band> on|off`. Payload: { faction, band_id, on }.
signal bench_auto_requested(payload: Dictionary)
## Pass over the waiting auto order's item - `bench_auto_skip <faction> <band>`. Payload: { faction, band_id }.
signal bench_auto_skip_requested(payload: Dictionary)

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
## The HUD CanvasLayer, so this `RefCounted` has a node to parent the panel into.
var _host: Node = null
## **THE ROOM THE CARD IS BOUNDED BY — the HUD's `LayoutRoot`, NOT the window.** The reserved-edge
## registry has already inset that node by every docked panel's strip, so a card bounded by it is
## bounded by the same rect the map and the rest of the HUD are drawn in. Handed down rather than
## looked up, because a `RefCounted` reaching into its host's scene tree is the coupling the
## controller pattern exists to avoid.
var _room_bounds: Control = null

var _panel: CraftingPanel = null
## The band entity the panel is open on; `NO_BAND_ENTITY` when it is closed.
var _open_entity: int = NO_BAND_ENTITY

## Sentinel for "no band" on the entity handle. `entity` is client-local identity only — every
## command below names the band by its DURABLE `band_id` instead.
const NO_BAND_ENTITY := -1

# --- The per-world crafting catalogues (`SubsistenceSection`, decoded beside the kit roster) ---
var _materials: Array = []
var _band_legend: Array = []
var _recipes: Array = []
var _craft_knowledge: Array = []

func setup(host: Node, band_labor: HudBandLaborState, room_bounds: Control = null) -> void:
	_host = host
	_band_labor = band_labor
	_room_bounds = room_bounds

## Ingest the four catalogues as ONE call, because they are one fact: a recipe book without its
## materials would render a rail with no craft tracks and a ledger with costs in materials the panel
## cannot name. A non-Array is ignored (the last value stands), matching `set_kit_roster` — a delta
## carries a section only when it changed, so absence means unchanged and never "the world has none".
func set_catalogues(materials: Variant, band_legend: Variant, recipes: Variant,
		craft_knowledge: Variant) -> void:
	if materials is Array:
		_materials = materials
	if band_legend is Array:
		_band_legend = band_legend
	if recipes is Array:
		_recipes = recipes
	if craft_knowledge is Array:
		_craft_knowledge = craft_knowledge
	if is_open():
		render()

## Open the panel on `band`, or close it if it is already open on that band — the launch button is a
## toggle, like every other panel this HUD hangs off a header glyph.
func toggle_for(band: Dictionary) -> void:
	if band.is_empty():
		return
	var entity := int(band.get("entity", NO_BAND_ENTITY))
	if is_open() and entity == _open_entity:
		close()
		return
	_open_entity = entity
	render()

func open_for(band: Dictionary) -> void:
	if band.is_empty():
		return
	_open_entity = int(band.get("entity", NO_BAND_ENTITY))
	render()

func close() -> void:
	_open_entity = NO_BAND_ENTITY
	if _panel != null and is_instance_valid(_panel):
		_panel.dismiss()

func is_open() -> bool:
	return _open_entity != NO_BAND_ENTITY and _panel != null and is_instance_valid(_panel) \
		and _panel.is_open()

## Keep the panel live as the band's stock, bench and equipment move turn to turn. Called from the
## same per-snapshot seam that refreshes the Band/City dock, so the two surfaces are never a turn
## apart.
func refresh_snapshot() -> void:
	if _open_entity == NO_BAND_ENTITY:
		return
	render()

## Rebuild the panel against the live roster. A subject that has left the roster closes the panel
## rather than leaving it showing a band that no longer exists.
func render() -> void:
	if _open_entity == NO_BAND_ENTITY or _band_labor == null:
		return
	var bands := _band_labor.player_bands()
	var index := _index_of(bands, _open_entity)
	if index < 0:
		close()
		return
	var band: Dictionary = bands[index]
	_ensure_panel()
	_panel.render({
		CraftingPanel.PAYLOAD_BAND: band,
		CraftingPanel.PAYLOAD_BAND_LABEL: HudFormat.band_name(band),
		CraftingPanel.PAYLOAD_BAND_INDEX: index + 1,
		CraftingPanel.PAYLOAD_BAND_COUNT: bands.size(),
		CraftingPanel.PAYLOAD_BAND_OPTIONS: _band_options(bands),
		CraftingPanel.PAYLOAD_MATERIALS: _materials,
		CraftingPanel.PAYLOAD_BAND_LEGEND: _band_legend,
		CraftingPanel.PAYLOAD_RECIPES: _recipes,
		# The player's own tracks, and this is the ONE place they are filtered — so the rail cannot end
		# up quoting another people's Tanning. **The wire now carries the VIEWER's alone**
		# (`.claude/rules/core_sim/factions.md` → "Which frame sections are viewer-scoped"), so the
		# filter is defence in depth rather than the boundary; it stays for the reason above.
		CraftingPanel.PAYLOAD_CRAFT_KNOWLEDGE: _player_craft_knowledge(),
		# **THE CREW STEPPER'S CEILING, NOT THE BAND'S IDLE COUNT.** `effective_idle` nets the bench
		# out (a worker at the bench is assigned labor); the stepper asks how many COULD stand at the
		# bench, which keeps the crew already on it — the sim's `benchable()` against its `idle()`.
		CraftingPanel.PAYLOAD_IDLE_WORKERS: _band_labor.benchable_workers(band),
	})

## The room the card is bounded by changed shape — a panel docked or released an edge, or the event
## bar appeared, flipped edge, grew a row or was hidden. **Re-fit, do not re-render**: the payload is
## unchanged, so rebuilding the ledger would throw away the player's scroll position to answer a
## question about geometry. A closed panel needs nothing — it takes the room as it finds it when it
## next opens.
func refit_room() -> void:
	if not is_open():
		return
	_panel.refit()

## The panel node, for the harnesses. `null` until the panel has been opened once.
func panel() -> CraftingPanel:
	return _panel

# ---- wiring -----------------------------------------------------------------

func _ensure_panel() -> void:
	if _panel != null and is_instance_valid(_panel):
		return
	_panel = CraftingPanel.new()
	_panel.room_bounds = _room_bounds
	_host.add_child(_panel)
	_panel.closed.connect(close)
	_panel.band_selected.connect(_on_band_selected)
	_panel.cycle_requested.connect(_on_cycle_requested)
	_panel.enqueue_requested.connect(_on_enqueue_requested)
	_panel.crew_changed.connect(_on_crew_changed)
	_panel.order_count_changed.connect(_on_order_count_changed)
	_panel.order_remove_requested.connect(_on_order_remove_requested)
	_panel.order_raise_requested.connect(_on_order_raise_requested)
	_panel.bench_priority_requested.connect(_on_bench_priority_requested)
	_panel.bench_auto_requested.connect(_on_bench_auto_requested)
	_panel.bench_auto_skip_requested.connect(_on_bench_auto_skip_requested)

func _on_band_selected(entity: int) -> void:
	_open_entity = entity
	render()

## The arrows WALK the roster; the dropdown JUMPS. Both are dead today — there is exactly one player
## band — and that is the shipped convention (the actor is always explicit), not a bug.
func _on_cycle_requested(delta: int) -> void:
	var bands := _band_labor.player_bands()
	if bands.is_empty():
		return
	var index := _index_of(bands, _open_entity)
	if index < 0:
		return
	var next: int = posmod(index + delta, bands.size())
	_open_entity = int((bands[next] as Dictionary).get("entity", NO_BAND_ENTITY))
	render()

## **EVERY ORDER HAS A COUNT**, so a count below one is not an order and nothing goes out.
func _on_enqueue_requested(recipe_id: String, count: int) -> void:
	var band := _open_band()
	if band.is_empty() or recipe_id == "" or count < 1:
		return
	bench_enqueue_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"recipe_id": recipe_id,
		"count": count,
	})

func _on_crew_changed(workers: int) -> void:
	var band := _open_band()
	if band.is_empty():
		return
	bench_crew_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"workers": maxi(workers, 0),
	})

## **THE QUEUE EDITS NAME AN ORDER BY ITS PLACE** in the published `bench.orders` (0 = the head of the queue), the
## index the server's queue verbs address. All three go out through the same seam the crew does.
func _on_order_count_changed(order: int, count: int) -> void:
	var band := _open_band()
	if band.is_empty() or order < 0 or count < 1:
		return
	bench_order_count_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"order": order,
		"count": count,
	})

func _on_order_remove_requested(order: int) -> void:
	var band := _open_band()
	if band.is_empty() or order < 0:
		return
	bench_remove_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"order": order,
	})

## Index 0 cannot be raised — the server refuses it — so the panel draws no ↑ there and this drops it.
func _on_order_raise_requested(order: int) -> void:
	var band := _open_band()
	if band.is_empty() or order < 1:
		return
	bench_raise_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"order": order,
	})

## **THE RANK NAMES THE BAND AND THE LEVEL, and nothing else** — one bench per band, so the verb has
## no order argument. The level arrives already normalized through `HudWorkVocab.work_priority_of`, so
## this seam re-spells nothing; it goes out through the same relay the other three verbs do.
func _on_bench_priority_requested(level: String) -> void:
	var band := _open_band()
	if band.is_empty():
		return
	bench_priority_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"level": level,
	})

## Auto names the band and the state it asks for, and nothing else (one bench per band).
func _on_bench_auto_requested(on: bool) -> void:
	var band := _open_band()
	if band.is_empty():
		return
	bench_auto_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"on": on,
	})

func _on_bench_auto_skip_requested() -> void:
	var band := _open_band()
	if band.is_empty():
		return
	bench_auto_skip_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
	})

# ---- lookups ----------------------------------------------------------------

func _open_band() -> Dictionary:
	var bands := _band_labor.player_bands()
	var index := _index_of(bands, _open_entity)
	return bands[index] if index >= 0 else {}

func _index_of(bands: Array, entity: int) -> int:
	for i in range(bands.size()):
		if bands[i] is Dictionary and int((bands[i] as Dictionary).get("entity", NO_BAND_ENTITY)) == entity:
			return i
	return -1

func _band_options(bands: Array) -> Array:
	var options: Array = []
	for i in range(bands.size()):
		if not (bands[i] is Dictionary):
			continue
		var band: Dictionary = bands[i]
		options.append({
			"label": HudFormat.band_name(band),
			"entity": int(band.get("entity", NO_BAND_ENTITY)),
		})
	return options

func _player_craft_knowledge() -> Array:
	var tracks: Array = []
	for track_variant in _craft_knowledge:
		if not (track_variant is Dictionary):
			continue
		var track: Dictionary = track_variant
		if int(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_FACTION_KEY, -1)) == HudConst.PLAYER_FACTION_ID:
			tracks.append(track)
	return tracks
