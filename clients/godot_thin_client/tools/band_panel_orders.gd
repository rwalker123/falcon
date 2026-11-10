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

const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

var h

const WORKERS := 34
const LEADER := "Orrin"

func _order(kind: String, target: int, priority: String, draw: float, on_hand: float,
		hold: Array, refill: Array, take: float, sustainable: float = 0.0, consumers: bool = true) -> Dictionary:
	return {"kind": kind, "target": target, "priority": priority, "chief_owned": true, "draw": draw,
		"on_hand": on_hand, "has_site": true, "site_x": h.ROSTER_NEAR_TILE.x, "site_y": h.ROSTER_NEAR_TILE.y,
		"sustainable": sustainable, "has_consumers": consumers, "hold_heads": hold[0], "hold_wanted": hold[1],
		"refill_heads": refill[0], "refill_wanted": refill[1], "take": take,
		"limit": "hands", "limit_item": "", "land_gives": 0.0}

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
	h._assert_band_panel("…and the header carries the leader's portrait (art on disk: %s)" % LeaderPortrait.has_art(),
		h._collect_meta_controls(h._panel, String(HudOrdersVocab.GROUP_PORTRAIT_META)).size() == (1 if LeaderPortrait.has_art() else 0))
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

	await _limits()

	await _released_wood()

	var leaderless: Dictionary = h._workings_band_fixture(h.WORKINGS_DEMAND)
	await _show(leaderless, "band_orders_work_leaderless")
	h._assert_band_panel("band orders: a leaderless band's Work tab draws no group", _group_count() == 0)
	h._assert_band_panel("…and keeps today's sections", _has("AGRICULTURE") or _has("Agriculture"))

	h._push_bands([h._band_fixture()])
	h._hud._bandpanel.rerender()
	await h._settle()


# ---- A released order: the player crews it ("Crew it yourself" on the wood sheet) -----------------------

func _released_wood() -> void:
	var orders: BandOrdersController = h._hud._bandorders
	var all := _all_orders()
	all[3]["chief_owned"] = false
	var band := chief_band(all)
	# The wire after a release: the wood rows are the player's, on one worker's cap, tile on target_x/target_y.
	for row in band["labor_assignments"]:
		if String(row.get("kind", "")) == "extract" and String(row.get("material", "")) == "wood":
			row["order_kind"] = ""
			row["chief_owned"] = false
			row["max_workers"] = 1
			row["workers"] = 1
	await _show(band, "band_orders_work_released")
	var wood_line: Node = null
	for line in _lines():
		if String((line as Node).get_meta(HudOrdersVocab.ORDER_LINE_META)) == "wood":
			wood_line = line
	h._assert_band_panel("released: wood keeps its line in the group", wood_line != null and _line_kinds().size() == 4)
	var words := _texts(wood_line)
	var joined := " ".join(words)
	h._assert_band_panel("…it reads 'You crew it' with the hand-back link (%s)" % joined,
		joined.contains("You crew it") and joined.contains("Hand it to Orrin") and joined.contains("on hand")
		and not joined.contains("keep 30") and not joined.contains("Refilling"))
	h._assert_band_panel("…no Priority chip, crew bar or caret on it",
		_descendant_meta_count(wood_line, HudOrdersVocab.ORDER_PRIORITY_META) == 0
		and _descendant_meta_count(wood_line, HudOrdersVocab.ORDER_CREW_BAR_META) == 0
		and _descendant_meta_count(wood_line, HudOrdersVocab.ORDER_CARET_META) == 0)
	h._assert_band_panel("…the summary counts the other three orders only",
		_has("Orrin's orders take 21 of 34 workers"))
	var sent: Array = []
	orders.band_order_requested.connect(func(payload: Dictionary) -> void: sent.append(payload))
	var hand := _meta_button(HudOrdersVocab.ORDER_HAND_BACK_META, "wood")
	if hand != null:
		hand.pressed.emit()
	h._assert_band_panel("…pressing the link sends the whole-kind wood claim",
		sent.size() == 1 and String(sent[0].get("kind", "")) == "wood"
		and String(sent[0].get("action", "")) == "claim" and not sent[0].has("x") and not sent[0].has("herd_id"))
	if sent.size() == 1:
		h._assert_band_panel("…rendered as band_order <f> <b> wood claim",
			String(MAIN_SCRIPT.format_band_order(sent[0]).get("line", "")).ends_with("wood claim"))

func _descendant_meta_count(node: Node, meta: StringName) -> int:
	var count := 1 if node.has_meta(meta) else 0
	for child in node.get_children():
		count += _descendant_meta_count(child, meta)
	return count

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
		# The Wood order sheet is a KIND_ORDER sheet too: beside the panel, never over it.
		h._hud._drawercompose.open_order_sheet(int(band["entity"]), "wood")
		await h._settle()
		await h._settle()
		var wood_rect := Rect2(sheet._card.global_position, sheet._card.size)
		h._assert_band_panel("%s: the Wood sheet clears the Band panel's card too (%s vs %s)" % [frame, wood_rect, panel_rect],
			not wood_rect.intersects(panel_rect) and "\n".join(PackedStringArray(_texts(sheet))).contains("Crew all wood yourself"))
		h._hud._drawercompose.close_compose_sheet()
	h._panel.set_dock(SIDE_RIGHT)

func _first_chip() -> Button:
	for node in h._collect_meta_controls(h._panel, String(HudOrdersVocab.ORDER_PRIORITY_META)):
		if node is Button and node.get_parent().get_parent().has_meta(HudOrdersVocab.ORDER_LINE_META):
			return node
	return null


# ---- What holds a refill up: the wire's `limit` words the order's amber state -------------------------------

const SLED := "sled"

func _limits() -> void:
	h._hud._bandpanel.set_item_display_names({HOE: "Hoes", SLED: "Sled"})
	var orders := _all_orders()
	# Food: the land is the limit (4.2 a turn). Fodder: a sled is, 1 of 2 hands short. Stone: hands.
	# Wood: the land again, with no figure on the wire.
	var by_kind := {}
	for order in orders:
		by_kind[String(order["kind"])] = order
	var food: Dictionary = by_kind["food"]
	food.merge({"refill_heads": 0, "refill_wanted": 12, "limit": "land", "land_gives": 4.2}, true)
	var fodder: Dictionary = by_kind["fodder"]
	fodder.merge({"refill_heads": 1, "refill_wanted": 2, "limit": "kit", "limit_item": SLED}, true)
	var stone: Dictionary = by_kind["stone"]
	stone.merge({"refill_heads": 0, "refill_wanted": 3, "limit": "hands"}, true)
	var wood: Dictionary = by_kind["wood"]
	wood.merge({"refill_heads": 2, "refill_wanted": 5, "limit": "land", "land_gives": 0.0}, true)
	var band := chief_band(orders)
	for row in band["labor_assignments"]:
		if String(row.get("order_kind", "")) == "fodder":
			row[SourceForecast.ASSIGNMENT_KIT_TOE_KEY] = [_toe(SLED, 2.0, 1.0), _toe(HOE, 3.0, 0.0)]
	await _show(band, "band_orders_work_limits")
	h._assert_band_panel("band orders: limit land with a figure says what the patches give, no hand count",
		_has("Holding — the patches near camp give 4.2 a turn at most") and not _has("12 wanted"))
	h._assert_band_panel("…limit kit names the sled, singular",
		_has("Refilling slowly — short 1 sled"))
	h._assert_band_panel("…the kit clause drops the sled the state names and keeps the hoes",
		_has("· short 3 hoes") and not _has("short 3 hoes, 1 sled") and not _has("1 sled, "))
	h._assert_band_panel("…limit hands keeps the hand wording", _has("Holding — no hands to refill (3 wanted)"))
	h._assert_band_panel("…limit land with no figure says the stands give too little",
		_has("Refilling slowly — the stands near camp give too little"))
	# Plural, and no limit at all.
	fodder.merge({"refill_heads": 0, "refill_wanted": 2}, true)
	food.merge({"limit": "", "land_gives": 0.0}, true)
	await _show(chief_band(orders), "band_orders_work_limits_plural")
	h._assert_band_panel("…limit kit with two short and no heads reads Holding, plural",
		_has("Holding — short 2 sleds to refill"))
	h._assert_band_panel("…no limit names no shortfall", _has("Holding") and not _has("Holding — the patches"))
	await _limits_falling_and_full()


# ---- Falling and slow lines that name a kit or a full tile (the order's `limit` + `limit_count`) ----------

const BASKET := "basket_item"
const KIT_SHORT_COUNT := 7

func _limits_falling_and_full() -> void:
	h._hud._bandpanel.set_item_display_names({BASKET: "Baskets", HOE: "Hoes", SLED: "Sled"})
	var orders := _all_orders()
	var by_kind := {}
	for order in orders:
		by_kind[String(order["kind"])] = order
	# Four ways to fall: a kit short, every tile full, the land's cap, and plain hands.
	by_kind["food"].merge({"draw": 4.0, "take": 3.0, "sustainable": 8.0, "limit": "kit", "limit_item": BASKET,
		"limit_count": KIT_SHORT_COUNT}, true)
	by_kind["fodder"].merge({"draw": 2.0, "take": 1.0, "sustainable": 3.0, "limit": "full"}, true)
	by_kind["stone"].merge({"draw": 1.0, "take": 0.5, "sustainable": 5.0, "limit": "land", "land_gives": 4.2}, true)
	by_kind["wood"].merge({"draw": 1.5, "take": 1.0, "sustainable": 4.0, "limit": "hands", "hold_heads": 3,
		"hold_wanted": 5}, true)
	await _show(chief_band(orders), "band_orders_work_falling_limits")
	h._assert_band_panel("band orders: falling names a kit short, with the count",
		_has("Falling — short 7 baskets"))
	h._assert_band_panel("…falling with every tile full says so", _has("Falling — the patches near camp are full"))
	h._assert_band_panel("…falling under the land's cap names the cap",
		_has("Falling — the stands near camp give 4.2 a turn at most"))
	h._assert_band_panel("…falling on hands keeps the hand count", _has("Falling — 2 hands short of keeping up"))
	h._assert_band_panel("…several falling orders: the bare summary, no cause",
		_has("Food, Fodder, Stone and Wood are running down."))
	# One order falling on a kit: the summary gives its reason as the line words it.
	var lone := _all_orders()
	lone[0].merge({"draw": 4.0, "take": 3.0, "sustainable": 8.0, "limit": "kit", "limit_item": BASKET,
		"limit_count": KIT_SHORT_COUNT}, true)
	await _show(chief_band(lone), "band_orders_work_falling_kit")
	h._assert_band_panel("…one order falling on a kit: the summary names the kit",
		_has("Food is running down — short 7 baskets."))
	h._assert_band_panel("…and never says hands", not _has("hands to keep up"))
	# Refilling slowly with the take above the draw: a kit short, and every tile full, with no hands short.
	var slow := _all_orders()
	slow[0].merge({"on_hand": 12.0, "limit": "kit", "limit_item": BASKET, "limit_count": KIT_SHORT_COUNT}, true)
	slow[1].merge({"on_hand": 6.0, "limit": "full"}, true)
	await _show(chief_band(slow), "band_orders_work_slow_limits")
	h._assert_band_panel("…kit limit below target reads Refilling slowly, not green Refilling",
		_has("Refilling slowly — short 7 baskets"))
	h._assert_band_panel("…every tile full below target reads Refilling slowly",
		_has("Refilling slowly — the patches near camp are full"))
	h._assert_band_panel("…the waiting summary names no hands",
		_has("Food and Fodder wait.") and not _has("Not enough hands"))
	var solo := _all_orders()
	solo[0].merge({"on_hand": 12.0, "limit": "kit", "limit_item": BASKET, "limit_count": KIT_SHORT_COUNT}, true)
	await _show(chief_band(solo), "band_orders_work_slow_kit")
	h._assert_band_panel("…one waiting order gives its reason", _has("Food waits — short 7 baskets."))
	# Holding with the tiles full.
	var held := _all_orders()
	held[0].merge({"on_hand": 12.0, "refill_heads": 0, "refill_wanted": 4, "limit": "full"}, true)
	await _show(chief_band(held), "band_orders_work_holding_full")
	h._assert_band_panel("…holding with every tile full", _has("Holding — the patches near camp are full"))
