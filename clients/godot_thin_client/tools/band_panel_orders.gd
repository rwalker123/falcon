extends RefCounted

## **BAND ORDERS ON THE WORK TAB** (#860, `.claude/rules/client/band-orders.md`) -- `band_panel_preview`'s
## states for the orders group, in a file of their own like the Trade tab's, APPENDED LAST so no earlier
## frame moves. The sheets are `ui_preview/chapters/band_orders.gd`'s.
##
## Frames: all four orders (`band_orders_work_all`), fodder hidden while nothing eats hay
## (`band_orders_work_no_fodder`), a band short of hands to refill (`band_orders_work_slow`), a band
## short of hands to keep up with wood's stands regrowing too little (`band_orders_work_short`), and a
## leaderless band's Work tab (`band_orders_work_leaderless`) -- which must carry no group at all.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

var h

const WORKERS := 34
const LEADER := "Orrin"

func _order(kind: String, target: int, priority: String, draw: float, on_hand: float,
		hold: Array, refill: Array, take: float, sustainable: float = 0.0, consumers: bool = true) -> Dictionary:
	return {"kind": kind, "target": target, "priority": priority, "chief_owned": true, "draw": draw,
		"on_hand": on_hand, "has_site": true, "site_x": h.ROSTER_NEAR_TILE.x, "site_y": h.ROSTER_NEAR_TILE.y,
		"sustainable": sustainable, "has_consumers": consumers, "hold_heads": hold[0], "hold_wanted": hold[1],
		"refill_heads": refill[0], "refill_wanted": refill[1], "take": take}

func _row(kind: String, x: int, y: int, workers: int, order_kind: String, chief: bool, extra: Dictionary = {}) -> Dictionary:
	var row := {"kind": kind, "workers": workers, "target_x": x, "target_y": y, "x": x, "y": y,
		"order_kind": order_kind if chief else "", "chief_owned": chief, "floor": 0.5, "has_yield": true,
		"kit_id": "", "max_workers": 0, "hold_crew": 1, "reach_crew": workers,
		"kitted_staffed": workers / 2, "bare_staffed": workers - workers / 2, "useful_crew": workers}
	row.merge(extra, true)
	return row

func chief_band(orders: Array) -> Dictionary:
	var band: Dictionary = h._band_fixture()
	band["leader_kind"] = 1
	band["leader_name"] = LEADER
	band["working_age"] = WORKERS
	band["material_store"] = [{"material_id": "hide", "amount": 14.0}, {"material_id": "bone", "amount": 6.0},
		{"material_id": "fibre", "amount": 3.0}]
	band["band_orders"] = orders
	var near: Vector2i = h.ROSTER_NEAR_TILE
	var by_kind := {}
	for order in orders:
		by_kind[String(order["kind"])] = String(order["priority"])
	band["labor_assignments"] = [
		_row("forage", 71, 18, 6, "food", true, {"actual_yield": 2.4, "sustainable_yield": 2.4,
			"fodder_yield": 0.25, "material_yield": [{"material_id": "fibre", "amount": 0.05}]}),
		_row("hunt", 70, 17, 8, "food", true, {"fauna_id": "game_deer_07", "herd_id": "game_deer_07",
			"actual_yield": 4.4, "sustainable_yield": 4.4, "material_yield": [{"material_id": "hide", "amount": 0.4},
			{"material_id": "bone", "amount": 0.2}]}),
		_row("forage", 70, 19, 4, "fodder", true, {"actual_yield": 0.06, "fodder_yield": 0.4}),
		_row("extract", near.x, near.y, 5, "stone", true, {"material": "stone", "fauna_id": "",
			"material_yield": [{"material_id": "stone", "amount": 2.0}]}),
		_row("extract", near.x, near.y, 3, "wood", true, {"material": "wood", "fauna_id": "",
			"material_yield": [{"material_id": "wood", "amount": 1.4}]}),
		_row("forage", 66, 19, 2, "", false, {"actual_yield": 0.3}),
		{"kind": "scout", "workers": 2, "scouting_range": 3, "scouting_reach": 3},
	]
	# A chief's row carries its order's Priority, as the sim writes it.
	for row in band["labor_assignments"]:
		if String(row.get("order_kind", "")) != "":
			row["priority"] = String(by_kind.get(String(row["order_kind"]), "normal"))
	return band

func _all_orders() -> Array:
	return [
		_order("food", 20, "high", 4.0, 12.0, [8, 8], [4, 4], 6.0),
		_order("fodder", 10, "normal", 2.0, 6.0, [2, 2], [2, 2], 3.0),
		_order("stone", 30, "normal", 1.0, 12.4, [2, 2], [3, 3], 2.0, 5.0),
		_order("wood", 30, "low", 1.5, 8.0, [3, 3], [7, 7], 2.2, 4.0),
	]

func _show(band: Dictionary, frame: String) -> void:
	h._push_bands([band])
	h._hud._bandpanel.rerender()
	await h._settle()
	await h._save(frame)

func _texts(node: Node, out: Array = []) -> Array:
	if node is Label:
		out.append((node as Label).text)
	elif node is Button:
		out.append((node as Button).text)
	for child in node.get_children():
		_texts(child, out)
	return out

func _has(needle: String) -> bool:
	for t in _texts(h._panel):
		if String(t).contains(needle):
			return true
	return false

func _group_count() -> int:
	return h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_GROUP_META)).size()

func _lines() -> Array:
	return h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_LINE_META))

func run(harness) -> void:
	h = harness
	h._set_forage_patches([])
	h._set_world_herds(h._herd_fixtures())
	h._hud.update_intensification([h._workings_knowledge_row()])
	h._hud.update_deposit_rungs(h._deposit_rung_catalog())
	# The chief never overdraws: his wood is cut at the stand's own sustainable take, so the deposit
	# row the Work tab reads states a take AT that figure and no over-cut warning is true of it.
	h._hud.update_deposits([
		h._workings_row(h.ROSTER_NEAR_TILE, h.WORKINGS_STONE, false, 2.0),
		h._workings_row(h.ROSTER_NEAR_TILE, h.WORKINGS_WOOD, true, h.WORKINGS_WOOD_SUSTAINABLE)])
	h._panel.set_dock(SIDE_RIGHT)
	h._panel.set_active_tab(&"work")

	await _show(chief_band(_all_orders()), "band_orders_work_all")
	h._assert_band_panel("band orders: the Work tab draws ONE orders group", _group_count() == 1)
	h._assert_band_panel("…titled for the chief", _has("ORRIN'S ORDERS") or _has("Orrin's orders"))
	h._assert_band_panel("…with a line per order (4 got %d)" % _lines().size(), _lines().size() == 4)
	h._assert_band_panel("…Food's own line", _has("· 12 turns · keep 20 turns"))
	h._assert_band_panel("…a material's line", _has("· 12.4 on hand · keep 30"))
	h._assert_band_panel("…the refilling state with its ETA", _has("Refilling · back to 20 turns in ≈"))
	h._assert_band_panel("…a mixed patch's ↳ Also line under Fodder", _has("↳ Also +0.25 fodder from"))
	h._assert_band_panel("…and under Food for the hay meadow", _has("↳ Also +0.06 food from"))
	h._assert_band_panel("…the summary line", _has("Orrin's orders take 31 of 34 workers."))
	h._assert_band_panel("…the by-products", _has("Hide · 14 on hand") and _has("+0.4 a turn from hunting"))
	h._assert_band_panel("…and the player's row under Yours", h._collect_meta_controls(h._panel,
		String(HudWorkVocab.WORK_SECTION_HEAD_META)).size() >= 2)

	# The foot of the same tab: the summary, the legend, Also coming in and Yours.
	var scroll = h._hud._bandpanel._work_sections_scroll
	if scroll != null:
		scroll.scroll_vertical = 100000
		await h._settle()
		await h._save("band_orders_work_all_foot")

	var no_fodder := _all_orders()
	no_fodder[1]["has_consumers"] = false
	await _show(chief_band(no_fodder), "band_orders_work_no_fodder")
	h._assert_band_panel("band orders: fodder is skipped while nothing eats hay (3 got %d)" % _lines().size(),
		_lines().size() == 3)

	var slow := _all_orders()
	slow[2]["refill_heads"] = 1
	slow[3]["refill_heads"] = 0
	await _show(chief_band(slow), "band_orders_work_slow")
	h._assert_band_panel("band orders: no hands to refill reads Holding", _has("Holding — no hands to refill (7 wanted)"))
	h._assert_band_panel("…and a slow refill names the hands short", _has("Refilling slowly — 2 hands short"))
	h._assert_band_panel("…the summary warns in amber words", _has("Not enough hands for every order — Low ones wait."))

	var short := _all_orders()
	short[0]["hold_heads"] = 5
	short[3]["sustainable"] = 0.7
	await _show(chief_band(short), "band_orders_work_short")
	h._assert_band_panel("band orders: a short keep-up crew reads Falling", _has("Falling — 3 hands short of keeping up"))
	h._assert_band_panel("…wood's stands that regrow too little say so", _has("Falling 0.8 a turn — the stands regrow too little"))
	h._assert_band_panel("…and the summary goes red", _has("Not enough hands to keep up — Low orders are running down."))

	var leaderless: Dictionary = h._workings_band_fixture(h.WORKINGS_DEMAND)
	await _show(leaderless, "band_orders_work_leaderless")
	h._assert_band_panel("band orders: a leaderless band's Work tab draws no group", _group_count() == 0)
	h._assert_band_panel("…and keeps today's sections", _has("AGRICULTURE") or _has("Agriculture"))

	h._push_bands([h._band_fixture()])
	h._hud._bandpanel.rerender()
	await h._settle()
