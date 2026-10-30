extends RefCounted

## **BAND ORDERS ON THE WORK TAB** (#860, `.claude/rules/client/band-orders.md`) -- `band_panel_preview`'s
## states for the orders group, in a file of their own like the Trade tab's, APPENDED LAST so no earlier
## frame moves. The sheets are `ui_preview/chapters/band_orders.gd`'s.
##
## Every order starts FOLDED (`band_orders_work_all`); `band_orders_work_open` has Food and Fodder open and
## `band_orders_work_reserve_open` its reserve line open too.
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
	var row := {"kind": kind, "workers": workers, "target_x": x, "target_y": y,
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
		# Two chief rows with nobody on them: the order's reserve.
		_row("forage", 66, 21, 0, "food", true, {"actual_yield": 0.0}),
		_row("forage", 65, 21, 0, "food", true, {"actual_yield": 0.0}),
		{"kind": "scout", "workers": 2, "scouting_range": 3, "scouting_reach": 3},
	]
	# A chief's row carries its order's Priority, as the sim writes it.
	for row in band["labor_assignments"]:
		if String(row.get("order_kind", "")) != "":
			row["priority"] = String(by_kind.get(String(row["order_kind"]), "normal"))
	return band

func _all_orders() -> Array:
	return [
		_order("food", 20, "high", 4.0, 12.0, [8, 8], [4, 4], 6.0, 8.0),
		_order("fodder", 10, "normal", 2.0, 6.0, [2, 2], [2, 2], 3.0, 3.0),
		_order("stone", 30, "normal", 1.0, 12.4, [2, 2], [3, 3], 2.0, 5.0),
		_order("wood", 30, "low", 1.5, 8.0, [3, 3], [7, 7], 2.2, 4.0),
	]

func _show(band: Dictionary, frame: String) -> void:
	h._push_bands([band])
	h._hud._bandpanel.rerender()
	await h._settle()
	await h._save(frame)
	_fits(frame)

## Every Work-tab frame is also judged against its zone: nothing the orders group draws may ask for more
## width or height than the zone holds (a clipping host hides an overrun in the PNG).
func _fits(frame: String) -> void:
	h._current_state = frame
	h._assert_zone_content_fits()

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

func _line_kinds() -> Array:
	var kinds: Array = []
	for line in _lines():
		kinds.append(String((line as Node).get_meta(HudOrdersVocab.ORDER_LINE_META)))
	return kinds

func _carets_open() -> Array:
	var out: Array = []
	for caret in h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_CARET_META)):
		if (caret.get_parent() as Node).has_meta(HudOrdersVocab.ORDER_LINE_META):
			out.append(bool(caret.get_meta(HudOrdersVocab.ORDER_CARET_META)))
	return out

func _reserve_lines() -> Array:
	return h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_RESERVE_META))

func _row_priority_pills() -> int:
	return h._collect_meta_controls(h._panel, String(HudWorkVocab.WORK_ROW_PRIORITY_PILL_META)).size()

func _row_count() -> int:
	return h._collect_meta_controls(h._panel, String(HudWorkVocab.WORK_ROW_ACCOUNTS_META)).size()

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

	var band := chief_band(_all_orders())
	await _show(band, "band_orders_work_all")
	h._assert_band_panel("band orders: the Work tab draws ONE orders group", _group_count() == 1)
	h._assert_band_panel("…titled for the chief", _has("ORRIN'S ORDERS") or _has("Orrin's orders"))
	h._assert_band_panel("…with a line per order (4 got %d)" % _lines().size(), _lines().size() == 4)
	h._assert_band_panel("…in the fixed order Food, Fodder, Stone, Wood", _line_kinds() == ["food", "fodder", "stone", "wood"])
	h._assert_band_panel("…Food's own line", _has("· 12 turns · keep 20 turns"))
	h._assert_band_panel("…a material's line", _has("· 12.4 on hand · keep 30"))
	h._assert_band_panel("…the refilling state with its ETA", _has("Refilling · back to 20 turns in ≈"))
	h._assert_band_panel("…every order starts FOLDED: no site rows, no Also lines, no reserve line",
		not _has("↳ Also") and _carets_open() == [false, false, false, false] and _reserve_lines().is_empty())
	h._assert_band_panel("…the summary line", _has("Orrin's orders take 31 of 34 workers."))
	h._assert_band_panel("…the by-products", _has("Hide · 14 on hand") and _has("+0.4 a turn from hunting"))
	h._assert_band_panel("…and the player's row under Yours", h._collect_meta_controls(h._panel,
		String(HudWorkVocab.WORK_SECTION_HEAD_META)).size() >= 2)

	# Open Food and Fodder: their rows and Also lines appear, the zero-hand rows fold into one reserve line.
	var orders: BandOrdersController = h._hud._bandorders
	# The fold key is the panel band's own id (the harness stamps one on the pushed band).
	band = h._hud._band_labor.panel_band()
	orders.toggle_fold(band, "food")
	orders.toggle_fold(band, "fodder")
	await h._settle()
	await h._save("band_orders_work_open")
	_fits("band_orders_work_open")
	h._assert_band_panel("band orders: opening an order shows its rows and Also lines",
		_has("↳ Also +0.25 fodder from") and _has("↳ Also +0.06 food from"))
	h._assert_band_panel("…the caret is remembered per kind (food, fodder open)",
		_carets_open() == [true, true, false, false])
	h._assert_band_panel("…chief rows drop their own Priority chip (only the player's row keeps one: %d)" % _row_priority_pills(),
		_row_priority_pills() == 1)
	h._assert_band_panel("…the two unstaffed food rows fold into `+2 sites in reserve`",
		_has("+2 sites in reserve") and _reserve_lines().size() == 1)
	var before_reserve := _row_count()
	orders.toggle_fold(band, "food", BandOrdersController.PART_RESERVE)
	await h._settle()
	await h._save("band_orders_work_reserve_open")
	_fits("band_orders_work_reserve_open")
	h._assert_band_panel("…and opening the reserve line brings its two rows back (%d -> %d)" % [before_reserve, _row_count()],
		_row_count() == before_reserve + 2)
	orders.toggle_fold(band, "food", BandOrdersController.PART_RESERVE)
	orders.toggle_fold(band, "food")
	orders.toggle_fold(band, "fodder")
	await h._settle()
	h._assert_band_panel("…folding them again hides everything",
		_carets_open() == [false, false, false, false] and not _has("↳ Also"))

	# The foot of the same tab: the summary, the legend, Also coming in and Yours.
	var scroll = h._hud._bandpanel._work_sections_scroll
	if scroll != null:
		scroll.scroll_vertical = 100000
		await h._settle()
		await h._save("band_orders_work_all_foot")
		_fits("band_orders_work_all_foot")

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
	h._assert_band_panel("…the summary names the orders that wait", _has("Not enough hands for every order — Stone and Wood wait."))

	# Food short on hands AND on refill crew, but its take outruns its draw: it is rising, not falling.
	var rising := _all_orders()
	rising[0] = _order("food", 20, "high", 3.29, 6.6, [5, 8], [0, 9], 7.97, 14.2)
	rising[1] = _order("fodder", 10, "normal", 40.1, 0.01, [2, 2], [0, 0], 0.4, 0.51)
	await _show(chief_band(rising), "band_orders_work_rising")
	h._assert_band_panel("band orders: take above draw with a short crew reads Refilling slowly, not Falling",
		_has("Refilling slowly — 12 hands short"))
	h._assert_band_panel("…fodder whose patches give less than it draws says so",
		_has("Falling 39.6 a turn — the patches give too little"))
	h._assert_band_panel("…never At target below the target", not _has("At target"))
	h._assert_band_panel("…and the summary names Food (amber)",
		_has("Not enough hands for every order — Food waits."))

	var short := _all_orders()
	short[0]["hold_heads"] = 5
	short[0]["take"] = 3.0
	short[2]["hold_heads"] = 1
	short[2]["take"] = 0.5
	short[3]["sustainable"] = 0.7
	await _show(chief_band(short), "band_orders_work_short")
	h._assert_band_panel("band orders: a short keep-up crew with the take under the draw reads Falling", _has("Falling — 3 hands short of keeping up"))
	h._assert_band_panel("…wood's stands that regrow too little say so", _has("Falling 0.8 a turn — the stands regrow too little"))
	h._assert_band_panel("…and the summary goes red, naming the orders", _has("Not enough hands to keep up — Food and Stone are running down."))

	await _kit_and_doors()

	var leaderless: Dictionary = h._workings_band_fixture(h.WORKINGS_DEMAND)
	await _show(leaderless, "band_orders_work_leaderless")
	h._assert_band_panel("band orders: a leaderless band's Work tab draws no group", _group_count() == 0)
	h._assert_band_panel("…and keeps today's sections", _has("AGRICULTURE") or _has("Agriculture"))

	h._push_bands([h._band_fixture()])
	h._hud._bandpanel.rerender()
	await h._settle()


# ---- The order line's doors and its kit clause --------------------------------------------------------

const HOE := "hoe_item"
const SPEAR := "spear_item"

func _toe(item: String, required: float, filled: float) -> Dictionary:
	return {HudBandLaborState.POOL_TOE_ITEM_KEY: item, HudBandLaborState.POOL_TOE_REQUIRED_KEY: required,
		HudBandLaborState.POOL_TOE_FILLED_KEY: filled}

func _meta_button(meta: StringName, kind: String = "") -> Button:
	for node in h._collect_meta_controls(h._panel, String(meta)):
		if node is Button and (kind == "" or String(node.get_meta(meta)) == kind):
			return node
	return null

func _kit_and_doors() -> void:
	h._hud._bandpanel.set_item_display_names({HOE: "Hoes", SPEAR: "Spears"})
	var orders: BandOrdersController = h._hud._bandorders
	# One item: Food's forage row wants 6 hoes and holds 3, its fodder twin is not short.
	var band := chief_band(_all_orders())
	for row in band["labor_assignments"]:
		if String(row.get("kind", "")) == "forage" and String(row.get("order_kind", "")) == "food" \
				and int(row.get("workers", 0)) > 0:
			row[SourceForecast.ASSIGNMENT_KIT_TOE_KEY] = [_toe(HOE, 6.0, 3.0)]
	await _show(band, "band_orders_work_kit_one")
	h._assert_band_panel("band orders: an order whose row is short of hoes says so in amber",
		_has("· short 3 hoes") and _meta_button(HudOrdersVocab.ORDER_KIT_META) != null)
	var kit_button := _meta_button(HudOrdersVocab.ORDER_KIT_META)
	h._assert_band_panel("…the tooltip names the row and the count",
		kit_button != null and kit_button.tooltip_text.contains(" needs 3 hoes"))

	# Two items across two rows, sorted by the count short.
	band = chief_band(_all_orders())
	var seen := 0
	for row in band["labor_assignments"]:
		if String(row.get("order_kind", "")) == "food" and int(row.get("workers", 0)) > 0:
			if String(row.get("kind", "")) == "forage":
				row[SourceForecast.ASSIGNMENT_KIT_TOE_KEY] = [_toe(HOE, 6.0, 3.0), _toe(SPEAR, 4.0, 2.0)]
			else:
				row[SourceForecast.ASSIGNMENT_KIT_TOE_KEY] = [_toe(SPEAR, 3.0, 0.0)]
			seen += 1
	await _show(band, "band_orders_work_kit_two")
	h._assert_band_panel("band orders: two items sort by the count short (%d rows)" % seen,
		_has("· short 5 spears, 3 hoes"))
	var tip := _meta_button(HudOrdersVocab.ORDER_KIT_META).tooltip_text
	h._assert_band_panel("…and the tooltip has one line per short row", tip.split("\n").size() >= 3)

	# Folded and open both show it; open shows the rows too.
	band = h._hud._band_labor.panel_band()
	orders.toggle_fold(band, "food")
	await h._settle()
	await h._save("band_orders_work_kit_open")
	_fits("band_orders_work_kit_open")
	h._assert_band_panel("…visible with the order open too", _meta_button(HudOrdersVocab.ORDER_KIT_META) != null)
	orders.toggle_fold(band, "food")
	await h._settle()

	# The doors: each opens the sheet; the clause reaches the bench-open path.
	var opened: Array = []
	var bench: Array = []
	var sheet_cb := func(entity: int, kind: String) -> void: opened.append(kind)
	var bench_cb := func(b: Dictionary) -> void: bench.append(b)
	orders.order_sheet_requested.connect(sheet_cb)
	orders.crafting_requested.connect(bench_cb)
	var doors := {
		"name": _meta_button(HudOrdersVocab.ORDER_NAME_META, "food"),
		"detail": _meta_button(HudOrdersVocab.ORDER_DETAIL_META, "food"),
		"chip": _first_chip(),
		"chevron": _meta_button(HudOrdersVocab.ORDER_OPEN_META, "food")}
	for door in doors:
		var button: Button = doors[door]
		h._assert_band_panel("band orders: the %s door exists" % door, button != null)
		if button == null:
			continue
		opened.clear()
		button.pressed.emit()
		h._assert_band_panel("…pressing the %s opens the Food sheet (got %s)" % [door, str(opened)],
			opened == ["food"])
		h._assert_band_panel("…the %s says what it does" % door,
			button.tooltip_text == "Set Orrin's food order")
	var clause := _meta_button(HudOrdersVocab.ORDER_KIT_META)
	bench.clear()
	clause.pressed.emit()
	h._assert_band_panel("…pressing the kit clause reaches the bench-open path", bench.size() == 1)
	h._assert_band_panel("…and the crafting bench opens", h._hud.crafting_panel().is_open())
	h._hud.close_crafting_panel()
	orders.order_sheet_requested.disconnect(sheet_cb)
	orders.crafting_requested.disconnect(bench_cb)

	# Hover looks: the name link, the chip and the chevron lit.
	var link: Button = doors["name"]
	var chip: Button = doors["chip"]
	link.add_theme_stylebox_override("normal", link.get_theme_stylebox("hover"))
	chip.add_theme_stylebox_override("normal", chip.get_theme_stylebox("hover"))
	await h._settle()
	await h._save("band_orders_work_hover")
	_fits("band_orders_work_hover")
	await _food_sheet_beside_panel()

## The chief's Food sheet floats BESIDE the Band panel's card, whichever edge it is docked to, and its
## FOOD SOURCES rows carry each patch's real tile (the wire's `target_x` / `target_y`), never `(-1, -1)`.
func _food_sheet_beside_panel() -> void:
	var band := chief_band(_all_orders())
	h._push_bands([band])
	h._hud._bandpanel.rerender()
	for side in [SIDE_LEFT, SIDE_RIGHT]:
		h._panel.set_dock(side)
		h._hud._bandpanel.rerender()
		await h._settle()
		h._hud._drawercompose.open_order_sheet(int(band["entity"]), "food")
		await h._settle()
		await h._settle()
		var frame := "band_orders_food_sheet_dock_%s" % ("left" if side == SIDE_LEFT else "right")
		await h._save(frame)
		var sheet = h._hud._drawercompose._compose_sheet
		var sheet_rect := Rect2(sheet._card.global_position, sheet._card.size)
		var panel_rect: Rect2 = h._panel.card_rect()
		h._assert_band_panel("%s: the Food sheet clears the Band panel's card (%s vs %s)" % [frame, sheet_rect, panel_rect],
			not sheet_rect.intersects(panel_rect))
		var joined := "\n".join(PackedStringArray(_texts(sheet)))
		h._assert_band_panel("%s: a source row names the patch's real tile" % frame,
			joined.contains("(71, 18)") and not joined.contains("(-1, -1)"))
		h._hud._drawercompose.close_compose_sheet()
	h._panel.set_dock(SIDE_RIGHT)

func _first_chip() -> Button:
	for node in h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_PRIORITY_META)):
		if node is Button and node.get_parent().get_parent().has_meta(HudOrdersVocab.ORDER_LINE_META):
			return node
	return null
