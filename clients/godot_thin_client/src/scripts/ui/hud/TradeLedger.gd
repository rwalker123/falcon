class_name TradeLedger
extends RefCounted

## **THE TRADE TAB'S ARITHMETIC** (issue #731) — every figure the tab, its overflow panel and the Band
## panel's Food/Fodder popovers state about a crossing, derived from the band dict the native decoder
## publishes. All-`static` and node-free, so the zone builder, the popover and the preview harness read
## ONE answer and none of them can disagree with another about a sum.
##
## ⛔ **A SUM IS WITHIN ONE GOOD, NEVER ACROSS TWO.** A good's row nets that good across its own rating
## piles; `hide` and `bone` stay two rows, because a hide-plus-bone scalar is the retired trade axis
## (arc #527). Nothing here adds two commodities together, and nothing may.

# ---- CROSSINGS -------------------------------------------------------------------------------------

## Every crossing on `band`, as the decoder sent it.
static func crossings(band: Dictionary) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var list_variant: Variant = band.get(HudTradeVocab.CROSSINGS_KEY, [])
	if not (list_variant is Array):
		return out
	for row_variant in list_variant:
		if row_variant is Dictionary:
			out.append(row_variant)
	return out

static func cause_of(crossing: Dictionary) -> int:
	return int(crossing.get(HudTradeVocab.CROSSING_CAUSE, HudTradeVocab.CAUSE_POOLED))

static func commodity_of(crossing: Dictionary) -> String:
	return String(crossing.get(HudTradeVocab.CROSSING_COMMODITY, ""))

static func is_import(crossing: Dictionary) -> bool:
	return int(crossing.get(HudTradeVocab.CROSSING_DIRECTION, HudTradeVocab.DIRECTION_IN)) \
		== HudTradeVocab.DIRECTION_IN

## The crossing's amount, SIGNED by its direction: `+` came in, `-` went out.
static func signed_amount(crossing: Dictionary) -> float:
	var amount := absf(float(crossing.get(HudTradeVocab.CROSSING_AMOUNT, 0.0)))
	return amount if is_import(crossing) else -amount

static func readings_of(crossing: Dictionary) -> Array:
	var readings_variant: Variant = crossing.get(HudTradeVocab.CROSSING_READINGS, [])
	return readings_variant if readings_variant is Array else []

## **THE PILE'S IDENTITY** — its rating, spelled off each reading's band name in the material's own
## axis order. Two ratings of one material are two piles and never share a key; food and fodder carry
## no readings and are one pile each.
static func rating_key(crossing: Dictionary) -> String:
	var parts: Array[String] = []
	for reading_variant in readings_of(crossing):
		if reading_variant is Dictionary:
			var reading: Dictionary = reading_variant
			parts.append("%s=%s" % [String(reading.get(HudCraftingVocab.READING_AXIS_KEY, "")),
				String(reading.get(HudCraftingVocab.READING_BAND_NAME_KEY, ""))])
	return "|".join(parts)

## Is this crossing TRADE — the tab's business? `pooled` and the two shipment causes are; a party
## coming home, a party's launch larder and a split's dowry are not (`HudTradeVocab.TRADE_CAUSES`).
static func is_trade(crossing: Dictionary) -> bool:
	return HudTradeVocab.TRADE_CAUSES.has(cause_of(crossing))

static func is_pooled(crossing: Dictionary) -> bool:
	return cause_of(crossing) == HudTradeVocab.CAUSE_POOLED

static func is_shipment(crossing: Dictionary) -> bool:
	return HudTradeVocab.SHIPMENT_CAUSES.has(cause_of(crossing))

## The signed net of `commodity` across every crossing whose cause is in `causes` — the popover's one
## reader. Summed within the one commodity, never across two.
static func cause_net(band: Dictionary, commodity: String, causes: Array) -> float:
	var net := 0.0
	for crossing in crossings(band):
		if commodity_of(crossing) == commodity and causes.has(cause_of(crossing)):
			net += signed_amount(crossing)
	return net

## Is anything on the tab this turn — a pooled good or a shipment?
static func has_trade(band: Dictionary) -> bool:
	for crossing in crossings(band):
		if is_trade(crossing):
			return true
	return false

# ---- GOODS -----------------------------------------------------------------------------------------

## Where a commodity sorts: food first, fodder second, every material after them.
const RANK_FOOD := 0
const RANK_FODDER := 1
const RANK_MATERIAL := 2

static func commodity_rank(commodity: String) -> int:
	if commodity == HudTradeVocab.COMMODITY_FOOD:
		return RANK_FOOD
	if commodity == HudTradeVocab.COMMODITY_FODDER:
		return RANK_FODDER
	return RANK_MATERIAL

static func is_material(commodity: String) -> bool:
	return commodity_rank(commodity) == RANK_MATERIAL

## The word a commodity is read as: the larder rows' own labels for food and fodder, the capitalised
## id for a material (`HudLoadoutVocab.material_label`, this client's one idiom for that).
static func commodity_label(commodity: String) -> String:
	match commodity_rank(commodity):
		RANK_FOOD:
			return HudDisclosureVocab.DETAIL_ROW_FOOD
		RANK_FODDER:
			return HudDisclosureVocab.DETAIL_ROW_FODDER
	return HudLoadoutVocab.material_label(commodity)

## One good's field names in `goods_net` / a shipment's cargo.
const GOOD_COMMODITY := "commodity"
const GOOD_NET := "net"
const GOOD_PILES := "piles"
const PILE_READINGS := "readings"
const PILE_NET := "net"

## **THE LOCAL ARM, ONE ENTRY PER GOOD** — `{commodity, net, piles: [{readings, net}]}`, the good's
## net summed across its own rating piles and each pile's net beside it (the hover card). Food and
## fodder lead; materials follow by `|net|` descending, then by id so equal nets hold their place.
static func goods_net(band: Dictionary) -> Array[Dictionary]:
	var pooled: Array[Dictionary] = []
	for crossing in crossings(band):
		if is_pooled(crossing):
			pooled.append(crossing)
	return group_goods(pooled)

## `crossings` grouped by commodity, then by rating pile, each netted — the shape `goods_net` and a
## shipment's cargo share. Piles are ordered by `|net|` descending.
static func group_goods(list: Array[Dictionary]) -> Array[Dictionary]:
	var by_good: Dictionary = {}
	var order: Array[String] = []
	for crossing in list:
		var commodity := commodity_of(crossing)
		if not by_good.has(commodity):
			by_good[commodity] = {GOOD_COMMODITY: commodity, GOOD_NET: 0.0, "_piles": {}, "_order": []}
			order.append(commodity)
		var good: Dictionary = by_good[commodity]
		good[GOOD_NET] = float(good[GOOD_NET]) + signed_amount(crossing)
		var key := rating_key(crossing)
		var piles: Dictionary = good["_piles"]
		if not piles.has(key):
			piles[key] = {PILE_READINGS: readings_of(crossing), PILE_NET: 0.0}
			(good["_order"] as Array).append(key)
		var pile: Dictionary = piles[key]
		pile[PILE_NET] = float(pile[PILE_NET]) + signed_amount(crossing)
	var goods: Array[Dictionary] = []
	for commodity in order:
		var good: Dictionary = by_good[commodity]
		var piles_out: Array[Dictionary] = []
		for key in good["_order"]:
			piles_out.append((good["_piles"] as Dictionary)[key])
		piles_out.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
			return absf(float(a[PILE_NET])) > absf(float(b[PILE_NET])))
		goods.append({GOOD_COMMODITY: commodity, GOOD_NET: float(good[GOOD_NET]), GOOD_PILES: piles_out})
	goods.sort_custom(_good_sorts_before)
	return goods

static func _good_sorts_before(a: Dictionary, b: Dictionary) -> bool:
	var ra := commodity_rank(String(a[GOOD_COMMODITY]))
	var rb := commodity_rank(String(b[GOOD_COMMODITY]))
	if ra != rb:
		return ra < rb
	var na := absf(float(a[GOOD_NET]))
	var nb := absf(float(b[GOOD_NET]))
	if not is_equal_approx(na, nb):
		return na > nb
	return String(a[GOOD_COMMODITY]) < String(b[GOOD_COMMODITY])

## Does `net` read `even` — too small for the one-decimal readout to state?
static func is_even(net: float) -> bool:
	return absf(net) < HudTradeVocab.EVEN_FLOOR

# ---- SHIPMENTS -------------------------------------------------------------------------------------

## One shipment's field names.
const SHIPMENT_DIRECTION := "direction"
const SHIPMENT_PARTY_ID := "party_id"
const SHIPMENT_COUNTERPARTY_ID := "counterparty_band_id"
const SHIPMENT_COUNTERPARTY_NAME := "counterparty_name"
const SHIPMENT_COUNTERPARTY_FACTION := "counterparty_faction"
const SHIPMENT_TOTAL := "total"
const SHIPMENT_CARGO := "cargo"

## **THE ROUTE ARM, ONE ENTRY PER SHIPMENT.** A shipment is the crossings ONE party carried — grouped
## on `party_id`, falling back to the counterparty where the party is unnamed — and in ONE direction,
## so a band that ships to a neighbour that ships back reads as two shipments with the right arrow on
## each. `{direction, party_id, counterparty_*, total, cargo}`; `cargo` is `group_goods`' shape with
## each good's `net` as its (unsigned) amount. Ordered by total descending.
static func shipments(band: Dictionary) -> Array[Dictionary]:
	var groups: Dictionary = {}
	var order: Array[String] = []
	for crossing in crossings(band):
		if not is_shipment(crossing):
			continue
		var party := int(crossing.get(HudTradeVocab.CROSSING_PARTY_ID, HudTradeVocab.NO_BAND))
		var counterparty := int(crossing.get(HudTradeVocab.CROSSING_COUNTERPARTY_ID, HudTradeVocab.NO_BAND))
		var direction := int(crossing.get(HudTradeVocab.CROSSING_DIRECTION, HudTradeVocab.DIRECTION_IN))
		var key := "%d|%s" % [direction,
			("p%d" % party) if party != HudTradeVocab.NO_BAND else ("c%d" % counterparty)]
		if not groups.has(key):
			var carried: Array[Dictionary] = []
			groups[key] = {
				SHIPMENT_DIRECTION: direction,
				SHIPMENT_PARTY_ID: party,
				SHIPMENT_COUNTERPARTY_ID: counterparty,
				SHIPMENT_COUNTERPARTY_NAME: String(crossing.get(HudTradeVocab.CROSSING_COUNTERPARTY_NAME, "")),
				SHIPMENT_COUNTERPARTY_FACTION: int(crossing.get(HudTradeVocab.CROSSING_COUNTERPARTY_FACTION,
					HudConst.PLAYER_FACTION_ID)),
				"_crossings": carried,
			}
			order.append(key)
		var group_crossings: Array[Dictionary] = groups[key]["_crossings"]
		group_crossings.append(crossing)
	var out: Array[Dictionary] = []
	for key in order:
		var group: Dictionary = groups[key]
		var cargo := group_goods(group["_crossings"])
		var total := 0.0
		for good in cargo:
			good[GOOD_NET] = absf(float(good[GOOD_NET]))
			for pile in good[GOOD_PILES]:
				pile[PILE_NET] = absf(float(pile[PILE_NET]))
			total += float(good[GOOD_NET])
		group.erase("_crossings")
		group[SHIPMENT_CARGO] = cargo
		group[SHIPMENT_TOTAL] = total
		out.append(group)
	out.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return float(a[SHIPMENT_TOTAL]) > float(b[SHIPMENT_TOTAL]))
	return out

## The shipments in one direction, largest first.
static func shipments_in_direction(band: Dictionary, direction: int) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for shipment in shipments(band):
		if int(shipment[SHIPMENT_DIRECTION]) == direction:
			out.append(shipment)
	return out

## **THE TAB'S BADGE: THIS TURN'S SHIPMENTS, BOTH WAYS.** Pooling never counts — it happens most
## turns and whether the player looks or not, so a badge that counted it would never go out.
static func shipment_count(band: Dictionary) -> int:
	return shipments(band).size()

## The name a shipment's other end is read by: the sim's, or `Band #<id>` where it could not name one.
static func counterparty_label(shipment: Dictionary) -> String:
	var name := String(shipment.get(SHIPMENT_COUNTERPARTY_NAME, "")).strip_edges()
	if name != "":
		return name
	return HudFormat.BAND_ID_FALLBACK_NAME_FORMAT % int(shipment.get(SHIPMENT_COUNTERPARTY_ID,
		HudTradeVocab.NO_BAND))

# ---- THE NETWORK -----------------------------------------------------------------------------------

static func network_id(band: Dictionary) -> int:
	return int(band.get(HudTradeVocab.NETWORK_ID_KEY, HudTradeVocab.NO_NETWORK))

static func network_span(band: Dictionary) -> int:
	return int(band.get(HudTradeVocab.NETWORK_SPAN_KEY, 0))

static func band_id_of(band: Dictionary) -> int:
	return int(band.get("band_id", HudConst.NO_BAND_ID))

## **THE CAMPS THIS BAND POOLS WITH** — every band in `bands` sharing its `supply_network_id`, this
## band included. Empty for a band in no network. The pool is single-faction by construction
## (`supply::pools_freely`), so the player's own bands are the whole of it.
static func network_members(band: Dictionary, bands: Array) -> Array[Dictionary]:
	var members: Array[Dictionary] = []
	var id := network_id(band)
	if id == HudTradeVocab.NO_NETWORK:
		return members
	for candidate_variant in bands:
		if candidate_variant is Dictionary \
				and network_id(candidate_variant) == id:
			members.append(candidate_variant)
	return members

static func pooling_links(band: Dictionary) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var links_variant: Variant = band.get(HudTradeVocab.POOLING_LINKS_KEY, [])
	if not (links_variant is Array):
		return out
	for link_variant in links_variant:
		if link_variant is Dictionary:
			out.append(link_variant)
	return out

## One camp row's field names in `camp_rows`.
const CAMP_BAND := "band"
const CAMP_NAME := "name"
const CAMP_IS_SELF := "is_self"
const CAMP_DIRECT := "direct"
const CAMP_RUNG_ID := "rung_id"
const CAMP_DISTANCE := "distance"
const CAMP_VIA := "via"

## **EVERY NETWORK MEMBER, AS THE CAMPS PANEL LISTS THEM** — this band first, then its DIRECT links
## (each with its rung and its link distance, off this band's own `pooling_links`), then the RELAY
## camps, which pool with this band only through another member: their row names the first hop on
## the shortest chain (`via Barrowmere`) and the plain hex distance between the two camps. Direct and
## relay rows each sort by distance, then name.
##
## **THE CHAIN IS A BFS OVER EVERY MEMBER'S OWN LINKS**, because a band publishes only its direct
## links — a chain is not a clique. Ties between equally short chains break on the first hop's lower
## link distance, then its name, so the answer is a pure function of the snapshot.
static func camp_rows(band: Dictionary, members: Array[Dictionary], grid_width: int,
		wrap_horizontal: bool) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	if members.is_empty():
		return rows
	var self_id := band_id_of(band)
	var by_id: Dictionary = {}
	for member in members:
		by_id[band_id_of(member)] = member
	rows.append(_camp_row(band, true, true, HudTradeVocab.OPEN_GROUND_RUNG, 0, ""))
	# The direct links, off THIS band's own list.
	var direct: Array[Dictionary] = []
	var direct_distance: Dictionary = {}
	for link in pooling_links(band):
		var other_id := int(link.get(HudTradeVocab.LINK_BAND_ID, HudTradeVocab.NO_BAND))
		if other_id == self_id or not by_id.has(other_id) or direct_distance.has(other_id):
			continue
		var distance := int(link.get(HudTradeVocab.LINK_DISTANCE, 0))
		direct_distance[other_id] = distance
		direct.append(_camp_row(by_id[other_id], false, true,
			String(link.get(HudTradeVocab.LINK_RUNG_ID, HudTradeVocab.OPEN_GROUND_RUNG)), distance, ""))
	direct.sort_custom(_camp_sorts_before)
	rows.append_array(direct)
	# The relays: BFS from this band, carrying the FIRST HOP each node was reached through.
	var first_hop: Dictionary = {}
	var frontier: Array[int] = []
	for other_id in direct_distance:
		first_hop[other_id] = other_id
		frontier.append(other_id)
	var seen: Dictionary = {self_id: true}
	for other_id in direct_distance:
		seen[other_id] = true
	while not frontier.is_empty():
		var next_hop: Dictionary = {}
		for node_id in frontier:
			var node: Dictionary = by_id.get(node_id, {})
			for link in pooling_links(node):
				var reached := int(link.get(HudTradeVocab.LINK_BAND_ID, HudTradeVocab.NO_BAND))
				if seen.has(reached) or not by_id.has(reached):
					continue
				var hop := int(first_hop[node_id])
				if not next_hop.has(reached) or _hop_beats(hop, int(next_hop[reached]),
						direct_distance, by_id):
					next_hop[reached] = hop
		frontier = []
		for reached in next_hop:
			seen[reached] = true
			first_hop[reached] = next_hop[reached]
			frontier.append(reached)
	var relays: Array[Dictionary] = []
	for member in members:
		var member_id := band_id_of(member)
		if member_id == self_id or direct_distance.has(member_id):
			continue
		var via := ""
		if first_hop.has(member_id):
			via = HudFormat.band_name(by_id[int(first_hop[member_id])])
		var here := SourceForecast.band_tile(band)
		var there := SourceForecast.band_tile(member)
		var distance := SourceForecast.hex_distance_wrapped(here.x, here.y, there.x, there.y,
			grid_width, wrap_horizontal)
		relays.append(_camp_row(member, false, false, HudTradeVocab.OPEN_GROUND_RUNG, distance, via))
	relays.sort_custom(_camp_sorts_before)
	rows.append_array(relays)
	return rows

## Does first hop `a` beat `b` for the same node? Lower link distance, then name.
static func _hop_beats(a: int, b: int, direct_distance: Dictionary, by_id: Dictionary) -> bool:
	var da := int(direct_distance.get(a, 0))
	var db := int(direct_distance.get(b, 0))
	if da != db:
		return da < db
	return HudFormat.band_name(by_id[a]) < HudFormat.band_name(by_id[b])

static func _camp_row(member: Dictionary, is_self: bool, direct: bool, rung_id: String,
		distance: int, via: String) -> Dictionary:
	return {
		CAMP_BAND: member,
		CAMP_NAME: HudFormat.band_name(member),
		CAMP_IS_SELF: is_self,
		CAMP_DIRECT: direct,
		CAMP_RUNG_ID: rung_id,
		CAMP_DISTANCE: distance,
		CAMP_VIA: via,
	}

static func _camp_sorts_before(a: Dictionary, b: Dictionary) -> bool:
	if int(a[CAMP_DISTANCE]) != int(b[CAMP_DISTANCE]):
		return int(a[CAMP_DISTANCE]) < int(b[CAMP_DISTANCE])
	return String(a[CAMP_NAME]) < String(b[CAMP_NAME])

## One scoped camp's field names in `good_across_network`.
const SCOPED_ROWS := "rows"
const SCOPED_LOST := "lost"
const SCOPED_MOVED := "moved"
const SCOPED_EVEN := "even"
const SCOPED_NET := "net"

## **ONE GOOD ACROSS THE NETWORK** — each camp's net of `commodity` this turn, off that camp's OWN
## pooled crossings summed across ratings: this band first, then the camps that moved it by `|net|`
## descending, then the ones that sat even. `lost` is `-(Σ every member's net)` — the friction the
## distance ate, the one figure on the tab nobody received — so the column DELIBERATELY does not sum
## to zero. `moved` / `even` count the OTHER camps.
static func good_across_network(band: Dictionary, commodity: String,
		camps: Array[Dictionary]) -> Dictionary:
	var moved_rows: Array[Dictionary] = []
	var even_rows: Array[Dictionary] = []
	var self_row: Dictionary = {}
	var total := 0.0
	for camp in camps:
		var member: Dictionary = camp[CAMP_BAND]
		var net := cause_net(member, commodity, [HudTradeVocab.CAUSE_POOLED])
		total += net
		var row := camp.duplicate()
		row[SCOPED_NET] = net
		if bool(camp[CAMP_IS_SELF]):
			self_row = row
		elif is_even(net):
			even_rows.append(row)
		else:
			moved_rows.append(row)
	moved_rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return absf(float(a[SCOPED_NET])) > absf(float(b[SCOPED_NET])))
	var rows: Array[Dictionary] = []
	if not self_row.is_empty():
		rows.append(self_row)
	rows.append_array(moved_rows)
	rows.append_array(even_rows)
	return {
		SCOPED_ROWS: rows,
		SCOPED_LOST: -total,
		SCOPED_MOVED: moved_rows.size(),
		SCOPED_EVEN: even_rows.size(),
	}

## The rung id's own name — `trail` off `route:trail`, `""` for open ground.
static func rung_name(rung_id: String) -> String:
	var at := rung_id.rfind(HudTradeVocab.RUNG_KEY_SEPARATOR)
	return rung_id.substr(at + 1) if at >= 0 else rung_id

## The published ladder rung for `rung_id`, or `{}` where the catalog does not carry it.
static func rung_entry(rung_id: String, catalog: Array) -> Dictionary:
	if rung_id == HudTradeVocab.OPEN_GROUND_RUNG:
		return {}
	for entry_variant in catalog:
		if entry_variant is Dictionary \
				and HudRouteVocab.catalog_rung_key(entry_variant) == rung_id:
			return entry_variant
	return {}
