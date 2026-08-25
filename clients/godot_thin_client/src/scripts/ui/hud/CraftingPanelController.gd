class_name CraftingPanelController
extends RefCounted

## The MATERIALS & CRAFTING cluster (`docs/plan_crafting_and_materials.md` §7) — the controller half
## of `CraftingPanel`: it owns the panel node, holds the per-world crafting catalogues, resolves which
## band the panel is showing, and turns the panel's six signals into the three commands the bench
## takes.
##
## Built on the `TurnOrbController` / `DisclosureController` idiom: `HudLayer` holds one as
## `_crafting`, hands it the shared `HudBandLaborState` BY REFERENCE and a HOST `Node` (a `RefCounted`
## cannot `add_child`), keeps thin delegators for the entry points reached BY NAME, and RELAYS this
## controller's three signals onto its own so `Main` can format the commands.
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

## Stage a recipe on the band's bench — `set_bench <faction> <band> recipe <id>`. **The player staffs
## the bench**: no crew rides here, so an idle bench stages at zero and waits for the `− n +` stepper
## (a swap keeps the crew already standing there). The bench is staffed like a worked source rather
## than through a standing role, which is why there is no Crafter role card anywhere.
signal set_bench_requested(payload: Dictionary)
## Re-crew the running bench, leaving the job and its progress alone — `bench_crew <faction> <band>
## workers <n>`.
signal bench_crew_requested(payload: Dictionary)
## Take the job off the bench — `clear_bench <faction> <band>`. The crew returns to the idle pool and
## the pile already drawn is spent, which is what the button's tooltip names before it is pressed.
signal clear_bench_requested(payload: Dictionary)
## Rank the bench against the band's other work — `bench_priority <faction> <band> high|normal|low`
## (`docs/plan_standing_upkeep.md` §4.9 item 9b). **A SIBLING VERB, not a `work_priority` token**:
## that grammar reads a lone trailing token as a herd id, so `work_priority … bench low` would be
## ambiguous with a herd named `bench`. It names the band and nothing else, one bench at a time meaning
## there is no job argument to disambiguate — and it is legal on an IDLE bench, a rank being a standing
## statement about the bench rather than about the job on it.
signal bench_priority_requested(payload: Dictionary)

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
		CraftingPanel.PAYLOAD_BAND_LABEL: HudFormat.band_display_name(band, index + 1),
		CraftingPanel.PAYLOAD_BAND_INDEX: index + 1,
		CraftingPanel.PAYLOAD_BAND_COUNT: bands.size(),
		CraftingPanel.PAYLOAD_BAND_OPTIONS: _band_options(bands),
		CraftingPanel.PAYLOAD_MATERIALS: _materials,
		CraftingPanel.PAYLOAD_BAND_LEGEND: _band_legend,
		CraftingPanel.PAYLOAD_RECIPES: _recipes,
		# The player's own tracks. The wire carries every faction's, exactly as the sedentarization and
		# knowledge vectors do, and this is the ONE place they are filtered — so the rail cannot end up
		# quoting another people's Tanning.
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
	_panel.make_requested.connect(_on_make_requested)
	_panel.crew_changed.connect(_on_crew_changed)
	_panel.clear_bench_requested.connect(_on_clear_bench_requested)
	_panel.bench_priority_requested.connect(_on_bench_priority_requested)

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

func _on_make_requested(recipe_id: String) -> void:
	var band := _open_band()
	if band.is_empty() or recipe_id == "":
		return
	set_bench_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"recipe_id": recipe_id,
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

## **THE VERB TAKES NO SUBJECT BEYOND THE BAND** — one job at a time, so `clear_bench` names the band
## and nothing else. It goes out through the same seam the other two do; this launcher gets no branch
## of its own.
func _on_clear_bench_requested() -> void:
	var band := _open_band()
	if band.is_empty():
		return
	clear_bench_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
	})

## **THE RANK NAMES THE BAND AND THE LEVEL, and nothing else** — one bench at a time, so the verb has
## no job argument. The level arrives already normalized through `HudWorkVocab.work_priority_of`, so
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
			"label": HudFormat.band_display_name(band, i + 1),
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
