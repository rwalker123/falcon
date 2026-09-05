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

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 99

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## The shared pointer-input layer — the destination pick below goes through the engine's real
## dispatch, the `crafting_bench` gesture probe's convention.
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## `Main`'s reservation rules, borrowed rather than restated — the `crafting_bench` convention.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

## **THE CLIENT'S ONE DEFINITION OF "THE PLAYER IS TYPING"**, asked here about the cargo field
## exactly as `KeyboardArbiter` asks it (issue #620). It is the predicate that decides whether the
## POLLED gameplay keys may act, so a cargo control it does not recognise means WASD pans the map
## while a number is being typed into it — the failure this chapter checks for by name.
const TextEntryFocus := preload("res://src/scripts/TextEntryFocus.gd")

const BAND_PANEL_RESERVER := &"band_panel"

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## Which popup entry the last driven press landed on, written by `_press_popup_entry`'s witness.
var _popup_entry_pressed := POPUP_NO_ENTRY_PRESSED

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

## **THE THIRD MASS LEVER** (issue #590) - what one unit of HAY costs in pack space. Deliberately not
## 1.0 and not the material weight either: a bale priced at either would make the three-term mass
## expression indistinguishable from a two-term one that lumped hay in with food, so a client that
## dropped the fodder term would still pass every number below.
const TRADE_FODDER_CARRY_WEIGHT := 0.5

## The party the picker is driven to, and therefore the cap the meter is drawn against (4 × 10 = 40).
const TRADE_PARTY_WORKERS := 4

## …and the party the over-cap state falls back to: one worker carries 10, which the same 20-mass
## manifest cannot fit into. The refusal is the CLIENT's courtesy — the server's own is unchanged and
## remains the authority — so the state exists to show the player never has to meet it.
const OVER_CAP_PARTY_WORKERS := 1

## **THE LIVE TIE IS THE FIRST ENTRY THE PICKER LISTS, AND THAT IS THE INTERESTING SEAT.** The
## `connections` fixture puts the live tie ahead of the parked one, so the entry a player must reach
## is index 0 — the seat an `OptionButton` selects on its own as the first item is added, and
## therefore the one seat a pick can be swallowed on. A probe aimed anywhere else tests a picker in a
## state the reported defect cannot occur in.
const LIVE_TIE_ENTRY := 0

## Where in an entry's own row the press is aimed: the middle of it, on both axes. The popup's rows
## are drawn rather than published as nodes, so the row is derived from the popup's height and its
## item count — and aiming at the CENTRE is what keeps the derivation honest against a theme that
## pads a row, since a press that lands on the wrong row is caught by the assertion beside it.
const POPUP_ROW_CENTRE := 0.5

## The popup reported no entry under the press at all — a failure, never a skip.
const POPUP_NO_ENTRY_PRESSED := -1

## **AN UNCHOSEN PICKER HOLDS NO SELECTION.** `OptionButton.selected` when nothing has been picked,
## and the reading that must agree with the `Choose…` face beside it.
const PICKER_NOTHING_CHOSEN := -1

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

## …and its HAY, a larder apart. Deliberately a different figure from the provisions above, on the
## coarser scale hay is quoted in, so a sheet that read one store for the other shows it.
const SHIPPER_FODDER := 41.0

## What the player loads: food, plus one pile of hide. `12 + 2 × 4 = 20` of a 40 cap, so the meter is
## half full and the send is live — the state a picker exists to reach.
const LOADED_FOOD := 12.0
const LOADED_HIDE := 4.0

## …and then the HAY, loaded onto the same manifest (issue #590). `0.5 × 6 = 3` more mass, taking the
## meter to 23 of the same 40 cap: still sendable, so the state shows a three-account manifest a
## player can actually dispatch rather than one the cap refuses for an unrelated reason.
const LOADED_FODDER := 6.0

## --- WHAT THE TYPED FIELD IS DRIVEN WITH (issue #620) --------------------------------------------
## An amount that fits both caps, so the field's plain reading is checkable before any clamp is.
## **A TENTH RATHER THAN A WHOLE UNIT, and that is what makes the floor rule testable at all**: it
## leaves the pack's remaining room for the hide row on `7.35`, where flooring and rounding give
## different answers. Every figure in this block is a whole tenth from a whole-unit food amount, and
## a client that rounded would have passed the lot.
const TYPED_FOOD := 8.1

## Text no float can be read out of. The row must keep what it had — **reverting, never zeroing**: a
## player who mistypes has not asked to unload the wagon.
const TYPED_UNPARSEABLE := "eleventy"

## An amount above what the band HOLDS of the bone pile (3.1) and well UNDER the pack headroom there,
## so only the pile can be what clamps it.
const TYPED_OVER_HELD := 9.0

## …and one above the PACK's remaining room for the fair-hide pile while that pile (14.2) still has
## plenty in it, so only the pack can be what clamps that one. **The two cases are deliberately
## opposite** — a single wrong clamp cannot satisfy both.
const TYPED_OVER_CAP := 100.0

## What is typed into a row and then STEPPED from without an Enter in between — the reported defect
## (issue #620 follow-up). Deliberately far from every amount the row has held, so `typed + step`,
## `drawn + step` and `typed` are three different readings.
const TYPED_THEN_STEPPED := 20.0

## How far below a row's ceiling the clamp state starts, as a fraction of one step: **less than a
## whole one**, so the press that follows OVERSHOOTS and the clamp is what decides where it lands. At
## a whole step the press would land exactly on the ceiling and the state would assert nothing.
const STEP_PARTIAL_FRACTION := 0.5

## Zero, typed. The ONE way a row is emptied, since every malformed reading reverts instead.
const TYPED_ZERO := 0.0

## The other two piles this block drives, spelled as the sheet composes their faces — the
## `EXCELLENT_HIDE_ROW` convention, so a reworded row fails here rather than silently matching
## nothing.
const FAIR_HIDE_ROW := "hide · tough: fair · supple: good"
const BONE_ROW := "bone · dense: excellent · long: fair"

## What the in-flight party is carrying, and the pack it fills. The cap is the SHIPMENT lever's
## product (4 × 10), which is what the sim publishes on a trade party's `expeditionCarryCap`.
const PARTY_CARGO_FOOD := 12.0
const PARTY_CARGO_CAP := 40.0

## The two material rows a shipment carries on the wire — `MaterialPayoff`, one per material, NEVER
## summed. A readout that added them would be the retired trade axis under a new name.
const PARTY_CARGO_HIDE := 4.0
const PARTY_CARGO_BONE := 1.2

## The HAY the same party is walking (issue #590) — `expedition_cargo_fodder`, the third account. It
## must show as its OWN term on the `Carrying:` row: a shipment of grain and a shipment of feed are
## different shipments, and a party that folded the two into one figure would be quoting a food
## delivery the destination's larder never receives.
const PARTY_CARGO_FODDER := 6.0

## The band's Food line under a transfer (arc #527). `balance_supply_networks` has moved food between
## neighbouring larders every turn since turn one, so these two terms are not a trade feature: a band
## co-networked with a neighbour has both, and before them the Food line was short by the whole move.
##
## **EACH CROSSES A DIFFERENT KIND OF LINK, because the row names the link and not the direction**
## (issue #548): the arrival came over the automatic `Local exchange`, the departure went down a
## `Trade route` with a party — which is this chapter's own subject. Two kinds rather than two
## directions of one kind is what keeps them TWO rows, since a kind is netted into one.
const TRANSFER_LOCAL_IN := 1.4
const TRANSFER_ROUTE_OUT := 0.5

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
		# **THE PICKER'S OWN SELECTION MUST SAY WHAT ITS FACE SAYS**, and this is the reading the
		# reported defect fails: `OptionButton.add_item` seats `current` on the first selectable entry
		# it is handed, so a picker showing `Choose…` was quietly holding entry 0 — and Godot then
		# refuses to report a pick of the entry it believes is already current, which is exactly the
		# entry the player has to click. Asserted beside the face, because either alone looks right.
		h._assert_hud("…and the picker holds NO selection while its face says %s"
				% HudComposeVocab.COMPOSE_DESTINATION_CHOOSE,
			picker.selected == PICKER_NOTHING_CHOSEN
				and picker.text == HudComposeVocab.COMPOSE_DESTINATION_CHOOSE)
	# **AN UNCHOSEN DESTINATION CANNOT SEND**, and the button says so rather than vanishing.
	var blocked := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("…and the send is present and disabled until a destination is named",
		blocked is Button and (blocked as Button).disabled)

	# **STATE — A DESTINATION CHOSEN.** Picked with REAL POINTER INPUT — a press on the picker's face
	# and a press on the popup entry, both through `Viewport.push_input`. See
	# `_pick_destination_through_the_popup` for why an `emit_signal("item_selected", …)` cannot say
	# anything about this control.
	if picker != null:
		await _pick_destination_through_the_popup(picker, LIVE_TIE_ENTRY)
	await h._settle()
	await h._save("trade_picker_destination")
	# The sheet rebuilt around the choice, so the picker is a NEW control — and it must come back
	# holding the chosen entry under the chosen band's name, the same face/selection pairing asked of
	# the empty one above.
	var chosen := _destination_picker()
	h._assert_hud("the rebuilt picker wears the chosen band's name over the chosen entry",
		chosen != null and chosen.selected == LIVE_TIE_ENTRY
			and chosen.text == NEIGHBOUR_DISPLAY_NAME)
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
	# The meter reads the sim's own expression — food + fodder_carry_weight × fodder
	# + material_carry_weight × Σ materials — against party × the pack lever, composed here from the
	# fixture's side so the two arrive at one number from opposite ends. THIS manifest carries no
	# hay yet, so its middle term is zero; the `trade_cargo_hay` state below is what prices one.
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

	# **STATE — A BALE ON THE SAME MANIFEST** (issue #590). Hay is the THIRD cargo account: its own
	# row beside the food one, drawn off the band's fodder larder rather than its provisions, and
	# priced into the same pack at a carry weight of its own. The row is loaded through its own `+`
	# for the reason the others are — the clamp and the per-press rerender are what a player uses.
	_load(HudComposeVocab.COMPOSE_CARGO_FODDER_LABEL, LOADED_FODDER)
	await h._settle()
	await h._save("trade_cargo_hay")
	# **THE METER TAKES THE HAY TERM**, which is the whole reason the lever is on the wire: the sim
	# weighs `food + fodder_weight × hay + material_weight × Σ materials` and refuses what will not
	# fit, so a meter short of a term clears a load the server then rejects.
	var hay_mass := LOADED_FOOD + TRADE_FODDER_CARRY_WEIGHT * LOADED_FODDER \
		+ TRADE_MATERIAL_CARRY_WEIGHT * LOADED_HIDE
	var hay_meter := Q.find_meta_node(_parties_zone(), BandPanelController.TRADE_MASS_METER_META)
	h._assert_hud("the mass meter prices the hay into the pack",
		hay_meter is Label
			and (hay_meter as Label).text.contains(
				HudCraftingVocab.BATCH_AMOUNT_FORMAT % hay_mass))
	# …and the UNDER-PRICED reading is asserted ABSENT, because it is the plausible one: a manifest
	# whose hay term was dropped still shows a mass, a cap and a live send, and every assertion above
	# would pass on it. The number that must not appear is the one the two-term expression gives.
	h._assert_hud("…and never weighs a bale as free",
		hay_meter is Label
			and not (hay_meter as Label).text.contains(
				HudCraftingVocab.BATCH_AMOUNT_FORMAT % expected_mass))
	var hay_send := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("a three-account manifest under the cap can still be sent",
		hay_send is Button and not (hay_send as Button).disabled)

	# **STATE — THE SAME MANIFEST OVER THE CAP.** The party shrinks to one worker, so the cap falls to
	# 10 against a mass of 23 and the send refuses BEFORE the server has to. The refusal is the
	# client's courtesy; the server's own remains the authority.
	_set_party(OVER_CAP_PARTY_WORKERS)
	await h._settle()
	await h._save("trade_cargo_over_cap")
	var over_send := Q.find_meta_node(_parties_zone(), HudWidgets.SEND_TRADE_CONFIRM_META)
	h._assert_hud("an over-cap manifest cannot be sent",
		over_send is Button and (over_send as Button).disabled)
	h._assert_hud("…and the sheet says which way to fix it",
		_sheet_text().contains(HudComposeVocab.COMPOSE_CARGO_OVER_CAP_REASON))

	await _run_typed_cargo_states()

	# **STATE — THE FOOD LINE WITH A TRANSFER IN IT.** Not a trade readout: the supply network moves
	# food between neighbouring larders every turn, so any co-networked band carries these two terms.
	# They are itemized in the BREAKDOWN and deliberately absent from the `/turn` headline, which is
	# the STEADY rate on the sim's own basis — see `DetailFormat.band_net_food`.
	h._hud._bandpanel._close_party_compose()
	panel.set_active_tab(BandCityPanel.ZONE_BAND)
	var transferring := _shipper_band()
	# **A TURN'S OWN FRAME, so every one of these figures carries the same two magnitudes** — which is
	# what the sim publishes there, the per-turn copies being taken off the accumulators immediately
	# before the turn's capture. **The ROWS read the two LINK-KIND terms**; the summed per-turn pair
	# rides along because `DetailFormat.band_has_food_flow` gates the whole readout on it, and the
	# accumulating pair because it is what closes the larder identity — a fixture omitting either
	# would not be the live shape `population_to_dict` decodes.
	transferring["transfer_received"] = TRANSFER_LOCAL_IN
	transferring["transfer_sent"] = TRANSFER_ROUTE_OUT
	transferring["transfer_received_turn"] = TRANSFER_LOCAL_IN
	transferring["transfer_sent_turn"] = TRANSFER_ROUTE_OUT
	transferring[DetailFormat.TRANSFER_LOCAL_RECEIVED_TURN_KEY] = TRANSFER_LOCAL_IN
	transferring[DetailFormat.TRANSFER_ROUTE_SENT_TURN_KEY] = TRANSFER_ROUTE_OUT
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
	var local_in_row := DetailFormat.food_breakdown_row(TRANSFER_LOCAL_IN,
		DetailFormat.TRANSFER_LABEL_LOCAL)
	var route_out_row := DetailFormat.food_breakdown_row(-TRANSFER_ROUTE_OUT,
		DetailFormat.TRANSFER_LABEL_ROUTE)
	h._assert_hud("the Food breakdown itemizes what arrived over the local exchange (%s)"
		% local_in_row.strip_edges(), breakdown.contains(local_in_row))
	h._assert_hud("…and what left down a trade route, as its own row (%s)"
		% route_out_row.strip_edges(), breakdown.contains(route_out_row))
	_click_food_breakdown()
	await h._settle()

	# **THE COMMAND-REFRESHED FRAME (issue #517), PNG-LESS — the same band, the same two rows, on the
	# frame a dispatched command re-captured.** The sim clears `transferReceived` / `transferSent`
	# straight after the turn's capture reads them and rebuilds a refreshed frame from live
	# components, so on this frame the ACCUMULATING pair is 0 and every per-turn figure — the summed
	# pair and the four link-kind terms alike — is untouched. A breakdown reading the accumulator
	# loses both rows the instant the player does anything, which is what a live game showed on a real
	# 0.56 transfer.
	#
	# **A FRAME CANNOT SAY THIS** — the two states differ only in which field the rows were read from,
	# and the state that is wrong renders no rows at all rather than wrong ones. It is also asserted
	# as a PAIR with the turn frame above: a client reading the per-turn pair passes both, and one
	# reading whichever is non-zero passes both too — which is why the accumulator is zeroed here
	# rather than merely left behind.
	var refreshed := transferring.duplicate(true)
	refreshed["transfer_received"] = 0.0
	refreshed["transfer_sent"] = 0.0
	h._hud.update_band_alerts([refreshed, _neighbour_band()])
	h._hud.show_unit_selection(refreshed)
	await h._settle()
	_click_food_breakdown()
	await h._settle()
	var refreshed_breakdown := _collect_text(h)
	h._assert_hud("a command-refreshed frame still itemizes the local exchange",
		refreshed_breakdown.contains(local_in_row))
	h._assert_hud("…and the trade route, the accumulating pair having been cleared",
		refreshed_breakdown.contains(route_out_row))
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
	# **THE HAY IS A TERM OF ITS OWN**, in fodder's own one-decimal rendering, and it is named `hay`
	# where the wire says `fodder` — the player's word on the face, the sim's on the wire.
	h._assert_hud("the shipment says it is carrying hay",
		cargo_row.contains("%s %s" % [SourceForecast.format_fodder(PARTY_CARGO_FODDER),
			BandDetailLines.TRADE_CARGO_FODDER_TERM]))
	# …and NEVER added to the food it rides beside. Asserted as an absence for the materials' reason:
	# a row quoting `18.0` would look like a perfectly ordinary shipment, and it would be promising
	# the destination a food delivery its larder is never going to see.
	h._assert_hud("…and never adds its hay to its bread",
		not cargo_row.contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % (
			PARTY_CARGO_FOOD + PARTY_CARGO_FODDER)))
	# **THE FIGURE AGAINST THE CAP IS THE WHOLE PACK'S MASS**, the same expression the compose sheet's
	# meter priced this manifest with (`DetailFormat.shipment_mass`). Composed here from the fixture's
	# own terms, so the row and the meter arrive at one number from opposite ends — a `Carrying:` that
	# put the food alone over the cap rendered a full pack as one-sixth full and every other assertion
	# on this row still passed.
	var expected_party_mass := PARTY_CARGO_FOOD \
		+ TRADE_FODDER_CARRY_WEIGHT * PARTY_CARGO_FODDER \
		+ TRADE_MATERIAL_CARRY_WEIGHT * (PARTY_CARGO_HIDE + PARTY_CARGO_BONE)
	h._assert_hud("the shipment is weighed as a whole pack, against the pack's own cap",
		cargo_row.contains("%s / %s" % [
			HudCraftingVocab.BATCH_AMOUNT_FORMAT % expected_party_mass,
			HudCraftingVocab.BATCH_AMOUNT_FORMAT % PARTY_CARGO_CAP]))
	# The hunt-only rows must be ABSENT: a shipment carries no floor and no stop to report.
	h._assert_hud("a shipment states no ORDERS row — it carries no floor",
		not party_text.contains(TRADE_ABSENT_ORDERS_KEY))

	# Hand the reference band back, so a chapter appended after this one starts where the rest do.
	h._hud.update_band_alerts([BandFx.band_fixture()])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()

## **THE TYPED CARGO FIELD AND ITS `Max`** (issue #620) — six states, and every one of them is about
## a REFUSAL, because the amount a player types is the one input on this sheet the client cannot take
## at its word.
##
## **THE TWO CLAMPS ARE STAGED ON DIFFERENT ROWS, WITH THE CAPS THE OTHER WAY ROUND.** The bone pile
## is small (3.1) against plenty of pack headroom, so only what the band HOLDS can clamp it; the hay
## larder is large (41.0) against a nearly full pack, so only the PACK can clamp that one. A client
## that implemented one cap and called it `row_max` passes exactly one of the two — which is the
## point, since a single wrong clamp looks entirely plausible on either row alone.
##
## Every expectation below is composed from the fixture's own levers through `_fixture_row_max`, so
## the sheet and this chapter arrive at each number from opposite ends. The WRONG answers are
## asserted absent beside the right one wherever a plausible mistake has its own value — the pile
## when the pack should bind, the headroom when the pile should, and the headroom measured with the
## row's OWN load still counted against it, which is what makes `Max` unable to reach the cap.
func _run_typed_cargo_states() -> void:
	# The over-cap state left one worker on the stepper. The pack is the typed field's other cap, so
	# it is settled first, exactly as it is before the manifest is priced anywhere else on this sheet.
	_set_party(TRADE_PARTY_WORKERS)
	await h._settle()

	# **STATE — AN AMOUNT TYPED AND TAKEN.** The plain reading, before any refusal: 8.1 food fits both
	# caps, so what the player typed is what the row carries and what the meter prices.
	_type_cargo(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, _typed(TYPED_FOOD))
	await h._settle()
	await h._save("trade_cargo_typed")
	var typed_mass := _fixture_mass(TYPED_FOOD, LOADED_FODDER, LOADED_HIDE)
	h._assert_hud("a typed amount inside both caps is taken as given",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == _typed(TYPED_FOOD))
	h._assert_hud("…and the mass meter re-prices the manifest around it",
		_meter_text().contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % typed_mass))

	# **STATE — TEXT THAT NAMES NO AMOUNT, TWICE, THROUGH BOTH COMMIT PATHS.** Emptied and left
	# (`focus_exited`, the player who selects-all, deletes and clicks away) and then submitted as
	# nonsense (`text_submitted`). Both must put the last committed amount back: **zeroing a row is an
	# explicit act**, and a composed load destroyed by a stray keystroke is the worse failure by far.
	var field := _cargo_field(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
	h._assert_hud("the food row offers a typed field to empty", field != null)
	if field != null:
		field.grab_focus()
		# ⛔ **THE KEYBOARD TRAP, ASSERTED BY NAME.** `KeyboardArbiter` suppresses the client's polled
		# gameplay keys only while `TextEntryFocus` recognises the focused control — so a `SpinBox` or
		# a bespoke widget here would leave WASD panning the map on the keystrokes meant for this
		# number, with nothing on screen to say so.
		h._assert_hud("a focused cargo field IS the client's definition of the player typing",
			TextEntryFocus.held_in(h.get_viewport()))
		field.text = ""
		field.release_focus()
		await h._settle()
		h._assert_hud("emptying the field and leaving it puts the last amount back, it does not unload the row",
			_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == _typed(TYPED_FOOD))
		h._assert_hud("…and hands the keyboard back on the way out",
			not TextEntryFocus.held_in(h.get_viewport()))
	_type_cargo(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, TYPED_UNPARSEABLE)
	await h._settle()
	await h._save("trade_cargo_typed_invalid")
	h._assert_hud("text naming no amount is refused the same way",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == _typed(TYPED_FOOD))
	h._assert_hud("…and the manifest it was typed into is untouched",
		_meter_text().contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % typed_mass))

	# **STATE — CLAMPED BY WHAT THE BAND HOLDS.** 9 of a 3.1 pile, with 10.4 units of pack headroom
	# still under it: only the PILE can be what stops this.
	var bone_headroom := _fixture_headroom(TRADE_MATERIAL_CARRY_WEIGHT,
		_fixture_mass(TYPED_FOOD, LOADED_FODDER, LOADED_HIDE))
	_type_cargo(BONE_ROW, _typed(TYPED_OVER_HELD))
	await h._settle()
	await h._save("trade_cargo_typed_held")
	h._assert_hud("an amount above what the band HOLDS is clamped to the pile",
		_cargo_field_text(BONE_ROW) == _typed(SHIPPER_BONE_HELD))
	h._assert_hud("…and not to the pack headroom, which is the larger of the two caps here",
		_cargo_field_text(BONE_ROW) != _typed(_floor_tenth(bone_headroom)))
	# **AND THE ROW'S `Max` GOES DEAD ON THE PILE, not merely on the pack.** The typed clamp above
	# floors its ceiling, and 9 is over the pile either way, so a client whose `row_max` is the
	# headroom alone still lands this row on something plausible — while `Max` stays enabled forever,
	# offering an amount the band does not have and answering the press with nothing.
	h._assert_hud("…and Max on a row holding all the band has is disabled, saying that is why",
		_cargo_max_is_disabled_with(BONE_ROW, HudComposeVocab.COMPOSE_CARGO_MAX_AT_CAP_HINT))
	var held_clamped_mass := _fixture_mass(TYPED_FOOD, LOADED_FODDER,
		LOADED_HIDE + SHIPPER_BONE_HELD)
	h._assert_hud("…and the meter prices the clamped row, not the typed one",
		_meter_text().contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % held_clamped_mass))

	# **STATE — CLAMPED BY THE PACK, ON A CEILING THE TENTH DOES NOT DIVIDE.** 100 of a 14.2 pile with
	# only 7.35 units' worth of pack space left: the caps are the other way round from the state above,
	# so only the PACK can stop this one — **and 7.35 is what makes floor-versus-round visible.** A
	# client that rounded would load 7.4, which is over the cap by a tenth of a unit of hide: the
	# server refuses it and the player never touched anything but this field.
	var hide_headroom := _fixture_headroom(TRADE_MATERIAL_CARRY_WEIGHT, held_clamped_mass)
	var hide_room := _fixture_row_max(SHIPPER_FAIR_HIDE_HELD, TRADE_MATERIAL_CARRY_WEIGHT,
		held_clamped_mass)
	_type_cargo(FAIR_HIDE_ROW, _typed(TYPED_OVER_CAP))
	await h._settle()
	await h._save("trade_cargo_typed_cap")
	h._assert_hud("an amount the PACK cannot carry is clamped to what still fits",
		_cargo_field_text(FAIR_HIDE_ROW) == _typed(hide_room))
	h._assert_hud("…and not to the pile, which is the larger of the two caps here",
		_cargo_field_text(FAIR_HIDE_ROW) != _typed(SHIPPER_FAIR_HIDE_HELD))
	h._assert_hud("…and the ceiling is FLOORED onto the tenth, never rounded up past the cap",
		_cargo_field_text(FAIR_HIDE_ROW) != _typed(_round_tenth(hide_headroom)))
	h._assert_hud("…and Max on the row that just reached its ceiling is disabled, saying so",
		_cargo_max_is_disabled_with(FAIR_HIDE_ROW, HudComposeVocab.COMPOSE_CARGO_MAX_AT_CAP_HINT))
	var cap_clamped_mass := _fixture_mass(TYPED_FOOD, LOADED_FODDER,
		LOADED_HIDE + SHIPPER_BONE_HELD + hide_room)
	h._assert_hud("…and the meter prices the clamped row against the pack it nearly fills",
		_meter_text().contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % cap_clamped_mass))

	# **STATE — `Max` FILLS THE ROW.** The hay is typed back to zero first, which is also the only way
	# a row is emptied at all, and the freed pack space is then handed to the food row by one press.
	_type_cargo(HudComposeVocab.COMPOSE_CARGO_FODDER_LABEL, _typed(TYPED_ZERO))
	await h._settle()
	h._assert_hud("a typed zero DOES unload the row — the one act that empties one",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FODDER_LABEL) == _typed(TYPED_ZERO))
	# **THE FOOD ROW'S OWN LOAD IS NOT IN THIS TERM**, because the headroom a row may grow into is
	# measured over the OTHER rows — the whole point of the state below, and a mistake this harness
	# made first, which is what makes the absent-value assertion worth having.
	var others_mass := _fixture_mass(0.0, TYPED_ZERO,
		LOADED_HIDE + SHIPPER_BONE_HELD + hide_room)
	var food_room := _fixture_row_max(SHIPPER_PROVISIONS,
		HudComposeVocab.COMPOSE_CARGO_FOOD_CARRY_WEIGHT, others_mass)
	_press_cargo_max(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
	await h._settle()
	await h._save("trade_cargo_max")
	h._assert_hud("Max loads the most of the row the pack can still take",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == _typed(food_room))
	h._assert_hud("…measured over the OTHER rows, never against the row's own load",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
			!= _typed(_floor_tenth(_trade_cargo_cap() - others_mass - TYPED_FOOD)))
	h._assert_hud("…and never simply to the whole pile",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) != _typed(SHIPPER_PROVISIONS))
	h._assert_hud("…which takes the manifest back to exactly the pack's cap",
		_meter_text().contains(HudCraftingVocab.BATCH_AMOUNT_FORMAT % _trade_cargo_cap()))
	# **THE TWO DEAD `Max` STATES, SIDE BY SIDE ON ONE FRAME, EACH SAYING WHICH CAP KILLED IT.** The
	# food row sits AT the ceiling it just reached; the emptied hay row has no pack space left at all.
	# A single disabled-with-one-message button would satisfy neither claim.
	h._assert_hud("…and the button that did it is now disabled, at the ceiling it just reached",
		_cargo_max_is_disabled_with(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
			HudComposeVocab.COMPOSE_CARGO_MAX_AT_CAP_HINT))
	h._assert_hud("…while Max on a row with no pack space left says THAT instead",
		_cargo_max_is_disabled_with(HudComposeVocab.COMPOSE_CARGO_FODDER_LABEL,
			HudComposeVocab.COMPOSE_CARGO_MAX_NO_ROOM_HINT))

	# **STATE — A TYPED VALUE THEN A STEPPER PRESS, WITHOUT ENTER IN BETWEEN** (the reported defect).
	# The press is a REAL pointer gesture, because what broke is the ORDER the engine runs things in
	# and no faked `pressed` can see it: a click moves keyboard focus, focus loss commits the field,
	# the commit rebuilds the sheet, and the rebuild frees the very button the click is still inside.
	#
	# **Both faults it stacked are asserted at once.** A stepper that captured its row's amount at
	# BUILD time steps from what the row was drawn with; a stepper whose press is eaten by that
	# rebuild does not step at all. The one right answer is `typed + step`, and it is reachable only by
	# flushing the field first and resolving the amount live.
	_type_cargo(FAIR_HIDE_ROW, _typed(TYPED_ZERO))
	await h._settle()
	var drawn_with := _cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL).to_float()
	_write_cargo_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, _typed(TYPED_THEN_STEPPED))
	await _click_cargo_control(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
		HudWidgets.CARGO_CONTROL_PLUS)
	await h._settle()
	await h._save("trade_cargo_typed_then_stepped")
	h._assert_hud("a `+` after a typed value steps from what was TYPED, not from what was drawn",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
			== _typed(TYPED_THEN_STEPPED + HudComposeVocab.COMPOSE_CARGO_STEP))
	h._assert_hud("…and it is a STEP, not a press eaten by the rebuild the commit triggers",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) != _typed(TYPED_THEN_STEPPED))
	h._assert_hud("…and never from the amount the row was drawn with (%s)" % _typed(drawn_with),
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
			!= _typed(drawn_with + HudComposeVocab.COMPOSE_CARGO_STEP))
	# **THE KEYBOARD STAYS IN THE FIELD ACROSS THE PRESS**, which is what makes the flush deterministic
	# rather than a race: the row's buttons take no focus, so the field is never committed out from
	# under the click, and the rebuilt row takes the caret back.
	h._assert_hud("…and the keyboard is still in the field the player was typing into",
		TextEntryFocus.held_in(h.get_viewport()))

	# …and the same for `−`, which is the other half of the same capture.
	_write_cargo_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, _typed(TYPED_THEN_STEPPED))
	await _click_cargo_control(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
		HudWidgets.CARGO_CONTROL_MINUS)
	await h._settle()
	h._assert_hud("a `−` after a typed value steps down from what was TYPED",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
			== _typed(TYPED_THEN_STEPPED - HudComposeVocab.COMPOSE_CARGO_STEP))

	# **STATE — `+` IS BOUNDED BY THE PACK, NOT ONLY BY THE PILE.** The food row is taken to HALF A
	# STEP below its ceiling, so the next press overshoots — and must land ON the ceiling rather than
	# past it. The band still holds 84 throughout, so under the old rule (`amount >= held`) `+` stays
	# live here and a press carries the manifest over the cap the meter above just cleared.
	var pack_room := _fixture_row_max(SHIPPER_PROVISIONS,
		HudComposeVocab.COMPOSE_CARGO_FOOD_CARRY_WEIGHT,
		_fixture_mass(0.0, TYPED_ZERO, LOADED_HIDE + SHIPPER_BONE_HELD))
	var overshoot := HudComposeVocab.COMPOSE_CARGO_STEP * STEP_PARTIAL_FRACTION
	_type_cargo(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, _typed(pack_room - overshoot))
	await h._settle()
	await _click_cargo_control(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
		HudWidgets.CARGO_CONTROL_PLUS)
	await h._settle()
	await h._save("trade_cargo_step_clamped")
	h._assert_hud("a `+` that would overrun the ceiling clamps to it",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == _typed(pack_room))
	h._assert_hud("…rather than stepping past it, which is what a clamp to the PILE alone allows",
		_cargo_field_text(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) != _typed(
			pack_room - overshoot + HudComposeVocab.COMPOSE_CARGO_STEP))
	h._assert_hud("…leaving the pack full rather than overfull",
		_meter_text().contains(HudComposeVocab.COMPOSE_CARGO_MASS_FORMAT % [
			HudFormat.meter_bar(HudConst.PROGRESS_PERCENT_SCALE,
				HudComposeVocab.COMPOSE_CARGO_MASS_CELLS),
			HudCraftingVocab.BATCH_AMOUNT_FORMAT % _trade_cargo_cap(),
			HudCraftingVocab.BATCH_AMOUNT_FORMAT % _trade_cargo_cap()]))
	h._assert_hud("…and `+` is now dead because the PACK is full, though the band holds far more",
		_cargo_control_is_disabled(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
			HudWidgets.CARGO_CONTROL_PLUS))
	h._assert_hud("…while `−` stays live, the row being nowhere near empty",
		not _cargo_control_is_disabled(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
			HudWidgets.CARGO_CONTROL_MINUS))
## The pack this chapter's party carries — `party × per_worker_carry`, the sim's own product.
func _trade_cargo_cap() -> float:
	return float(TRADE_PARTY_WORKERS) * TRADE_PER_WORKER_CARRY

## The sim's mass expression over this chapter's own levers, composed from the fixture's side so the
## sheet's meter and this chapter meet at one number from opposite ends. Materials arrive as ONE
## pack-space total and are never a readout — the `Carrying:` rule, one screen over.
func _fixture_mass(food: float, fodder: float, materials: float) -> float:
	return food + fodder * TRADE_FODDER_CARRY_WEIGHT + materials * TRADE_MATERIAL_CARRY_WEIGHT

## How much of one row the PACK alone would still take — the second cap on its own, and therefore
## the answer a client that implemented only that one gives. **RAW, on neither grid**: each caller
## puts it on the one its own claim is about, since the whole point of the hide state below is that
## flooring and rounding this number disagree.
func _fixture_headroom(weight: float, other_mass: float) -> float:
	return maxf((_trade_cargo_cap() - other_mass) / weight, 0.0)

## …and the most one row may still take: **BOTH caps**, the pack headroom measured over the mass of
## the OTHER rows only, floored onto the tenth every composed amount is floored onto.
func _fixture_row_max(held: float, weight: float, other_mass: float) -> float:
	return _floor_tenth(maxf(minf(held, (_trade_cargo_cap() - other_mass) / weight), 0.0))

## …and the ROUNDED answer, which is what a plausible wrong client gives and therefore the value
## asserted ABSENT beside the floored one. It exists only to be a wrong answer.
func _round_tenth(amount: float) -> float:
	var scale: float = pow(10.0, HudComposeVocab.COMPOSE_CARGO_AMOUNT_DECIMALS)
	return roundf(amount * scale) / scale

## FLOOR, never round — the rule the sheet composes every amount by, mirrored here so this chapter
## cannot pass a client that rounds. The grid is the client's own declared precision.
func _floor_tenth(amount: float) -> float:
	var scale: float = pow(10.0, HudComposeVocab.COMPOSE_CARGO_AMOUNT_DECIMALS)
	return floorf(amount * scale) / scale

## An amount as the field spells it, so an assertion compares the string the player reads.
func _typed(amount: float) -> String:
	return HudCraftingVocab.BATCH_AMOUNT_FORMAT % amount

## Type `text` into one cargo row's field and commit it with Enter — `text_submitted`, the signal the
## engine emits for the key, carrying the field's own text.
func _type_cargo(needle: String, text: String) -> void:
	var field := _cargo_field(needle)
	h._assert_hud("the cargo row for %s offers a typed field" % needle, field != null)
	if field == null:
		return
	field.text = text
	field.text_submitted.emit(text)

## Put text in one cargo row's field and **leave it uncommitted** — no Enter, no focus change. The
## keyboard is taken first, because that is the state the reported defect happens in: a player mid-
## edit reaching for the stepper beside the field.
func _write_cargo_text(needle: String, text: String) -> void:
	var field := _cargo_field(needle)
	h._assert_hud("the cargo row for %s offers a field to type into" % needle, field != null)
	if field == null:
		return
	field.grab_focus()
	field.text = text
	# `LineEdit.text =` does not emit `text_changed` (only user edits do), so the harness raises the
	# same signal a keystroke would — otherwise the half-typed text is invisible to everything that
	# tracks it, and the state under test would be one no player can reach.
	field.text_changed.emit(text)

## Click one of a cargo row's controls with a REAL pointer gesture, press and release a frame apart.
## **Not `pressed.emit()`**: the defect this drives is about what the ENGINE does between the press
## and the release — the focus move, the commit it fires and the rebuild that frees the button — and
## a synthesised signal skips every one of those steps.
func _click_cargo_control(needle: String, control: String) -> void:
	var button := _cargo_control(_parties_zone(), needle, control)
	h._assert_hud("the cargo row for %s offers a live %s" % [needle, control],
		button is Button and not (button as Button).disabled)
	if not (button is Button) or (button as Button).disabled:
		return
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		button.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h.get_tree().process_frame

## Is one of a row's controls greyed out? Answers `true` for a control that is not there at all, so a
## row that stopped rendering one cannot pass as "correctly disabled".
func _cargo_control_is_disabled(needle: String, control: String) -> bool:
	var button := _cargo_control(_parties_zone(), needle, control)
	return not (button is Button) or (button as Button).disabled

## Press one cargo row's `Max`.
func _press_cargo_max(needle: String) -> void:
	var button := _cargo_control(_parties_zone(), needle, HudWidgets.CARGO_CONTROL_MAX)
	h._assert_hud("the cargo row for %s offers a live Max" % needle,
		button != null and not button.disabled)
	if button != null and not button.disabled:
		button.pressed.emit()

## Is one row's `Max` dead for the stated reason? Both halves asserted together: a button disabled
## with the WRONG reason tells the player which cap stopped them, wrongly.
func _cargo_max_is_disabled_with(needle: String, reason: String) -> bool:
	var button := _cargo_control(_parties_zone(), needle, HudWidgets.CARGO_CONTROL_MAX)
	return button != null and button.disabled and button.tooltip_text == reason

func _cargo_field(needle: String) -> LineEdit:
	return _cargo_control(_parties_zone(), needle, HudWidgets.CARGO_CONTROL_FIELD) as LineEdit

func _cargo_field_text(needle: String) -> String:
	var field := _cargo_field(needle)
	return field.text if field != null else ""

## The live mass meter's face.
func _meter_text() -> String:
	var meter := Q.find_meta_node(_parties_zone(), BandPanelController.TRADE_MASS_METER_META)
	return (meter as Label).text if meter is Label else ""

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

## **THE PICK IS DRIVEN AS A POINTER GESTURE, BECAUSE THAT IS THE PART THAT BROKE.** A player's pick
## is a press on the picker's face — which opens the popup, `OptionButton` running at
## `ACTION_MODE_BUTTON_PRESS` — and then a press on an entry INSIDE that popup, which is the only
## thing that reaches `OptionButton`'s own selection path and therefore the only thing that decides
## whether `item_selected` fires at all.
##
## **A `picker.emit_signal("item_selected", i)` cannot fail**: it calls the connected lambda by hand,
## so it passes on a picker whose popup never opens, whose entries cannot be reached, and — the
## reported defect — whose selection the engine silently declines to change, `add_item` having already
## seated `current` on the entry the player is about to click. This state was rewritten from that
## faked signal to these two presses because the faked one was green throughout.
##
## Both presses are delivered in WINDOW coordinates through `InputProbe`: the popup is an embedded
## subwindow, and `Viewport.push_input` un-stretches an event into canvas space before forwarding it
## to one, so a raw canvas point misses it. Every step fails loudly through `_assert_hud` — a probe
## that quietly found nothing would leave every state after it rendering the unchosen sheet.
func _pick_destination_through_the_popup(picker: OptionButton, entry: int) -> void:
	var viewport: Viewport = h.get_viewport()
	var face := InputProbe.canvas_to_window(viewport, h.get_window(),
		picker.get_global_rect().get_center())
	InputProbe.hover(viewport, face)
	InputProbe.press_left(viewport, face)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, face)
	await h.get_tree().process_frame
	var popup := picker.get_popup()
	h._assert_hud("a press on the destination picker's face opens its popup", popup.visible)
	if not popup.visible:
		return
	await _press_popup_entry(popup, entry)
	# The point pressed is DERIVED from the popup's own rect (an item's rect is not published), so the
	# claim is checked rather than trusted: the popup itself says which entry the press hit, and a
	# theme or a third tie that moved the rows fails here instead of quietly picking the parked band.
	# **Read off the MEMBER, never off a return value.** A press that lands commits the pick, which
	# rerenders the sheet and FREES this popup — so the helper runs on a node that may die under it, and
	# an aborted GDScript call answers with its return type's DEFAULT. `0` is a legal entry index, so a
	# returned answer would have reported "landed on entry 0" for a helper that never finished. The
	# member's own sentinel is what fails instead.
	h._assert_hud("…and the press lands on the live tie's own entry (%d, wanted %d)"
			% [_popup_entry_pressed, entry],
		_popup_entry_pressed == entry)

## Press one entry of the OPEN popup, leaving `_popup_entry_pressed` holding the entry the popup says
## the press hit (`POPUP_NO_ENTRY_PRESSED` when it hit none). The signal is only LISTENED to; the press
## itself is a real pointer gesture, which is the whole point of this state.
##
## **The witness writes to a MEMBER, not to a local.** A GDScript lambda captures a local by VALUE, so
## a `var landed` assigned inside the callback keeps the closure's own copy and the caller reads the
## initial value forever — which reports every press as having landed on nothing, whatever really
## happened. It cost a run to find; the member is captured through `self` and does propagate.
##
## **THE POPUP IS FREED UNDER THIS FUNCTION, AND THAT IS THE CONTROL BEHAVING CORRECTLY.** A pick runs
## the entry's `on_pick`, which rerenders the sheet, which `queue_free`s the row the picker and its
## popup hang off — so by the frame after the release the popup is gone. The deferred free is what
## makes it safe (nothing is freed while Godot is still inside `activate_item`), and the teardown here
## is guarded rather than assumed: an unguarded `disconnect` raises, which ABORTS this call, and an
## aborted call answers with its return type's default rather than failing.
func _press_popup_entry(popup: PopupMenu, entry: int) -> void:
	var viewport: Viewport = h.get_viewport()
	_popup_entry_pressed = POPUP_NO_ENTRY_PRESSED
	var witness := func(index: int) -> void:
		_popup_entry_pressed = index
	popup.index_pressed.connect(witness)
	var row_height := float(popup.size.y) / float(maxi(popup.item_count, 1))
	var point := InputProbe.canvas_to_window(viewport, h.get_window(), Vector2(
		float(popup.position.x) + float(popup.size.x) * POPUP_ROW_CENTRE,
		float(popup.position.y) + row_height * (float(entry) + POPUP_ROW_CENTRE)))
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h.get_tree().process_frame
	if is_instance_valid(popup):
		popup.index_pressed.disconnect(witness)

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
		var plus := _cargo_control(_parties_zone(), needle, HudWidgets.CARGO_CONTROL_PLUS)
		if plus == null or plus.disabled:
			break
		plus.emit_signal("pressed")
		pressed += 1
	h._assert_hud("the cargo row for %s took the whole load" % needle, pressed == presses)

## One control of the cargo row whose name label contains `needle`, found by the two metas the row
## carries (`HudWidgets.CARGO_ROW_KEY_META` on the row, `CARGO_CONTROL_META` on each control).
##
## **IT USED TO WALK THE ROW POSITIONALLY** — the `+` was "the last child" — which the typed field and
## its `Max` broke the moment they joined the row (issue #620): the walk found `Max` and pressed it
## believing it was the `+`. A meta is the only handle that survives a control being added.
func _cargo_control(root: Node, needle: String, control: String) -> Control:
	if root is HBoxContainer and (root as HBoxContainer).has_meta(HudWidgets.CARGO_ROW_KEY_META):
		var row := root as HBoxContainer
		if row.get_child_count() > 0 and row.get_child(0) is Label \
				and (row.get_child(0) as Label).text.contains(needle):
			for child in row.get_children():
				if child is Control and String((child as Control).get_meta(
						HudWidgets.CARGO_CONTROL_META, "")) == control:
					return child as Control
	for child in root.get_children():
		var found := _cargo_control(child, needle, control)
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
	# **THE HAY LARDER IS NOT A `stores` KEY** — the wire publishes it as the cohort's own
	# `fodder_store`, a second larder that never converts into the first.
	band["fodder_store"] = SHIPPER_FODDER
	band["idle_workers"] = 9
	band["labor_assignments"] = []
	band["band_move_tiles_per_turn"] = BAND_MOVE_TILES_PER_TURN
	band["expedition_trade_per_worker_carry"] = TRADE_PER_WORKER_CARRY
	band["expedition_trade_material_carry_weight"] = TRADE_MATERIAL_CARRY_WEIGHT
	band["expedition_trade_fodder_carry_weight"] = TRADE_FODDER_CARRY_WEIGHT
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
		_batch("hide", SHIPPER_FAIR_HIDE_HELD,
			[["tough", 0.45, "fair"], ["supple", 0.58, "good"]]),
		_batch("hide", 6.0, [["tough", 0.90, "excellent"], ["supple", 0.15, "poor"]]),
		_batch("bone", SHIPPER_BONE_HELD, [["dense", 0.82, "excellent"], ["long", 0.35, "fair"]]),
	]

## The BONE pile, named because the typed-clamp state above is measured against it: it is small
## enough that the PILE is what clamps a large typed amount there, while the pack still has room.
const SHIPPER_BONE_HELD := 3.1

## …and the FAIR-hide pile, which is the case the other way round: large enough that the PACK is what
## clamps a large typed amount into it, on a ceiling that is not a whole tenth.
const SHIPPER_FAIR_HIDE_HELD := 14.2

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
		"expedition_cargo_fodder": PARTY_CARGO_FODDER,
		"expedition_cargo_materials": [
			{"material_id": "hide", "amount": PARTY_CARGO_HIDE},
			{"material_id": "bone", "amount": PARTY_CARGO_BONE},
		],
		"expedition_carry_cap": PARTY_CARGO_CAP,
		# **BOTH CARRY-WEIGHT LEVERS RIDE EVERY COHORT**, party included (the native decoder echoes
		# them onto each one), and the `Carrying:` row needs both: what the cap is checked against is
		# `food + fodder_weight × hay + material_weight × Σ materials`, so a fixture missing either
		# would price this pack under what the sim weighs it at.
		"expedition_trade_material_carry_weight": TRADE_MATERIAL_CARRY_WEIGHT,
		"expedition_trade_fodder_carry_weight": TRADE_FODDER_CARRY_WEIGHT,
		"tile_info": {
			"x": 67, "y": 20,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}
