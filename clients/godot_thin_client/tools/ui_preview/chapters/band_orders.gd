extends RefCounted

## **BAND ORDERS -- THE SHEETS** (#860, `.claude/rules/client/band-orders.md`): the stone and wood sheets
## as the chief's order and as the player's, the Food and Fodder sheets (below and above target), and
## the hunt / gather sheets for a row the chief runs and for one the player does. The Work tab's
## orders group is judged in `band_panel_preview` (`tools/band_panel_orders.gd`).
##
## Every claim reads the REAL sheet: a button is found by the meta it wears and pressed, and the line
## `Main.format_band_order` renders from the payload it emitted is compared with the text the command
## grammar owns. Appended LAST so no earlier frame moves.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const WorkingsCh := preload("res://tools/ui_preview/chapters/workings.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

const EXPECTED_CHECKPOINTS := 30

var h
var _workings

## The band's entity and the chief's name.
const LEADER_NAME := "Orrin"
const WORKERS := 34
const HERD_ID := "game_deer_07"
const HAY_X := 70
const HAY_Y := 19
const HARVEST_X := 71
const HARVEST_Y := 18

var _sent: Array = []

func run(harness) -> void:
	h = harness
	_workings = WorkingsCh.new()
	h._hud.band_order_requested.connect(func(payload: Dictionary) -> void: _sent.append(payload))
	h._hud.update_deposit_rungs(_workings._deposit_catalog())
	h._hud.update_ladder_knowledge(_workings._deposit_knowledge_roster())
	await _stone_sheets()
	await _wood_sheets()
	await _food_sheets()
	await _hunt_sheets()
	h._hud._drawercompose.close_compose_sheet()
	await h._settle()

# ---- Fixtures ------------------------------------------------------------------------------------

func _order(kind: String, target: int, priority: String, owned: bool, draw: float, on_hand: float,
		hold: Array, refill: Array, take: float, sustainable: float = 0.0) -> Dictionary:
	return {"kind": kind, "target": target, "priority": priority, "chief_owned": owned, "draw": draw,
		"on_hand": on_hand, "has_site": true, "site_x": _workings.WORKING_TILE_X,
		"site_y": _workings.WORKING_TILE_Y, "sustainable": sustainable, "has_consumers": true,
		"hold_heads": hold[0], "hold_wanted": hold[1], "refill_heads": refill[0],
		"refill_wanted": refill[1], "take": take}

func _row(kind: String, x: int, y: int, workers: int, order_kind: String, chief: bool,
		extra: Dictionary = {}) -> Dictionary:
	var row := {"kind": kind, "workers": workers, "target_x": x, "target_y": y, "x": x, "y": y,
		"order_kind": order_kind if chief else "", "chief_owned": chief, "floor": 0.5,
		"has_yield": true, "kit_id": "", "max_workers": 0, "hold_crew": 1, "reach_crew": workers,
		"kitted_staffed": workers / 2, "bare_staffed": workers - workers / 2, "useful_crew": workers}
	row.merge(extra, true)
	return row

## Orrin's band, standing on the working's hex, with the four orders and the rows each runs.
func chief_band(stone_owned: bool = true, wood_owned: bool = true, food_owned: bool = true,
		on_hand_food: float = 12.0, wood_sustainable: float = 4.0, food_rows_chief: bool = true) -> Dictionary:
	var band: Dictionary = _workings._band_at_the_working()
	band["leader_kind"] = 1
	band["leader_name"] = LEADER_NAME
	band["working_age"] = WORKERS
	band["material_store"] = [{"material_id": "stone", "amount": 12.4}, {"material_id": "wood", "amount": 8.0}]
	var food_refill := 4 if on_hand_food < 20.0 else 0
	band["band_orders"] = [
		_order("food", 20, "high", food_owned, 4.0, on_hand_food, [8, 8], [food_refill, food_refill], 6.0),
		_order("fodder", 10, "normal", food_owned, 2.0, 6.0, [2, 2], [2, 2], 3.0),
		_order("stone", 30, "normal", stone_owned, 1.0, 12.4, [2, 2], [3, 3], 2.0),
		_order("wood", 30, "low", wood_owned, 1.5, 8.0, [3, 3], [7, 7], 2.2, wood_sustainable),
	]
	band["labor_assignments"] = [
		_row("forage", HARVEST_X, HARVEST_Y, 6, "food", food_rows_chief, {"actual_yield": 2.4,
			"sustainable_yield": 2.4, "fodder_yield": 0.25, "material_yield": [{"material_id": "fibre", "amount": 0.05},
			{"material_id": "hide", "amount": 0.0}]}),
		_row("hunt", 70, 17, 8, "food", food_rows_chief, {"fauna_id": HERD_ID, "herd_id": HERD_ID,
			"actual_yield": 4.4, "sustainable_yield": 4.4, "material_yield": [{"material_id": "hide", "amount": 0.4},
			{"material_id": "bone", "amount": 0.2}]}),
		_row("forage", HAY_X, HAY_Y, 4, "fodder", food_rows_chief, {"actual_yield": 0.06, "fodder_yield": 0.4,
			"sustainable_yield": 0.06}),
		_row("extract", _workings.WORKING_TILE_X, _workings.WORKING_TILE_Y, 5, "stone", stone_owned,
			{"material": "stone", "fauna_id": "", "material_yield": [{"material_id": "stone", "amount": 2.0}]}),
		_row("extract", _workings.WORKING_TILE_X, _workings.WORKING_TILE_Y, 3, "wood", wood_owned,
			{"material": "wood", "fauna_id": "", "material_yield": [{"material_id": "wood", "amount": 1.4}]}),
		_row("extract", 70, 17, 2, "wood", wood_owned,
			{"material": "wood", "fauna_id": "", "material_yield": [{"material_id": "wood", "amount": 0.8}]}),
		{"kind": "scout", "workers": 2},
	]
	return BandFx.with_band_id(band)

func _push(band: Dictionary) -> void:
	h._hud.update_band_alerts([band])

func _pens() -> Array:
	var pens: Array = []
	for i in range(2):
		var pen: Dictionary = HerdFx.starving_pen_herd_fixture()
		pen["id"] = "pen_%d" % i
		pen["has_owner"] = true
		pen["owner"] = 0
		pen["x"] = 66 + 3 * i
		pen["y"] = 18 + 2 * i
		pen["pen_pasture_fraction"] = 0.4 + 0.2 * float(i)
		pens.append(pen)
	# A TAMED herd (not penned) that draws fodder shares the list; one its range feeds outright does not.
	var tamed: Dictionary = HerdFx.fully_tamed_herd_fixture()
	tamed["id"] = "tamed_0"
	tamed["has_owner"] = true
	tamed["owner"] = 0
	tamed["x"] = 63
	tamed["y"] = 17
	tamed["pen_pasture_fraction"] = 0.7
	tamed["fodder_draw"] = 2.5
	tamed["pen_fodder_shortfall"] = 0.5
	pens.append(tamed)
	var fed_by_range: Dictionary = HerdFx.fully_tamed_herd_fixture()
	fed_by_range["id"] = "tamed_1"
	fed_by_range["has_owner"] = true
	fed_by_range["owner"] = 0
	fed_by_range["x"] = 60
	fed_by_range["y"] = 15
	fed_by_range["pen_pasture_fraction"] = 1.0
	fed_by_range["fodder_draw"] = 0.0
	fed_by_range["pen_fodder_shortfall"] = 0.0
	pens.append(fed_by_range)
	return pens

func _sheet() -> Node:
	return h._hud._drawercompose._compose_sheet

func _find_meta(node: Node, meta: StringName) -> Node:
	if node.has_meta(meta) and not node.is_queued_for_deletion():
		return node
	for child in node.get_children():
		if child.is_queued_for_deletion():
			continue
		var found := _find_meta(child, meta)
		if found != null:
			return found
	return null

func _texts(node: Node, out: Array = []) -> Array:
	if node.is_queued_for_deletion():
		return out
	if node is Label:
		out.append((node as Label).text)
	elif node is RichTextLabel:
		out.append((node as RichTextLabel).get_parsed_text())
	elif node is Button:
		out.append((node as Button).text)
	for child in node.get_children():
		_texts(child, out)
	return out

func _has_text(needle: String) -> bool:
	for t in _texts(_sheet()):
		if String(t).contains(needle):
			return true
	return false

func _press(meta: StringName) -> bool:
	var button := _find_meta(_sheet(), meta)
	if button == null or not (button is Button):
		return false
	(button as Button).pressed.emit()
	return true

## The text line the last payload renders to.
func _last_line() -> String:
	if _sent.is_empty():
		return ""
	return String(MAIN_SCRIPT.format_band_order(_sent[_sent.size() - 1]).get("line", ""))

func _open_deposit(working: Dictionary) -> void:
	h._hud._drawercompose.close_compose_sheet()
	h._hud._drawercompose.open_deposit_compose(working)

# ---- Stone ----------------------------------------------------------------------------------------

func _stone_sheets() -> void:
	var band := chief_band()
	var bid := int(band["band_id"])
	_push(band)
	_open_deposit(_workings._stone_working(_workings.STONE_TAKE))
	await h._settle()
	await h._save("band_orders_stone_chief")
	h._assert_hud("the stone sheet as the chief's order names him and the material",
		_has_text("Orrin keeps stone on hand"))
	h._assert_hud("…and says what he sends", _has_text("Orrin sends 5 diggers this turn"))
	h._assert_hud("…and the kitted/bare split", _has_text("2 with sleds · 3 bare-handed"))
	h._assert_hud("…and the ETA", _has_text("Back to 30 in ≈"))
	h._assert_hud("…with no Assign diggers eyebrow or commit row",
		not _has_text("Stop working here") and _find_meta(_sheet(), HudWidgets.COMPOSE_COMMIT_META) == null)
	_sent.clear()
	h._assert_hud("the − button is live", _press(BandOrdersSheets.META_STEPPER_LESS))
	h._assert_hud("…and sends `set 29 normal` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d stone set 29 normal" % bid)
	_sent.clear()
	_press(BandOrdersSheets.META_STEPPER_MORE)
	h._assert_hud("the + button sends `set 31 normal` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d stone set 31 normal" % bid)
	_sent.clear()
	var high := _find_priority_button("high")
	h._assert_hud("the Priority picker is there", high != null)
	if high != null:
		high.pressed.emit()
	h._assert_hud("…and High sends `set 30 high` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d stone set 30 high" % bid)
	_sent.clear()
	_press(BandOrdersSheets.META_RELEASE)
	h._assert_hud("Crew it yourself sends `release` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d stone release" % bid)

	# Opened from the ORDER LINE: the panel selects the order's site tile first (the harness has no map,
	# so the focus is stood in for by showing that tile), and the sheet is the site's own.
	var stone: Dictionary = _workings._stone_working(_workings.STONE_TAKE)
	h._hud.update_deposits([stone])
	h._show_tile(_workings._workings_tile([stone]))
	h._hud._drawercompose.close_compose_sheet()
	h._hud._drawercompose.open_order_sheet(int(band["entity"]), "stone")
	await h._settle()
	h._assert_hud("opening Stone from the order line opens the chief's sheet on the site's terrain",
		_has_text("Orrin keeps stone on hand") and _has_text("Karst Highland"))

	# Another rock than the one he digs: his site is named.
	var other: Dictionary = _workings._stone_working(_workings.STONE_TAKE)
	other["tile_x"] = 24
	other["tile_y"] = 16
	_open_deposit(other)
	await h._settle()
	h._assert_hud("on a rock he is not digging, the sheet says where he is",
		_has_text("Orrin is digging at (%d, %d)." % [_workings.WORKING_TILE_X, _workings.WORKING_TILE_Y]))

	# The player's stone.
	var mine := chief_band(false)
	_push(mine)
	_open_deposit(_workings._stone_working(_workings.STONE_TAKE))
	await h._settle()
	await h._save("band_orders_stone_yours")
	h._assert_hud("the player's stone shows the offer strip", _has_text("Orrin could keep stone on hand for you."))
	h._assert_hud("…over today's Assign diggers sheet", _find_meta(_sheet(), HudWidgets.COMPOSE_COMMIT_META) != null)
	_sent.clear()
	h._assert_hud("Hand it to Orrin is live", _press(BandOrdersSheets.META_CLAIM))
	h._assert_hud("…and sends `claim` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d stone claim" % int(mine["band_id"]))

func _find_priority_button(level: String) -> Button:
	return _find_priority(_sheet(), level)

func _find_priority(node: Node, level: String) -> Button:
	if node is Button and node.has_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META) \
			and String(node.get_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META)) == level \
			and not node.is_queued_for_deletion():
		return node as Button
	for child in node.get_children():
		var found := _find_priority(child, level)
		if found != null:
			return found
	return null

# ---- Wood -----------------------------------------------------------------------------------------

func _wood_sheets() -> void:
	var band := chief_band(true, true, true, 12.0, 4.0)
	_push(band)
	_open_deposit(_workings._wood_working(_workings.WOOD_OVER_CUT))
	await h._settle()
	await h._save("band_orders_wood_chief")
	h._assert_hud("the wood sheet names the stands he works, with `here`", _has_text("· here"))
	h._assert_hud("…and that he leaves them to grow back", _has_text("He leaves every stand to grow back."))
	h._assert_hud("…and no short warning while the stands regrow enough", not _has_text("⚠ Stands in reach"))
	var short := chief_band(true, true, true, 12.0, 0.7)
	_push(short)
	_open_deposit(_workings._wood_working(_workings.WOOD_OVER_CUT))
	await h._settle()
	await h._save("band_orders_wood_short")
	h._assert_hud("stands that regrow too little draw the amber line",
		_has_text("⚠ Stands in reach regrow 0.7 wood a turn"))

# ---- Food and fodder --------------------------------------------------------------------------------

func _food_sheets() -> void:
	var band := chief_band()
	h._set_world_herds(_pens() + [HerdFx.herd_fixture()])
	_push(band)
	h._hud._drawercompose.open_order_sheet(int(band["entity"]), "food")
	await h._settle()
	await h._save("band_orders_food_below")
	h._assert_hud("the Food sheet names the chief", _has_text("Orrin keeps the band fed"))
	h._assert_hud("…states what the band eats", _has_text("eats 4.0 food a turn."))
	h._assert_hud("…lists a hunt source", _has_text("Hunt Red Deer"))
	h._assert_hud("…and that he works every row at Best", _has_text("works every row at Best and never overdraws."))
	h._assert_hud("…and the by-products", _has_text("Also brings in hide +0.4 and bone +0.2 a turn."))
	_sent.clear()
	_press(BandOrdersSheets.META_STEPPER_MORE)
	h._assert_hud("the food stepper sends `set 21 high` (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d food set 21 high" % int(band["band_id"]))

	var above := chief_band(true, true, true, 26.0)
	_push(above)
	h._hud._drawercompose.refresh_compose_sheet()
	await h._settle()
	await h._save("band_orders_food_above")
	h._assert_hud("above the target he eases every row to Learning",
		_has_text("Above 20 turns — Orrin eases every row to Learning."))

	h._hud._drawercompose.open_order_sheet(int(band["entity"]), "fodder")
	await h._settle()
	await h._save("band_orders_fodder")
	h._assert_hud("the Fodder sheet names the herds", _has_text("Orrin keeps the herds fed"))
	h._assert_hud("…and what eats the hay", _has_text("What eats it".to_upper()) or _has_text("What eats it"))
	h._assert_hud("…with a range/fodder split bar per herd", _find_meta(_sheet(), BandOrdersSheets.META_PEN_BAR) != null)
	h._assert_hud("…listing the tamed herd that draws fodder beside the pens", _has_text("(63, 17)"))
	h._assert_hud("…and not the tamed herd its range feeds outright", not _has_text("(60, 15)"))

# ---- Hunt and gather ---------------------------------------------------------------------------------

func _hunt_sheets() -> void:
	var band := chief_band()
	_push(band)
	var herd: Dictionary = HerdFx.herd_fixture()
	h._show_herd(herd)
	h._compose_herd(herd)
	await h._settle()
	await h._save("band_orders_hunt_chief")
	h._assert_hud("a chief's hunt row wears his strip", _has_text("Orrin hunts this for"))
	h._assert_hud("…naming the food order", _find_meta(_sheet(), BandOrdersSheets.META_ORDER_LINK) != null)
	_sent.clear()
	h._assert_hud("Crew it yourself is live", _press(BandOrdersSheets.META_RELEASE))
	h._assert_hud("…and releases that herd (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d food release %s" % [int(band["band_id"]), HERD_ID])
	var mine := chief_band(true, true, true, 12.0, 4.0, false)
	_push(mine)
	h._compose_herd(herd)
	await h._settle()
	await h._save("band_orders_hunt_yours")
	h._assert_hud("a player's hunt row is offered to him", _has_text("Orrin could hunt this for the food order."))
	_sent.clear()
	_press(BandOrdersSheets.META_CLAIM)
	h._assert_hud("Hand it to Orrin claims that herd (got `%s`)" % _last_line(),
		_last_line() == "band_order 0 %d food claim %s" % [int(mine["band_id"]), HERD_ID])
