extends RefCounted

## THE OPENING LOADOUT PICKER (issue #629) — the turn-one outfitting window: two budgets, a pick
## list, and the readout that says what the pile is worth.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists
## it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter
## moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`. It runs LAST and
## ends by publishing a SHUT window, so the surface it stands up is gone before anything else could
## inherit it.
##
## ## MOST OF THIS CHAPTER IS ASSERTIONS, AND THAT IS THE POINT OF IT
##
## Every claim the third column makes renders as a perfectly plausible picture whatever it says. A
## row reading `×3`, a dash on a row that should read `×1`, a knowledge-gated bench tool quietly
## present — a screenshot cannot tell a correct one from a wrong one. So the arithmetic is asked of
## the rendered CONTROLS (by meta, never by scraping a subtree's text) and the frames carry the
## layout.
##
## ## THE THREE THINGS ONLY A FRAME CAN SHOW
##
## That the three columns fit side by side at the shipped width; that the swatch legend and the
## recipe rows draw the SAME five inks; and that the dismissed state leaves a reopen control on
## screen rather than nothing at all.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 25

const Q := preload("res://tools/ui_preview/node_query.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# ---- the window the sim publishes ---------------------------------------------------------------

## One kit per working-age hand of the shipped 30-person band — the sim DERIVES this, so a fixture
## states it rather than inventing a dial.
const KIT_BUDGET := 17
## `start_profiles.json` `opening_loadout.material_points`.
const MATERIAL_BUDGET := 30
## The shipped pick list, in the profile's own order — which is also the draw order of the resources
## column and of the legend above the recipe list.
const PICKABLE := ["bone", "fibre", "hide", "wood", "stone"]
## …and the pre-fill it opens on. Sums to 28 of 30, deliberately NOT to the budget: a fixture that
## opened with nothing left could not tell a working meter from one stuck at zero.
const DEFAULT_BONE := 3
const DEFAULT_FIBRE := 17
const DEFAULT_HIDE := 8
const DEFAULTS_TOTAL := DEFAULT_BONE + DEFAULT_FIBRE + DEFAULT_HIDE

## **THE GATED BENCH TOOL, PRESENT IN THE RECIPE BOOK AND ABSENT FROM `craftable_recipe_ids`.** It is
## the whole reason the list of ids is published: a client that filtered on anything else — a refusal
## sentence, a `requires_knowledge` array it re-tested itself — would show this row.
const RECIPE_GATED := "tanning_frame"

const RECIPE_SPEARS := "spears"
const RECIPE_BASKETS := "baskets"
const RECIPE_TRAPS := "traps"
const RECIPE_SLED := "sled"
const RECIPE_CROOK := "crook"
## Costs 3 WOOD, and the default pile holds none — the unreachable row, which must be DIMMED and
## present rather than filtered away.
const RECIPE_EARTHMOVING := "earthmoving"

## What each row must read against the default pile (bone 3, fibre 17, hide 8, wood 0):
## `floor(min over inputs of held / required)`.
const EXPECTED_SPEARS := 3     # bone 1 → 3, fibre 2 → 8, hide 1 → 8
const EXPECTED_BASKETS := 3    # fibre 5 → 3, hide 1 → 8
const EXPECTED_TRAPS := 2      # fibre 6 → 2, bone 1 → 3
const EXPECTED_SLED := 1       # hide 6 → 1, fibre 2 → 8
const EXPECTED_CROOK := 3      # bone 1 → 3, fibre 2 → 8
const EXPECTED_EARTHMOVING := 0

## The kit the chapter buys, and how many of it.
const KIT_STALKING := "big_game"
const KIT_PRESSES := 3
## The roster entry that must NEVER be offered — it grants nothing, which is how the picker knows.
const KIT_NONE := "none"

## A stepper loop's ceiling. It is a GUARD, not the expected count: a `+` that stopped working would
## otherwise spin this chapter until the watchdog killed the whole run.
const STEPPER_PRESS_LIMIT := 64

const STEPPER_PLUS_FACE := "+"

func run(harness) -> void:
	h = harness
	# The two catalogues the picker JOINS onto, pushed through the seams `Main` really uses. Neither
	# is part of the loadout section: the kit roster rides the equipment config blob and a recipe's
	# input costs ride the recipe book, and the picker reads both rather than a second copy.
	h._hud.update_equipment_config(JSON.stringify(_equipment_config()))
	h._hud.update_crafting_catalogues(null, null, _recipes(), [])

	# **THE WINDOW OPENS ITSELF.** Nothing below asks it to — the first frame carrying `open` is what
	# stands the card up, which is the behaviour a player meets after watching a world generate.
	h._hud.update_opening_loadout(_window())
	await h._settle()
	_assert_opened_itself()
	_assert_kits_start_at_zero()
	_assert_defaults_seeded()
	_assert_gated_recipe_is_absent()
	_assert_recipe_counts()
	_assert_reachable_first()
	_assert_orb_row()
	await h._save("starting_loadout")

	await _pick_a_kit()
	await h._save("starting_loadout_picked")

	await _spend_both_budgets()
	await h._save("starting_loadout_spent")

	await _dismiss_and_reopen()
	await _commit_and_be_refused()
	_assert_window_shuts()

# ---- the opening state ------------------------------------------------------

func _controller() -> StartingLoadoutController:
	return h._hud.starting_loadout_panel()

func _panel() -> StartingLoadoutPanel:
	return _controller().panel()

func _assert_opened_itself() -> void:
	h._assert_hud("loadout — the picker opens ITSELF on the first frame the window is open",
		_controller().is_expanded())

## **EVERY KIT STARTS AT 0**, and the carry-nothing entry is not on the list at all. A roster row is
## found by its own meta rather than by its face: the face is a config string, so a text match would
## only confirm the fixture back to itself.
func _assert_kits_start_at_zero() -> void:
	var rows := _rows(HudLoadoutVocab.KIT_ROW_META)
	h._assert_hud("loadout — every kit row starts at 0 (%d rows, %d kits spent)"
			% [rows.size(), _controller().kits_spent()],
		not rows.is_empty() and _controller().kits_spent() == 0)
	h._assert_hud("loadout — the `%s` kit is not offered (it grants nothing)" % KIT_NONE,
		not rows.has(KIT_NONE))
	h._assert_hud("loadout — the whole kit budget is still there (%d of %d)"
			% [_controller().kits_left(), KIT_BUDGET],
		_controller().kits_left() == KIT_BUDGET)

## …and the resources column opens on the PROFILE'S DEFAULTS rather than on zero. It is the one
## column that does, and a picker that opened both at zero would look identical in a screenshot.
func _assert_defaults_seeded() -> void:
	var rows := _rows(HudLoadoutVocab.MATERIAL_ROW_META)
	h._assert_hud("loadout — one row per pickable material, in the profile's order (%s)"
			% str(rows),
		rows == PICKABLE)
	h._assert_hud("loadout — the pile opens on the published defaults (%d of %d spent)"
			% [_controller().materials_spent(), MATERIAL_BUDGET],
		_controller().materials_spent() == DEFAULTS_TOTAL)

## **THE KNOWLEDGE-GATED BENCH TOOL IS NOT DRAWN AT ALL** — not greyed, not in a locked group. It is
## in the recipe book this chapter pushed and off the published craftable list, so a client filtering
## on anything but that list fails here.
func _assert_gated_recipe_is_absent() -> void:
	var rows := _rows(HudLoadoutVocab.RECIPE_ROW_META)
	h._assert_hud("loadout — the gated `%s` has no row at all (%d rows drawn)"
			% [RECIPE_GATED, rows.size()],
		not rows.has(RECIPE_GATED))

## The column's whole arithmetic, read off the rendered count labels. `×N` answers *"how many if the
## WHOLE pile went on this one thing"*, so the rows do not have to sum to anything.
func _assert_recipe_counts() -> void:
	for expectation in [
		[RECIPE_SPEARS, EXPECTED_SPEARS], [RECIPE_BASKETS, EXPECTED_BASKETS],
		[RECIPE_TRAPS, EXPECTED_TRAPS], [RECIPE_SLED, EXPECTED_SLED],
		[RECIPE_CROOK, EXPECTED_CROOK], [RECIPE_EARTHMOVING, EXPECTED_EARTHMOVING],
	]:
		var recipe_id := String(expectation[0])
		var want := int(expectation[1])
		var got := _recipe_count(recipe_id)
		h._assert_hud("loadout — %s reads ×%d against the default pile (got %d)"
				% [recipe_id, want, got], got == want)
	# …and the unreachable row is DIMMED rather than dropped: it is the row that says the pile is
	# short of wood, which is the most useful thing the column can tell a player.
	var row := _row_node(HudLoadoutVocab.RECIPE_ROW_META, RECIPE_EARTHMOVING)
	h._assert_hud("loadout — the unreachable %s row is present and dimmed" % RECIPE_EARTHMOVING,
		row != null and row.modulate.a < 1.0)

func _assert_reachable_first() -> void:
	var rows := _rows(HudLoadoutVocab.RECIPE_ROW_META)
	var last_seen := -1
	var ordered := true
	for recipe_id in rows:
		var count := _recipe_count(String(recipe_id))
		if last_seen >= 0 and count > last_seen:
			ordered = false
		last_seen = count
	h._assert_hud("loadout — reachable rows sort first (%s)" % str(rows), ordered)

## The orb carries the window as a NON-BLOCKING warn row. Blocking it would make the client hold a
## turn the sim is perfectly willing to advance.
func _assert_orb_row() -> void:
	var rows := _controller().attention_rows()
	var row: Dictionary = rows[0] if not rows.is_empty() else {}
	h._assert_hud("loadout — the orb carries one non-blocking row for the open window (%s)"
			% str(row.get("detail", "")),
		rows.size() == 1 and not bool(row.get("blocking", false))
			and String(row.get("kind", "")) == HudAttentionVocab.ATTENTION_KIND_OPENING_LOADOUT)

# ---- driving the steppers ---------------------------------------------------

## Press the Stalking kit's `+` through the REAL button. The panel rebuilds on every press, so the
## button is re-found each time — a cached node here would be pressing a freed control.
func _pick_a_kit() -> void:
	for _i in range(KIT_PRESSES):
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	await h._settle()
	h._assert_hud("loadout — %d presses buy %d kits, and the meter says so (%d left of %d)"
			% [KIT_PRESSES, _controller().kits_spent(), _controller().kits_left(), KIT_BUDGET],
		_controller().kits_spent() == KIT_PRESSES
			and _controller().kits_left() == KIT_BUDGET - KIT_PRESSES)
	# The commit control states the CONSEQUENCE while either budget has something left in it.
	h._assert_hud("loadout — the commit control names the forfeit (%s)" % _controller().commit_label(),
		_controller().commit_label().begins_with(
			HudLoadoutVocab.COMMIT_FORFEIT_FORMAT.split("%s")[0]))

## Spend BOTH budgets to the last unit, then press once more. The extra press is the point: the
## controller clamps against the budget, so the `+` is disabled and nothing can be overspent — the
## sim rejects an overspent order WHOLE, so a client that let one be composed would throw the picks
## away on send.
func _spend_both_budgets() -> void:
	var presses := 0
	while _controller().kits_left() > 0 and presses < STEPPER_PRESS_LIMIT:
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
		presses += 1
	while _controller().materials_left() > 0 and presses < STEPPER_PRESS_LIMIT:
		_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
		presses += 1
	await h._settle()
	h._assert_hud("loadout — both budgets are spent to the unit (%d kits, %d units left)"
			% [_controller().kits_left(), _controller().materials_left()],
		_controller().kits_left() == 0 and _controller().materials_left() == 0)
	var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	h._assert_hud("loadout — `+` is disabled once the budget is gone",
		plus != null and plus.disabled)
	h._assert_hud("loadout — with nothing unspent the commit control simply confirms (%s)"
			% _controller().commit_label(),
		_controller().commit_label() == HudLoadoutVocab.COMMIT_CLEAR_LABEL)

# ---- dismiss, reopen, refuse ------------------------------------------------

## **THE WINDOW IS DISMISSIBLE AND THE PICKS SURVIVE IT.** A player has to be able to pan, zoom and
## read a tile before committing, and the reopen pill is what brings the card back.
func _dismiss_and_reopen() -> void:
	_controller().collapse()
	await h._settle()
	var pill := _panel().reopen_pill()
	h._assert_hud("loadout — dismissed leaves a live reopen control on screen",
		not _controller().is_expanded() and pill != null and pill.visible)
	await h._save("starting_loadout_dismissed")
	# ⛔ **THE SNAPSHOT MUST NOT PUT THE CARD BACK.** Every frame carries the window's section while it
	# is open, so a re-render gated on *is the surface up* rather than *is the CARD up* re-expands the
	# picker the player just dismissed, on the very next snapshot and every one after it — which makes
	# the window undismissible while looking, in any single frame, exactly right.
	h._hud.update_opening_loadout(_window())
	await h._settle()
	h._assert_hud("loadout — a snapshot does NOT re-open a card the player dismissed",
		not _controller().is_expanded() and _controller().is_open())
	var spent := _controller().kits_spent()
	pill.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — reopening keeps every pick (%d kits)" % _controller().kits_spent(),
		_controller().is_expanded() and _controller().kits_spent() == spent)

## Commit, then publish a frame that STILL says `open`. That is the sim refusing the order — it fails
## whole and changes nothing — and the picker must come back saying so rather than leaving the player
## believing they are outfitted.
func _commit_and_be_refused() -> void:
	var commit := Q.find_meta_node(_panel(), HudLoadoutVocab.COMMIT_BUTTON_META) as Button
	h._assert_hud("loadout — the card carries a commit control", commit != null)
	commit.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — committing puts the card away optimistically",
		not _controller().is_expanded())
	h._hud.update_opening_loadout(_window())
	await h._settle()
	h._assert_hud("loadout — a window still open after a commit re-opens the card on the refusal",
		_controller().is_expanded())
	h._assert_hud("loadout — and the refusal is stated on the card",
		Q.has_label_containing(_panel(), StartingLoadoutController.REFUSAL_NOTICE))
	await h._save("starting_loadout_refused")

## The turn advanced: the sim says the window has shut, and the whole surface goes with it.
func _assert_window_shuts() -> void:
	var shut := _window()
	shut[HudLoadoutVocab.OPEN_KEY] = false
	h._hud.update_opening_loadout(shut)
	h._assert_hud("loadout — a shut window takes the whole surface off screen",
		not _controller().is_open())
	h._assert_hud("loadout — …and the orb's row goes with it",
		_controller().attention_rows().is_empty())

# ---- lookups ----------------------------------------------------------------

## Every row carrying `meta`, as the VALUES it was stamped with — the ids, in draw order. Asked of
## the rendered controls rather than of the payload, so the assertion is about what is on screen.
func _rows(meta: StringName) -> Array:
	var ids: Array = []
	_collect(_panel(), meta, ids)
	return ids

func _collect(node: Node, meta: StringName, into: Array) -> void:
	if node == null:
		return
	if node is Control and (node as Control).has_meta(meta):
		into.append(String((node as Control).get_meta(meta)))
	for child in node.get_children():
		_collect(child, meta, into)

func _row_node(meta: StringName, id: String) -> Control:
	return _find_row(_panel(), meta, id)

func _find_row(node: Node, meta: StringName, id: String) -> Control:
	if node == null:
		return null
	if node is Control and (node as Control).has_meta(meta) \
			and String((node as Control).get_meta(meta)) == id:
		return node as Control
	for child in node.get_children():
		var found := _find_row(child, meta, id)
		if found != null:
			return found
	return null

## A recipe row's count, read off the label's own meta rather than off its text: the face is `×3` or
## a dash, and parsing either back into a number would be re-implementing the renderer to check it.
func _recipe_count(recipe_id: String) -> int:
	var row := _row_node(HudLoadoutVocab.RECIPE_ROW_META, recipe_id)
	if row == null:
		return -1
	var label := Q.find_meta_node(row, HudLoadoutVocab.RECIPE_COUNT_META)
	return int(label.get_meta(HudLoadoutVocab.RECIPE_COUNT_META)) if label != null else -1

func _plus_button(meta: StringName, id: String) -> Button:
	var row := _row_node(meta, id)
	return Q.find_button_by_text(row, STEPPER_PLUS_FACE) if row != null else null

func _press_plus(meta: StringName, id: String) -> void:
	var plus := _plus_button(meta, id)
	if plus != null and not plus.disabled:
		plus.pressed.emit()

# ---- fixtures ---------------------------------------------------------------

func _window() -> Dictionary:
	return {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.KIT_BUDGET_KEY: KIT_BUDGET,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: MATERIAL_BUDGET,
		HudLoadoutVocab.PICKABLE_MATERIALS_KEY: PICKABLE.duplicate(),
		HudLoadoutVocab.MATERIAL_DEFAULTS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "bone",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_BONE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "fibre",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_FIBRE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "hide",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_HIDE},
		],
		HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY: [
			RECIPE_SLED, RECIPE_CROOK, RECIPE_BASKETS, RECIPE_TRAPS, RECIPE_SPEARS,
			RECIPE_EARTHMOVING,
		],
	}

## The shipped kit roster's shape, `none` included — the picker has to drop it, so the fixture has to
## offer it.
func _equipment_config() -> Dictionary:
	return {HudLoadoutVocab.CONFIG_KITS_KEY: [
		_kit(KIT_STALKING, "Stalking kit", ["hunt"], ["spears", "sled"]),
		_kit("trapping", "Trapping kit", ["hunt"], ["traps", "sled"]),
		_kit("gathering", "Harvesting kit", ["forage"], ["baskets"]),
		_kit("hurdling", "Hurdling kit", ["builders", "husbandry"], ["crook"]),
		_kit("roadbuilding", "Roadbuilding kit", ["builders", "roadwork"], ["earthmoving"]),
		_kit(KIT_NONE, "No kit", ["hunt", "forage"], []),
	]}

func _kit(id: String, display_name: String, jobs: Array, uses: Array) -> Dictionary:
	return {
		HudLoadoutVocab.KIT_ID_KEY: id,
		HudLoadoutVocab.KIT_DISPLAY_NAME_KEY: display_name,
		HudLoadoutVocab.KIT_JOBS_KEY: jobs,
		HudLoadoutVocab.KIT_USES_KEY: uses,
	}

## The recipe book, in the wire's own shape — the shipped costs, transcribed, plus the gated tool
## that must not be drawn.
func _recipes() -> Array:
	return [
		_recipe(RECIPE_SLED, "Sled", 8.0, {"hide": 6.0, "fibre": 2.0}),
		_recipe(RECIPE_CROOK, "Crook", 5.0, {"bone": 1.0, "fibre": 2.0}),
		_recipe(RECIPE_BASKETS, "Baskets", 6.0, {"fibre": 5.0, "hide": 1.0}),
		_recipe(RECIPE_TRAPS, "Traps", 6.0, {"fibre": 6.0, "bone": 1.0}),
		_recipe(RECIPE_SPEARS, "Spears", 6.0, {"bone": 1.0, "fibre": 2.0, "hide": 1.0}),
		_recipe(RECIPE_EARTHMOVING, "Earthmoving tools", 8.0, {"wood": 3.0, "bone": 2.0}),
		_recipe(RECIPE_GATED, "Tanning frame", 12.0, {"fibre": 8.0, "bone": 2.0}),
	]

func _recipe(id: String, display_name: String, work: float, inputs: Dictionary) -> Dictionary:
	var rows: Array = []
	for material_id in inputs.keys():
		rows.append({
			HudLoadoutVocab.RECIPE_INPUT_MATERIAL_ID_KEY: String(material_id),
			HudLoadoutVocab.RECIPE_INPUT_AMOUNT_KEY: float(inputs[material_id]),
		})
	return {
		HudLoadoutVocab.RECIPE_ID_KEY: id,
		HudLoadoutVocab.RECIPE_DISPLAY_NAME_KEY: display_name,
		HudLoadoutVocab.RECIPE_WORK_KEY: work,
		HudLoadoutVocab.RECIPE_INPUTS_KEY: rows,
	}
