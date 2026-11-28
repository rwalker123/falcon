class_name CraftingPanelController
extends RefCounted

## The MATERIALS & CRAFTING cluster (`docs/plan_crafting_and_materials.md` §7) — the controller half
## of `CraftingPanel`: it owns the panel node, holds the per-world crafting catalogues, resolves which
## band the panel is showing, and turns the panel's signals into the six commands the bench takes —
## `bench_enqueue`, `bench_order_count`, `bench_remove`, `bench_raise`, `bench_priority`.
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
## count <n>`: Make sends a count of one, a suggestion's Queue its whole count. **The sim staffs
## the bench**: no crew rides here; every order is staffed each turn from the band's idle hands
## and the panel never proposes a crew. There is no Crafter role card anywhere.
signal bench_enqueue_requested(payload: Dictionary)
## Change one order's count — `bench_order_count <faction> <band> order <i> count <n>`.
signal bench_order_count_requested(payload: Dictionary)
## Take one order off the queue — `bench_remove <faction> <band> order <i>`. A drawn pile is lost,
## which is what the control's tooltip names before it is pressed. The well's ✕ is `order <worked>`.
signal bench_remove_requested(payload: Dictionary)
## Move one order up a place — `bench_raise <faction> <band> order <i>`, never index 0.
signal bench_raise_requested(payload: Dictionary)
## Rank ONE order against the bench's others — `bench_priority <faction> <band> order <i>
## high|normal|low` (`<i>` the order's index in `bench.orders`, the same index the other queue verbs
## address). **A SIBLING VERB, not a `work_priority` token**: that grammar reads a lone trailing token as
## a herd id. Payload: { faction, band_id, order, level }.
signal bench_priority_requested(payload: Dictionary)
## Switch auto-craft on or off for the band's bench - `bench_auto <faction> <band> on|off`. Payload: { faction, band_id, on }.
signal bench_auto_requested(payload: Dictionary)
## Pass over the waiting auto order's item - `bench_auto_skip <faction> <band>`. Payload: { faction, band_id }.
signal bench_auto_skip_requested(payload: Dictionary)
## Order a whole kit as ONE bench order (#780) - `bench_enqueue_kit <faction> <band> kit <id> kits <n>
## recipe <id> ...`. Payload: { faction, band_id, kit_id, kits, recipe_ids }.
signal bench_enqueue_kit_requested(payload: Dictionary)
## Take a whole kit order off the bench - `bench_remove_kit <faction> <band> kit_order <id>`. Payload:
## { faction, band_id, kit_order }. Items already made stay in the store.
signal bench_remove_kit_requested(payload: Dictionary)
## Move a whole kit order up past the unit ahead of it - `bench_raise_kit <faction> <band> kit_order <id>`.
## Payload: { faction, band_id, kit_order }.
signal bench_raise_kit_requested(payload: Dictionary)

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
## **THE TRIBE'S ENTRY IS OPEN** (`.claude/rules/client/tribe-panel.md`): `HudTribeVocab.NO_TRIBE` while a band's
## own bench shows. The tribe stands in the picker for its members and has no bench of its own, so while it is
## open `_open_entity` holds its first member (the panel's address) and the body is `CraftingTribeView`'s.
var _open_tribe_id: int = HudTribeVocab.NO_TRIBE
## The picker's fold: closed each time the panel opens, opened to reach a member's bench.
var _tribe_fold_open: bool = false

## Sentinel for "no band" on the entity handle. `entity` is client-local identity only — every
## command below names the band by its DURABLE `band_id` instead.
const NO_BAND_ENTITY := -1

# --- The per-world crafting catalogues (`SubsistenceSection`, decoded beside the kit roster) ---
var _materials: Array = []
var _band_legend: Array = []
var _recipes: Array = []
var _craft_knowledge: Array = []
## The parsed `equipmentConfigJson`, kept for the bench tools' craft stats (`CraftingPanel._tool_benefit`).
var _equipment_config: Dictionary = {}

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

## The whole effective equipment config as JSON. Only a bench tool's craft stats are read from it here;
## an empty blob leaves the last value standing (a delta carries it only when it changed).
func set_equipment_config(json: Variant) -> void:
	if not (json is String) or String(json).is_empty():
		return
	var parsed: Variant = JSON.parse_string(String(json))
	_equipment_config = parsed if parsed is Dictionary else {}
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
	open_for(band)

## `HudTribeVocab.CRAFTING_OPEN_TRIBE_KEY` on the band says the `\u2692` was pressed from the tribe's own page:
## the panel then opens on the tribe's entry rather than on the member the page's `panel_band()` happens to be.
func open_for(band: Dictionary) -> void:
	if band.is_empty():
		return
	_open_entity = int(band.get("entity", NO_BAND_ENTITY))
	_open_tribe_id = TribeRoster.tribe_id_of(band) \
		if bool(band.get(HudTribeVocab.CRAFTING_OPEN_TRIBE_KEY, false)) else HudTribeVocab.NO_TRIBE
	_tribe_fold_open = false
	render()

func close() -> void:
	_open_entity = NO_BAND_ENTITY
	_open_tribe_id = HudTribeVocab.NO_TRIBE
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
	var position := _picker_position(bands, band)
	var tribe_members: Array = TribeRoster.members(bands, _open_tribe_id) \
		if _open_tribe_id != HudTribeVocab.NO_TRIBE else []
	if _open_tribe_id != HudTribeVocab.NO_TRIBE and tribe_members.is_empty():
		_open_tribe_id = HudTribeVocab.NO_TRIBE
	var label := HudFormat.band_name(band)
	if not tribe_members.is_empty():
		band = TribeRoster.address_band(tribe_members)
		label = String(band.get(HudTribeVocab.KEY_TRIBE_NAME, ""))
		position = _picker_position(bands, band)
	_panel.render({
		CraftingPanel.PAYLOAD_BAND: band,
		CraftingPanel.PAYLOAD_BAND_LABEL: label,
		CraftingPanel.PAYLOAD_BAND_INDEX: position["index"] + 1,
		CraftingPanel.PAYLOAD_BAND_COUNT: position["count"],
		CraftingPanel.PAYLOAD_BAND_OPTIONS: _band_options(bands),
		CraftingPanel.PAYLOAD_TRIBE_FOLD: TribePicker.has_tribe(bands),
		CraftingPanel.PAYLOAD_TRIBE_FOLD_OPEN: _tribe_fold_open,
		CraftingPanel.PAYLOAD_TRIBE_MEMBERS: tribe_members,
		CraftingPanel.PAYLOAD_MATERIALS: _materials,
		CraftingPanel.PAYLOAD_BAND_LEGEND: _band_legend,
		CraftingPanel.PAYLOAD_RECIPES: _recipes,
		CraftingPanel.PAYLOAD_EQUIPMENT_CONFIG: _equipment_config,
		# The player's own tracks, and this is the ONE place they are filtered — so the rail cannot end
		# up quoting another people's Tanning. **The wire now carries the VIEWER's alone**
		# (`.claude/rules/core_sim/factions.md` → "Which frame sections are viewer-scoped"), so the
		# filter is defence in depth rather than the boundary; it stays for the reason above.
		CraftingPanel.PAYLOAD_CRAFT_KNOWLEDGE: _player_craft_knowledge(),
		# **THE HEADER'S SPARE COUNT** — the idle figure the labor UI already tells the player
		# (`people_idle`), never a figure of the panel's own.
		CraftingPanel.PAYLOAD_IDLE_WORKERS: _band_labor.people_idle(band),
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
	_panel.tribe_selected.connect(_on_tribe_selected)
	_panel.tribe_fold_toggled.connect(_on_tribe_fold_toggled)
	_panel.cycle_requested.connect(_on_cycle_requested)
	_panel.enqueue_requested.connect(_on_enqueue_requested)
	_panel.order_count_changed.connect(_on_order_count_changed)
	_panel.order_remove_requested.connect(_on_order_remove_requested)
	_panel.order_raise_requested.connect(_on_order_raise_requested)
	_panel.bench_priority_requested.connect(_on_bench_priority_requested)
	_panel.bench_auto_requested.connect(_on_bench_auto_requested)
	_panel.bench_auto_skip_requested.connect(_on_bench_auto_skip_requested)
	_panel.kit_enqueue_requested.connect(_on_kit_enqueue_requested)
	_panel.kit_remove_requested.connect(_on_kit_remove_requested)
	_panel.kit_raise_requested.connect(_on_kit_raise_requested)

func _on_band_selected(entity: int) -> void:
	_open_entity = entity
	_open_tribe_id = HudTribeVocab.NO_TRIBE
	render()

## The tribe's entry was picked: the panel shows what the tribe's bands make for it (`CraftingTribeView`).
func _on_tribe_selected(tribe_id: int) -> void:
	var members := TribeRoster.members(_band_labor.player_bands(), tribe_id)
	if members.is_empty():
		return
	_open_tribe_id = tribe_id
	_open_entity = int(TribeRoster.address_band(members).get("entity", NO_BAND_ENTITY))
	render()

func _on_tribe_fold_toggled() -> void:
	_tribe_fold_open = not _tribe_fold_open
	render()

## The arrows WALK the top-level entries (a tribe is one of them); once a MEMBER's bench is up they walk that
## tribe's members instead. The dropdown JUMPS.
func _on_cycle_requested(delta: int) -> void:
	var bands := _band_labor.player_bands()
	if bands.is_empty():
		return
	var index := _index_of(bands, _open_entity)
	if index < 0:
		return
	var band: Dictionary = bands[index]
	if _open_tribe_id == HudTribeVocab.NO_TRIBE and TribeRoster.is_member(band):
		var members := TribeRoster.members(bands, TribeRoster.tribe_id_of(band))
		var at := 0
		for i in range(members.size()):
			if int((members[i] as Dictionary).get("entity", NO_BAND_ENTITY)) == _open_entity:
				at = i
		_open_entity = int((members[posmod(at + delta, members.size())] as Dictionary).get("entity", NO_BAND_ENTITY))
		render()
		return
	var subjects := TribeRoster.subjects(bands)
	var here := TribeRoster.subject_index_of_tribe(subjects, _open_tribe_id) \
		if _open_tribe_id != HudTribeVocab.NO_TRIBE \
		else TribeRoster.subject_index_of_band(subjects, _open_entity)
	var next: Dictionary = subjects[posmod(here + delta, subjects.size())]
	if String(next[TribeRoster.SUBJECT_KIND]) == TribeRoster.SUBJECT_TRIBE:
		_on_tribe_selected(int(next[TribeRoster.SUBJECT_TRIBE_ID]))
		return
	_on_band_selected(int((next[TribeRoster.SUBJECT_BAND_DICT] as Dictionary).get("entity", NO_BAND_ENTITY)))

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

## **THE QUEUE EDITS NAME AN ORDER BY ITS PLACE** in the published `bench.orders` (0 = the head of the queue), the
## index the server's queue verbs address. All of them go out through the same relay.
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

## **THE RANK NAMES AN ORDER BY ITS INDEX** in `bench.orders`, and the level. The level arrives already
## normalized through `HudWorkVocab.work_priority_of`, so this seam re-spells nothing.
func _on_bench_priority_requested(order: int, level: String) -> void:
	var band := _open_band()
	if band.is_empty() or order < 0:
		return
	bench_priority_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"order": order,
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

## **A KIT ORDER NAMES THE KIT, HOW MANY KITS AND ONE RECIPE PER ITEM.** Nothing goes out for no kit, a count
## below one, or a missing recipe.
func _on_kit_enqueue_requested(kit_id: String, kits: int, recipe_ids: Array) -> void:
	var band := _open_band()
	if band.is_empty() or kit_id == "" or kits < 1 or recipe_ids.is_empty():
		return
	var kept: Array = []
	for recipe_id in recipe_ids:
		if String(recipe_id) != "":
			kept.append(String(recipe_id))
	if kept.size() != recipe_ids.size():
		return
	bench_enqueue_kit_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"kit_id": kit_id,
		"kits": kits,
		"recipe_ids": kept,
	})

func _on_kit_remove_requested(kit_order: int) -> void:
	var band := _open_band()
	if band.is_empty() or kit_order < 1:
		return
	bench_remove_kit_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"kit_order": kit_order,
	})

func _on_kit_raise_requested(kit_order: int) -> void:
	var band := _open_band()
	if band.is_empty() or kit_order < 1:
		return
	bench_raise_kit_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"kit_order": kit_order,
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

## The picker's entries: **a tribe stands in for its members** with a fold beside it (`TribePicker`), and a lone
## band is listed on its own. Each is `{label, entity | tribe_id, indent}`.
func _band_options(bands: Array) -> Array:
	var options: Array = []
	for entry in TribePicker.entries(bands, _tribe_fold_open):
		options.append({
			"label": String(entry[TribePicker.ENTRY_LABEL]),
			"entity": int(entry.get(TribePicker.ENTRY_ENTITY, NO_BAND_ENTITY)),
			"tribe_id": int(entry.get(TribePicker.ENTRY_TRIBE_ID, HudTribeVocab.NO_TRIBE)),
			"indent": int(entry[TribePicker.ENTRY_INDENT]),
		})
	return options

## `{index, count}` for the `n / N` beside the arrows: of the tribe's members while one member's bench is up,
## else of the top-level entries.
func _picker_position(bands: Array, band: Dictionary) -> Dictionary:
	if _open_tribe_id == HudTribeVocab.NO_TRIBE and TribeRoster.is_member(band):
		var members := TribeRoster.members(bands, TribeRoster.tribe_id_of(band))
		for i in range(members.size()):
			if int((members[i] as Dictionary).get("entity", NO_BAND_ENTITY)) == int(band.get("entity", -2)):
				return {"index": i, "count": members.size()}
	var subjects := TribeRoster.subjects(bands)
	var at := TribeRoster.subject_index_of_tribe(subjects, _open_tribe_id) \
		if _open_tribe_id != HudTribeVocab.NO_TRIBE \
		else TribeRoster.subject_index_of_band(subjects, int(band.get("entity", -2)))
	return {"index": maxi(at, 0), "count": subjects.size()}

func _player_craft_knowledge() -> Array:
	var tracks: Array = []
	for track_variant in _craft_knowledge:
		if not (track_variant is Dictionary):
			continue
		var track: Dictionary = track_variant
		if int(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_FACTION_KEY, -1)) == HudConst.PLAYER_FACTION_ID:
			tracks.append(track)
	return tracks
