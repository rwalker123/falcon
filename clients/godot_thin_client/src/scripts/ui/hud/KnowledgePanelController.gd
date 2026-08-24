class_name KnowledgePanelController
extends RefCounted

## The KNOWLEDGE SCREEN's controller (`docs/plan_knowledge_screen.md` §3, §4) — it owns the panel
## node, resolves the sources the *"nothing is using it"* verdict is asked of, holds the view state
## (which node is selected, which filter is live) and answers the launcher's PIP count.
##
## Built on the `CraftingPanelController` idiom: `HudLayer` holds one as `_knowledge`, hands it the
## shared `HudBandLaborState` and `FactionReadouts` BY REFERENCE plus a HOST `Node` (a `RefCounted`
## cannot `add_child`), and connects `BandCityPanel.knowledge_requested` to `toggle()`.
##
## **IT EMITS NO COMMANDS, AND THAT IS THE ARC'S WHOLE POINT.** Every other panel controller in this
## HUD turns presses into orders; this one turns them into a reading. Knowledge is earned by practice,
## so there is nothing here to spend and nothing to queue — see `KnowledgePanel`'s docstring.
##
## ---
##
## ## Which sources the verdict is asked of, and why the two webs differ
##
## **This is forced by the wire.** A forage patch carries `owner` / `has_owner`, so the plant half
## scans every patch and filters on ownership — the same test `AttentionController._under_kept_rung_attention`
## makes, and the same reason it can run outside the band loop. **A herd carries no owner field
## client-side at all**, so the animal half walks the player bands' own HUNT ASSIGNMENTS and resolves
## each to the live herd, exactly as `_starving_pen_attention` and `_under_kept_herd_attention` do —
## a scan of `world_herds()` would count a rival's pen as the player's.
##
## The live dict is the authority on a herd, never the assignment's launch-time target: herds MIGRATE.
##
## ## "New this turn" is ONE diff over BOTH webs, and it is not `_announce_knowledge_unlock`'s
##
## The ladder tracks and the craft tracks arrive through different ingests, so a diff per ingest would
## make the LAND column's "new" and the CRAFT column's "new" two different rules — and the one that
## drifted would be invisible, since both render as a plausible pill count. One diff over the SAME
## roster the panel draws cannot disagree with what is on screen.
##
## **It deliberately does not reuse `FactionReadouts._announce_knowledge_unlock`'s diff**, which
## answers a different question: that one is fire-once-EVER per faction+track and survives across
## turns, because a nudge repeated is noise. This one is "since the turn ticked", and has to go quiet
## again next turn.
##
## **THE FIRST OBSERVATION LEARNS NOTHING.** A fresh connect or a rehydrated save arrives with tracks
## already complete and no prior value to compare them against, so seeding the baseline is all the
## first pass does — otherwise every discovery a returning player ever made would light up as "new
## this turn". That is the same trap `_announce_knowledge_unlock` guards with its "no prior value ⇒
## not announced" rule, met here with an explicit `UNSEEN_TURN` sentinel rather than an empty
## dictionary, because an empty baseline is indistinguishable from a faction that knows nothing.

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
var _topbar: FactionReadouts = null
## The HUD CanvasLayer, so this `RefCounted` has a node to parent the panel into.
var _host: Node = null
## The room the card is bounded by — the HUD's `FloatingRoom`. See `CraftingPanelController`.
var _room_bounds: Control = null

var _panel: KnowledgePanel = null
var _open: bool = false

## VIEW state, and it is deliberately not snapshot state: which node the player is reading and which
## filter they set survive a turn tick, exactly as the crafting ledger's fold state does.
var _selected: String = ""
var _filter: StringName = HudKnowledgeVocab.FILTER_ALL

# --- The craft catalogues, forwarded from the same `Main` call the crafting panel is fed by ---
var _recipes: Array = []
var _craft_knowledge: Array = []

## "No snapshot has been observed yet." A real turn is `>= 0` — `HudBandLaborState._current_turn`
## starts at `-1` and the sim's first turn is 0 — so the sentinel has to sit outside that range.
const UNSEEN_TURN := -2
var _diff_turn: int = UNSEEN_TURN
## The knowledge keys that were already known when the CURRENT turn began, and the ones that finished
## during it. Both are `{key: true}` sets.
var _known_at_turn_start: Dictionary = {}
var _learned_this_turn: Dictionary = {}

func setup(host: Node, band_labor: HudBandLaborState, topbar: FactionReadouts,
		room_bounds: Control = null) -> void:
	_host = host
	_band_labor = band_labor
	_topbar = topbar
	_room_bounds = room_bounds

## The world's recipe book and each faction's craft knowledge, forwarded from
## `HudLayer.update_crafting_catalogues` — the SAME call `CraftingPanelController.set_catalogues`
## takes. Two readers of one wire field rather than a copy: this panel needs `recipes` for the
## "made of it" join and `craft_knowledge` for the CRAFT column, and re-deriving either would be a
## second answer to a question the crafting panel already asks.
##
## A non-Array is ignored (the last value stands), matching every other catalogue setter — a delta
## carries a section only when it changed, so absence means unchanged and never "the world has none".
func set_catalogues(recipes: Variant, craft_knowledge: Variant) -> void:
	if recipes is Array:
		_recipes = recipes
	if craft_knowledge is Array:
		_craft_knowledge = craft_knowledge
	if is_open():
		render()

## Open the screen, or close it if it is already open — the launch button is a toggle, like every
## other panel this HUD hangs off a header glyph.
func toggle() -> void:
	if is_open():
		close()
		return
	open()

## **OPENING CLEARS THE PIP by clearing what the pip counts? NO — it does not, and that is
## deliberate.** §4 says the pip "clears when the screen is opened"; what actually clears an unspent
## count is USING the knowledge, and a pip that went quiet on a look would tell the player they had
## dealt with something they had not. The count is derived fresh every render (`unspent_count`), so it
## goes away exactly when a source starts standing on the discovery — which is the honest trigger and
## the one the state's own definition already gives.
func open() -> void:
	_open = true
	render()

func close() -> void:
	_open = false
	if _panel != null and is_instance_valid(_panel):
		_panel.dismiss()

func is_open() -> bool:
	return _open and _panel != null and is_instance_valid(_panel) and _panel.is_open()

## Keep the screen live as tracks advance and sources are improved turn to turn. Called from the same
## per-snapshot seam that refreshes the Band/City dock, so the two surfaces are never a turn apart.
##
## **THE DIFF RUNS WHETHER OR NOT THE PANEL IS OPEN**, because the pip is on the Band/City header and
## has to be right on a turn the player never opened the screen — and because a diff that only ran
## while open would compare against a baseline from whenever it was last looked at.
func refresh_snapshot() -> void:
	_update_learned_this_turn()
	if _open:
		render()

## Rebuild the panel against the live model.
func render() -> void:
	if not _open or _band_labor == null:
		return
	_ensure_panel()
	_panel.render({
		KnowledgePanel.PAYLOAD_DOMAINS: domains(),
		KnowledgePanel.PAYLOAD_SELECTED: _selected,
		KnowledgePanel.PAYLOAD_FILTER: _filter,
	})

## The room the card is bounded by changed shape. **Re-fit, do not re-render** — the payload is
## unchanged, and rebuilding to answer a question about geometry would throw the reading away.
func refit_room() -> void:
	if not is_open():
		return
	_panel.refit()

## WORLD BOUNDARY (`HudLayer.reset_world_state`). The screen closes and the turn diff is dropped: a
## new world restarts the turn counter and re-teaches every track, so a baseline from the previous
## world would report the new one's first discoveries as already-known — or, worse, the old world's
## as new.
func reset_world_state() -> void:
	close()
	_selected = ""
	_filter = HudKnowledgeVocab.FILTER_ALL
	_diff_turn = UNSEEN_TURN
	_known_at_turn_start.clear()
	_learned_this_turn.clear()

## **THE LAUNCHER'S PIP.** How many discoveries the faction has earned and nothing is using. Derived
## fresh, never latched — see `open()`.
##
## Answerable with the panel CLOSED and never built, which is the point: the pip is what tells a
## player there is something on a screen they have not opened.
func unspent_count() -> int:
	return KnowledgeRoster.count_matching(KnowledgeRoster.flatten(domains()),
		HudKnowledgeVocab.FILTER_UNUSED)

## The roster the panel draws and the pip counts, built off one model so the two cannot disagree.
## Public because every assertion in the harness asks it rather than reading a Label back.
func domains() -> Array[Dictionary]:
	return KnowledgeRoster.build_domains(model())

## The inputs `KnowledgeRoster` derives from. Public for the harness, which needs to stage a model and
## check the verdict without a HUD.
func model() -> Dictionary:
	var bands := _band_labor.player_bands() if _band_labor != null else []
	return {
		KnowledgeRoster.MODEL_TRACKS: _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) \
			if _topbar != null else {},
		KnowledgeRoster.MODEL_CRAFT_KNOWLEDGE: _player_craft_knowledge(),
		KnowledgeRoster.MODEL_PATCHES: _owned_patches(),
		KnowledgeRoster.MODEL_HERDS: _worked_herds(bands),
		KnowledgeRoster.MODEL_RECIPES: _recipes,
		KnowledgeRoster.MODEL_OWNED_ITEMS: _owned_items(bands),
		KnowledgeRoster.MODEL_OWNED_MATERIALS: _owned_materials(bands),
		KnowledgeRoster.MODEL_BENCH_RECIPES: _bench_recipes(bands),
		KnowledgeRoster.MODEL_LEARNED_THIS_TURN: _learned_this_turn,
	}

## The panel node, for the harnesses. `null` until the screen has been opened once.
func panel() -> KnowledgePanel:
	return _panel

## What finished during the current turn — for the harness, and for Slice C's attention producer.
func learned_this_turn() -> Dictionary:
	return _learned_this_turn

# ---- wiring -----------------------------------------------------------------

func _ensure_panel() -> void:
	if _panel != null and is_instance_valid(_panel):
		return
	_panel = KnowledgePanel.new()
	_panel.room_bounds = _room_bounds
	_host.add_child(_panel)
	_panel.closed.connect(close)
	_panel.node_selected.connect(_on_node_selected)
	_panel.filter_selected.connect(_on_filter_selected)

## Selecting the node already selected DESELECTS it, back to the placeholder. A reading has no
## "close" of its own, and a player who has finished with one should not have to find another node to
## get out of it.
func _on_node_selected(key: String) -> void:
	_selected = "" if key == _selected else key
	render()

func _on_filter_selected(key: StringName) -> void:
	_filter = key
	render()

# ---- the turn diff ----------------------------------------------------------

## Roll the "new this turn" set forward. See the class docstring for why the first observation seeds
## the baseline and reports nothing.
func _update_learned_this_turn() -> void:
	var turn := _band_labor.current_turn() if _band_labor != null else UNSEEN_TURN
	var known_now := _known_keys()
	if _diff_turn == UNSEEN_TURN:
		_known_at_turn_start = known_now
		_learned_this_turn = {}
		_diff_turn = turn
		return
	if turn == _diff_turn:
		# A second snapshot inside one turn (an optimistic reconcile, a re-render) must not wipe what
		# the turn has already taught — the baseline is the TURN's, not the frame's.
		return
	var fresh := {}
	for key in known_now:
		if not _known_at_turn_start.has(key):
			fresh[key] = true
	_learned_this_turn = fresh
	_known_at_turn_start = known_now
	_diff_turn = turn

## Every knowledge key the faction currently KNOWS, both webs, taken off the roster itself rather than
## off the two wire vectors — so the diff and the columns can never disagree about what "known" means
## (a craft's `known` flag is not the ladder's `>= KNOWLEDGE_COMPLETE`, and re-deriving either here
## would be a third reading of the same question).
func _known_keys() -> Dictionary:
	var known := {}
	for node in KnowledgeRoster.flatten(KnowledgeRoster.build_domains(_diff_model())):
		if String(node.get(HudKnowledgeVocab.NODE_STATE, "")) == HudKnowledgeVocab.NODE_STATE_KNOWN:
			known[String(node[HudKnowledgeVocab.NODE_KEY])] = true
	return known

## The model WITHOUT the turn diff — what `_known_keys` builds its roster from. It exists to break the
## cycle: `model()` carries `_learned_this_turn`, which is what this pass is computing.
##
## Only `NODE_STATE` is read off the result, and nothing in the source scans can change a state, so
## the empty pools are free rather than a shortcut: a track is known because its meter says so.
func _diff_model() -> Dictionary:
	return {
		KnowledgeRoster.MODEL_TRACKS: _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) \
			if _topbar != null else {},
		KnowledgeRoster.MODEL_CRAFT_KNOWLEDGE: _player_craft_knowledge(),
	}

# ---- resolving the faction's sources ----------------------------------------

## The faction's OWN forage patches. **Ownership is the patch's own `owner` field**, which is what
## makes an assignment-free scan attributable and what stops this counting a rival's tended ground.
func _owned_patches() -> Array:
	var owned: Array = []
	if _band_labor == null:
		return owned
	var lookup := _band_labor.forage_patch_lookup()
	for tile_key in lookup:
		var patch_variant: Variant = lookup[tile_key]
		if not (patch_variant is Dictionary):
			continue
		var patch: Dictionary = patch_variant
		if not bool(patch.get("has_owner", false)) \
				or int(patch.get("owner", -1)) != HudConst.PLAYER_FACTION_ID:
			continue
		owned.append(patch)
	return owned

## The herds the faction's bands WORK, resolved through their own hunt assignments — a herd carries no
## owner field client-side, so this walk IS the ownership test. De-duplicated by herd id: two bands
## may hunt one herd, and a herd counted twice would inflate the "stands on it" count on the detail
## pane without changing any verdict, which is exactly the kind of wrong number nobody notices.
func _worked_herds(bands: Array) -> Array:
	var herds: Array = []
	var seen := {}
	if _band_labor == null:
		return herds
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		for assignment_variant in HudBandLaborState.labor_assignments_of(band_variant as Dictionary):
			if not (assignment_variant is Dictionary):
				continue
			var assignment: Dictionary = assignment_variant
			if String(assignment.get("kind", "")).to_lower() != SourceForecast.LABOR_KIND_HUNT:
				continue
			var herd_id := String(assignment.get("fauna_id", ""))
			if herd_id == "" or seen.has(herd_id):
				continue
			# Herds MIGRATE, so the LIVE dict is the authority on this one — never the assignment's
			# launch-time copy.
			var herd := _band_labor.find_world_herd(herd_id)
			if herd.is_empty():
				continue
			seen[herd_id] = true
			herds.append(herd)
	return herds

## `{item_id: count}` over every player band. **`count`, never `remaining`** — the crafting panel's
## own ownership rule: a batch that runs out of units is REMOVED, so a worn-out item and one never
## made both read `remaining 0`.
func _owned_items(bands: Array) -> Dictionary:
	var owned := {}
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		for batch_variant in (band_variant as Dictionary).get(
				HudCraftingVocab.BAND_EQUIPMENT_BATCHES_KEY, []):
			if not (batch_variant is Dictionary):
				continue
			var batch: Dictionary = batch_variant
			var item_id := String(batch.get(HudCraftingVocab.EQUIPMENT_ITEM_ID_KEY, ""))
			if item_id == "":
				continue
			owned[item_id] = int(owned.get(item_id, 0)) \
				+ int(batch.get(HudCraftingVocab.EQUIPMENT_COUNT_KEY, 0))
	return owned

## `{material_id: amount}` over every player band — the pile a craft's STOCK recipes make, which is
## the other half of "your people are holding something made of it".
func _owned_materials(bands: Array) -> Dictionary:
	var owned := {}
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		for batch_variant in (band_variant as Dictionary).get(
				HudCraftingVocab.BAND_MATERIAL_BATCHES_KEY, []):
			if not (batch_variant is Dictionary):
				continue
			var batch: Dictionary = batch_variant
			var material_id := String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, "")).strip_edges()
			if material_id == "":
				continue
			owned[material_id] = float(owned.get(material_id, 0.0)) \
				+ float(batch.get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
	return owned

## The recipe ids on a player band's bench right now. An empty `recipe_id` is an IDLE bench, which is
## a different statement from a blocked one and contributes nothing here.
func _bench_recipes(bands: Array) -> Array:
	var running: Array = []
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		var bench_variant: Variant = (band_variant as Dictionary).get(
			HudCraftingVocab.BAND_BENCH_KEY, {})
		if not (bench_variant is Dictionary):
			continue
		var recipe_id := String((bench_variant as Dictionary).get(
			HudCraftingVocab.BENCH_RECIPE_ID_KEY, "")).strip_edges()
		if recipe_id != "" and not running.has(recipe_id):
			running.append(recipe_id)
	return running

## The player's own craft tracks. The wire carries every faction's, exactly as the sedentarization and
## intensification vectors do.
func _player_craft_knowledge() -> Array:
	var tracks: Array = []
	for track_variant in _craft_knowledge:
		if not (track_variant is Dictionary):
			continue
		var track: Dictionary = track_variant
		if int(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_FACTION_KEY, -1)) == HudConst.PLAYER_FACTION_ID:
			tracks.append(track)
	return tracks
