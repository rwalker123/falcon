extends RefCounted

## **A SCROLL REBUILT PER SNAPSHOT KEEPS THE PLAYER'S PLACE, AND DRAWS IT ON THE FIRST FRAME** —
## `band_panel_preview`'s PNG-less claims for the per-turn scroll carry, in a file of their own like the
## other satellites, driven from the harness's run order (`h`) and APPENDED LAST so no earlier frame
## moves. See `.claude/rules/client/harness-band-panel.md` -> "A scrolled list survives a turn".
##
## For each scrolled list the panel rebuilds on a snapshot (the Work sections, the Band zone, the
## Parties list, the Peoples roster, the Trade tab) the claim is about what the PLAYER SEES, not about
## the variable: scroll it partway down, take the zone's pixels, push the SAME roster through the real
## `update_band_alerts` (what a turn does), and for each of the first three DRAWN frames after it
## require the same row y, the same `scroll_vertical` and byte-identical pixels. Identical data must draw
## identical pixels; a frame drawn at the top of the list and corrected on the next is a flash.

const TRADE_TAB := preload("res://tools/band_panel_trade_tab.gd")

## The frames checked after the re-render: the first one is the frame that used to flash.
const FRAMES_AFTER_TURN := 3
## The list must really scroll, and the player must really be somewhere other than the top, or the
## claim passes on an unscrolled list.
const MIN_SCROLL_RANGE := 120.0
const MIN_BEFORE_OFFSET := 60
## How far down the list the player is, as a fraction of what it can scroll.
const SCROLL_FRACTION := 0.5
## Per-case fixture sizes.
const PARTY_COUNT := 24
const PARTY_ENTITY_BASE := 1100
const TIE_COUNT := 40
const TIE_ENTITY_BASE := 1300
const HISTORY_ROWS := 40
const BAND_ZONE_PAGE_CANVAS := Vector2i(1500, 420)
const NARROW_CANVAS := Vector2i(1500, 900)
const SHORT_CANVAS := Vector2i(1500, 480)
const BAND_ENTITY := 904
const LAYOUT_RETRIES := 3
## The second band the page-change claims cycle to, and how many roads the roster fixture keeps so the
## EXPANDED roster really scrolls.
const OTHER_BAND_ENTITY := 905
const ROSTER_ROAD_COUNT := 40
const ROSTER_ROAD_ORIGIN := Vector2i(72, 18)

var h

func run(harness) -> void:
	h = harness
	h._pin_canvas(NARROW_CANVAS)
	h._panel.set_dock(SIDE_LEFT)

	h._hud.update_food_modules(h._many_forage_modules())
	h._set_forage_patches(h._many_source_patch_fixtures())
	var many := [h._many_sources_band_fixture()]
	h._push_bands(many)
	h._panel.set_active_tab(&"work")
	await _carry("Work sections", HudWorkVocab.WORK_SECTIONS_SCROLL_NAME, many)
	await _page_change("Work sections", HudWorkVocab.WORK_SECTIONS_SCROLL_NAME, many)
	await _roster_page_change()

	# The Band tab only overflows a short box.
	h._pin_canvas(SHORT_CANVAS)
	var band_roster := [h._arrivals_band_fixture()]
	h._push_bands(band_roster)
	h._panel.set_active_tab(&"band")
	await _carry("Band zone", HudWorkVocab.BAND_ZONE_SCROLL_NAME, band_roster)
	# The faction page's Band zone is shorter than a band's, so a still shorter canvas keeps its range
	# above the carried offset.
	h._pin_canvas(BAND_ZONE_PAGE_CANVAS)
	await _page_change("Band zone", HudWorkVocab.BAND_ZONE_SCROLL_NAME, band_roster)
	h._pin_canvas(SHORT_CANVAS)
	h._pin_canvas(NARROW_CANVAS)

	var parties: Array = [h._band_fixture()]
	for i in PARTY_COUNT:
		var party: Dictionary = h._raid_party_fixture().duplicate(true)
		party["entity"] = PARTY_ENTITY_BASE + i
		party["id"] = "Raiders %d" % (i + 1)
		parties.append(party)
	h._push_bands(parties)
	h._panel.set_active_tab(&"parties")
	await _carry("Parties list", HudWorkVocab.PARTIES_LIST_NAME, parties)
	# The second band keeps a run of parties of its own, so the list exists on the page cycled TO.
	var other_parties: Array = []
	for i in PARTY_COUNT:
		var party: Dictionary = h._raid_party_fixture().duplicate(true)
		party["entity"] = PARTY_ENTITY_BASE + PARTY_COUNT + i
		party["id"] = "Hunters %d" % (i + 1)
		party["home_band_entity"] = OTHER_BAND_ENTITY
		other_parties.append(party)
	await _page_change("Parties list", HudWorkVocab.PARTIES_LIST_NAME, parties, other_parties)

	var roster: Array = [h._band_fixture()]
	h._push_bands(roster)
	var ties: Array = []
	for i in TIE_COUNT:
		ties.append({"observer_band_id": BAND_ENTITY + h.FIXTURE_BAND_ID_OFFSET,
			"subject_band_id": TIE_ENTITY_BASE + i + h.FIXTURE_BAND_ID_OFFSET,
			"subject_name": "Reedfolk %d" % (i + 1), "strength": 0.5, "first_contact_turn": 1,
			"last_contact_turn": 1, "last_seen_x": 71, "last_seen_y": 18, "last_seen_turn": 1})
	h._hud.update_connections(ties)
	h._panel.set_active_tab(&"peoples")
	await _carry("Peoples roster", HudConnectionsVocab.LIST_NAME, roster)
	await _page_change("Peoples roster", HudConnectionsVocab.LIST_NAME, roster)
	h._hud.update_connections([])

	var trade := TRADE_TAB.new()
	trade.h = h
	var history: Array = []
	for i in HISTORY_ROWS:
		history.append(trade._wide_history()[i % trade._wide_history().size()].duplicate(true))
	h._hud.update_trade_history(history)
	h._panel.set_active_tab(&"trade")
	await _carry("Trade tab", TradeZoneController.TAB_SCROLL_NAME, roster)
	# ...and the second band deals too, so the Trade tab exists on the page cycled TO.
	var other_history: Array = []
	for row in history:
		var dealing: Dictionary = (row as Dictionary).duplicate(true)
		dealing["own_band"] = OTHER_BAND_ENTITY + h.FIXTURE_BAND_ID_OFFSET
		other_history.append(dealing)
	h._hud.update_trade_history(history + other_history)
	await _page_change("Trade tab", TradeZoneController.TAB_SCROLL_NAME, roster)
	h._hud.update_trade_history([])
	h._push_bands([h._band_fixture()])
	h._panel.set_active_tab(&"work")
	await h._settle()

## One list: scroll it, re-render it the way a turn does, and watch the first drawn frames.
func _carry(label: String, scroll_name: String, roster: Array) -> void:
	await h._settle()
	var scroll := _find(h._panel, scroll_name)
	if scroll == null:
		h._fail("%s: no scroll named %s on screen" % [label, scroll_name])
		return
	var bar := scroll.get_v_scroll_bar()
	var scrollable := bar.max_value - bar.page
	h._assert_band_panel("%s: PRECONDITION - the list really scrolls (%.0f px of range)" % [label, scrollable],
		scrollable >= MIN_SCROLL_RANGE)
	scroll.scroll_vertical = int(scrollable * SCROLL_FRACTION)
	await h._settle()
	var before_offset := scroll.scroll_vertical
	h._assert_band_panel("%s: PRECONDITION - the player is well below the top (%d)" % [label, before_offset],
		before_offset >= MIN_BEFORE_OFFSET)
	var content := scroll.get_child(0) as Control
	var before_y := content.global_position.y
	var rect_px := _pixel_rect(scroll)
	var before_image := _crop(rect_px)

	h._push_bands(roster)
	for frame in FRAMES_AFTER_TURN:
		await RenderingServer.frame_post_draw
		var now := _find(h._panel, scroll_name)
		if now == null:
			h._fail("%s: frame %d: the list vanished" % [label, frame + 1])
			continue
		var tag := "%s: drawn frame %d after the turn" % [label, frame + 1]
		h._assert_band_panel("%s - the list was rebuilt (a new node), so the carry is what is under test" % tag,
			now != scroll)
		var now_content := now.get_child(0) as Control
		h._assert_band_panel("%s - a row sits at the same y (%.1f then, %.1f now)"
				% [tag, before_y, now_content.global_position.y],
			is_equal_approx(before_y, now_content.global_position.y))
		h._assert_band_panel("%s - scroll_vertical is the player's (%d then, %d now)"
				% [tag, before_offset, now.scroll_vertical], now.scroll_vertical == before_offset)
		var after_image := _crop(rect_px)
		h._assert_band_panel("%s - the zone's pixels are identical (%d bytes differ)"
				% [tag, _differing(before_image, after_image)],
			before_image != null and after_image != null
				and before_image.get_data() == after_image.get_data())
	await h._settle()

## **A PAGE CHANGE STARTS EVERY LIST AT THE TOP.** Another band, and the faction page, are a different
## subject: band A's place in a list must not open band B's, nor come back when the player cycles home.
## Reached through the REAL cycler. The list is scrolled on A, then B is shown (when B has the list it
## must read 0), then A again (it exists there, so this half is never vacuous), then the faction page and
## back.
func _page_change(label: String, scroll_name: String, roster: Array, other_extras: Array = []) -> void:
	var other: Dictionary = (roster[0] as Dictionary).duplicate(true)
	other["entity"] = OTHER_BAND_ENTITY
	other["id"] = "Band 3"
	var both: Array = roster.duplicate()
	both.insert(1, other)
	both.append_array(other_extras)
	h._push_bands(both)
	_sync_selection_roster(both)
	await h._settle()
	var carried := await _scroll_down(label, scroll_name)
	h._hud.cycle_panel_band(BandCityPanel.CYCLE_NEXT)
	await h._settle()
	await _expect_top("%s: another band" % label, scroll_name, carried, true)
	carried = await _scroll_down(label, scroll_name)
	h._hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	await h._settle()
	await _expect_top("%s: back on the first band" % label, scroll_name, carried, true)
	carried = await _scroll_down(label, scroll_name)
	h._hud.cycle_panel_band(BandCityPanel.CYCLE_PREV)
	await h._settle()
	await _expect_top("%s: the faction page" % label, scroll_name, carried, false)
	h._hud.cycle_panel_band(BandCityPanel.CYCLE_NEXT)
	await h._settle()
	await _expect_top("%s: from the faction page to the first band" % label, scroll_name, carried, true)
	h._push_bands(roster)
	await h._settle()

## The cycler reaches a band through the tile's occupant roster, which a real snapshot refreshes with the
## band's own dict. The harness's roster is whatever the last tile selection left, so a band the cycler
## walks back to would be drawn from that stale copy (4 rows, not the 34 just pushed) and its list
## would be too short to scroll. Put the pushed bands into it.
func _sync_selection_roster(bands: Array) -> void:
	var units: Array = h._hud._selectioncard._selection.roster_units()
	for band in h._stamp_band_ids(bands):
		var entity := int((band as Dictionary).get("entity", -1))
		var replaced := false
		for i in units.size():
			if int((units[i] as Dictionary).get("entity", -1)) == entity:
				units[i] = band
				replaced = true
		if not replaced:
			units.append(band)

## The expanded road roster is the Work tab's second list; it is scrolled on its own.
func _roster_page_change() -> void:
	var roads: Array = []
	# Both bands keep a long run of roads, so the roster list exists on the page the player cycles TO.
	for i in ROSTER_ROAD_COUNT:
		roads.append(h._roster_road_row(ROSTER_ROAD_ORIGIN + Vector2i(i, 0),
			HudRouteVocab.RUNG_KEY_PATH, h.ROSTER_BAND_ID, true))
		roads.append(h._roster_road_row(ROSTER_ROAD_ORIGIN + Vector2i(i, 1),
			HudRouteVocab.RUNG_KEY_PATH, h.ROSTER_OTHER_BAND_ID, true))
	h._hud.update_route_rungs(h._road_queue_catalog())
	h._hud.update_road_network(roads)
	var band: Dictionary = h._roster_band_fixture(h.ROSTER_ROADWORK_DEMAND)
	h._hud._bandpanel._roster_expanded = &""
	h._hud._bandpanel._toggle_roster_expanded(HudConst.LABOR_KIND_ROADWORK)
	await _page_change("Expanded roster", HudWorkVocab.ROSTER_EXPANDED_SCROLL_NAME, [band])
	h._hud._bandpanel._roster_expanded = &""
	h._hud.update_road_network([])
	h._hud.update_route_rungs([])
	h._push_bands([h._many_sources_band_fixture()])
	await h._settle()

func _scroll_down(label: String, scroll_name: String) -> int:
	await h._settle()
	var scroll := _find(h._panel, scroll_name)
	# A list rebuilt this frame can report no range until it has been laid out once more.
	for retry in LAYOUT_RETRIES:
		if scroll != null and _range_of(scroll) >= MIN_SCROLL_RANGE:
			break
		h._hud._bandpanel.rerender()
		await h._settle()
		scroll = _find(h._panel, scroll_name)
	if scroll == null:
		h._fail("%s: no scroll named %s to scroll" % [label, scroll_name])
		return 0
	var scrollable := _range_of(scroll)
	h._assert_band_panel("%s: PRECONDITION - the list really scrolls (%.0f px; band %d)"
			% [label, scrollable, int(h._hud._band_labor.panel_band().get("entity", -1))],
		scrollable >= MIN_SCROLL_RANGE)
	scroll.scroll_vertical = int(scrollable * SCROLL_FRACTION)
	await h._settle()
	return scroll.scroll_vertical

func _range_of(scroll: ScrollContainer) -> float:
	var bar := scroll.get_v_scroll_bar()
	return bar.max_value - bar.page

## **THE DESTINATION LIST MUST BE ABLE TO HOLD THE CARRIED PLACE**, or reading 0 proves nothing: the
## precondition is that its scroll range exceeds the offset just left behind, so a carried offset would
## show. `must_exist` is false only for the faction page, where a list the page does not draw (the
## narrow-shell Peoples and Trade tabs) has no place to carry.
func _expect_top(label: String, scroll_name: String, carried: int, must_exist: bool) -> void:
	var scroll := _find(h._panel, scroll_name)
	if scroll == null:
		h._assert_band_panel("%s - the list is on that page" % label, not must_exist)
		return
	h._assert_band_panel("%s - PRECONDITION - the list could hold the carried place (%.0f px of range, left at %d)"
			% [label, _range_of(scroll), carried], _range_of(scroll) > carried)
	h._assert_band_panel("%s - starts at the top (scroll_vertical %d)" % [label, scroll.scroll_vertical],
		scroll.scroll_vertical == 0)

func _find(node: Node, wanted: String) -> ScrollContainer:
	if node is ScrollContainer and String(node.name) == wanted \
			and not node.is_queued_for_deletion() and (node as Control).is_visible_in_tree():
		return node
	for child in node.get_children():
		var found := _find(child, wanted)
		if found != null:
			return found
	return null

## The scroll's rect in the captured image's pixels (the window is scaled against the canvas).
func _pixel_rect(scroll: Control) -> Rect2i:
	var vp: Viewport = h.get_viewport()
	var xf: Transform2D = vp.get_final_transform() * vp.get_canvas_transform()
	return Rect2i(xf * scroll.get_global_rect())

func _crop(rect_px: Rect2i) -> Image:
	var image: Image = h.get_viewport().get_texture().get_image()
	if image == null:
		return null
	return image.get_region(rect_px.intersection(Rect2i(Vector2i.ZERO, image.get_size())))

func _differing(a: Image, b: Image) -> int:
	if a == null or b == null:
		return -1
	var da := a.get_data()
	var db := b.get_data()
	if da.size() != db.size():
		return -1
	var count := 0
	for i in da.size():
		if da[i] != db[i]:
			count += 1
	return count
