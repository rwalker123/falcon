class_name StartingLoadoutController
extends RefCounted

## The OPENING LOADOUT cluster (issue #629) — the controller half of `StartingLoadoutPanel`: it owns
## the panel node, holds the published window state and the three catalogues the picker joins onto,
## HOLDS THE ALLOCATION, and turns the panel's five signals into the one command the window takes.
##
## Built on the `CraftingPanelController` / `KnowledgePanelController` idiom: `HudLayer` holds one as
## `_loadout`, hands it a HOST `Node` (a `RefCounted` cannot `add_child`) and the room the card is
## bounded by, keeps thin delegators for the entry points reached BY NAME, and RELAYS this
## controller's command signal onto its own so `Main` can format the line.
##
## ## THE ALLOCATION LIVES HERE AND NOWHERE ELSE
##
## The panel renders a payload and emits intents; every clamp, every remainder and every "how many
## could I make" is computed here. That is what makes the budgets un-overspendable without the panel
## knowing what a budget is — and it is why a re-render never loses the player's picks.
##
## ## THE WINDOW OPENS ITSELF ONCE, AND IS DISMISSIBLE AFTERWARDS
##
## It opens automatically on the FIRST frame where `open == true` — a player who has just watched a
## world generate should not have to find the screen — and after that the player owns whether the
## card is up. `_auto_opened` is what makes it once rather than every snapshot: re-opening a card the
## player put away, every turn, is the failure mode the fork panel's `_auto_opened_forks` already
## guards against.
##
## ## ⛔ AN APPLY IS A REPLACEMENT, SO A COMMIT IS NOT THE END OF ANYTHING
##
## **Committing does not shut the window** — only the turn advance does. The sim treats an apply as a
## replacement rather than an addition, so the order may be sent, revised and sent again as often as
## the player likes, and re-sending the SAME allocation is an ordinary act rather than an error path.
## Commit therefore sends the line and collapses the card so the player can look at the map; the
## picks are kept and the reopen pill and the orb's row both stay live.
##
## ⛔ **`open` IS THEREFORE NOT A SUCCESS SIGNAL, and this controller used to read it as one.** It
## held an `_awaiting_commit` flag and treated a still-open window on the next frame as a REFUSAL,
## which was right while an accepted order closed the window and is now the opposite of right: under
## replacement semantics every SUCCESSFUL commit leaves the window open, so that branch would post
## *"that order was refused"* after each one. The flag and its notice are gone. **A genuine refusal is
## not visible on this card at all** — the server only `warn!`s it to the log stream — and nothing here
## may go back to inferring one from `open`.

## Send the composed loadout — `set_starting_loadout <faction> [kit <id> <n>]... [material <id>
## <n>]...`. **It fails CLOSED and WHOLE server-side**, so the client sends the entire allocation in
## one line and never a diff.
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

# --- The published window (`opening_loadout`) ---
var _open: bool = false
var _kit_budget: int = 0
var _material_budget: int = 0
var _pickable: Array = []
var _craftable_recipe_ids: Array = []

# --- The catalogues the picker JOINS onto, both already published for other consumers ---
## The parsed `equipment_config_json` — the kit roster's one home.
var _equipment_config: Dictionary = {}
## The recipe book (`snapshot["recipes"]`), which is where a recipe's input costs and work live.
var _recipes: Array = []

# --- The player's picks ---
## `kit_id -> count`. Seeded from the published kit defaults the first time a window opens, exactly as
## the material column is — the profile suggests a starting kit spread and the player revises it.
var _kit_picks: Dictionary = {}
## `material_id -> units`. Seeded from the published defaults the first time a window opens.
var _material_picks: Dictionary = {}
var _defaults_seeded: bool = false

var _auto_opened: bool = false
## The rows last handed to the orb, so an unchanged half is not re-pushed — `set_knowledge_attention`
## records what a needless full-registry push costs.
var _attention_rows: Array = []

## A recipe with no inputs at all cannot be priced against a pile, so the column reads it as
## unreachable rather than as infinitely makeable. Nothing in the shipped book is such a recipe; this
## is what a hand-edited one would render as.
const UNPRICED_RECIPE_COUNT := 0

func setup(host: Node, room_bounds: Control = null) -> void:
	_host = host
	_room_bounds = room_bounds

# ---- ingest -----------------------------------------------------------------

## The window itself (`opening_loadout`). A non-Dictionary is ignored — a delta carries a section
## only when it changed, so absence means unchanged and never "the window vanished".
func set_window(state: Variant) -> void:
	if not (state is Dictionary):
		return
	var window: Dictionary = state
	var was_open := _open
	_open = bool(window.get(HudLoadoutVocab.OPEN_KEY, false))
	_kit_budget = int(window.get(HudLoadoutVocab.KIT_BUDGET_KEY, 0))
	_material_budget = int(window.get(HudLoadoutVocab.MATERIAL_BUDGET_KEY, 0))
	_pickable = window.get(HudLoadoutVocab.PICKABLE_MATERIALS_KEY, [])
	_craftable_recipe_ids = window.get(HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY, [])
	if not _defaults_seeded:
		_seed_defaults(window.get(HudLoadoutVocab.MATERIAL_DEFAULTS_KEY, []),
			window.get(HudLoadoutVocab.KIT_DEFAULTS_KEY, []))
	if not _open:
		# **THE TURN ADVANCED.** That is the ONLY thing that shuts this window — an accepted order
		# does not — so the whole surface goes and whatever was left of either budget is forfeit.
		close()
		_push_attention()
		return
	if not was_open and not _auto_opened:
		_auto_opened = true
		_open_card()
	# ⛔ **`is_expanded`, NOT `is_open`.** A DISMISSED picker is still "open" — the panel node is
	# visible, carrying the reopen pill — so a re-render gated on `is_open()` puts the card the player
	# just dismissed straight back on screen, on the very next snapshot and every one after it. That is
	# the whole point of the window being dismissible, undone by one accessor.
	elif is_expanded():
		render()
	else:
		# Dismissed, and the window is still open: keep the reopen pill live and the picks intact.
		_ensure_panel()
		_panel.collapse()
	_push_attention()

## The whole effective `EquipmentConfig`, serialized. Parsed HERE and nowhere else in the HUD — the
## kit roster has no typed wire field, and this blob is where it rides.
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

# ---- open / close -----------------------------------------------------------

func is_open() -> bool:
	return _panel != null and is_instance_valid(_panel) and _panel.is_open()

func is_expanded() -> bool:
	return _panel != null and is_instance_valid(_panel) and _panel.is_expanded()

## Show the full card. Reached by name from the preview harnesses and from the orb's row.
func open() -> void:
	if not _open:
		return
	_open_card()

## Put the CARD away and leave the reopen pill — the player wants to look at the map. The window is
## still open as far as the sim is concerned.
func collapse() -> void:
	if not _open or _panel == null or not is_instance_valid(_panel):
		return
	_panel.collapse()

## The whole surface goes. The window has shut, or the world was rebuilt.
func close() -> void:
	if _panel != null and is_instance_valid(_panel):
		_panel.close()

## A world rebuild: everything about the previous world's opening loadout is gone, picks included.
func reset_world_state() -> void:
	_open = false
	_auto_opened = false
	_defaults_seeded = false
	_kit_picks = {}
	_material_picks = {}
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
	if not _open:
		return
	_ensure_panel()
	var materials := _material_rows()
	_panel.render({
		StartingLoadoutPanel.PAYLOAD_KITS: _kit_rows(),
		StartingLoadoutPanel.PAYLOAD_MATERIALS: materials,
		StartingLoadoutPanel.PAYLOAD_RECIPES: _recipe_rows(materials),
		StartingLoadoutPanel.PAYLOAD_KIT_BUDGET: {
			StartingLoadoutPanel.BUDGET_SPENT: kits_spent(),
			StartingLoadoutPanel.BUDGET_TOTAL: _kit_budget,
		},
		StartingLoadoutPanel.PAYLOAD_MATERIAL_BUDGET: {
			StartingLoadoutPanel.BUDGET_SPENT: materials_spent(),
			StartingLoadoutPanel.BUDGET_TOTAL: _material_budget,
		},
	})

## COLUMN 1's rows — the kit roster out of the parsed equipment config, in the config's own order,
## with **the `none` kit dropped by its EMPTY `uses`** rather than by matching its id: a roster that
## renamed the carry-nothing entry would still be excluded, and a roster that gave it items would
## rightly start offering it.
func _kit_rows() -> Array:
	var rows: Array = []
	var roster: Variant = _equipment_config.get(HudLoadoutVocab.CONFIG_KITS_KEY, [])
	if not (roster is Array):
		return rows
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
		rows.append({
			"id": kit_id,
			"display_name": String(entry.get(HudLoadoutVocab.KIT_DISPLAY_NAME_KEY, kit_id)),
			# The config publishes a job as its raw id and no display name for it — every surface in
			# this client capitalizes such an id, and so does this one.
			"jobs_text": _joined_labels(entry.get(HudLoadoutVocab.KIT_JOBS_KEY, []),
				HudLoadoutVocab.KIT_JOBS_SEPARATOR, false),
			"uses_text": _joined_labels(uses, HudLoadoutVocab.KIT_USES_SEPARATOR, true),
			"count": int(_kit_picks.get(kit_id, 0)),
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

## COLUMN 2's rows — one per PICKABLE material, in the profile's own order, each carrying the swatch
## the legend and the recipe rows will draw. Resolving the colour ONCE, here, is what makes the three
## places it appears one key rather than three tables.
func _material_rows() -> Array:
	var rows: Array = []
	for index in range(_pickable.size()):
		var material_id := String(_pickable[index])
		if material_id.is_empty():
			continue
		rows.append({
			"id": material_id,
			"label": HudLoadoutVocab.material_label(material_id),
			"color": HudLoadoutVocab.swatch_color(index),
			"units": int(_material_picks.get(material_id, 0)),
		})
	return rows

## COLUMN 3's rows — **only the recipes the sim published as craftable**, priced against the pile.
##
## `count = floor(min over inputs of allocated[material] / required)`: how many of THIS one thing the
## whole pile could make. It is deliberately not a simultaneous build plan — two rows both reading
## `×2` are two answers to two separate questions, which is what `BUILDS_NOTE` says on screen.
##
## **Reachable first**, then by count descending, then in the recipe book's own order — a stable sort
## over the book, so a tie never reshuffles between renders.
func _recipe_rows(materials: Array) -> Array:
	if _craftable_recipe_ids.is_empty() or _recipes.is_empty():
		return []
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
			var held := float(int(_material_picks.get(material_id, 0)))
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

func kits_spent() -> int:
	var total := 0
	for count in _kit_picks.values():
		total += int(count)
	return total

func materials_spent() -> int:
	var total := 0
	for units in _material_picks.values():
		total += int(units)
	return total

func kits_left() -> int:
	return maxi(_kit_budget - kits_spent(), 0)

func materials_left() -> int:
	return maxi(_material_budget - materials_spent(), 0)

# ---- the orb's row ----------------------------------------------------------

## Producer — the opening loadout window. **ONE ROW FOR THE WHOLE TIME THE WINDOW IS OPEN**, spent or
## not: the card is dismissible and this row's `Open ▸` is the guaranteed way back to it, so a
## producer that fell silent once both budgets were clear would strand a player who had finished
## picking, put the card away, and then wanted to revise before ending the turn.
##
## **WHAT MOVES IS THE SEVERITY AND THE WORDING.** Both budgets clear ⇒ `ready`, and the row reads as
## done; anything unspent ⇒ `warn`, and it reads as a warning naming what is left. `ready` ranks BELOW
## `info`, so a satisfied loadout never takes the orb's accent off a real warning elsewhere, and it
## still paints the orb when it is the highest entry present.
##
## **NON-LOCATING** (an opening loadout is a faction fact and no hex holds it) and deliberately **NOT
## `blocking`** in either state: closing the window is the sim's business, so the `Advance ▸` footer
## stays live and this row only ever warns.
func attention_rows() -> Array:
	if not _open:
		return []
	var parts: Array[String] = []
	var kits := kits_left()
	if kits == 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_KITS_ONE)
	elif kits > 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_KITS_MANY % kits)
	var units := materials_left()
	if units == 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_UNITS_ONE)
	elif units > 1:
		parts.append(HudLoadoutVocab.ATTENTION_DETAIL_UNITS_MANY % units)
	var complete := parts.is_empty()
	return [{
		"kind": HudAttentionVocab.ATTENTION_KIND_OPENING_LOADOUT,
		# Never `critical` on the unspent arm either: nothing is being lost yet, and the row shares the
		# popover with starvation rows that genuinely are.
		"severity": HudAttentionVocab.ATTENTION_SEVERITY_READY if complete \
			else HudAttentionVocab.ATTENTION_SEVERITY_WARN,
		"label": HudLoadoutVocab.ATTENTION_LABEL_READY if complete \
			else HudLoadoutVocab.ATTENTION_LABEL_UNSPENT,
		"detail": HudLoadoutVocab.ATTENTION_DETAIL_READY if complete \
			else HudLoadoutVocab.ATTENTION_DETAIL_SEPARATOR.join(parts),
		"x": HudAttentionVocab.ATTENTION_NON_LOCATING,
		"y": HudAttentionVocab.ATTENTION_NON_LOCATING,
	}]

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
	_panel.kit_count_changed.connect(_on_kit_count_changed)
	_panel.material_units_changed.connect(_on_material_units_changed)
	_panel.commit_requested.connect(_on_commit_requested)

func _on_dismissed() -> void:
	collapse()

func _on_reopened() -> void:
	_open_card()

## **THE CLAMP LIVES HERE, and it is a clamp on the BUDGET rather than on the row.** A kit's own
## ceiling is whatever is left plus what it already holds, so pressing `+` on a spent budget is a
## no-op instead of an overspend the sim would reject whole.
func _on_kit_count_changed(kit_id: String, count: int) -> void:
	var current := int(_kit_picks.get(kit_id, 0))
	var ceiling := current + kits_left()
	_kit_picks[kit_id] = clampi(count, 0, ceiling)
	render()
	_push_attention()

func _on_material_units_changed(material_id: String, units: int) -> void:
	var current := int(_material_picks.get(material_id, 0))
	var ceiling := current + materials_left()
	_material_picks[material_id] = clampi(units, 0, ceiling)
	render()
	_push_attention()

## Send the whole allocation as one line and collapse the card, so the player can look at the map.
##
## **THE ORDER MAY BE SENT AGAIN, AND SENDING THE SAME ONE TWICE IS NOT AN ERROR** — an apply is a
## replacement, so nothing here tracks whether a commit is the first. The picks stay exactly as they
## are and the reopen pill brings the card back for a revision.
##
## A zero row is dropped: the grammar's empty tail is a real order (*spend nothing*), so naming a kit
## with a count of zero would only be a longer way of saying the same thing.
func _on_commit_requested() -> void:
	var kits: Array = []
	for kit_id in _kit_picks.keys():
		var count := int(_kit_picks[kit_id])
		if count > 0:
			kits.append({"id": String(kit_id), "count": count})
	var materials: Array = []
	for material_id in _material_picks.keys():
		var units := int(_material_picks[material_id])
		if units > 0:
			materials.append({"id": String(material_id), "units": units})
	set_starting_loadout_requested.emit({
		"faction": HudConst.PLAYER_FACTION_ID,
		"kits": kits,
		"materials": materials,
	})
	collapse()

## Seed BOTH columns from the profile's published defaults — **once per world**, so a delta re-stating
## them cannot overwrite what the player has since chosen.
##
## ⛔ **THE COUNTS GO IN AS PUBLISHED, neither clamped nor summed against their budget.** The sim
## already fitted the kit spread to `kit_budget` (a head count it knows and the profile does not) and
## scales it proportionally when it binds; a second clamp here would disagree with the first, and the
## player would see a pre-fill the sim did not send.
##
## A material the profile names that is not PICKABLE is dropped — it could not be spent and would
## strand part of the budget. **The kit half needs no such filter**: the roster it is drawn against is
## the published equipment config, and a kit absent from that roster simply renders no row, so an
## unknown id costs a dictionary entry nobody reads rather than a phantom control.
func _seed_defaults(materials: Variant, kits: Variant) -> void:
	_defaults_seeded = true
	if materials is Array:
		var pickable: Dictionary = {}
		for id_variant in _pickable:
			pickable[String(id_variant)] = true
		for entry_variant in materials:
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant
			var material_id := String(entry.get(HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY, ""))
			if material_id.is_empty() or not pickable.has(material_id):
				continue
			_material_picks[material_id] = int(
				entry.get(HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY, 0))
	if kits is Array:
		for entry_variant in kits:
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant
			var kit_id := String(entry.get(HudLoadoutVocab.KIT_DEFAULT_ID_KEY, ""))
			if kit_id.is_empty():
				continue
			_kit_picks[kit_id] = int(entry.get(HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY, 0))
