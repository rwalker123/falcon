extends RefCounted

## WHICH LINK THE GOODS CROSSED — the local supply network made legible (issue #548). **A UX
## PROTOTYPE, rendered for review.**
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. It is appended LAST for exactly that reason. See
## `.claude/rules/client/test-harnesses.md`.
##
## **THE WHOLE FEATURE IS TWO LABELS.** A band's larder moves for reasons it never stated: goods cross
## to and from other camps every turn and the popover said only `From other bands`. What a player can
## act on is not who was at the other end — it is WHICH LINK it crossed, because one of the two they
## built and the other happens whether they look or not:
##
##   * `⇄ Local exchange` — `balance_supply_networks`, the automatic balancing between camps within
##     reach of one another.
##   * `⇄ Trade route` — a shipment: a party arriving with cargo, or the draw one takes on launch.
##
## ⛔ **NO COUNTERPARTY IS NAMED, AND THAT WAS BUILT AND REJECTED.** Bands have no names in this game
## (issue #615), so every named row was a placeholder — and a variable-length name list dragged a
## pixel-fitting apparatus behind it to stop rows wrapping. Two fixed phrases cannot wrap.
##
## ⛔ **AND NO PROSE.** No mechanism sentence, no radius, no range warning. The rows are the readout.
##
## **FOOD AND FODDER GET THE IDENTICAL PAIR**, from `DetailFormat`'s own constants rather than a copy,
## differing only in the number's resolution. It ends by handing the reference band back, so a chapter
## appended after it starts where every other one does.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 21

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## **THIS CHAPTER'S OWN ENTITIES**, so its payloads cannot be confused with the reference band's — and
## so each state's disclosure gets its own popover key (`DetailFormat.breakdown_key` is
## `<kind>:<entity>`). `BandFx.with_band_id` derives each `band_id` from the entity with a fixed
## offset, which is what keeps a payload that sent the ENTITY distinguishable from one that sent the
## handle.
const QUIET_ENTITY := 951
const LINKED_ENTITY := 952
const BOTH_WAYS_ENTITY := 953

## **THE FOOD LEDGER'S TURN.** Goods arrived over both kinds of link — the automatic balancing with a
## short neighbor, and a shipment that walked in. Two different stories about one larder, which is the
## whole reason there are two phrases.
const FOOD_LOCAL_IN := 1.4
const FOOD_ROUTE_IN := 3.0

## …and what a party carried OFF on the turn it launched. **The row states the DIFFERENCE**: a camp
## that took 3.00 in and sent 2.00 out down its routes reads one `⇄ Trade route +1.00`, because each
## link kind is netted into a single signed row.
const FOOD_ROUTE_OUT := 2.0

## **THE FODDER LEDGER'S SAME TWO LINKS**, at that account's own one-decimal resolution. Hay in over
## the local exchange, hay out down a route — the two directions, so the frame shows the SIGN doing
## the work a `From` / `To` pair of phrases used to do.
const HAY_GROWN := 5.0
const HAY_PENS := 6.0
const HAY_LOCAL_IN := 2.0
const HAY_ROUTE_OUT := 0.5

## The hay stock and its runway, so the `Fodder:` row above the popover reads as a real larder rather
## than as a gate the fixture happened to trip.
const HAY_STORE := 100.0
const HAY_TURNS := 100.0

## What `DetailFormat.breakdown_key` puts between a breakdown's kind and its band entity.
const BREAKDOWN_KEY_SEPARATOR := ":"


func run(harness) -> void:
	h = harness

	# **STATE 1 — NOTHING CROSSED EITHER LINK, AND IT IS THE NEGATIVE CASE THAT MATTERS.** A camp with
	# no exchange and no shipment carries its ordinary rows and NOT ONE transfer row, on either
	# account. Every state below passes on a client that renders the pair unconditionally, and a row
	# reading `⇄ Local exchange +0.00` would be a readout for a thing that did not happen.
	h._hud.show_unit_selection(_quiet_band())
	await h._settle()
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, QUIET_ENTITY)
	await h._settle()
	await h._save("supply_quiet")
	var quiet_food: Array[String] = h._hud._disclosures.food_breakdown_lines(_quiet_band())
	var quiet_fodder: Array[String] = h._hud._disclosures.fodder_breakdown_lines(_quiet_band())
	h._assert_hud("a quiet camp's Food breakdown still states its own three flows",
		_any(quiet_food, DetailFormat.FOOD_LABEL_GATHERED)
			and _any(quiet_food, DetailFormat.FOOD_LABEL_CONSUMED))
	h._assert_hud("…and its Fodder breakdown its own two",
		_any(quiet_fodder, DetailFormat.FODDER_LABEL_GROWN)
			and _any(quiet_fodder, DetailFormat.FODDER_LABEL_PENS))
	# **CLAIMED ON THE GLYPH, NOT ON THE LABELS.** A row naming some third link kind would slip past a
	# test for these two phrases; nothing carrying the transfer mark may appear here at all.
	h._assert_hud("…and NEITHER account carries a transfer row of any kind",
		not _any(quiet_food, DetailFormat.TRANSFER_GLYPH)
			and not _any(quiet_fodder, DetailFormat.TRANSFER_GLYPH))
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, QUIET_ENTITY)
	await h._settle()

	# **STATE 2 — THE FOOD LEDGER, BOTH KINDS OF LINK.** The rows sit among `Gathered` / `Hunted` /
	# `Consumed`, where the unit is already established, and they say which link the goods crossed rather
	# than describing a mechanism or naming a camp.
	h._hud.show_unit_selection(_linked_band())
	await h._settle()
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, LINKED_ENTITY)
	await h._settle()
	await h._save("supply_food_links")
	var links_text := _collect_text(h)
	var local_in_row := DetailFormat.food_breakdown_row(FOOD_LOCAL_IN,
		DetailFormat.TRANSFER_LABEL_LOCAL)
	var route_in_row := DetailFormat.food_breakdown_row(FOOD_ROUTE_IN,
		DetailFormat.TRANSFER_LABEL_ROUTE)
	h._assert_hud("the Food ledger states the automatic exchange (%s)" % local_in_row.strip_edges(),
		links_text.contains(local_in_row))
	h._assert_hud("…and the trade route, as its own row (%s)" % route_in_row.strip_edges(),
		links_text.contains(route_in_row))
	# ⛔ **NO CAMP IS NAMED AND NO PROSE IS ADDED**, which is what earlier cuts of this readout got
	# wrong. Asserted over the PRODUCED lines: every row is an indented breakdown row, so a sentence or
	# a footer shows up here as a line that is not one.
	var links_rows: Array[String] = h._hud._disclosures.food_breakdown_lines(_linked_band())
	h._assert_hud("…and the breakdown is rows ONLY — no sentence, no footer, no radius",
		_prose_lines(links_rows).is_empty())
	# ⛔ **AND NOTHING SAYS "POOLED", IN EITHER SPELLING OF NEIGHBOR.** Both were rejected by name.
	h._assert_hud("…and no row says 'pooled', or spells neighbor the British way",
		not _any_lower(links_rows, "pool") and not _any_lower(links_rows, "neighbour"))

	# **STATE 3 — BOTH DIRECTIONS ON ONE LINK, NETTED INTO ONE ROW.** A camp can take a shipment in and
	# launch a party out on the same turn; the ledger states that as ONE signed `Trade route` row at
	# the difference. One row per kind is the decided behaviour for these four terms, and it is the
	# opposite of the rule the generic pair above keeps — hence the frame.
	h._hud.show_unit_selection(_both_ways_band())
	await h._settle()
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, BOTH_WAYS_ENTITY)
	await h._settle()
	await h._save("supply_food_route_both_ways")
	var both_text := _collect_text(h)
	var route_net_row := DetailFormat.food_breakdown_row(FOOD_ROUTE_IN - FOOD_ROUTE_OUT,
		DetailFormat.TRANSFER_LABEL_ROUTE)
	h._assert_hud("a shipment in and a party out render as ONE netted row (%s)"
		% route_net_row.strip_edges(), both_text.contains(route_net_row))
	# **THE TWO GROSS FIGURES ARE ASSERTED ABSENT.** A ledger that emitted them as a pair renders two
	# perfectly plausible rows, and every other claim in this block would still pass.
	h._assert_hud("…and neither gross figure survives beside it",
		not both_text.contains(route_in_row)
			and not both_text.contains(DetailFormat.food_breakdown_row(-FOOD_ROUTE_OUT,
				DetailFormat.TRANSFER_LABEL_ROUTE)))
	# **ONE ROW PER KIND, COUNTED**, since "the label appears once" is the whole claim and a `contains`
	# cannot make it.
	var both_rows: Array[String] = h._hud._disclosures.food_breakdown_lines(_both_ways_band())
	h._assert_hud("…and the kind takes exactly one row, its ▲/▼ saying which way the turn went",
		_count_containing(both_rows, DetailFormat.TRANSFER_LABEL_ROUTE) == 1)
	h._assert_hud("…as does the local exchange beside it",
		_count_containing(both_rows, DetailFormat.TRANSFER_LABEL_LOCAL) == 1)
	# **AND A KIND THAT CANCELS EXACTLY RENDERS NOTHING**, which is the consequence of netting and is
	# stated here as a fact of the readout rather than left to be discovered. The net falls under the
	# account's floor and is omitted, exactly as every other flow in this ledger is.
	var cancelled := _both_ways_band()
	cancelled[DetailFormat.TRANSFER_ROUTE_SENT_TURN_KEY] = FOOD_ROUTE_IN
	var cancelled_rows: Array[String] = h._hud._disclosures.food_breakdown_lines(cancelled)
	h._assert_hud("a turn whose arrivals and departures cancel shows no row for that kind",
		not _any(cancelled_rows, DetailFormat.TRANSFER_LABEL_ROUTE)
			and _any(cancelled_rows, DetailFormat.TRANSFER_LABEL_LOCAL))
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FOOD, BOTH_WAYS_ENTITY)
	await h._settle()

	# **STATE 4 — THE FODDER LEDGER, THE SAME TWO PHRASES.** Held beside state 2: the two accounts must
	# word one event ONE way. An earlier cut gave them two vocabularies, and that alone made the
	# readout unreadable.
	h._hud.show_unit_selection(_linked_band())
	await h._settle()
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FODDER, LINKED_ENTITY)
	await h._settle()
	await h._save("supply_fodder_links")
	var fodder_text := _collect_text(h)
	h._assert_hud("the Fodder popover keeps its two existing flows",
		fodder_text.contains(DetailFormat.fodder_breakdown_row(HAY_GROWN,
			DetailFormat.FODDER_LABEL_GROWN))
			and fodder_text.contains(DetailFormat.fodder_breakdown_row(-HAY_PENS,
				DetailFormat.FODDER_LABEL_PENS)))
	h._assert_hud("…and gains the hay that crossed the local exchange",
		fodder_text.contains(DetailFormat.fodder_breakdown_row(HAY_LOCAL_IN,
			DetailFormat.TRANSFER_LABEL_LOCAL)))
	h._assert_hud("…and the hay that left down a trade route, as a debit",
		fodder_text.contains(DetailFormat.fodder_breakdown_row(-HAY_ROUTE_OUT,
			DetailFormat.TRANSFER_LABEL_ROUTE)))
	# **THE CONSISTENCY CLAIM, MADE MECHANICALLY.** Both ledgers' transfer rows are stripped of their
	# numbers and compared as SETS: if one account ever re-words the link the other states, this fails
	# — which a pair of frames alone could never catch, and which is the defect that sank an earlier
	# cut of this readout.
	h._assert_hud("…and the two accounts word the link IDENTICALLY, not merely similarly",
		_transfer_phrases(links_rows)
			== _transfer_phrases(h._hud._disclosures.fodder_breakdown_lines(_linked_band())))
	_click_breakdown(HudDisclosureVocab.BREAKDOWN_KIND_FODDER, LINKED_ENTITY)
	await h._settle()

	# **THE FALLBACK, PNG-LESS — the state EVERY band on today's wire is in.** None of the four food
	# keys is published yet, so the ledger must still render the generic pair it renders now. Asserted
	# rather than left to the wire half to discover: the fork that preserves it is one line, and is
	# exactly the kind of line a later simplification deletes.
	var legacy := _linked_band()
	for key in [DetailFormat.TRANSFER_LOCAL_RECEIVED_TURN_KEY,
			DetailFormat.TRANSFER_LOCAL_SENT_TURN_KEY,
			DetailFormat.TRANSFER_ROUTE_RECEIVED_TURN_KEY,
			DetailFormat.TRANSFER_ROUTE_SENT_TURN_KEY]:
		legacy.erase(key)
	legacy["transfer_received_turn"] = FOOD_LOCAL_IN
	legacy["transfer_sent_turn"] = FOOD_ROUTE_OUT
	var legacy_rows: Array[String] = h._hud._disclosures.food_breakdown_lines(legacy)
	h._assert_hud("a frame with no link kinds still renders today's two generic rows",
		_any(legacy_rows, DetailFormat.FOOD_LABEL_TRANSFER_RECEIVED)
			and _any(legacy_rows, DetailFormat.FOOD_LABEL_TRANSFER_SENT))
	h._assert_hud("…and states no link kind it was not told",
		not _any(legacy_rows, DetailFormat.TRANSFER_LABEL_LOCAL)
			and not _any(legacy_rows, DetailFormat.TRANSFER_LABEL_ROUTE))

	# Hand the reference band back, so a chapter appended after this one starts where the rest do.
	h._hud.update_band_alerts([BandFx.band_fixture()])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()


# ---- FIXTURES ------------------------------------------------------------------------------------

## A camp where nothing crossed either link — no transfer keys on either account. Everything else is
## the reference band with a hay larder, so the only reason this state renders no transfer row is that
## there was no transfer.
func _quiet_band() -> Dictionary:
	return _hay_keeper(QUIET_ENTITY, "Band 1")

## The camp of states 2 and 4 — goods in over BOTH links on the food account, hay in over one and out
## down the other on the fodder account. One band, both popovers, so the two can be held side by side.
func _linked_band() -> Dictionary:
	var band := _hay_keeper(LINKED_ENTITY, "Band 2")
	band[DetailFormat.TRANSFER_LOCAL_RECEIVED_TURN_KEY] = FOOD_LOCAL_IN
	band[DetailFormat.TRANSFER_ROUTE_RECEIVED_TURN_KEY] = FOOD_ROUTE_IN
	# **THE HAY MOVES DIFFERENTLY FROM THE GRAIN, DELIBERATELY.** Same turn, same two links, different
	# amounts and one different direction — so a ledger reading the other account's figures fails here
	# instead of looking plausible on every frame.
	band[DetailFormat.FODDER_TRANSFER_LOCAL_RECEIVED_TURN_KEY] = HAY_LOCAL_IN
	band[DetailFormat.FODDER_TRANSFER_ROUTE_SENT_TURN_KEY] = HAY_ROUTE_OUT
	return band

## …and the camp that took a shipment in and launched a party out on one turn — state 3's subject, and
## the only shape whose row is a DIFFERENCE rather than a single flow.
func _both_ways_band() -> Dictionary:
	var band := _linked_band()
	band["entity"] = BOTH_WAYS_ENTITY
	band["id"] = "Band 3"
	band = BandFx.with_band_id(band)
	band[DetailFormat.TRANSFER_ROUTE_SENT_TURN_KEY] = FOOD_ROUTE_OUT
	return band

## The chapter's band shape: the reference fixture under this chapter's own entity and handle, plus a
## hay larder — without one there is no `Fodder:` row and no fodder popover to make the claims about.
func _hay_keeper(entity: int, id: String) -> Dictionary:
	var band := BandFx.band_fixture()
	band["id"] = id
	band["entity"] = entity
	band = BandFx.with_band_id(band)
	band["fodder_store"] = HAY_STORE
	band["turns_of_fodder"] = HAY_TURNS
	band["fodder_income"] = HAY_GROWN
	band["fodder_need"] = HAY_PENS
	return band


# ---- HARNESS PLUMBING ----------------------------------------------------------------------------

## The `⇄` phrase of every transfer row in a produced ledger, with the indent, glyph and number
## stripped off — what the two accounts must agree on exactly. Comparing the WORDS rather than whole
## rows is what lets the food ledger's two decimals and the fodder ledger's one both pass; DEDUPED and
## SORTED, because the claim is which phrases each account uses, not the order or the count its own
## flows happen to arrive in.
func _transfer_phrases(lines: Array) -> Array:
	var phrases: Array = []
	for line in lines:
		var text := String(line)
		var at := text.find(DetailFormat.TRANSFER_GLYPH)
		if at >= 0 and not phrases.has(text.substr(at)):
			phrases.append(text.substr(at))
	phrases.sort()
	return phrases

## Every produced line that is NOT an indented breakdown row — a sentence, a footer, a paragraph. The
## claim this exists for is that there are none: earlier cuts of this readout explained themselves in
## prose and were rejected for it, and prose is invisible to a `contains` test for row text.
func _prose_lines(lines: Array) -> Array:
	var prose: Array = []
	for line in lines:
		if not String(line).begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT):
			prose.append(String(line))
	return prose

func _any(lines: Array, needle: String) -> bool:
	for line in lines:
		if String(line).contains(needle):
			return true
	return false

func _any_lower(lines: Array, needle: String) -> bool:
	for line in lines:
		if String(line).to_lower().contains(needle):
			return true
	return false

func _count_containing(lines: Array, needle: String) -> int:
	var found := 0
	for line in lines:
		if String(line).contains(needle):
			found += 1
	return found

func _breakdown_meta(kind: String, entity: int) -> String:
	return HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + kind + BREAKDOWN_KEY_SEPARATOR \
		+ str(entity)

## Open (or close) a disclosure the way a CLICK does — `meta_clicked` on the live label, which is the
## same edge `DisclosureController.wire_label` connected.
func _click_breakdown(kind: String, entity: int) -> void:
	var meta := _breakdown_meta(kind, entity)
	# **SEARCHED FROM THE HARNESS ROOT, not from the HUD.** A player band's detail can render into the
	# Band/City panel, a sibling CanvasLayer rather than a child of the HUD, and a HUD-rooted walk
	# would silently find nothing.
	var label := _find_meta_label(h, meta)
	if label == null:
		# **A CLICK THAT NEVER HAPPENED IS A FAILED PRECONDITION, NOT AN ADVISORY** — every assertion
		# that follows is about what the OPEN popover holds, and all of them pass on a drawer that
		# rendered no disclosure at all.
		h._fail("no detail label offering '%s' — the disclosure was never rendered" % meta)
		return
	label.meta_clicked.emit(meta)

func _find_meta_label(node: Node, meta: String) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).text.contains("[url=%s]" % meta):
		return node
	for child in node.get_children():
		var found := _find_meta_label(child, meta)
		if found != null:
			return found
	return null

## Every piece of text rendered under the harness, joined — the popover included, since it parents
## into the HUD as a `Window` child.
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
