class_name StartingLoadoutController
extends RefCounted

## The OUTFITTING cluster (issue #629) — the controller half of `StartingLoadoutPanel`: it owns the
## panel node, holds every band's published window and the three catalogues the picker joins onto,
## HOLDS THE ALLOCATIONS, and turns the panel's six signals into the one command a window takes.
##
## Built on the `CraftingPanelController` / `KnowledgePanelController` idiom: `HudLayer` holds one as
## `_loadout`, hands it a HOST `Node` (a `RefCounted` cannot `add_child`) and the room the card is
## bounded by, keeps thin delegators for the entry points reached BY NAME, and RELAYS this
## controller's command signal onto its own so `Main` can format the line.
##
## ## ⛔ EVERY BAND GETS A WINDOW, AND THIS HOLDS ONE STATE PER BAND
##
## The spawned band's window GRANTS — its picks MINT gear against two budgets — and a band that
## splits hands its splinter a window of its own, which on a parent with no grant left is a TAKE:
## the picks MOVE gear out of the parent's ledger and the cap is what that parent can supply. Both
## kinds live in `_bands`, keyed by the durable `band_id`; `_subject` is the one the card is
## currently rendering, and the card's own band switcher is what moves it.
##
## **THE ALLOCATION LIVES HERE AND NOWHERE ELSE.** The panel renders a payload and emits intents;
## every clamp, every remainder and every "how many could I make" is computed here. That is what
## makes a budget un-overspendable and a supply un-overdrawable without the panel knowing what either
## is — and it is why a re-render never loses the player's picks.
##
## ## ⛔ A TAKE'S KIT CAP CANNOT BE DRAWN PER KIT ROW
##
## `sled` is used by both `big_game` and `trapping`, so five of each needs TEN sleds: the rows are not
## independent and a per-row cap would be a different rule from the one the sim refuses on. Every
## take clamp here EXPANDS the whole order through the roster's `uses` lists and checks the expansion
## against `parent_item_supply` — see `_take_kit_ceiling`.
##
## ## THE WINDOW OPENS ITSELF ONCE PER BAND, AND IS DISMISSIBLE AFTERWARDS
##
## A band's card opens automatically on the FIRST frame its window is open — a player who has just
## watched a world generate, or just split a band, should not have to find the screen — and after
## that the player owns whether the card is up. `_auto_opened` is what makes it once PER BAND rather
## than every snapshot: re-opening a card the player put away, every turn, is the failure mode the
## fork panel's `_auto_opened_forks` already guards against.
##
## ## ⛔ THE CARD DRAWS WHAT THE BAND HOLDS, AND EVERY PRESS SENDS A REPLACEMENT AT ONCE
##
## **There is no draft.** The sim applies a band's default outfit the moment the band is made — the
## opening band at world start, a splinter at its split, both down the same path a player's
## `set_starting_loadout` takes — so `loadout_window.kits` / `.materials` always describe gear the
## band ACTUALLY HOLDS. A band nobody touches keeps its default; there is no such thing as an
## allocation waiting to be filled in.
##
## So a `+`/`−` on a row emits `set_starting_loadout` for that band there and then, carrying the whole
## allocation as it stands AFTER the press — the verb is a whole-order replacement, never a diff. The
## footer control CLOSES the card and sends nothing; nothing is lost by pressing it, and nothing is
## lost by not.
##
## ⛔ **THE LOCAL WRITE IS OPTIMISTIC, AND ITS WARRANT IS THE COMMAND BESIDE IT.** A press has to move
## the number under the player's finger on the frame it was pressed, so the picks are written before
## the send's outcome is known — and the ONE invariant that makes that safe here is that **a local
## value is only ever written together with a command being sent**. A send that did not go therefore
## takes its write back: the payload carries the allocation as it stood BEFORE the press
## (`REVERT_KITS` / `REVERT_MATERIALS`), and `Main` hands it straight back to `revert_order` when
## `_send_formatted_command` answers `false` — `hud-modules.md` → "AN OPTIMISTIC WRITE NEEDS A
## ROLLBACK".
##
## ## ⛔ A PUBLISHED ALLOCATION IS ADOPTED UNLESS IT IS THIS CARD'S OWN ECHO
##
## The sim recaptures after every dispatched command, so each press comes back as a frame restating
## what it just ordered. `BAND_UNECHOED` holds the orders this card has SENT and not yet seen come
## back: a published allocation found in that list is our own echo and the card is already showing it,
## and anything else is the sim having moved this band's outfit for its own reasons (a split re-fitting
## the parent to its reduced budget) and is adopted whole.
##
## **Comparing against what was SENT rather than against what was last SEEN is what stops a second
## press flickering back.** Press twice quickly and the first press's echo lands while the card is
## already showing the second's; against a last-seen copy that echo reads as a change and drags the
## card back a step, twice per pair of presses.
##
## ⛔ **`open` IS THEREFORE NOT A SUCCESS SIGNAL, and this controller used to read it as one.** It
## held an `_awaiting_commit` flag and treated a still-open window on the next frame as a REFUSAL,
## which was right while an accepted order closed the window and is now the opposite of right: under
## replacement semantics every SUCCESSFUL commit leaves the window open, so that branch would post
## *"that order was refused"* after each one. The flag and its notice are gone. **A genuine refusal is
## not visible on this card at all** — the server only `warn!`s it to the log stream — and nothing here
## may go back to inferring one from `open`.

## Send one band's composed loadout — `set_starting_loadout <faction> <band> [kit <id> <n>]...
## [material <id> <n>]...`. **It fails CLOSED and WHOLE server-side**, so the client sends the entire
## allocation in one line and never a diff. Emitted on EVERY stepper press, and the payload carries
## the pre-press allocation so a refused send can be undone — see `REVERT_KITS`.
signal set_starting_loadout_requested(payload: Dictionary)
## The orb registry's loadout half changed (or emptied). `HudLayer` relays it to `TurnOrbController`,
## which folds it in with the band, knowledge and fork halves.
signal attention_changed(rows: Array)

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
## The HUD CanvasLayer, so this `RefCounted` has a node to parent the panel into.
var _host: Node = null
## **THE ROOM THE CARD IS BOUNDED BY — the HUD's floating room, NOT the window.** Handed down rather
## than looked up, because a `RefCounted` reaching into its host's scene tree is the coupling the
## controller pattern exists to avoid.
var _room_bounds: Control = null

var _panel: StartingLoadoutPanel = null

# --- The CAMPAIGN's half (`opening_loadout`), one per world ---
## ⛔ **THE TWO PRE-FILLS ARE GONE FROM HERE, AND NOTHING MAY DRAW THEM AGAIN.** `openingLoadout`
## still publishes `kitDefaults` / `materialDefaults`, but the SIM has already applied that spread to
## the band by the time the window is published — so seeding a card from it a second time would show,
## and then order, twice the gear the band holds. What survives is the pick list (a grant's offered
## materials, in the profile's own order) and the craftable ids (the third column's filter).
var _pickable: Array = []
var _craftable_recipe_ids: Array = []

# --- The catalogues the picker JOINS onto, both already published for other consumers ---
## The parsed `equipment_config_json` — the kit roster's one home.
var _equipment_config: Dictionary = {}
## The recipe book (`snapshot["recipes"]`), which is where a recipe's input costs and work live.
var _recipes: Array = []

# --- One state per band with an OPEN window ---
## `band_id -> Dictionary` in the `BAND_*` key vocabulary below.
var _bands: Dictionary = {}
## The roster's own order, so the band switcher never reshuffles between snapshots.
var _band_order: Array[int] = []
## The band the card is rendering. `HudConst.NO_BAND_ID` while nothing is open.
var _subject: int = HudConst.NO_BAND_ID
## Bands whose card has already stood itself up once this world.
var _auto_opened: Dictionary = {}
## The rows last handed to the orb, so an unchanged half is not re-pushed — `set_knowledge_attention`
## records what a needless full-registry push costs.
var _attention_rows: Array = []

# ---- one band's state, by key ---------------------------------------------------------------
## Its display name, resolved once per snapshot through `HudFormat.band_name` — the client's ONE
## naming rule, so this card calls a band what every other surface calls it.
const BAND_NAME := "name"
## `HudLoadoutVocab.GRANT_PARENT_BAND_ID` for a grant; otherwise the band a take draws from.
const BAND_PARENT := "parent"
## …and that band's name, for the copy that has to say where the gear comes from.
const BAND_PARENT_NAME := "parent_name"
const BAND_KIT_BUDGET := "kit_budget"
const BAND_MATERIAL_BUDGET := "material_budget"
## `item_id -> units` / `material_id -> units`, a take's caps. Empty on a grant window.
const BAND_ITEM_SUPPLY := "item_supply"
const BAND_MATERIAL_SUPPLY := "material_supply"
## The materials this window may pick, in draw order. The campaign pick list on a grant; **what the
## home band actually HOLDS on a take** — the pick list binds the grant and deliberately not a take,
## since a material a band crafted for itself must still be transferable to its own splinter.
const BAND_MATERIAL_ORDER := "material_order"
## ⛔ **`kit_id -> count` and `material_id -> units` — WHAT THE BAND HOLDS, never a draft.** Every
## write to either is accompanied by the command that orders it, so these two maps are always either
## the sim's own published rows or an order this card has just sent.
##
## **A ROW AT ZERO IS ERASED RATHER THAN STORED**, so the maps ARE the allocation: a `{gathering: 0}`
## entry is a row nobody holds, it drops out of the composed line anyway, and keeping it would make
## "is anything ordered" answer yes to an empty take.
const BAND_KIT_PICKS := "kit_picks"
const BAND_MATERIAL_PICKS := "material_picks"
## ⛔ **THE ORDERS THIS CARD HAS SENT AND NOT YET SEEN COME BACK**, oldest first — each a
## `{kits, materials}` pair of `id -> amount` maps, i.e. the same shape the published allocation is
## read into, so the two compare directly.
##
## It is what tells this card's own echo from a change the sim made for its own reasons. The sim
## recaptures after every dispatched command, so a press comes back as a frame restating it; while
## two presses are in flight the FIRST echo arrives after the card is already showing the SECOND, and
## a card comparing against what it last SAW would drag itself back a step on every pair of presses.
## A match settles that order and every order before it; anything else is adopted, and clears the list.
const BAND_UNECHOED := "unechoed"

## A recipe with no inputs at all cannot be priced against a pile, so the column reads it as
## unreachable rather than as infinitely makeable. Nothing in the shipped book is such a recipe; this
## is what a hand-edited one would render as.
const UNPRICED_RECIPE_COUNT := 0

## What one kit costs in units of one item, per kit ordered. A roster entry that listed an item twice
## would cost two, which is why `_take_kit_ceiling` COUNTS the `uses` list rather than reading its
## size as a set.
const ITEM_UNITS_PER_USE := 1

## ⛔ **THE ROLLBACK HANDLE — the allocation as it stood BEFORE the press, on the payload.** `Main`
## reads neither key (`format_set_starting_loadout` ignores them) and hands the whole payload back to
## `revert_order` when the line did not go, which is `pending_entity`'s own shape one verb over.
##
## **It is the WHOLE allocation rather than the one row that moved, because the verb is a whole-order
## replacement**: there is no other un-acknowledged edit on this band for a whole-allocation restore
## to discard, every local write having been sent as it was made.
const REVERT_KITS := "revert_kits"
const REVERT_MATERIALS := "revert_materials"

func setup(host: Node, room_bounds: Control = null) -> void:
	_host = host
	_room_bounds = room_bounds

# ---- ingest -----------------------------------------------------------------

## The CAMPAIGN's half (`opening_loadout`): the pick list and the craftable ids. A non-Dictionary is
## ignored — a delta carries a section only when it changed, so absence means unchanged and never
## "the world forgot its pick list".
##
## ⛔ **NOTHING HERE OPENS OR SHUTS A WINDOW.** `open` and the two budgets left this section when the
## window became a fact about one BAND; they arrive on the cohorts, through `set_bands`.
##
## ⛔ **AND NOTHING HERE SEEDS A CARD.** The section's two pre-fills are read by nothing: the sim
## applies that spread at the band's creation, so a card seeded from it as well would draw — and
## order — the gear twice.
func set_campaign_loadout(state: Variant) -> void:
	if not (state is Dictionary):
		return
	var campaign: Dictionary = state
	_pickable = campaign.get(HudLoadoutVocab.PICKABLE_MATERIALS_KEY, [])
	_craftable_recipe_ids = campaign.get(HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY, [])
	if is_expanded():
		render()

## **THE PLAYER'S BANDS, and one window per band that has one open.** Fed from the same roster split
## `update_band_alerts` already computes, so this cluster never walks `populations` a second time.
##
## The whole roster is passed rather than only the bands with windows: a take's copy names its HOME
## band, and that band may have no window of its own at all.
func set_bands(bands: Variant) -> void:
	if not (bands is Array):
		return
	var roster: Array = bands
	var names: Dictionary = {}
	for entry_variant in roster:
		if entry_variant is Dictionary:
			var entry: Dictionary = entry_variant
			var id := int(entry.get(HudLoadoutVocab.BAND_ID_KEY, HudConst.NO_BAND_ID))
			if id != HudConst.NO_BAND_ID:
				names[id] = HudFormat.band_name(entry)
	var order: Array[int] = []
	for entry_variant in roster:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var window_variant: Variant = entry.get(HudLoadoutVocab.WINDOW_KEY, null)
		if not (window_variant is Dictionary):
			continue
		var window: Dictionary = window_variant
		# **ABSENT AND SHUT MEAN THE SAME THING**: this band has nothing to outfit right now.
		if not bool(window.get(HudLoadoutVocab.OPEN_KEY, false)):
			continue
		var band_id := int(entry.get(HudLoadoutVocab.BAND_ID_KEY, HudConst.NO_BAND_ID))
		if band_id == HudConst.NO_BAND_ID:
			continue
		order.append(band_id)
		_ingest_window(band_id, window, names)
	# **A WINDOW THAT SHUT IS A WINDOW THAT IS GONE.** The turn advanced, so its picks are history
	# and its budget — if it was a grant — is forfeit; keeping the state would put a card back up on
	# a band the sim will refuse.
	for held in _bands.keys():
		if not order.has(held):
			_bands.erase(held)
	_band_order = order
	_settle_subject()

## The whole effective `EquipmentConfig`, serialized. Parsed HERE and nowhere else in the HUD — the
## kit roster has no typed wire field, and this blob is where it rides. It is also what a take's cap
## is expanded through, so a card without it can offer no kit at all.
func set_equipment_config(json: Variant) -> void:
	if not (json is String) or String(json).is_empty():
		return
	var parsed: Variant = JSON.parse_string(String(json))
	_equipment_config = parsed if parsed is Dictionary else {}
	if is_expanded():
		render()

## The recipe book — the ONE source of a recipe's input costs and work value. The picker never
## re-derives a cost from anywhere else.
func set_recipes(recipes: Variant) -> void:
	if not (recipes is Array):
		return
	_recipes = recipes
	if is_expanded():
		render()

## Refresh one band's published window, and take its allocation from the wire unless it is this
## card's own echo — see `BAND_UNECHOED`.
func _ingest_window(band_id: int, window: Dictionary, names: Dictionary) -> void:
	var state: Dictionary = _bands.get(band_id, {})
	var parent := int(window.get(HudLoadoutVocab.PARENT_BAND_ID_KEY,
		HudLoadoutVocab.GRANT_PARENT_BAND_ID))
	state[BAND_NAME] = String(names.get(band_id, ""))
	state[BAND_PARENT] = parent
	state[BAND_PARENT_NAME] = String(names.get(parent, ""))
	state[BAND_KIT_BUDGET] = int(window.get(HudLoadoutVocab.KIT_BUDGET_KEY, 0))
	state[BAND_MATERIAL_BUDGET] = int(window.get(HudLoadoutVocab.MATERIAL_BUDGET_KEY, 0))
	var item_supply := _supply_map(window.get(HudLoadoutVocab.PARENT_ITEM_SUPPLY_KEY, []))
	var material_supply := _supply_map(window.get(HudLoadoutVocab.PARENT_MATERIAL_SUPPLY_KEY, []))
	state[BAND_ITEM_SUPPLY] = item_supply
	state[BAND_MATERIAL_SUPPLY] = material_supply
	state[BAND_MATERIAL_ORDER] = _pickable if parent == HudLoadoutVocab.GRANT_PARENT_BAND_ID \
		else _supply_order(window.get(HudLoadoutVocab.PARENT_MATERIAL_SUPPLY_KEY, []))
	var published := {
		HudLoadoutVocab.WINDOW_KITS_KEY: _allocation_map(
			window.get(HudLoadoutVocab.WINDOW_KITS_KEY, []),
			HudLoadoutVocab.KIT_DEFAULT_ID_KEY, HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY),
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: _allocation_map(
			window.get(HudLoadoutVocab.WINDOW_MATERIALS_KEY, []),
			HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY, HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY),
	}
	if not state.has(BAND_UNECHOED):
		state[BAND_KIT_PICKS] = {}
		state[BAND_MATERIAL_PICKS] = {}
		state[BAND_UNECHOED] = []
	# ⛔ **THIS CARD'S OWN ECHO, OR THE SIM'S OWN MOVE — there is no third case.** A published
	# allocation this card SENT is already on screen, so adopting it would be a no-op at best and, with
	# a second press already made, a step backwards. Anything else is the band's outfit having moved
	# for the sim's own reasons — a split re-fitting the parent to its reduced budget
	# (`fission::rebalance_partitioned_grant`), an order refused whole, or the default the sim applied
	# when it made the band — and the wire is the authority on what the band holds.
	var unechoed: Array = state[BAND_UNECHOED]
	var echoed := unechoed.find(published)
	if echoed >= 0:
		# That order and every order before it have landed; later ones are still out.
		state[BAND_UNECHOED] = unechoed.slice(echoed + 1)
	else:
		state[BAND_UNECHOED] = []
		_adopt_published(state, published)
	_bands[band_id] = state

## ⛔ **THE PUBLISHED ALLOCATION, WHOLE AND UNCLAMPED.** The sim already fitted this spread to the
## band's budgets when it accepted it, so a second clamp here would disagree with the first — and a
## band the sim has genuinely left over its budget must READ as over budget rather than be quietly
## trimmed into looking fine (`_over_allowance` is the row that says so).
##
## A material this window cannot pick is dropped — it could not be spent and would strand part of the
## budget. **The kit half needs no such filter**: the roster it is drawn against is the published
## equipment config, so a kit absent from that roster renders no row at all and costs a dictionary
## entry nobody reads rather than a phantom control.
func _adopt_published(state: Dictionary, published: Dictionary) -> void:
	var offered: Dictionary = {}
	for id_variant in state.get(BAND_MATERIAL_ORDER, []):
		offered[String(id_variant)] = true
	var kits: Dictionary = {}
	for kit_variant in (published[HudLoadoutVocab.WINDOW_KITS_KEY] as Dictionary).keys():
		var count := int((published[HudLoadoutVocab.WINDOW_KITS_KEY] as Dictionary)[kit_variant])
		if count > 0:
			kits[String(kit_variant)] = count
	var materials: Dictionary = {}
	var published_materials: Dictionary = published[HudLoadoutVocab.WINDOW_MATERIALS_KEY]
	for material_variant in published_materials.keys():
		var units := int(published_materials[material_variant])
		if units > 0 and offered.has(String(material_variant)):
			materials[String(material_variant)] = units
	state[BAND_KIT_PICKS] = kits
	state[BAND_MATERIAL_PICKS] = materials

## An accepted-allocation half as `id -> amount`, for the comparison above — a DICT rather than the
## published array, so a re-ordered but identical allocation is not read as a change.
func _allocation_map(rows: Variant, id_key: String, amount_key: String) -> Dictionary:
	var map: Dictionary = {}
	if not (rows is Array):
		return map
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var id := String(row.get(id_key, ""))
		if not id.is_empty():
			map[id] = int(row.get(amount_key, 0))
	return map

## `[{id, units}]` → `id -> units`. A row the sim publishes twice cannot happen (both supplies are
## keyed maps sim-side), so the later row simply wins rather than summing.
func _supply_map(rows: Variant) -> Dictionary:
	var map: Dictionary = {}
	if not (rows is Array):
		return map
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var id := String(row.get(HudLoadoutVocab.SUPPLY_ID_KEY, ""))
		if id.is_empty():
			continue
		map[id] = int(row.get(HudLoadoutVocab.SUPPLY_UNITS_KEY, 0))
	return map

## The same rows as an ORDER — the sim publishes both supplies sorted, so this is the draw order a
## take's resources column and its legend share.
func _supply_order(rows: Variant) -> Array:
	var ids: Array = []
	if not (rows is Array):
		return ids
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var id := String((row_variant as Dictionary).get(HudLoadoutVocab.SUPPLY_ID_KEY, ""))
		if not id.is_empty():
			ids.append(id)
	return ids

## Which band the card renders, and whether it stands itself up. Called after every roster ingest.
##
## **A BAND WHOSE WINDOW HAS NEVER BEEN SEEN OPENS THE CARD ONCE** — the spawned band on turn one, a
## splinter on the turn it is made. Every pending band is marked in the same pass, so two splits in
## one turn cannot queue two pop-ups on two consecutive snapshots.
func _settle_subject() -> void:
	if _bands.is_empty():
		# **THE TURN ADVANCED ON THE LAST OPEN WINDOW.** That is the ONLY thing that shuts one — an
		# accepted order does not — so the whole surface goes with it.
		_subject = HudConst.NO_BAND_ID
		close()
		_push_attention()
		return
	if not _bands.has(_subject):
		_subject = _band_order[0]
	var pending := HudConst.NO_BAND_ID
	for band_id in _band_order:
		if _auto_opened.has(band_id):
			continue
		_auto_opened[band_id] = true
		if pending == HudConst.NO_BAND_ID:
			pending = band_id
	if pending != HudConst.NO_BAND_ID:
		_subject = pending
		_open_card()
	# ⛔ **`is_expanded`, NOT `is_open`.** A DISMISSED picker is still "open" — the panel node is
	# visible, carrying the reopen pill — so a re-render gated on `is_open()` puts the card the player
	# just dismissed straight back on screen, on the very next snapshot and every one after it. That is
	# the whole point of the window being dismissible, undone by one accessor.
	elif is_expanded():
		render()
	else:
		# Dismissed, and a window is still open: keep the reopen pill live and the picks intact.
		_ensure_panel()
		_panel.collapse()
	_push_attention()

# ---- open / close -----------------------------------------------------------

func is_open() -> bool:
	return _panel != null and is_instance_valid(_panel) and _panel.is_open()

func is_expanded() -> bool:
	return _panel != null and is_instance_valid(_panel) and _panel.is_expanded()

## Show the full card. Reached by name from the preview harnesses and from the orb's row.
func open() -> void:
	if _bands.is_empty():
		return
	_open_card()

## Render a DIFFERENT band's window. Two callers: the card's band switcher, and the turn orb's row for
## that band (through `TurnOrbController`, off the row's own
## `HudAttentionVocab.ATTENTION_PANEL_SUBJECT`). It declines a band with no open window, so a stale
## subject on a row the registry has not caught up with opens nothing rather than the wrong card.
func open_band(band_id: int) -> void:
	if not _bands.has(band_id):
		return
	_subject = band_id
	_open_card()
	_push_attention()

## The band the card is currently outfitting, for the harnesses and the orb.
func subject_band_id() -> int:
	return _subject

## True while the subject's window MOVES gear off another band rather than minting it.
func is_take() -> bool:
	return _parent_of(_subject_state()) != HudLoadoutVocab.GRANT_PARENT_BAND_ID

## Put the CARD away and leave the reopen pill — the player wants to look at the map. The window is
## still open as far as the sim is concerned.
func collapse() -> void:
	if _bands.is_empty() or _panel == null or not is_instance_valid(_panel):
		return
	_panel.collapse()

## The whole surface goes. Every window has shut, or the world was rebuilt.
func close() -> void:
	if _panel != null and is_instance_valid(_panel):
		_panel.close()

## A world rebuild: everything about the previous world's outfitting is gone, picks included.
func reset_world_state() -> void:
	_bands = {}
	_band_order = []
	_subject = HudConst.NO_BAND_ID
	_auto_opened = {}
	_equipment_config = {}
	_recipes = []
	_pickable = []
	_craftable_recipe_ids = []
	close()
	_push_attention()

## The room the card is bounded by changed shape. **Re-fit, do not re-render** — the payload is
## unchanged, so rebuilding the columns would answer a question about geometry by throwing away the
## player's scroll position.
func refit_room() -> void:
	if not is_open():
		return
	_panel.refit()

## The panel node, for the harnesses.
func panel() -> StartingLoadoutPanel:
	return _panel

# ---- render -----------------------------------------------------------------

func render() -> void:
	if _bands.is_empty():
		return
	_ensure_panel()
	var band := _subject_state()
	var take := _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID
	var materials := _material_rows(band)
	_panel.render({
		StartingLoadoutPanel.PAYLOAD_TITLE: _title(band),
		StartingLoadoutPanel.PAYLOAD_SUBTITLE: _subtitle(band, take),
		StartingLoadoutPanel.PAYLOAD_BANDS: _band_tabs(),
		StartingLoadoutPanel.PAYLOAD_IS_TAKE: take,
		StartingLoadoutPanel.PAYLOAD_KITS: _kit_rows(band, take),
		StartingLoadoutPanel.PAYLOAD_MATERIALS: materials,
		StartingLoadoutPanel.PAYLOAD_RECIPES: _recipe_rows(band, materials),
		StartingLoadoutPanel.PAYLOAD_KIT_BUDGET: {
			StartingLoadoutPanel.BUDGET_SPENT: kits_spent(),
			StartingLoadoutPanel.BUDGET_TOTAL: _kit_total(band, take),
		},
		StartingLoadoutPanel.PAYLOAD_MATERIAL_BUDGET: {
			StartingLoadoutPanel.BUDGET_SPENT: materials_spent(),
			StartingLoadoutPanel.BUDGET_TOTAL: _material_total(band, take),
		},
	})

## **THE CARD NAMES ITS BAND**, because a split can leave two windows open at once and a card headed
## only *"the band"* would leave the player composing an order for a band they cannot identify.
func _title(band: Dictionary) -> String:
	var band_name := String(band.get(BAND_NAME, ""))
	if band_name.is_empty():
		return HudLoadoutVocab.PANEL_TITLE
	return HudLoadoutVocab.PANEL_TITLE_FORMAT % band_name

## …and the subtitle says where the gear COMES FROM, which is the one fact that separates the two
## windows. A grant mints, so it speaks of what the people carry; a take moves gear out of the home
## band's own ledger, so it names that band.
func _subtitle(band: Dictionary, take: bool) -> String:
	if not take:
		return HudLoadoutVocab.PANEL_SUBTITLE
	var parent_name := String(band.get(BAND_PARENT_NAME, ""))
	if parent_name.is_empty():
		return HudLoadoutVocab.PANEL_SUBTITLE
	return HudLoadoutVocab.PANEL_SUBTITLE_TAKE_FORMAT % parent_name

## The switcher's rows, in the roster's own order. The panel draws nothing for a single row — the
## ordinary case is one window, and a control naming the only band there is teaches nothing.
func _band_tabs() -> Array:
	var tabs: Array = []
	for band_id in _band_order:
		var state: Dictionary = _bands.get(band_id, {})
		tabs.append({
			"band_id": band_id,
			"label": _band_label(band_id, state),
			"subject": band_id == _subject,
		})
	return tabs

func _band_label(band_id: int, state: Dictionary) -> String:
	var band_name := String(state.get(BAND_NAME, ""))
	if not band_name.is_empty():
		return band_name
	return HudFormat.band_name({HudLoadoutVocab.BAND_ID_KEY: band_id})

## COLUMN 1's rows — the kit roster out of the parsed equipment config, in the config's own order,
## with **the `none` kit dropped by its EMPTY `uses`** rather than by matching its id: a roster that
## renamed the carry-nothing entry would still be excluded, and a roster that gave it items would
## rightly start offering it.
##
## **`can_add` IS PER ROW, and on a take it has to be.** A grant's rows share one budget, so they all
## carry the same answer; a take's cap is per ITEM, so `big_game` can be exhausted (no spears left at
## home) while `gathering` is still free.
func _kit_rows(band: Dictionary, take: bool) -> Array:
	var rows: Array = []
	var roster: Variant = _equipment_config.get(HudLoadoutVocab.CONFIG_KITS_KEY, [])
	if not (roster is Array):
		return rows
	var picks: Dictionary = band.get(BAND_KIT_PICKS, {})
	var budget_left := kits_left()
	for entry_variant in roster:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var uses: Array = entry.get(HudLoadoutVocab.KIT_USES_KEY, [])
		if uses.is_empty():
			continue
		var kit_id := String(entry.get(HudLoadoutVocab.KIT_ID_KEY, ""))
		if kit_id.is_empty():
			continue
		var count := int(picks.get(kit_id, 0))
		rows.append({
			"id": kit_id,
			"display_name": String(entry.get(HudLoadoutVocab.KIT_DISPLAY_NAME_KEY, kit_id)),
			# The config publishes a job as its raw id and no display name for it — every surface in
			# this client capitalizes such an id, and so does this one.
			"jobs_text": _joined_labels(entry.get(HudLoadoutVocab.KIT_JOBS_KEY, []),
				HudLoadoutVocab.KIT_JOBS_SEPARATOR, false),
			"uses_text": _joined_labels(uses, HudLoadoutVocab.KIT_USES_SEPARATOR, true),
			"count": count,
			"can_add": _take_kit_ceiling(band, kit_id) > count if take else budget_left > 0,
		})
	return rows

func _joined_labels(ids: Variant, separator: String, as_items: bool) -> String:
	if not (ids is Array):
		return ""
	var parts: Array[String] = []
	for id_variant in ids:
		var id := String(id_variant)
		parts.append(DetailFormat.kit_item_label(id).capitalize() if as_items else id.capitalize())
	return separator.join(parts)

## COLUMN 2's rows — one per material this window may pick, in the published order, each carrying the
## swatch the legend and the recipe rows will draw. Resolving the colour ONCE, here, is what makes the
## three places it appears one key rather than three tables.
##
## The ring is indexed by a material's position in THIS window's own list, which on a grant is the
## profile's pick list and on a take is what the home band holds. Two cards can therefore paint one
## material differently; each card is internally consistent, which is the property the key needs.
func _material_rows(band: Dictionary) -> Array:
	var rows: Array = []
	var picks: Dictionary = band.get(BAND_MATERIAL_PICKS, {})
	var offered: Array = band.get(BAND_MATERIAL_ORDER, [])
	for index in range(offered.size()):
		var material_id := String(offered[index])
		if material_id.is_empty():
			continue
		var units := int(picks.get(material_id, 0))
		rows.append({
			"id": material_id,
			"label": HudLoadoutVocab.material_label(material_id),
			"color": HudLoadoutVocab.swatch_color(index),
			"units": units,
			"can_add": _material_ceiling(band, material_id) > units,
		})
	return rows

## COLUMN 3's rows — **only the recipes the sim published as craftable**, priced against the pile.
##
## `count = floor(min over inputs of allocated[material] / required)`: how many of THIS one thing the
## whole pile could make. It is deliberately not a simultaneous build plan — two rows both reading
## `×2` are two answers to two separate questions, which is what the column head says on screen.
##
## **Reachable first**, then by count descending, then in the recipe book's own order — a stable sort
## over the book, so a tie never reshuffles between renders.
func _recipe_rows(band: Dictionary, materials: Array) -> Array:
	if _craftable_recipe_ids.is_empty() or _recipes.is_empty():
		return []
	var picks: Dictionary = band.get(BAND_MATERIAL_PICKS, {})
	var colors: Dictionary = {}
	for material_variant in materials:
		if material_variant is Dictionary:
			var material: Dictionary = material_variant
			colors[String(material.get("id", ""))] = material.get("color")
	var allowed: Dictionary = {}
	for id_variant in _craftable_recipe_ids:
		allowed[String(id_variant)] = true
	var rows: Array = []
	for order in range(_recipes.size()):
		if not (_recipes[order] is Dictionary):
			continue
		var recipe: Dictionary = _recipes[order]
		var recipe_id := String(recipe.get(HudLoadoutVocab.RECIPE_ID_KEY, ""))
		if not allowed.has(recipe_id):
			continue
		var inputs: Array = []
		var count := -1
		for input_variant in recipe.get(HudLoadoutVocab.RECIPE_INPUTS_KEY, []):
			if not (input_variant is Dictionary):
				continue
			var input: Dictionary = input_variant
			var material_id := String(input.get(HudLoadoutVocab.RECIPE_INPUT_MATERIAL_ID_KEY, ""))
			var amount := float(input.get(HudLoadoutVocab.RECIPE_INPUT_AMOUNT_KEY, 0.0))
			inputs.append({
				"material_id": material_id,
				"amount": amount,
				"color": colors.get(material_id, HudLoadoutVocab.BUDGET_REMAINDER_COLOR),
			})
			var held := float(int(picks.get(material_id, 0)))
			# A required amount of zero would divide by zero; it also cannot bind, so it is skipped.
			var possible := UNPRICED_RECIPE_COUNT if amount <= 0.0 else int(floorf(held / amount))
			count = possible if count < 0 else mini(count, possible)
		rows.append({
			"id": recipe_id,
			"display_name": String(recipe.get(HudLoadoutVocab.RECIPE_DISPLAY_NAME_KEY, recipe_id)),
			"work": float(recipe.get(HudLoadoutVocab.RECIPE_WORK_KEY, 0.0)),
			"count": UNPRICED_RECIPE_COUNT if count < 0 else count,
			"inputs": inputs,
			"order": order,
		})
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		if int(a["count"]) != int(b["count"]):
			return int(a["count"]) > int(b["count"])
		return int(a["order"]) < int(b["order"]))
	return rows

# ---- the arithmetic the panel never does ------------------------------------

func _subject_state() -> Dictionary:
	return _bands.get(_subject, {})

func _parent_of(band: Dictionary) -> int:
	return int(band.get(BAND_PARENT, HudLoadoutVocab.GRANT_PARENT_BAND_ID))

## What the KIT meter is spent against. A grant spends KIT SLOTS; **a take spends ITEM UNITS**, since
## that is the currency its cap is denominated in and the currency the sim refuses on.
func kits_spent() -> int:
	var band := _subject_state()
	if _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID:
		var total := 0
		for units in _expanded_items(band, "").values():
			total += int(units)
		return total
	var slots := 0
	for count in band.get(BAND_KIT_PICKS, {}).values():
		slots += int(count)
	return slots

func materials_spent() -> int:
	var total := 0
	for units in _subject_state().get(BAND_MATERIAL_PICKS, {}).values():
		total += int(units)
	return total

## What the meter still shows. On a grant that is budget left to MINT; on a take it is supply left AT
## HOME — which is not forfeited when the turn advances, it simply stays where it is.
func kits_left() -> int:
	var band := _subject_state()
	var take := _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID
	return maxi(_kit_total(band, take) - kits_spent(), 0)

func materials_left() -> int:
	var band := _subject_state()
	var take := _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID
	return maxi(_material_total(band, take) - materials_spent(), 0)

## The kit meter's denominator: the band's own working-age head count on a grant, and on a take the
## WHOLE of `parent_item_supply`.
##
## **The sim publishes only items some kit carries**, so the sum is already the pile this column can
## draw down: the three items no kit `uses` — `bone_awl`, `loom`, `tanning_frame` — are the
## knowledge-gated bench tools, and shop equipment stays with the workshop that built it rather than
## walking out with a splinter (`.claude/rules/core_sim/starting-loadout.md`). Re-deriving that filter
## here off the roster would be a second copy of `EquipmentConfig::item_is_kit_carried`, and a second
## copy is a second answer the day the two disagree.
func _kit_total(band: Dictionary, take: bool) -> int:
	if not take:
		return int(band.get(BAND_KIT_BUDGET, 0))
	var total := 0
	for units in band.get(BAND_ITEM_SUPPLY, {}).values():
		total += int(units)
	return total

func _material_total(band: Dictionary, take: bool) -> int:
	if not take:
		return int(band.get(BAND_MATERIAL_BUDGET, 0))
	var total := 0
	for units in band.get(BAND_MATERIAL_SUPPLY, {}).values():
		total += int(units)
	return total

## The units a kit puts in hands, one per `uses` entry and COUNTED rather than de-duplicated: a
## roster entry naming an item twice grants two.
func _kit_uses(kit_id: String) -> Dictionary:
	var per_kit: Dictionary = {}
	var roster: Variant = _equipment_config.get(HudLoadoutVocab.CONFIG_KITS_KEY, [])
	if not (roster is Array):
		return per_kit
	for entry_variant in roster:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if String(entry.get(HudLoadoutVocab.KIT_ID_KEY, "")) != kit_id:
			continue
		for use_variant in entry.get(HudLoadoutVocab.KIT_USES_KEY, []):
			var item_id := String(use_variant)
			per_kit[item_id] = int(per_kit.get(item_id, 0)) + ITEM_UNITS_PER_USE
		return per_kit
	return per_kit

## ⛔ **THE ORDER, EXPANDED TO ITEMS — the only form a take's cap can be checked in.** `sled` is used
## by both `big_game` and `trapping`, so the two rows ADD: five of each is ten sleds, and a per-row
## cap would let the client compose an order the sim refuses whole. `skip_kit` leaves one row out, so
## a clamp can price that row against everything else.
func _expanded_items(band: Dictionary, skip_kit: String) -> Dictionary:
	var expanded: Dictionary = {}
	var picks: Dictionary = band.get(BAND_KIT_PICKS, {})
	for kit_variant in picks.keys():
		var kit_id := String(kit_variant)
		if kit_id == skip_kit:
			continue
		var count := int(picks[kit_variant])
		if count <= 0:
			continue
		var per_kit := _kit_uses(kit_id)
		for item_variant in per_kit.keys():
			var item_id := String(item_variant)
			expanded[item_id] = int(expanded.get(item_id, 0)) \
				+ int(per_kit[item_variant]) * count
	return expanded

## How many of ONE kit a take may hold, given every OTHER kit it has already ordered. `0` for a kit
## the roster does not carry (or one that carries nothing), which is what stops the `+` on a row the
## home band cannot supply at all.
func _take_kit_ceiling(band: Dictionary, kit_id: String) -> int:
	var per_kit := _kit_uses(kit_id)
	if per_kit.is_empty():
		return 0
	var supply: Dictionary = band.get(BAND_ITEM_SUPPLY, {})
	var others := _expanded_items(band, kit_id)
	var ceiling := -1
	for item_variant in per_kit.keys():
		var item_id := String(item_variant)
		var left := int(supply.get(item_id, 0)) - int(others.get(item_id, 0))
		var fits := int(floorf(float(left) / float(int(per_kit[item_variant]))))
		ceiling = fits if ceiling < 0 else mini(ceiling, fits)
	return maxi(ceiling, 0)

## How many units of one material this window may hold. A grant is capped by what is LEFT of its
## budget plus what this row already holds; a take by what the home band can supply, which already
## includes the units this take is standing on.
func _material_ceiling(band: Dictionary, material_id: String) -> int:
	var held := int(band.get(BAND_MATERIAL_PICKS, {}).get(material_id, 0))
	if _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID:
		return int(band.get(BAND_MATERIAL_SUPPLY, {}).get(material_id, 0))
	return held + materials_left()

# ---- the orb's rows ---------------------------------------------------------

## Producer — the outfitting windows. **ONE ROW PER BAND WITH ONE OPEN**, spent or not: the card is
## dismissible and this row's `Open ▸` is the guaranteed way back to it, so a producer that fell
## silent once a budget was clear would strand a player who had finished picking, put the card away,
## and then wanted to revise before ending the turn.
##
## **WHAT MOVES IS THE SEVERITY AND THE WORDING.** A grant with both budgets clear ⇒ `ready`, and the
## row reads as done; anything unspent ⇒ `warn`, naming what is left. `ready` ranks BELOW `info`, so a
## satisfied loadout never takes the orb's accent off a real warning elsewhere, and it still paints
## the orb when it is the highest entry present.
##
## ⛔ **A TAKE NEVER SAYS `unspent`.** What a grant leaves unspent is GONE on the turn advance, which
## is the whole reason that word is on the orb; supply a take leaves behind stays with the home band
## and is lost by nobody. So a take's arms report what IS taken, and name the home band — two open
## windows otherwise put two identically-worded rows on one popover.
##
## **NON-LOCATING** (an outfitting window is a band fact and no hex holds it — the band may not even
## be where the player is looking) and deliberately **NOT `blocking`** in either state: closing the
## window is the sim's business, so the `Advance ▸` footer stays live and this row only ever warns.
func attention_rows() -> Array:
	var rows: Array = []
	for band_id in _band_order:
		var band: Dictionary = _bands.get(band_id, {})
		if band.is_empty():
			continue
		rows.append(_attention_row(band_id, band))
	return rows

## ⛔ **THREE ARMS, AND THE THIRD ONE IS A FLOOR UNDER THE OTHER TWO.** A window whose meter reads
## NEGATIVE — the band holding more than its budget or its home band's supply allows — is a state
## this row has no true wording for, so it must not take the wording that says *done*. It reads
## `warn` and says which way it is wrong.
##
## The completeness test was `remaining <= 0` over a remainder clamped at zero, so over-budget and
## fully-spent were literally the same answer; a live run showed a card reading `-6 / 22 left` beside
## a row calling it outfitted. The sim bug behind that `-6` is fixed and nothing here relies on it.
func _attention_row(band_id: int, band: Dictionary) -> Dictionary:
	var take := _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID
	var over := _over_allowance(band)
	# A grant is DONE when both budgets are clear — there is nothing left to mint. A take is done as
	# soon as an order stands: it forfeits nothing by leaving supply at home, so "everything drawn"
	# is not a state the player is working towards.
	var complete := not over and (_take_is_ordered(band) if take else _grant_is_complete(band))
	var label := HudLoadoutVocab.ATTENTION_LABEL_UNSPENT
	if over:
		label = HudLoadoutVocab.ATTENTION_LABEL_OVER
	elif complete:
		label = HudLoadoutVocab.ATTENTION_LABEL_READY
	var fact := _over_detail(band)
	if not over:
		fact = _take_detail(band) if take else _grant_detail(band)
	return {
		"kind": HudAttentionVocab.ATTENTION_KIND_OPENING_LOADOUT,
		# **THE BAND THIS ROW'S `Open ▸` MUST REACH.** The orb carries it and never reads it — see
		# `HudAttentionVocab.ATTENTION_PANEL_SUBJECT`. Without it the press opened whichever band the
		# card happened to be showing, which with two windows is a button that goes somewhere else.
		HudAttentionVocab.ATTENTION_PANEL_SUBJECT: band_id,
		# Never `critical` on the unfinished arm either: nothing is being lost yet, and the row shares
		# the popover with starvation rows that genuinely are.
		"severity": HudAttentionVocab.ATTENTION_SEVERITY_READY if complete \
			else HudAttentionVocab.ATTENTION_SEVERITY_WARN,
		"label": label,
		# **THE BAND LEADS THE DETAIL**, the idle-worker rows' own convention — see
		# `HudLoadoutVocab.ATTENTION_DETAIL_BAND_FORMAT`.
		"detail": HudLoadoutVocab.ATTENTION_DETAIL_BAND_FORMAT % [_band_label(band_id, band), fact],
		"x": HudAttentionVocab.ATTENTION_NON_LOCATING,
		"y": HudAttentionVocab.ATTENTION_NON_LOCATING,
	}

## **Is either meter NEGATIVE** — is this band holding more than its window allows? Asked of the
## SIGNED remainder, because `kits_left` / `materials_left` clamp at zero for the meter's sake and a
## clamp is precisely what hid this state.
func _over_allowance(band: Dictionary) -> bool:
	return _signed_kits(band) < 0 or _signed_materials(band) < 0

## …and by how much, in the bare count nouns the take arm already uses. `2 kits, 6 resources over
## budget`. **A take's "budget" is the home band's supply**, which is the same sentence one currency
## over: the band is standing on more than the window says it may have.
func _over_detail(band: Dictionary) -> String:
	var parts: Array[String] = []
	var kits := -_signed_kits(band)
	if kits == 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_KITS_ONE)
	elif kits > 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_KITS_MANY % kits)
	var units := -_signed_materials(band)
	if units == 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_RESOURCES_ONE)
	elif units > 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_RESOURCES_MANY % units)
	return HudLoadoutVocab.ATTENTION_DETAIL_OVER_FORMAT \
		% HudLoadoutVocab.ATTENTION_DETAIL_SEPARATOR.join(parts)

func _grant_is_complete(band: Dictionary) -> bool:
	return _remaining_kits(band) <= 0 and _remaining_materials(band) <= 0

## A take has an order standing once it names anything at all.
func _take_is_ordered(band: Dictionary) -> bool:
	return not band.get(BAND_KIT_PICKS, {}).is_empty() \
		or not band.get(BAND_MATERIAL_PICKS, {}).is_empty()

## A GRANT's detail: both remainders in one line, a budget already clear dropped rather than printed
## as a zero. **The remainder is named whatever the control that spends it is named** — the picker's
## second column is headed `RESOURCES`, and this row read `2 units unspent` beside it until a player
## asked what a unit was.
func _grant_detail(band: Dictionary) -> String:
	var parts: Array[String] = []
	var kits := _remaining_kits(band)
	if kits == 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_KITS_ONE)
	elif kits > 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_KITS_MANY % kits)
	var units := _remaining_materials(band)
	if units == 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_UNITS_ONE)
	elif units > 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_UNITS_MANY % units)
	if parts.is_empty():
		return HudLoadoutVocab.ATTENTION_DETAIL_READY
	return HudLoadoutVocab.ATTENTION_DETAIL_SEPARATOR.join(parts)

## A TAKE's detail: what has been taken. **It named the home band until the row named its OWN**, which
## was the only way two identically-worded rows could be told apart; the subject's name does that job
## properly now, and the card's subtitle is where the home band belongs.
func _take_detail(band: Dictionary) -> String:
	var parts: Array[String] = []
	var kits := 0
	for count in band.get(BAND_KIT_PICKS, {}).values():
		kits += int(count)
	if kits == 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_KITS_ONE)
	elif kits > 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_KITS_MANY % kits)
	var units := 0
	for held in band.get(BAND_MATERIAL_PICKS, {}).values():
		units += int(held)
	if units == 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_RESOURCES_ONE)
	elif units > 1:
		parts.append(HudLoadoutVocab.ATTENTION_COUNT_RESOURCES_MANY % units)
	if parts.is_empty():
		return HudLoadoutVocab.ATTENTION_DETAIL_TAKE_NONE
	return HudLoadoutVocab.ATTENTION_DETAIL_SEPARATOR.join(parts)

## The subject's own accessors answer for the SUBJECT; the orb has to ask about every band, so the two
## remainders are computed per band here rather than through `kits_left` / `materials_left`.
func _remaining_kits(band: Dictionary) -> int:
	return maxi(_signed_kits(band), 0)

func _remaining_materials(band: Dictionary) -> int:
	return maxi(_signed_materials(band), 0)

## ⛔ **THE UNCLAMPED REMAINDERS — what the card's own meter draws, negative included.** Every clamped
## reader above is written in terms of these, so the one place a negative can be seen is the one place
## it is asked about; a clamp applied before the question is what made an over-budget band read as
## finished.
##
## They are the whole window's currency, so they answer for a take as well as a grant: on a take the
## denominator is the home band's supply rather than a point budget, and the sign means the same
## thing either way.
func _signed_kits(band: Dictionary) -> int:
	var take := _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID
	var spent := 0
	if take:
		for units in _expanded_items(band, "").values():
			spent += int(units)
	else:
		for count in band.get(BAND_KIT_PICKS, {}).values():
			spent += int(count)
	return _kit_total(band, take) - spent

func _signed_materials(band: Dictionary) -> int:
	var spent := 0
	for units in band.get(BAND_MATERIAL_PICKS, {}).values():
		spent += int(units)
	return _material_total(band, _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID) - spent

func _push_attention() -> void:
	var rows := attention_rows()
	if rows == _attention_rows:
		return
	_attention_rows = rows
	attention_changed.emit(rows)

# ---- wiring -----------------------------------------------------------------

func _open_card() -> void:
	_ensure_panel()
	render()

func _ensure_panel() -> void:
	if _panel != null and is_instance_valid(_panel):
		return
	_panel = StartingLoadoutPanel.new()
	_panel.room_bounds = _room_bounds
	_host.add_child(_panel)
	_panel.dismissed.connect(_on_dismissed)
	_panel.reopened.connect(_on_reopened)
	_panel.band_selected.connect(_on_band_selected)
	_panel.kit_count_changed.connect(_on_kit_count_changed)
	_panel.material_units_changed.connect(_on_material_units_changed)

func _on_dismissed() -> void:
	collapse()

func _on_reopened() -> void:
	_open_card()

func _on_band_selected(band_id: int) -> void:
	open_band(band_id)

## **THE CLAMP LIVES HERE, and what it clamps against is the WINDOW's kind.** On a grant it is the
## budget: a kit's ceiling is whatever is left plus what it already holds, so pressing `+` on a spent
## budget is a no-op instead of an overspend the sim would reject whole. On a take it is the EXPANDED
## item supply — the same arithmetic the server refuses on, which is the only form that is right when
## two kits share an item.
func _on_kit_count_changed(kit_id: String, count: int) -> void:
	var band := _subject_state()
	if band.is_empty():
		return
	var picks: Dictionary = band[BAND_KIT_PICKS]
	var current := int(picks.get(kit_id, 0))
	var ceiling := _take_kit_ceiling(band, kit_id) \
		if _parent_of(band) != HudLoadoutVocab.GRANT_PARENT_BAND_ID \
		else current + kits_left()
	_write_pick(band, BAND_KIT_PICKS, kit_id, clampi(count, 0, ceiling))

func _on_material_units_changed(material_id: String, units: int) -> void:
	var band := _subject_state()
	if band.is_empty():
		return
	_write_pick(band, BAND_MATERIAL_PICKS, material_id,
		clampi(units, 0, _material_ceiling(band, material_id)))

## ⛔ **ONE ROW MOVES, THE WHOLE ORDER GOES, AND THE TWO HAPPEN TOGETHER.** This is the only writer of
## either pick map outside `_adopt_published`, which is what makes the invariant checkable: a local
## value is never written except beside the command that orders it.
##
## **A press the clamp refused writes and sends NOTHING.** The allocation is unchanged, so there is no
## replacement to send — and an order nobody asked for would put an entry in `BAND_UNECHOED` for a
## frame that is going to restate what is already on screen.
##
## **A row taken to zero is ERASED**, so the map is the allocation — see `BAND_KIT_PICKS`.
func _write_pick(band: Dictionary, picks_key: String, id: String, next: int) -> void:
	var picks: Dictionary = band[picks_key]
	if int(picks.get(id, 0)) == next:
		return
	var revert_kits: Dictionary = (band[BAND_KIT_PICKS] as Dictionary).duplicate()
	var revert_materials: Dictionary = (band[BAND_MATERIAL_PICKS] as Dictionary).duplicate()
	if next > 0:
		picks[id] = next
	else:
		picks.erase(id)
	# **THE OPTIMISTIC WRITE IS ON SCREEN BEFORE THE SEND**, which is the whole point of it; a refused
	# send re-renders from `revert_order`. `is_expanded`, never `is_open`, for `_settle_subject`'s
	# reason: rendering a dismissed card puts it back on screen.
	if is_expanded():
		render()
	_push_attention()
	_send_order(_subject, revert_kits, revert_materials)

## Send ONE band's whole allocation as one line. **Never a diff** — the verb fails closed and whole
## server-side, so a partial order has no meaning; and an EMPTY tail is a real order (*hold nothing*),
## which is why nothing here special-cases it.
##
## The pre-press allocation rides along as the rollback handle (`REVERT_KITS`), and the order rides
## into `BAND_UNECHOED` so the frame restating it is recognised as this card's own echo.
func _send_order(band_id: int, revert_kits: Dictionary, revert_materials: Dictionary) -> void:
	var band: Dictionary = _bands.get(band_id, {})
	if band.is_empty():
		return
	var kit_picks: Dictionary = band[BAND_KIT_PICKS]
	var material_picks: Dictionary = band[BAND_MATERIAL_PICKS]
	(band[BAND_UNECHOED] as Array).append({
		HudLoadoutVocab.WINDOW_KITS_KEY: kit_picks.duplicate(),
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: material_picks.duplicate(),
	})
	var kits: Array = []
	for kit_variant in kit_picks.keys():
		kits.append({"id": String(kit_variant), "count": int(kit_picks[kit_variant])})
	var materials: Array = []
	for material_variant in material_picks.keys():
		materials.append({
			"id": String(material_variant), "units": int(material_picks[material_variant]),
		})
	set_starting_loadout_requested.emit({
		"faction": HudConst.PLAYER_FACTION_ID,
		# **THE DURABLE BAND ID, never `entity`** — every band has a window of its own now, so the
		# command names one positionally and a rollback-renumbered entity would resolve to nothing.
		"band_id": band_id,
		"kits": kits,
		"materials": materials,
		REVERT_KITS: revert_kits,
		REVERT_MATERIALS: revert_materials,
	})

## ⛔ **THE SEND DID NOT GO, SO THE PRESS DID NOT HAPPEN.** Reached by `has_method` from
## `Main._on_hud_set_starting_loadout` (through `HudLayer.revert_starting_loadout`) with the very
## payload that was emitted, which is where the outcome is known — `hud-modules.md` → "AN OPTIMISTIC
## WRITE NEEDS A ROLLBACK".
##
## It restores BOTH halves, because the order it undoes was the whole allocation; and it drops the
## entry `_send_order` just queued, so a frame restating the allocation this card did NOT manage to
## order is correctly read as the sim's and adopted. **The queued entry is the LAST one** — a signal
## is delivered synchronously, so nothing can have been sent between the append and this call.
##
## It re-renders for the same reason the write did: a card keeping a number it has stopped believing
## is the defect, one screen showing two answers.
func revert_order(payload: Dictionary) -> void:
	var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
	var band: Dictionary = _bands.get(band_id, {})
	if band.is_empty():
		return
	band[BAND_KIT_PICKS] = (payload.get(REVERT_KITS, {}) as Dictionary).duplicate()
	band[BAND_MATERIAL_PICKS] = (payload.get(REVERT_MATERIALS, {}) as Dictionary).duplicate()
	var unechoed: Array = band[BAND_UNECHOED]
	if not unechoed.is_empty():
		unechoed.pop_back()
	if is_expanded():
		render()
	_push_attention()
