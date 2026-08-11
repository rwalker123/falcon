extends RefCounted

## THE SHIPMENT — the cargo picker and a trade party in flight (arc #527, issue #517).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## **THE TWO HALVES ANSWER DIFFERENT QUESTIONS, so both are here.** The PICKER is the only surface in
## the client that reads the `connections` section at all, and its whole point is what it refuses:
## a parked tie is listed and greyed, a remembered position is worded as remembered, and the mass
## meter moves before the server ever sees a manifest. The PARTY is the readout on the other side of
## the send, whose rows are its own — no quarry, no floor, no delivery ETA.
##
## It ends by releasing the panel and handing the reference band back, so a chapter appended after it
## starts where every other one does.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## `Main`'s reservation rules, borrowed rather than restated — the `crafting_bench` convention.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

const BAND_PANEL_RESERVER := &"band_panel"

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The SENDING band and the two bands it holds ties with — their own entities, so the states below
## cannot be confused with the reference band the rest of the run uses. `BandFx.with_band_id` derives
## each `band_id` from the entity with a fixed offset, which is what keeps a payload that sent the
## ENTITY distinguishable from one that sent the handle.
const SHIPPER_ENTITY := 981
const NEIGHBOUR_ENTITY := 982
const PARKED_ENTITY := 983

## The trade party in flight — an expedition cohort, so it must not collide with a band entity.
const TRADE_PARTY_ENTITY := 7181

## **THE TWO SHIPMENT-MASS LEVERS, AS THE SIM PUBLISHES THEM.** The cap is `party × per_worker_carry`
## and a material unit costs `material_carry_weight` of pack space, so a 4-worker party carries 40 and
## one hide costs 2. Stated here rather than borrowed from the hunt lever, which is a DIFFERENT pack:
## a fixture that reused it would render a cap `send_trade_expedition` refuses.
const TRADE_PER_WORKER_CARRY := 10.0
const TRADE_MATERIAL_CARRY_WEIGHT := 2.0

## The party the picker is driven to, and therefore the cap the meter is drawn against (4 × 10 = 40).
const TRADE_PARTY_WORKERS := 4

## …and the party the over-cap state falls back to: one worker carries 10, which the same 20-mass
## manifest cannot fit into. The refusal is the CLIENT's courtesy — the server's own is unchanged and
## remains the authority — so the state exists to show the player never has to meet it.
const OVER_CAP_PARTY_WORKERS := 1

## A full tie and a PARKED one. Zero is not "no tie" — it is the tie at rest, "we know such a people
## exist and have no current dealings" — and the picker must list it, disabled, rather than hide it.
const TIE_STRENGTH_LIVE := 0.75
const TIE_STRENGTH_PARKED := 0.0

## Where each subject was the last time this band SAW them, and on which turn. A connection grants
## `Discovered` and never `Seen`, so these are remembered positions and the sheet must word them so.
const NEIGHBOUR_LAST_SEEN := Vector2i(63, 21)
const NEIGHBOUR_LAST_SEEN_TURN := 38
const PARKED_LAST_SEEN := Vector2i(44, 9)
const PARKED_LAST_SEEN_TURN := 12

## The band's own tile. Far enough from the remembered neighbour that the walk is a real number of
## turns rather than zero, which is what makes the `≈ … out` line appear at all.
const SHIPPER_TILE := Vector2i(71, 18)

## Tiles per turn, echoed per-cohort as the sim does. Without it there is no travel line at all —
## the client quotes no fabricated arrival — so it is what makes the `≈` claim below checkable.
const BAND_MOVE_TILES_PER_TURN := 2.0

## What the shipper holds. The larder is one commodity and so one row; the materials are FOUR PILES —
## two of them `hide` at different ratings, which is the whole reason a manifest row shows its rating.
const SHIPPER_PROVISIONS := 84.0

## What the player loads: food, plus one pile of hide. `12 + 2 × 4 = 20` of a 40 cap, so the meter is
## half full and the send is live — the state a picker exists to reach.
const LOADED_FOOD := 12.0
const LOADED_HIDE := 4.0

## What the in-flight party is carrying, and the pack it fills. The cap is the SHIPMENT lever's
## product (4 × 10), which is what the sim publishes on a trade party's `expeditionCarryCap`.
const PARTY_CARGO_FOOD := 12.0
const PARTY_CARGO_CAP := 40.0

## The two material rows a shipment carries on the wire — `MaterialPayoff`, one per material, NEVER
## summed. A readout that added them would be the retired trade axis under a new name.
const PARTY_CARGO_HIDE := 4.0
const PARTY_CARGO_BONE := 1.2

## The band's Food line under a transfer (arc #527). `balance_supply_networks` has moved food between
## neighbouring larders every turn since turn one, so these two terms are not a trade feature: a band
## co-networked with a neighbour has both, and before them the Food line was short by the whole move.
const TRANSFER_RECEIVED := 1.4
const TRANSFER_SENT := 0.5

func run(harness) -> void:
	h = harness
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	# Fan the panel's reservation onto the HUD through `Main`'s own publisher, the `tile_panel`
	# convention — behaviour-neutral on a vertical dock, and routed anyway so a later re-dock cannot
	# leave this harness fanning out by a rule the client stopped using.
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, BAND_PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	panel.set_dock(SIDE_RIGHT)
	panel.set_active_tab(BandCityPanel.ZONE_PARTIES)
	h._hud.set_band_city_panel(panel)

	# The roster the picker resolves a tie's subject against: a tie carries only ids, so a subject
	# still in the roster is named exactly as the cycler names it and one that is not is named by
	# where it was. Both cases are staged.
	var roster: Array = [_shipper_band(), _neighbour_band()]
	h._hud.update_band_alerts(roster)
	h._hud.update_connections(_connections())
	h._hud.show_unit_selection(_shipper_band())
	await h._settle()
	# **STATE — THE FOOTER ITSELF, before anything is composed.** It exists to show the FIFTH button
	# beside the other four and, just as importantly, to show that its glyph DRAWS: a mark missing
	# from this client's fallback font renders as an invisible gap rather than as a tofu box, which no
	# assertion catches and only a rendered frame does.
	await h._save("trade_footer")

	# **STATE — THE SHIPMENT FORM, OPENED FROM THE PARTIES FOOTER.** Driven through the real mission
	# button rather than by setting the mission: the button is what arms the composing act, and a
	# harness that set the flag would render a sheet no player can reach.
	var trade_btn := _mission_button(HudComposeVocab.COMPOSE_MISSION_TRADE)
	h._assert_hud("the parties footer offers the shipment mission", trade_btn != null)
	if trade_btn != null:
		trade_btn.emit_signal("pressed")
	await h._settle()
	await h._save("trade_picker_empty")
	# The picker lists BOTH ties, and the parked one carries its reason IN ITS OWN LABEL. Asserted on
	# the fresh sheet, before a destination is chosen, because that is the state a player meets first.
	var picker := _destination_picker()
	h._assert_hud("the destination picker exists", picker != null)
	if picker != null:
		h._assert_hud("…listing both the live tie and the parked one",
			picker.item_count == _connections().size())
		h._assert_hud("…with the PARKED tie shown, disabled, carrying its reason",
			picker.is_item_disabled(1) and picker.get_item_text(1).contains(
				HudComposeVocab.COMPOSE_DESTINATION_PARKED_REASON))
	# **AN UNCHOSEN DESTINATION CANNOT SEND**, and the button says so rather than vanishing.
	var blocked := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("…and the send is present and disabled until a destination is named",
		blocked is Button and (blocked as Button).disabled)

	# **STATE — A DESTINATION CHOSEN.** Picked through the picker's own `item_selected`, so the real
	# `on_pick` runs; setting the member would test the harness instead.
	if picker != null:
		picker.emit_signal("item_selected", 0)
	await h._settle()
	await h._save("trade_picker_destination")
	var sheet_text := _sheet_text()
	# THE KEYSTONE, RENDERED: the position under the picker is where they WERE, and it says so.
	h._assert_hud("the destination's position is worded as REMEMBERED, not live",
		sheet_text.contains(HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT % [
			NEIGHBOUR_LAST_SEEN.x, NEIGHBOUR_LAST_SEEN.y, NEIGHBOUR_LAST_SEEN_TURN]))
	h._assert_hud("…and the walk quoted from it is approximate",
		sheet_text.contains(TRADE_APPROXIMATE_MARK))
	# **A MATERIAL ROW SHOWS ITS RATING.** Two piles of `hide` are two rows and are not the same
	# thing; the assertion names the rating so a row that dropped it cannot pass.
	h._assert_hud("a material row names the pile's rating, not just its material",
		sheet_text.contains(EXCELLENT_HIDE_ROW))

	# **STATE — A LOADED MANIFEST AND A LIVE MASS METER.** The party is raised through its own
	# stepper first, because the party is the CAP's other term: a manifest priced against whatever
	# count the previous chapter left behind would be measured against a pack nobody chose.
	_set_party(TRADE_PARTY_WORKERS)
	await h._settle()
	# Loaded through the rows' own `+` presses, so the clamp-to-the-pile and the meter are both
	# exercised by the controls a player uses.
	_load(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, LOADED_FOOD)
	await h._settle()
	_load(EXCELLENT_HIDE_ROW, LOADED_HIDE)
	await h._settle()
	await h._save("trade_cargo_loaded")
	# The meter reads the sim's own expression: food + weight × materials, against party × the pack
    # lever. Composed here from the fixture's side so the two arrive at one number from opposite ends.
	var expected_mass := LOADED_FOOD + TRADE_MATERIAL_CARRY_WEIGHT * LOADED_HIDE
	var expected_cap := float(TRADE_PARTY_WORKERS) * TRADE_PER_WORKER_CARRY
	var meter := Q.find_meta_node(_parties_zone(), BandPanelController.TRADE_MASS_METER_META)
	h._assert_hud("the mass meter exists", meter is Label)
	if meter is Label:
		h._assert_hud("…and states the manifest's mass against the party's pack",
			(meter as Label).text.contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % expected_mass)
				and (meter as Label).text.contains(
					HudCraftingVocab.BATCH_AMOUNT_FORMAT % expected_cap))
	var live_send := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("a manifest under the cap can be sent",
		live_send is Button and not (live_send as Button).disabled)

	# **STATE — THE SAME MANIFEST OVER THE CAP.** The party shrinks to one worker, so the cap falls to
	# 10 against a mass of 20 and the send refuses BEFORE the server has to. The refusal is the
	# client's courtesy; the server's own remains the authority.
	_set_party(OVER_CAP_PARTY_WORKERS)
	await h._settle()
	await h._save("trade_cargo_over_cap")
	var over_send := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("an over-cap manifest cannot be sent",
		over_send is Button and (over_send as Button).disabled)
	h._assert_hud("…and the sheet says which way to fix it",
		_sheet_text().contains(HudComposeVocab.COMPOSE_CARGO_OVER_CAP_REASON))

	# **STATE — THE FOOD LINE WITH A TRANSFER IN IT.** Not a trade readout: the supply network moves
	# food between neighbouring larders every turn, so any co-networked band carries these two terms
	# and the Food line was short by exactly them before this arc.
	h._hud._bandpanel._close_party_compose()
	panel.set_active_tab(BandCityPanel.ZONE_BAND)
	var transferring := _shipper_band()
	transferring["transfer_received"] = TRANSFER_RECEIVED
	transferring["transfer_sent"] = TRANSFER_SENT
	h._hud.update_band_alerts([transferring, _neighbour_band()])
	h._hud.show_unit_selection(transferring)
	await h._settle()
	await h._save("trade_food_ledger")
	# …and the same Food row OPENED, which is where the two terms are itemized. The headline alone
	# passes on a client that folded the transfer into some other row, so the breakdown is what makes
	# the claim about these two rows rather than about the net.
	_click_food_breakdown()
	await h._settle()
	await h._save("trade_food_transfers")
	var breakdown := _collect_text(h)
	h._assert_hud("the Food breakdown itemizes what arrived from other bands",
		breakdown.contains(DetailFormat.FOOD_LABEL_TRANSFER_RECEIVED))
	h._assert_hud("…and what left for them, as its own row",
		breakdown.contains(DetailFormat.FOOD_LABEL_TRANSFER_SENT))
	_click_food_breakdown()
	await h._settle()

	# **STATE — A TRADE PARTY IN FLIGHT.** Judged on what it does NOT say as much as on what it does:
	# a shipment publishes no floor, no delivery ETA and no trip bound, so none of those rows may
	# appear, and what stands in their place is who it is bound for and what is in the packs.
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(&"band_panel", SIDE_RIGHT, 0.0)
	panel.queue_free()
	await h.get_tree().process_frame
	h._hud.show_unit_selection(_trade_party())
	await h._settle()
	await h._save("trade_party_panel")
	var party_text: String = h._hud.occupant_detail.text
	h._assert_hud("a trade party names its MISSION",
		party_text.contains(HudExpeditionVocab.EXPEDITION_MISSION_LABELS[
			HudExpeditionVocab.EXPEDITION_MISSION_TRADE]))
	# **THE NAME IS THE CLIENT'S, because the sim publishes none.** The fixture's
	# `expeditionDestinationName` is `""` — the live shape — so this row can only read `Band 2` by
	# joining the roster on `expeditionDestinationBand`, which is the whole point of the fallback.
	h._assert_hud("…and who it is bound for, by NAME",
		Readout.detail_excerpt(party_text, BandDetailLines.TRADE_DESTINATION_ROW).contains(
			NEIGHBOUR_DISPLAY_NAME))
	# **THE KEY IS NEVER RENDERED.** `expeditionDestinationBand` is what the command addresses; a
	# player must never see it, and the fixture's id is distinctive enough to find if it leaked.
	h._assert_hud("…never by the id the command addresses",
		not party_text.contains(str(BandFx.FIXTURE_BAND_ID_OFFSET + NEIGHBOUR_ENTITY)))
	# **ONE TERM PER MATERIAL, NEVER SUMMED** — the sum is asserted ABSENT, because a row that added
	# hide to bone would still render two plausible numbers and every other assertion would pass.
	var cargo_row := Readout.detail_excerpt(party_text, BandDetailLines.TRADE_CARGO_ROW)
	h._assert_hud("the shipment's materials are one term per material",
		cargo_row.contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % PARTY_CARGO_HIDE)
			and cargo_row.contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % PARTY_CARGO_BONE))
	h._assert_hud("…and are never summed into one figure",
		not cargo_row.contains(
			HudCraftingVocab.BATCH_AMOUNT_FORMAT % (PARTY_CARGO_HIDE + PARTY_CARGO_BONE)))
	# The hunt-only rows must be ABSENT: a shipment carries no floor and no stop to report.
	h._assert_hud("a shipment states no ORDERS row — it carries no floor",
		not party_text.contains(TRADE_ABSENT_ORDERS_KEY))

	# Hand the reference band back, so a chapter appended after this one starts where the rest do.
	h._hud.update_band_alerts([BandFx.band_fixture()])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()

## The `≈` every figure derived from a REMEMBERED position wears. Read off the vocabulary's own format
## rather than typed, so a reworded sentence that dropped the mark fails here.
const TRADE_APPROXIMATE_MARK := "≈"

## The `Orders:` key a HUNT party states and a shipment must not. Spelled here because the assertion
## is an ABSENCE and there is no producer to read it from on this path.
const TRADE_ABSENT_ORDERS_KEY := "Orders:"

## The excellent-hide pile's row face, composed exactly as the sheet composes it, so the rating
## assertion and the row it is about cannot drift.
const EXCELLENT_HIDE_ROW := "hide · tough: excellent · supple: poor"

## What the neighbour is called on EVERY surface, and it is resolved the same way on each: the
## picker, the parties row and the drawer's `Bound for` all join `HudBandLaborState.band_label_for_id`
## onto the band's durable id, so a band cannot be called two things on two screens. Band 2 of 2 in
## the roster this chapter pushes.
const NEIGHBOUR_DISPLAY_NAME := "Band 2"

## The parties zone's live column — where every control this chapter drives is mounted.
func _parties_zone() -> Node:
	return h._hud._bandpanel._parties_zone_col

## The footer's mission button for one verb, found by the meta every launch button carries: their
## faces are vocabulary this chapter would otherwise be asserting its own copy of.
func _mission_button(mission: String) -> Button:
	var found := _find_mission_button(_parties_zone(), mission)
	return found

func _find_mission_button(root: Node, mission: String) -> Button:
	if root == null:
		return null
	if root is Button and String((root as Button).get_meta(
			HudWidgets.MISSION_LAUNCH_META, "")) == mission:
		return root as Button
	for child in root.get_children():
		var found := _find_mission_button(child, mission)
		if found != null:
			return found
	return null

## The destination picker — the sheet's one `OptionButton`, which is also the only one in this zone.
func _destination_picker() -> OptionButton:
	return _find_option_button(_parties_zone())

func _find_option_button(root: Node) -> OptionButton:
	if root == null:
		return null
	if root is OptionButton:
		return root as OptionButton
	for child in root.get_children():
		var found := _find_option_button(child)
		if found != null:
			return found
	return null

## Every word the open sheet renders, so an assertion can be made about the form as a whole rather
## than about whichever Label happened to hold a phrase.
func _sheet_text() -> String:
	return _collect_text(_parties_zone())

func _collect_text(root: Node) -> String:
	var text := ""
	if root is Label:
		text += (root as Label).text + "\n"
	elif root is RichTextLabel:
		text += (root as RichTextLabel).text + "\n"
	elif root is Button:
		text += (root as Button).text + "\n"
	for child in root.get_children():
		text += _collect_text(child)
	return text

## Open (or close) the shipper band's Food breakdown the way a CLICK does — `meta_clicked` on the live
## label carrying the row's own `[url]`, so the bound handler and its anchor run exactly as in the
## game. The key is `DetailFormat.breakdown_key`'s, composed from the band this chapter renders.
func _click_food_breakdown() -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX \
		+ HudDisclosureVocab.BREAKDOWN_KIND_FOOD + BREAKDOWN_KEY_SEPARATOR + str(SHIPPER_ENTITY)
	# **SEARCHED FROM THE HARNESS ROOT, not from the HUD.** A player band's detail renders into the
	# Band/City panel, which is a sibling CanvasLayer rather than a child of the HUD — a HUD-rooted
	# walk finds nothing and the click silently never happens.
	var label := _find_meta_label(h, meta)
	if label == null:
		_fail_no_disclosure(meta)
		return
	label.meta_clicked.emit(meta)

## What `DetailFormat.breakdown_key` puts between a breakdown's kind and its band entity.
const BREAKDOWN_KEY_SEPARATOR := ":"

func _fail_no_disclosure(meta: String) -> void:
	# **A CLICK THAT NEVER HAPPENED IS A FAILED PRECONDITION, NOT AN ADVISORY** — the assertions that
	# follow are about what the OPEN popover holds, and every one of them passes on a drawer that
	# rendered no disclosure at all.
	h._fail("no detail label offering '%s' — the Food disclosure was never rendered" % meta)

func _find_meta_label(node: Node, meta: String) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).text.contains("[url=%s]" % meta):
		return node
	for child in node.get_children():
		var found := _find_meta_label(child, meta)
		if found != null:
			return found
	return null

## Drive the PARTY stepper to `target` through its own −/+ buttons, one press at a time. The stepper
## row carries its live count as meta (`PARTY_STEPPER_COUNT_META`), so the loop reads the control's
## own answer rather than counting presses — a clamp the sheet applies is then visible as a loop that
## stops, instead of being papered over.
func _set_party(target: int) -> void:
	var guard := 0
	while guard < PARTY_STEPPER_MAX_PRESSES:
		var row := _party_stepper_row(_parties_zone())
		if row == null:
			break
		var count := int(row.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META, 0))
		if count == target:
			return
		var index := row.get_child_count() - 1 if count < target else PARTY_STEPPER_MINUS_INDEX
		var step := row.get_child(index)
		if not (step is Button) or (step as Button).disabled:
			break
		(step as Button).emit_signal("pressed")
		guard += 1
	h._assert_hud("the party stepper reached %d" % target,
		_party_stepper_count() == target)

func _party_stepper_count() -> int:
	var row := _party_stepper_row(_parties_zone())
	return int(row.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META, -1)) if row != null else -1

func _party_stepper_row(root: Node) -> HBoxContainer:
	if root is HBoxContainer and (root as HBoxContainer).has_meta(
			HudWidgets.PARTY_STEPPER_COUNT_META):
		return root as HBoxContainer
	for child in root.get_children():
		var found := _party_stepper_row(child)
		if found != null:
			return found
	return null

## Where the `−` sits in a stepper row: right after the key label, which `build_party_stepper_row`
## adds first. The `+` is the LAST child either way.
const PARTY_STEPPER_MINUS_INDEX := 1

## A bound on the stepper loop, so a stepper that never reaches its target ends the chapter with a
## failed assertion instead of spinning the harness. Comfortably above any party this chapter asks
## for and below anything a real band could field.
const PARTY_STEPPER_MAX_PRESSES := 64

## Load `amount` onto the cargo row whose face contains `needle`, through the row's OWN `+` button —
## pressed `amount / step` times, so the clamp-to-the-pile and the per-press rerender are both real.
## The row is found fresh on every press: the sheet is rebuilt each time, so a held reference is a
## freed node.
func _load(needle: String, amount: float) -> void:
	var pressed := 0
	var presses := int(round(amount / HudComposeVocab.COMPOSE_CARGO_STEP))
	while pressed < presses:
		var plus := _cargo_plus_button(_parties_zone(), needle)
		if plus == null or plus.disabled:
			break
		plus.emit_signal("pressed")
		pressed += 1
	h._assert_hud("the cargo row for %s took the whole load" % needle, pressed == presses)

## The `+` of the cargo row whose name label contains `needle`. A cargo row is a name label followed
## by the shared stepper faces, so the `+` is the row's LAST child — found structurally rather than by
## a text match, which would find every stepper on the sheet.
func _cargo_plus_button(root: Node, needle: String) -> Button:
	if root is HBoxContainer:
		var row := root as HBoxContainer
		var count := row.get_child_count()
		if count > 0 and row.get_child(0) is Label \
				and (row.get_child(0) as Label).text.contains(needle):
			var last := row.get_child(count - 1)
			if last is Button and (last as Button).text == HudWorkVocab.STEPPER_PLUS_FACE:
				return last as Button
	for child in root.get_children():
		var found := _cargo_plus_button(child, needle)
		if found != null:
			return found
	return null

## The band that sends the shipment. Its stores are what the manifest is drawn from, and its two
## shipment levers are what the mass meter is drawn against.
func _shipper_band() -> Dictionary:
	var band := BandFx.band_fixture()
	band["id"] = "Band 1"
	band["entity"] = SHIPPER_ENTITY
	band = BandFx.with_band_id(band)
	band["pos"] = [SHIPPER_TILE.x, SHIPPER_TILE.y]
	band["stores"] = {"provisions": SHIPPER_PROVISIONS}
	band["idle_workers"] = 9
	band["labor_assignments"] = []
	band["band_move_tiles_per_turn"] = BAND_MOVE_TILES_PER_TURN
	band["expedition_trade_per_worker_carry"] = TRADE_PER_WORKER_CARRY
	band["expedition_trade_material_carry_weight"] = TRADE_MATERIAL_CARRY_WEIGHT
	band["material_batches"] = _shipper_batches()
	return band

## The band the shipment is FOR. It is in the roster, so the picker names it exactly as the cycler
## does — which is the case a tie's subject takes whenever the player still holds the band.
func _neighbour_band() -> Dictionary:
	var band := BandFx.band_fixture()
	band["id"] = "Band 2"
	band["entity"] = NEIGHBOUR_ENTITY
	band = BandFx.with_band_id(band)
	band["pos"] = [NEIGHBOUR_LAST_SEEN.x, NEIGHBOUR_LAST_SEEN.y]
	band["labor_assignments"] = []
	return band

## **FOUR PILES, TWO OF THEM ONE MATERIAL AT TWO RATINGS.** That pair is the whole reason a manifest
## row shows its rating: a mammoth hide and a hare pelt are both `hide`, and a picker that merged them
## would offer a quantity of something the band does not hold.
func _shipper_batches() -> Array:
	return [
		_batch("hide", 14.2, [["tough", 0.45, "fair"], ["supple", 0.58, "good"]]),
		_batch("hide", 6.0, [["tough", 0.90, "excellent"], ["supple", 0.15, "poor"]]),
		_batch("bone", 3.1, [["dense", 0.82, "excellent"], ["long", 0.35, "fair"]]),
	]

func _batch(material_id: String, amount: float, readings: Array) -> Dictionary:
	var rows: Array = []
	for reading in readings:
		rows.append({"axis": String(reading[0]), "value": float(reading[1]),
			"band_name": String(reading[2])})
	return {"material_id": material_id, "amount": amount, "readings": rows, "variety_name": ""}

## The shipper's TIES — one live, one parked, both DIRECTED from this band. Every row is the wire's
## own shape: two ids, a strength, a remembered position and three turn stamps, with no faction
## column anywhere, because faction is a property of the endpoint.
func _connections() -> Array:
	return [
		_tie(NEIGHBOUR_ENTITY, TIE_STRENGTH_LIVE, NEIGHBOUR_LAST_SEEN, NEIGHBOUR_LAST_SEEN_TURN),
		_tie(PARKED_ENTITY, TIE_STRENGTH_PARKED, PARKED_LAST_SEEN, PARKED_LAST_SEEN_TURN),
	]

func _tie(subject_entity: int, strength: float, last_seen: Vector2i, turn: int) -> Dictionary:
	return {
		"observer_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + SHIPPER_ENTITY,
		"subject_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + subject_entity,
		"strength": strength,
		"last_seen_x": last_seen.x,
		"last_seen_y": last_seen.y,
		"last_seen_turn": turn,
		"last_contact_turn": turn,
		"first_contact_turn": turn,
	}

## The party walking the shipment. **Its destination NAME is what renders and its destination BAND is
## the key the command addresses** — both are on the fixture, so the assertion that the id never
## reaches the screen has something to find if it does.
func _trade_party() -> Dictionary:
	return {
		"id": "Traders 1",
		"size": TRADE_PARTY_WORKERS,
		"entity": TRADE_PARTY_ENTITY,
		"faction": 0,
		"pos": [67, 20],
		"turns_of_food": 6.0,
		"stores": {"provisions": 5.0},
		"is_expedition": true,
		"expedition_mission": "trade",
		"expedition_phase": "outbound",
		"expedition_destination_band": BandFx.FIXTURE_BAND_ID_OFFSET + NEIGHBOUR_ENTITY,
		# **EMPTY, WHICH IS WHAT EVERY LIVE SHIPMENT PUBLISHES.** Bands have no names in this game, so
		# the sim declines to guess rather than shipping the unit archetype it briefly did. The row is
		# therefore a test of the CLIENT's fallback — its own label for that band, joined on the id
		# beside it — and a fixture carrying a name would have tested nothing that ships.
		"expedition_destination_name": "",
		"expedition_cargo_food": PARTY_CARGO_FOOD,
		"expedition_cargo_materials": [
			{"material_id": "hide", "amount": PARTY_CARGO_HIDE},
			{"material_id": "bone", "amount": PARTY_CARGO_BONE},
		],
		"expedition_carry_cap": PARTY_CARGO_CAP,
		"tile_info": {
			"x": 67, "y": 20,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}
