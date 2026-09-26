extends RefCounted

## **THE BAND DOCK'S TRADE TAB** (issue #731) — `band_panel_preview`'s states for it, in a file of their
## own so the tab's fixtures and claims do not grow the 25k-line harness. Driven from the harness's run
## order (`h`), rendering into the same long-lived panel, and APPENDED LAST so no earlier frame moves.
##
## The frames: the narrow tab on a busy turn, a good's hover card, the camps list (unscoped and scoped
## to food) hanging under its row, a folded direction and its `N more shipments` list, the empty turn,
## the wide shell's option (iii) layout (Trade under Parties, the shell threshold still 1190), a list
## opening UPWARD off a bottom dock's row, and the SHORT tier. The
## claims are what a frame cannot say: which causes reached the tab, the badge's count, the tier the
## measurement chose, the relay's first hop, the friction row, and that no zone grew a scroll.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## The harness node — `_hud`, `_panel`, `_settle`, `_save`, `_assert_band_panel`, …
var h

## The network's five camps. `ASHFELL` is the panel's subject and keeps the harness band's entity so
## the panel resolves to it; the others are this file's own.
const ASHFELL := 904
const BARROWMERE := 911
const BITTERBROOK := 912
const ALDERFEN := 913
const EMBERLY := 914
const NETWORK_ID := 7
## The network's longest link — `supply_network_span_tiles`, the same at every member.
const NETWORK_SPAN := 5
## A band the shipments name that is NOT in the network — a foreign people's camp.
const DUSKWATER_ID := 6120
const DUSKWATER_NAME := "Duskwater"
const FOREIGN_FACTION := 1

## Positions (odd-r offset) — Emberly is the relay, reachable only through Barrowmere.
const POS := {
	ASHFELL: Vector2i(71, 18), BARROWMERE: Vector2i(73, 18), BITTERBROOK: Vector2i(75, 18),
	ALDERFEN: Vector2i(66, 18), EMBERLY: Vector2i(76, 18),
}
const NAMES := {
	ASHFELL: "Ashfell", BARROWMERE: "Barrowmere", BITTERBROOK: "Bitterbrook",
	ALDERFEN: "Alderfen", EMBERLY: "Emberly",
}

## The rung ids the links stand on, and the published ladder they join (`route_rungs`).
const RUNG_PATH := "route:path"
const RUNG_TRAIL := "route:trail"
const RUNG_DIRT := "route:dirt_road"
const RUNG_PAVED := "route:paved_road"

## **THE BUSY TURN'S FIGURES.** Food pooled in 6.4 at Ashfell; Barrowmere and Bitterbrook gave 3.4 and
## 3.3, so the network's nets sum to -0.3 — the friction row. Bone moves at three ratings; the tab
## states its NET (0.8 - 0.4 + 0.3 = 0.7) and the hover card the three piles.
const FOOD_POOLED_IN := 6.4
const FOOD_BARROW_OUT := 3.4
const FOOD_BITTER_OUT := 3.3
const FODDER_POOLED_OUT := 2.0
const BONE_GOOD_IN := 0.8
const BONE_FAIR_OUT := 0.4
const BONE_EXCELLENT_IN := 0.3
const HIDE_POOLED_IN := 0.02
const HIDE_SHIPPED_IN := 1.5
const BONE_SHIPPED_IN := 0.6
const FOOD_SHIPPED_OUT := 12.0
const WOOD_SHIPPED_OUT := 1.1
## Two causes on the same turn the tab must NOT show.
const FOOD_PARTY_HOME := 3.1
const FOOD_DOWRY_OUT := 18.0
const BARROWMERE_BONE_OUT := 0.8
const IMPORT_PARTY := 7001
const EXPORT_PARTY := 7002
## The busiest turn: every good the busy turn moves plus four more, and fifteen shipments.
const BUSIEST_EXTRA_GOODS := ["fibre", "wood", "stone", "hurdles"]
const BUSIEST_EXTRA_AMOUNT := 0.9
const BUSIEST_IMPORTS := 7
const BUSIEST_EXPORTS := 8
const BUSIEST_BASE_PARTY := 7200
const BUSIEST_AMOUNT_STEP := 1.3
## The fold state: this many imports, which is past `SHIPMENT_FOLD_TRIGGER`.
const MANY_IMPORTS := 6
const MANY_IMPORT_BASE_PARTY := 7100
const MANY_IMPORT_AMOUNT_STEP := 1.5

## Canvases: a tall side dock (the narrow tab), the 1920 bottom dock (the wide shell), and a narrow
## bottom dock (the tabbed shell at a strip's height, which is where the SHORT tier lands).
const SIDE_CANVAS := Vector2i(1500, 900)
const WIDE_CANVAS := Vector2i(1920, 1080)
const NARROW_BOTTOM_CANVAS := Vector2i(1100, 800)
## The three-zone threshold (issue #731 option iii leaves it here), asserted against the panel's own.
const THREE_ZONE_SHELL_MIN_WIDTH := 1190.0
## How far a popover's edge may sit from where its row puts it — rounding to whole pixels, not slack.
const ADJACENCY_TOLERANCE := 1.5

func run(harness) -> void:
	h = harness
	h._hud.update_route_rungs(_rung_catalog())
	var panel: BandCityPanel = h._panel
	var trade: TradeZoneController = h._hud._bandpanel.trade_zone()
	var start_canvas: Vector2i = h._pinned_canvas

	# ---- 1. THE NARROW TAB, A BUSY TURN ---------------------------------------------------------
	await h._pin_canvas(SIDE_CANVAS)
	h._push_bands(_network(_busy_crossings()))
	panel.set_dock(SIDE_LEFT)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	await h._settle()
	await h._save("trade_tab_narrow_busy")
	h._assert_shell_is_wide(false, "trade_tab_narrow_busy")
	var zone: Control = panel._zones.get(BandCityPanel.ZONE_TRADE)
	h._assert_band_panel("the band page declares a Trade tab, and the narrow shell draws it",
		zone != null and panel.shows_zone(BandCityPanel.ZONE_TRADE))
	h._assert_band_panel("…in the FULL tier, on a side dock with room for it",
		not trade.is_short_tier() and _find_named(zone, TradeZoneController.FULL_TIER_NAME) != null)
	var badge: Dictionary = panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {})
	h._assert_band_panel("the tab's badge counts this turn's SHIPMENTS, both ways — 2 (got `%s`)"
		% String(badge.get("text", "")), String(badge.get("text", "")) == "2")
	var goods := _metas(zone, TradeZoneController.ROW_GOOD_META)
	h._assert_band_panel("the Local arm is one line per GOOD, food and fodder first (got %s)" % str(goods),
		goods == ["provisions", "fodder", "bone", "hide"])
	var food_net := TradeLedger.goods_net(_subject())[0]
	h._assert_band_panel("…and food's line is the POOLED net alone — no homecoming, no dowry (%.1f)"
		% float(food_net[TradeLedger.GOOD_NET]),
		is_equal_approx(float(food_net[TradeLedger.GOOD_NET]), FOOD_POOLED_IN))
	h._assert_band_panel("…and a sub-decimal net reads `even`, never an arrow on 0.0",
		_row_text(zone, TradeZoneController.ROW_GOOD_META, "hide").contains(HudTradeVocab.EVEN_WORD))
	var shipments := _metas(zone, TradeZoneController.ROW_SHIPMENT_META)
	h._assert_band_panel("the Route arm is one line per shipment — an import and an export (got %s)"
		% str(shipments), shipments == [DUSKWATER_NAME, NAMES[BITTERBROOK]])
	var marks := _find_all(zone, "FactionMark")
	var foreign := marks.filter(func(m): return (m as FactionMark).faction() == FOREIGN_FACTION)
	h._assert_band_panel("every counterparty carries a faction mark, and the foreign one is theirs (%d marks)"
		% marks.size(), marks.size() == 2 and foreign.size() == 1
			and (foreign[0] as Control).tooltip_text.contains(HudTradeVocab.FACTION_THEIRS))
	h._assert_scroll_only_where_sanctioned()
	h._assert_zone_content_fits()

	# ---- 2. A GOOD'S HOVER CARD ------------------------------------------------------------------
	var bone_row := _row(zone, TradeZoneController.ROW_GOOD_META, "bone")
	trade.show_hover_for(bone_row)
	await h._settle()
	await h._save("trade_tab_hover_bone")
	var card := trade.hover_card()
	h._assert_band_panel("bone's hover card lists its three rating piles, each with its own amount",
		card != null and card.visible and card.get_child(0).get_child_count() == 1 + 3)
	h._assert_band_panel("…beside the row, never over it",
		card != null and not card.get_global_rect().intersects(bone_row.get_global_rect()))
	card.dismiss()

	# ---- 3. THE CAMPS LIST, UNDER THE NETWORK LINE -------------------------------------------------
	trade.open_list(TradeZoneController.KIND_CAMPS)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_camps")
	var list := trade.list_body()
	var list_text := _text_of(list)
	h._assert_band_panel("the camps list lists every member, this band first (5 rows)",
		trade.is_list_open() and list.get_child_count() == 5)
	h._assert_band_panel("…a relay camp names the first hop on its chain (`via Barrowmere`)",
		list_text.contains(HudTradeVocab.VIA_FORMAT % NAMES[BARROWMERE]))
	h._assert_band_panel("…an unroaded link reads `open ground`, a roaded one the rung's own word",
		list_text.contains(HudTradeVocab.OPEN_GROUND_WORD) and list_text.contains("trail"))
	_assert_hangs_from_its_row("trade_tab_camps", trade, false)
	h._assert_scroll_only_where_sanctioned()
	# The row that opened it closes it.
	trade.open_list(TradeZoneController.KIND_CAMPS)
	await h._settle()
	h._assert_band_panel("…and the row that opened the list closes it again",
		not trade.is_list_open())

	trade.open_list(TradeZoneController.KIND_CAMPS + TradeZoneController.KIND_SCOPE_SEPARATOR
		+ HudTradeVocab.COMMODITY_FOOD)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_camps_food")
	list_text = _text_of(list)
	h._assert_band_panel("scoped to food: `2 camps moved it; 2 sat even`",
		trade._popover_sub.text == HudTradeVocab.GOOD_SCOPE_SUMMARY_FORMAT % [
			HudTradeVocab.count_text(2, HudTradeVocab.CAMP_WORDS), 2])
	h._assert_band_panel("…and the friction row states what the distance ate (0.3)",
		list_text.contains(HudTradeVocab.LOST_IN_TRANSIT) and list_text.contains("%.1f"
			% (FOOD_BARROW_OUT + FOOD_BITTER_OUT - FOOD_POOLED_IN)))
	var food_anchor := trade.anchor_rect()
	_assert_hangs_from_its_row("trade_tab_camps_food", trade, false)
	# A DIFFERENT row swaps the content in place and re-anchors under itself.
	trade.open_list(TradeZoneController.KIND_CAMPS + TradeZoneController.KIND_SCOPE_SEPARATOR + "bone")
	await h._settle()
	await h._settle()
	h._assert_band_panel("another good's row swaps the list in place and re-anchors under itself",
		trade.is_list_open() and trade._popover_title.text == HudTradeVocab.GOOD_SCOPE_TITLE_FORMAT
			% TradeLedger.commodity_label("bone") and trade.anchor_rect().position.y
			> food_anchor.position.y)
	_assert_hangs_from_its_row("trade_tab_camps_bone", trade, false)
	# **A COUNT OF ONE READS SINGULAR.** Only Barrowmere moved bone besides this band.
	h._assert_band_panel("a count of one reads singular — `%s` (got `%s`)" % [
			HudTradeVocab.GOOD_SCOPE_SUMMARY_FORMAT % ["1 camp", 3], trade._popover_sub.text],
		trade._popover_sub.text == HudTradeVocab.GOOD_SCOPE_SUMMARY_FORMAT % ["1 camp", 3])
	trade.dismiss()

	# ---- 4. A FOLDED DIRECTION, AND ITS LIST -----------------------------------------------------
	h._push_bands(_network(_many_crossings()))
	await h._settle()
	await h._save("trade_tab_folded")
	zone = panel._zones.get(BandCityPanel.ZONE_TRADE)
	var more := _find_meta(zone, TradeZoneController.ROW_OPENS_META, TradeZoneController.KIND_ROUTE_IN)
	h._assert_band_panel("six imports fold to the three largest plus `3 more shipments`",
		_metas(zone, TradeZoneController.ROW_SHIPMENT_META).size() == HudTradeVocab.SHIPMENT_FOLD_KEEP
			and more != null and _text_of(more).contains(HudTradeVocab.count_text(3, HudTradeVocab.MORE_SHIPMENT_WORDS)))
	trade.open_list(TradeZoneController.KIND_ROUTE_IN)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_more_shipments")
	h._assert_band_panel("…and the fold row opens every import in the list popover (6)",
		_metas(list, TradeZoneController.ROW_SHIPMENT_META).size() == MANY_IMPORTS)
	_assert_hangs_from_its_row("trade_tab_more_shipments", trade, false)
	trade.dismiss()

	# ---- 5. THE EMPTY TURN -----------------------------------------------------------------------
	h._push_bands(_network([]))
	await h._settle()
	await h._save("trade_tab_empty")
	zone = panel._zones.get(BandCityPanel.ZONE_TRADE)
	badge = panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {})
	h._assert_band_panel("the empty turn states the network, then says nothing crossed",
		_find_named(zone, TradeZoneController.NETWORK_LINE_NAME) != null
			and _text_of(zone).contains(HudTradeVocab.EMPTY_HEAD))
	h._assert_band_panel("…and carries no badge", String(badge.get("text", "")) == "")

	# ---- 6. THE WIDE SHELL — option (iii) --------------------------------------------------------
	# The BUSIEST turn — eight goods and fifteen shipments, the proposal's own worst case — which is
	# what the strip's body cannot hold at full height.
	await h._pin_canvas(WIDE_CANVAS)
	h._push_bands(_network(_busiest_crossings()))
	panel.set_dock(SIDE_BOTTOM)
	await h._settle()
	await h._save("trade_tab_wide")
	h._assert_shell_is_wide(true, "trade_tab_wide")
	h._assert_band_panel("a band's shell threshold is still the three flanks' %.0f (got %.0f)"
		% [THREE_ZONE_SHELL_MIN_WIDTH, panel.wide_shell_min_width()],
		is_equal_approx(panel.wide_shell_min_width(), THREE_ZONE_SHELL_MIN_WIDTH))
	var parties: Control = panel._zones.get(BandCityPanel.ZONE_PARTIES)
	h._assert_band_panel("…no Trade flank, and the Trade content rides under Parties",
		not panel._wide_zone_hosts.has(BandCityPanel.ZONE_TRADE)
			and _find_named(parties, "TradeSection") != null)
	h._assert_band_panel("…in the SHORT tier: the full tab does not fit the strip's body",
		trade.is_short_tier() and _find_named(parties, TradeZoneController.SHORT_TIER_NAME) != null)
	h._assert_scroll_only_where_sanctioned()
	trade.open_list(TradeZoneController.KIND_ROUTE_BOTH)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_wide_route_list")
	h._assert_band_panel("the SHORT tier's route row opens BOTH directions (%d shipments)"
		% (BUSIEST_IMPORTS + BUSIEST_EXPORTS),
		_metas(list, TradeZoneController.ROW_SHIPMENT_META).size() == BUSIEST_IMPORTS + BUSIEST_EXPORTS)
	_assert_hangs_from_its_row("trade_tab_wide_route_list", trade, true)
	trade.dismiss()

	# ---- 6b. A SHORT LIST NEAR THE BOTTOM EDGE STILL OPENS BELOW -------------------------------
	# The camps list off the network line: the row sits in the strip, with far more room above it
	# than below — and the five camps fit below anyway, so that is where it opens. Only a list that
	# does NOT fit below (the route list above) takes the bigger side.
	trade.open_list(TradeZoneController.KIND_CAMPS)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_wide_short_below")
	var visible_screen: Rect2 = h.get_viewport().get_visible_rect()
	var anchor := trade.anchor_rect()
	h._assert_band_panel("precondition: the row sits near the bottom edge — more room above it than below (%.0f vs %.0f)"
			% [anchor.position.y - visible_screen.position.y, visible_screen.end.y - anchor.end.y],
		anchor.position.y - visible_screen.position.y > visible_screen.end.y - anchor.end.y)
	_assert_hangs_from_its_row("trade_tab_wide_short_below", trade, false)
	h._assert_band_panel("…and the whole list fits below, unscrolled (5 camps)",
		list.get_child_count() == 5
			and trade._popover_scroll.size.y >= list.get_combined_minimum_size().y - ADJACENCY_TOLERANCE)
	trade.dismiss()

	# ---- 6c. A HOVER CARD OFF A ROW INSIDE THE POPOVER -------------------------------------------
	# The local list off the SHORT tier's own row, and a good's hover card from inside it: beside the
	# popover, never under it.
	trade.open_list(TradeZoneController.KIND_LOCAL)
	await h._settle()
	await h._settle()
	var bone_in_list := _row(list, TradeZoneController.ROW_GOOD_META, "bone")
	trade.show_hover_for(bone_in_list)
	await h._settle()
	await h._save("trade_tab_wide_local_hover")
	_assert_hangs_from_its_row("trade_tab_wide_local_hover", trade, trade.list_opened_above())
	var hover := trade.hover_card()
	var hover_screen := hover.get_global_rect()
	h._assert_band_panel("…and a hover card off a row inside it never sits under the popover",
		hover.visible and not hover_screen.intersects(trade.list_rect()))
	hover.dismiss()
	trade.dismiss()

	# ---- 6d. EVERY LIST AGAIN, AT A RAISED INTERFACE SCALE -------------------------------------
	# The Options slider's own lever (`ClientSettings.ui_scale` → `UiScaler` → the window's
	# `content_scale_factor`). The same claims, with the side each list opens left to the rule.
	h._apply_ui_scale(h.SCALE_STATE_UI_SCALE)
	await h._pin_canvas(SIDE_CANVAS)
	h._push_bands(_network(_busy_crossings()))
	panel.set_dock(SIDE_LEFT)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	await h._settle()
	await h._settle()
	for kind in [TradeZoneController.KIND_CAMPS,
			TradeZoneController.KIND_CAMPS + TradeZoneController.KIND_SCOPE_SEPARATOR + HudTradeVocab.COMMODITY_FOOD,
			TradeZoneController.KIND_CAMPS + TradeZoneController.KIND_SCOPE_SEPARATOR + "bone"]:
		trade.open_list(kind)
		await h._settle()
		await h._settle()
		var frame := "trade_tab_scaled_%s" % kind.replace(TradeZoneController.KIND_SCOPE_SEPARATOR, "_")
		await h._save(frame)
		_assert_hangs_from_its_row(frame, trade, WANT_EITHER)
		trade.dismiss()
		await h._settle()
	h._push_bands(_network(_many_crossings()))
	await h._settle()
	trade.open_list(TradeZoneController.KIND_ROUTE_IN)
	await h._settle()
	await h._settle()
	await h._save("trade_tab_scaled_more_shipments")
	_assert_hangs_from_its_row("trade_tab_scaled_more_shipments", trade, WANT_EITHER)
	trade.dismiss()
	await h._pin_canvas(WIDE_CANVAS)
	h._push_bands(_network(_busiest_crossings()))
	panel.set_dock(SIDE_BOTTOM)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	await h._settle()
	await h._settle()
	for kind in [TradeZoneController.KIND_ROUTE_BOTH, TradeZoneController.KIND_CAMPS,
			TradeZoneController.KIND_LOCAL]:
		trade.open_list(kind)
		await h._settle()
		await h._settle()
		var frame := "trade_tab_scaled_wide_%s" % kind
		await h._save(frame)
		_assert_hangs_from_its_row(frame, trade, WANT_EITHER)
		trade.dismiss()
		await h._settle()
	h._apply_ui_scale(1.0)
	await h._settle()

	# ---- 7. THE SHORT TIER IN THE TABBED SHELL ---------------------------------------------------
	await h._pin_canvas(NARROW_BOTTOM_CANVAS)
	panel.set_dock(SIDE_BOTTOM)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	await h._settle()
	await h._save("trade_tab_short")
	h._assert_shell_is_wide(false, "trade_tab_short")
	zone = panel._zones.get(BandCityPanel.ZONE_TRADE)
	h._assert_band_panel("a narrow bottom dock's Trade tab lands in the SHORT tier, measured against its box",
		trade.is_short_tier() and _find_named(zone, TradeZoneController.SHORT_TIER_NAME) != null)
	h._assert_zone_content_fits()

	# **EVERY COUNT ON THE TAB, AT ONE AND AT TWO** — the pairs themselves, so a count no fixture
	# reaches at 1 (a 1-tile link, a single import) is still claimed.
	var singular_ok := true
	for words in [HudTradeVocab.CAMP_WORDS, HudTradeVocab.TILE_WORDS, HudTradeVocab.RATING_WORDS,
			HudTradeVocab.GOOD_WORDS, HudTradeVocab.SHIPMENT_WORDS, HudTradeVocab.IMPORT_WORDS,
			HudTradeVocab.EXPORT_WORDS, HudTradeVocab.MORE_SHIPMENT_WORDS]:
		var one := HudTradeVocab.count_text(1, words)
		var two := HudTradeVocab.count_text(2, words)
		singular_ok = singular_ok and not one.ends_with("s") and two.ends_with("s") \
			and one.begins_with("1 ") and two.begins_with("2 ")
	h._assert_band_panel("every Trade count reads singular at 1 and plural at 2", singular_ok)

	# Hand the panel back as the run found it.
	if start_canvas == Vector2i.ZERO:
		h._release_canvas_pin()
	else:
		await h._pin_canvas(start_canvas)
	panel.set_dock(SIDE_LEFT)
	panel.set_active_tab(BandCityPanel.ZONE_WORK)
	h._push_bands([h._band_fixture()])
	await h._settle()

## **THE POPOVER HANGS FROM ITS ROW** — its top one `POPOVER_GAP` under the row's bottom (or, opened
## upward, its bottom one gap over the row's top), its left edge on the Trade column's and the row
## inside its span. Adjacency, not mere visibility, is the claim: a popover that opened in the middle
## of the map is visible too. `want_above` is `WANT_BELOW` / `WANT_ABOVE`, or `WANT_EITHER` where the
## side is the rule's to pick (the scaled pass, whose geometry differs).
##
## **AND IT IS AS TALL AS ITS CONTENT WHEREVER THE ROOM ALLOWS** — the card's height at least the
## content's, and nothing to scroll. Only a list that genuinely exceeds the room on its side scrolls.
const WANT_BELOW := 0
const WANT_ABOVE := 1
const WANT_EITHER := -1

func _assert_hangs_from_its_row(where: String, trade: TradeZoneController, want_above: Variant) -> void:
	var popover := trade.list_rect()
	var row := trade.anchor_rect()
	var above := trade.list_opened_above()
	var want := int(want_above) if not (want_above is bool) else (WANT_ABOVE if want_above else WANT_BELOW)
	var gap := HudTradeVocab.POPOVER_GAP
	var vertical := absf(popover.end.y + gap - row.position.y) <= ADJACENCY_TOLERANCE if above \
		else absf(popover.position.y - (row.end.y + gap)) <= ADJACENCY_TOLERANCE
	var horizontal := popover.position.x <= row.position.x + ADJACENCY_TOLERANCE \
		and popover.end.x >= row.end.x - ADJACENCY_TOLERANCE
	var side_ok := want == WANT_EITHER or above == (want == WANT_ABOVE)
	h._assert_band_panel("%s: the list opens %s its row (popover %s, row %s)" % [where,
			"ABOVE" if above else "under", str(popover), str(row)],
		trade.is_list_open() and side_ok and vertical and horizontal)
	var content := trade.list_content_height()
	var room := trade.list_room()
	var bar := trade._popover_scroll.get_v_scroll_bar()
	var overflow := bar.max_value - bar.page
	if content <= room + ADJACENCY_TOLERANCE:
		h._assert_band_panel("%s: the content fits its room (%.0f of %.0f), so the card is its full height (%.0f) and nothing scrolls (%.0f to scroll)"
				% [where, content, room, popover.size.y, overflow],
			popover.size.y >= content - ADJACENCY_TOLERANCE and overflow <= ADJACENCY_TOLERANCE)
	else:
		h._assert_band_panel("%s: the content exceeds its room (%.0f of %.0f), so the card fills the room (%.0f) and scrolls"
				% [where, content, room, popover.size.y],
			absf(popover.size.y - room) <= ADJACENCY_TOLERANCE and overflow > 0.0)

# ---- FIXTURES ------------------------------------------------------------------------------------

func _id(entity: int) -> int:
	return entity + BandFx.FIXTURE_BAND_ID_OFFSET

## The five camps, with `subject_crossings` on Ashfell and each member's own pooled food.
func _network(subject_crossings: Array) -> Array:
	var food_nets := {
		BARROWMERE: -FOOD_BARROW_OUT, BITTERBROOK: -FOOD_BITTER_OUT, ALDERFEN: 0.0, EMBERLY: 0.0,
	}
	if subject_crossings.is_empty():
		food_nets = {BARROWMERE: 0.0, BITTERBROOK: 0.0, ALDERFEN: 0.0, EMBERLY: 0.0}
	var links := {
		ASHFELL: [[BARROWMERE, 2, RUNG_PATH], [BITTERBROOK, 4, RUNG_TRAIL], [ALDERFEN, 5, ""]],
		BARROWMERE: [[ASHFELL, 2, RUNG_PATH], [EMBERLY, 3, RUNG_DIRT]],
		BITTERBROOK: [[ASHFELL, 4, RUNG_TRAIL]],
		ALDERFEN: [[ASHFELL, 5, ""]],
		EMBERLY: [[BARROWMERE, 3, RUNG_DIRT]],
	}
	var bands: Array = []
	for entity in [ASHFELL, BARROWMERE, BITTERBROOK, ALDERFEN, EMBERLY]:
		var band: Dictionary = h._band_fixture()
		band["entity"] = entity
		band["name"] = NAMES[entity]
		band["id"] = NAMES[entity]
		var at: Vector2i = POS[entity]
		band["pos"] = [at.x, at.y]
		band["current_x"] = at.x
		band["current_y"] = at.y
		band[HudTradeVocab.NETWORK_ID_KEY] = NETWORK_ID
		band[HudTradeVocab.NETWORK_SPAN_KEY] = NETWORK_SPAN
		var own_links: Array = []
		for link in links[entity]:
			own_links.append({HudTradeVocab.LINK_BAND_ID: _id(int(link[0])),
				HudTradeVocab.LINK_DISTANCE: int(link[1]), HudTradeVocab.LINK_RUNG_ID: String(link[2])})
		band[HudTradeVocab.POOLING_LINKS_KEY] = own_links
		if entity == ASHFELL:
			band[HudTradeVocab.CROSSINGS_KEY] = subject_crossings
		else:
			var net := float(food_nets[entity])
			var own: Array = [] if TradeLedger.is_even(net) else [
				BandFx.transfer_crossing(HudTradeVocab.COMMODITY_FOOD,
					HudTradeVocab.DIRECTION_IN if net > 0.0 else HudTradeVocab.DIRECTION_OUT,
					HudTradeVocab.CAUSE_POOLED, absf(net))]
			# Barrowmere is the ONE other camp that moved bone — the count-of-one case.
			if entity == BARROWMERE and not subject_crossings.is_empty():
				own.append(_x("bone", HudTradeVocab.DIRECTION_OUT, HudTradeVocab.CAUSE_POOLED,
					BARROWMERE_BONE_OUT, [_reading("density", "good"), _reading("length", "fair")]))
			band[HudTradeVocab.CROSSINGS_KEY] = own
		bands.append(band)
	return bands

func _reading(axis: String, band_name: String) -> Dictionary:
	return {"axis": axis, "value": 0.5, "band_name": band_name}

func _busy_crossings() -> Array:
	var bone_good := [_reading("density", "good"), _reading("length", "fair")]
	var bone_fair := [_reading("density", "fair"), _reading("length", "fair")]
	var bone_excellent := [_reading("density", "excellent"), _reading("length", "good")]
	var hide_tough := [_reading("toughness", "excellent"), _reading("suppleness", "good")]
	var wood_hard := [_reading("hardness", "good"), _reading("pliancy", "fair")]
	return [
		_x(HudTradeVocab.COMMODITY_FOOD, HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_POOLED, FOOD_POOLED_IN),
		_x(HudTradeVocab.COMMODITY_FODDER, HudTradeVocab.DIRECTION_OUT, HudTradeVocab.CAUSE_POOLED,
			FODDER_POOLED_OUT),
		_x("bone", HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_POOLED, BONE_GOOD_IN, bone_good),
		_x("bone", HudTradeVocab.DIRECTION_OUT, HudTradeVocab.CAUSE_POOLED, BONE_FAIR_OUT, bone_fair),
		_x("bone", HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_POOLED, BONE_EXCELLENT_IN, bone_excellent),
		_x("hide", HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_POOLED, HIDE_POOLED_IN, hide_tough),
		# A foreign people's party, landing hide and bone.
		BandFx.transfer_crossing("hide", HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_SHIPMENT_IN,
			HIDE_SHIPPED_IN, hide_tough, DUSKWATER_ID, DUSKWATER_NAME, FOREIGN_FACTION, IMPORT_PARTY),
		BandFx.transfer_crossing("bone", HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_SHIPMENT_IN,
			BONE_SHIPPED_IN, bone_good, DUSKWATER_ID, DUSKWATER_NAME, FOREIGN_FACTION, IMPORT_PARTY),
		# Our own shipment out, food and wood.
		BandFx.transfer_crossing(HudTradeVocab.COMMODITY_FOOD, HudTradeVocab.DIRECTION_OUT,
			HudTradeVocab.CAUSE_SHIPMENT_OUT, FOOD_SHIPPED_OUT, [], _id(BITTERBROOK), NAMES[BITTERBROOK],
			HudConst.PLAYER_FACTION_ID, EXPORT_PARTY),
		BandFx.transfer_crossing("wood", HudTradeVocab.DIRECTION_OUT, HudTradeVocab.CAUSE_SHIPMENT_OUT,
			WOOD_SHIPPED_OUT, wood_hard, _id(BITTERBROOK), NAMES[BITTERBROOK],
			HudConst.PLAYER_FACTION_ID, EXPORT_PARTY),
		# ⛔ NOT TRADE — a hunt's haul home and a split's dowry. The tab must not show either.
		_x(HudTradeVocab.COMMODITY_FOOD, HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_PARTY_HOME,
			FOOD_PARTY_HOME),
		BandFx.transfer_crossing(HudTradeVocab.COMMODITY_FOOD, HudTradeVocab.DIRECTION_OUT,
			HudTradeVocab.CAUSE_DOWRY_OUT, FOOD_DOWRY_OUT, [], _id(EMBERLY), NAMES[EMBERLY]),
	]

## The busy turn's local arm, with SIX imports from six parties — past the fold trigger.
func _many_crossings() -> Array:
	var list: Array = []
	for crossing in _busy_crossings():
		if TradeLedger.is_pooled(crossing):
			list.append(crossing)
	for i in range(MANY_IMPORTS):
		list.append(BandFx.transfer_crossing(HudTradeVocab.COMMODITY_FOOD, HudTradeVocab.DIRECTION_IN,
			HudTradeVocab.CAUSE_SHIPMENT_IN, MANY_IMPORT_AMOUNT_STEP * (i + 1), [],
			DUSKWATER_ID + i, "%s %d" % [DUSKWATER_NAME, i + 1], FOREIGN_FACTION,
			MANY_IMPORT_BASE_PARTY + i))
	return list

## The busy turn's local arm plus four more goods, and fifteen shipments — seven in, eight out, to
## and from a mix of our own people and a foreign one.
func _busiest_crossings() -> Array:
	var list: Array = []
	for crossing in _busy_crossings():
		if TradeLedger.is_pooled(crossing):
			list.append(crossing)
	for good in BUSIEST_EXTRA_GOODS:
		list.append(_x(good, HudTradeVocab.DIRECTION_IN, HudTradeVocab.CAUSE_POOLED, BUSIEST_EXTRA_AMOUNT,
			[_reading("grade", "fair")]))
	for i in range(BUSIEST_IMPORTS + BUSIEST_EXPORTS):
		var inbound := i < BUSIEST_IMPORTS
		var foreign := i % 2 == 0
		list.append(BandFx.transfer_crossing(HudTradeVocab.COMMODITY_FOOD,
			HudTradeVocab.DIRECTION_IN if inbound else HudTradeVocab.DIRECTION_OUT,
			HudTradeVocab.CAUSE_SHIPMENT_IN if inbound else HudTradeVocab.CAUSE_SHIPMENT_OUT,
			BUSIEST_AMOUNT_STEP * (i + 1), [], DUSKWATER_ID + i, "%s %d" % [DUSKWATER_NAME, i + 1],
			FOREIGN_FACTION if foreign else HudConst.PLAYER_FACTION_ID, BUSIEST_BASE_PARTY + i))
	return list

func _x(commodity: String, direction: int, cause: int, amount: float, readings: Array = []) -> Dictionary:
	return BandFx.transfer_crossing(commodity, direction, cause, amount, readings)

func _subject() -> Dictionary:
	return h._hud._band_labor.panel_band()

func _rung_catalog() -> Array:
	return [
		{"rung_key": RUNG_PATH, "order": 1, "display_name": "Path",
			"friction_multiplier": 1.0, "holds_link_to_tiles": 0},
		{"rung_key": RUNG_TRAIL, "order": 2, "display_name": "Trail",
			"friction_multiplier": 0.85, "holds_link_to_tiles": 6},
		{"rung_key": RUNG_DIRT, "order": 3, "display_name": "Dirt Road",
			"friction_multiplier": 0.6, "holds_link_to_tiles": 10},
		{"rung_key": RUNG_PAVED, "order": 4, "display_name": "Paved Road",
			"friction_multiplier": 0.35, "holds_link_to_tiles": 16},
	]

# ---- NODE QUERIES --------------------------------------------------------------------------------

func _find_named(node: Node, wanted: String) -> Node:
	if node == null:
		return null
	if String(node.name) == wanted:
		return node
	for child in node.get_children():
		var found := _find_named(child, wanted)
		if found != null:
			return found
	return null

func _find_all(node: Node, class_name_wanted: String) -> Array:
	var out: Array = []
	if node == null:
		return out
	if node.get_script() != null and node.get_script().get_global_name() == class_name_wanted:
		out.append(node)
	for child in node.get_children():
		out.append_array(_find_all(child, class_name_wanted))
	return out

## Every value of `meta` in tree order under `node`.
func _metas(node: Node, meta: StringName) -> Array:
	var out: Array = []
	if node == null:
		return out
	if node.has_meta(meta):
		out.append(node.get_meta(meta))
	for child in node.get_children():
		out.append_array(_metas(child, meta))
	return out

func _find_meta(node: Node, meta: StringName, value: Variant) -> Control:
	if node == null:
		return null
	if node.has_meta(meta) and node.get_meta(meta) == value:
		return node as Control
	for child in node.get_children():
		var found := _find_meta(child, meta, value)
		if found != null:
			return found
	return null

func _row(node: Node, meta: StringName, value: Variant) -> Control:
	return _find_meta(node, meta, value)

func _row_text(node: Node, meta: StringName, value: Variant) -> String:
	return _text_of(_row(node, meta, value))

## Every Label / Button face under `node`, joined — what the frame says in words.
func _text_of(node: Node) -> String:
	if node == null:
		return ""
	var parts: Array[String] = []
	if node is Label:
		parts.append((node as Label).text)
	elif node is Button:
		parts.append((node as Button).text)
	for child in node.get_children():
		parts.append(_text_of(child))
	return " ".join(parts)
