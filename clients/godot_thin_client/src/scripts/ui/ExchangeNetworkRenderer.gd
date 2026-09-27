class_name ExchangeNetworkRenderer
extends RefCounted

## Renders the player's EXCHANGE NETWORK on the map (issue #624) — the `trade_network` map layer
## (`MapToggles`), drawn only while `ClientSettings.is_map_toggle_on` says so. Extracted from
## MapView's old `_draw_supply_links` (composition — MapView owns one and calls `draw_network`
## during its `_draw` pass, just under the band markers). Holds no state of its own: every frame it
## reads `_view.units` and the selection through the `_view` back-ref, and draws through the same
## canvas item.
##
## Three marks, and which one carries DIRECTION is the whole design:
##
## | mark | from | says |
## |---|---|---|
## | a gold glow between two camps | each band's `pooling_links` | these two pool — weight/brightness by the link's RUNG (open ground faint, a kept road stronger) |
## | a ring on a camp | its `pooled` food crossings this turn | gold = gave more than it took, cool = took more than it gave, none = about even |
## | a dashed arrow | this turn's `shipment_*` crossings | a trade party carried goods from this camp to that one |
##
## ⛔ **A LOCAL LINE HAS NO ARROWHEAD, AND MUST NOT GROW ONE.** Pooling is an anonymous pool
## (`core_sim/src/supply.rs` `balance_commodity`): every camp above its fair share pays in, every camp
## below draws out, and there is no fact of the form "A's grain went to B". A line with an arrow would
## assert a pairing the sim never decided. Each camp's OWN net is exact, so direction lives on the
## node — the ring — and only a shipment, which names both ends, gets an arrow.
##
## **Player's own camps only for the network**, exactly as the retired chain was — the pool is
## single-faction by construction. A shipment arrow may point at ANOTHER people's camp, so its far
## end goes through `_view._unit_hidden_by_fog`: no arrow to a camp the player cannot see.
##
## Every draw is under the band markers (MapView calls this before `draw_primary_bands`), so a line
## runs INTO a token rather than across it; the arrow is inset by the ring's radius at both ends so
## its head lands outside the receiving token instead of under it.

# ---- LOCAL EXCHANGE — the undirected pooling lines -----------------------------------------------
## **A GOLD GLOW, TWO STROKES PER LINK** — a wide, faint `HudStyle.TRADE` HALO underneath and a thin,
## brighter `TRADE` CORE on top. Gold because the map already spends blue on rivers and brown on
## roads: drawn in loam's pale-blue `SIGNAL`, these links read as rivers in play. The halo is what
## makes it read as LIGHT rather than as one more painted line on the terrain.
##
## Open ground (`HudTradeVocab.OPEN_GROUND_RUNG`): within the free reach, no kept road. The thin,
## dim rung — the terrain already draws the roads (#554), so this layer stays quiet.
const LOCAL_OPEN_CORE_WIDTH := 1.5
const LOCAL_OPEN_CORE_OPACITY := 0.55
const LOCAL_OPEN_HALO_WIDTH := 5.0
const LOCAL_OPEN_HALO_OPACITY := 0.14
## A link whose whole run is on a kept road rung — thicker and brighter, so a road-held network reads
## as one.
const LOCAL_ROAD_CORE_WIDTH := 2.5
const LOCAL_ROAD_CORE_OPACITY := 0.85
const LOCAL_ROAD_HALO_WIDTH := 8.0
const LOCAL_ROAD_HALO_OPACITY := 0.22
## A link touching the SELECTED band: this much more opacity (both strokes) and width (the core) than
## its rung's own, and the halo widened by the same step times `SELECTED_HALO_WIDTH_SCALE`.
const SELECTED_OPACITY_BOOST := 0.15
const SELECTED_WIDTH_BOOST := 1.0
const SELECTED_HALO_WIDTH_SCALE := 2.0
## Cap on a boosted opacity, so a boost never overflows the colour's alpha.
const MAX_OPACITY := 1.0

# ---- TRADE ROUTES — the directed shipment arrows ------------------------------------------------
const ROUTE_WIDTH := 2.0
const ROUTE_OPACITY := 0.75
const ROUTE_SELECTED_OPACITY := 1.0
## A dark CASING under the route arrow (`HudStyle.GROUND`), this much wider than the shaft — what
## lifts a light ink off light terrain, and what makes a route read bolder than a pooling line, which
## has none. Under the head too, grown by the same amount.
const ROUTE_CASING_EXTRA_WIDTH := 2.0
const ROUTE_CASING_OPACITY := 0.55
## The casing head is longer than the ink head by the casing's margin on BOTH its tip and its base.
const ROUTE_CASING_HEAD_GROWTH := 2.0 * ROUTE_CASING_EXTRA_WIDTH
## Dash length along the shaft, in pixels — dashed so a route never reads as a pooling line.
const ROUTE_DASH := 6.0
## The arrowhead's length, as a fraction of the hex radius, with a floor so it stays a head at far zoom.
const ROUTE_ARROWHEAD_FACTOR := 0.30
const ROUTE_ARROWHEAD_MIN := 7.0

# ---- NODE RINGS — each camp's direction of flow -------------------------------------------------
## Ring radius as a fraction of the hex radius — outside the band token (`MapView.BAND_TOKEN_RADIUS_FACTOR`)
## and its food-runway dot, so it rings the marker rather than cutting through it.
const RING_RADIUS_FACTOR := 0.62
const RING_WIDTH := 3.0
const RING_OPACITY := 0.95
const RING_SEGMENTS := 32

## On a WRAPPING map, a segment spanning more than this fraction of the drawn map is a wrap artifact
## (its two ends were placed on different copies of the map by `_hex_center_wrapped`), and is skipped
## — the disconnected-link idiom `map-renderers.md` describes. A map that does not wrap has no copies,
## so a long segment there is a real one: the retired chain skipped it anyway, which dropped any link
## wider than 40% of the map.
const WRAP_SKIP_FRACTION := 0.4
## Two camps closer than this (pixels) share a tile — there is no line to draw between them.
const MIN_SEGMENT_LENGTH := 0.5

## The marks one frame draws, keyed by what they are — each a list of Dictionaries naming BANDS
## (never pixels), so a harness can assert which links, arrows and rings the layer chose without
## reading a frame.
##   `MARKS_LINKS`  — `{a, b, rung_id, selected}` per undirected pooling pair, deduped
##   `MARKS_ARROWS` — `{sender, receiver, party_id, selected}` per shipment, deduped by party
##   `MARKS_RINGS`  — `{band, net, giver}` per ringed camp
const MARKS_LINKS := "links"
const MARKS_ARROWS := "arrows"
const MARKS_RINGS := "rings"
const MARK_A := "a"
const MARK_B := "b"
const MARK_RUNG_ID := "rung_id"
const MARK_SELECTED := "selected"
const MARK_SENDER := "sender"
const MARK_RECEIVER := "receiver"
const MARK_PARTY_ID := "party_id"
const MARK_BAND := "band"
const MARK_NET := "net"
const MARK_GIVER := "giver"

var _view: MapView = null

func _init(view: MapView) -> void:
	_view = view

## Draw the whole layer, or nothing when the `trade_network` toggle is off.
func draw_network(radius: float, origin: Vector2) -> void:
	var marks := collect_marks()
	var by_band := _visible_bands()
	for link in marks[MARKS_LINKS]:
		_draw_local_link(link, by_band, radius, origin)
	for arrow in marks[MARKS_ARROWS]:
		_draw_route_arrow(_center_of(by_band[arrow[MARK_SENDER]], radius, origin),
			_center_of(by_band[arrow[MARK_RECEIVER]], radius, origin), bool(arrow[MARK_SELECTED]), radius)
	for ring in marks[MARKS_RINGS]:
		var tint: Color = HudStyle.TRADE if bool(ring[MARK_GIVER]) else HudStyle.READY
		_view.draw_arc(_center_of(by_band[ring[MARK_BAND]], radius, origin), radius * RING_RADIUS_FACTOR,
			0.0, TAU, RING_SEGMENTS, Color(tint, RING_OPACITY), RING_WIDTH, true)

## What the layer would draw this frame — all three lists empty while the toggle is off.
func collect_marks() -> Dictionary:
	var marks := {MARKS_LINKS: [], MARKS_ARROWS: [], MARKS_RINGS: []}
	if not ClientSettings.is_map_toggle_on(MapToggles.TRADE_NETWORK):
		return marks
	var by_band := _visible_bands()
	var selected_band := _selected_player_band_id()
	marks[MARKS_LINKS] = _collect_local_links(by_band, selected_band)
	marks[MARKS_ARROWS] = _collect_shipments(by_band, selected_band)
	marks[MARKS_RINGS] = _collect_rings()
	return marks

## Every VISIBLE band by its band id — the player's own always, another people's only on a hex the
## player can see. A shipment's far end is looked up here, which is what keeps an arrow out of fog.
func _visible_bands() -> Dictionary:
	var by_band: Dictionary = {}
	for unit in _view.units:
		if _view._unit_hidden_by_fog(unit):
			continue
		if Array(unit.get("pos", [])).size() != 2:
			continue
		var band_id := TradeLedger.band_id_of(unit)
		if band_id != HudConst.NO_BAND_ID:
			by_band[band_id] = unit
	return by_band

## The selected band's band id when it is one of the player's, else `HudConst.NO_BAND_ID`.
func _selected_player_band_id() -> int:
	if _view.selected_unit_id < 0:
		return HudConst.NO_BAND_ID
	for unit in _view.units:
		if int(unit.get("entity", -1)) == _view.selected_unit_id and HudConst.is_player_unit(unit):
			return TradeLedger.band_id_of(unit)
	return HudConst.NO_BAND_ID

func _touches(selected_band: int, a: int, b: int) -> bool:
	return selected_band != HudConst.NO_BAND_ID and (a == selected_band or b == selected_band)

func _center_of(unit: Dictionary, radius: float, origin: Vector2) -> Vector2:
	var pos: Array = Array(unit.get("pos", []))
	return _view._hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)

func _is_wrap_artifact(a: Vector2, b: Vector2) -> bool:
	return _view._wrap_horizontal and absf(a.x - b.x) > _view.last_map_size.x * WRAP_SKIP_FRACTION

## Each pooling pair ONCE — `a–b` and `b–a` are the same link, and both bands list it. Both ends must
## be the player's own visible camps; the pool is single-faction, so that is every real link.
func _collect_local_links(by_band: Dictionary, selected_band: int) -> Array:
	var out: Array = []
	var seen: Dictionary = {}
	for unit in _view.units:
		if not HudConst.is_player_unit(unit):
			continue
		var self_id := TradeLedger.band_id_of(unit)
		if not by_band.has(self_id):
			continue
		for link in TradeLedger.pooling_links(unit):
			var other_id := int(link[HudTradeVocab.LINK_BAND_ID])
			var pair := Vector2i(mini(self_id, other_id), maxi(self_id, other_id))
			if seen.has(pair) or not by_band.has(other_id) \
					or not HudConst.is_player_unit(by_band[other_id]):
				continue
			seen[pair] = true
			out.append({
				MARK_A: pair.x,
				MARK_B: pair.y,
				MARK_RUNG_ID: String(link[HudTradeVocab.LINK_RUNG_ID]),
				MARK_SELECTED: _touches(selected_band, self_id, other_id),
			})
	return out

func _draw_local_link(link: Dictionary, by_band: Dictionary, radius: float, origin: Vector2) -> void:
	var a := _center_of(by_band[link[MARK_A]], radius, origin)
	var b := _center_of(by_band[link[MARK_B]], radius, origin)
	if a.distance_to(b) < MIN_SEGMENT_LENGTH or _is_wrap_artifact(a, b):
		return
	var on_road := String(link[MARK_RUNG_ID]) != HudTradeVocab.OPEN_GROUND_RUNG
	var core_width := LOCAL_ROAD_CORE_WIDTH if on_road else LOCAL_OPEN_CORE_WIDTH
	var core_opacity := LOCAL_ROAD_CORE_OPACITY if on_road else LOCAL_OPEN_CORE_OPACITY
	var halo_width := LOCAL_ROAD_HALO_WIDTH if on_road else LOCAL_OPEN_HALO_WIDTH
	var halo_opacity := LOCAL_ROAD_HALO_OPACITY if on_road else LOCAL_OPEN_HALO_OPACITY
	if bool(link[MARK_SELECTED]):
		core_width += SELECTED_WIDTH_BOOST
		halo_width += SELECTED_WIDTH_BOOST * SELECTED_HALO_WIDTH_SCALE
		core_opacity = minf(MAX_OPACITY, core_opacity + SELECTED_OPACITY_BOOST)
		halo_opacity = minf(MAX_OPACITY, halo_opacity + SELECTED_OPACITY_BOOST)
	# Halo first, so the core lies on top of its own glow.
	_view.draw_line(a, b, Color(HudStyle.TRADE, halo_opacity), halo_width, true)
	_view.draw_line(a, b, Color(HudStyle.TRADE, core_opacity), core_width, true)

## This turn's shipments, sender → receiver, one arrow per trade party. Read off the player's own
## camps' ledgers (`TradeLedger.net_shipment_crossings`, so a cancelled export has already netted to
## nothing); a shipment between two of the player's camps is on both ledgers, and `party_id` is what
## makes it one arrow. Returned cargo with no export left to net against is NOT a trade leg and draws
## no arrow.
func _collect_shipments(by_band: Dictionary, selected_band: int) -> Array:
	var out: Array = []
	var seen: Dictionary = {}
	for unit in _view.units:
		if not HudConst.is_player_unit(unit):
			continue
		var self_id := TradeLedger.band_id_of(unit)
		if not by_band.has(self_id):
			continue
		for crossing in TradeLedger.net_shipment_crossings(unit):
			if bool(crossing.get(TradeLedger.RETURNED_FLAG, false)):
				continue
			var counterparty := int(crossing[HudTradeVocab.CROSSING_COUNTERPARTY_ID])
			var sender := self_id
			var receiver := counterparty
			if TradeLedger.cause_of(crossing) == HudTradeVocab.CAUSE_SHIPMENT_IN:
				sender = counterparty
				receiver = self_id
			var party := int(crossing[HudTradeVocab.CROSSING_PARTY_ID])
			var key: Variant = party if party != HudTradeVocab.NO_BAND else Vector2i(sender, receiver)
			if seen.has(key) or not by_band.has(sender) or not by_band.has(receiver):
				continue
			seen[key] = true
			out.append({
				MARK_SENDER: sender,
				MARK_RECEIVER: receiver,
				MARK_PARTY_ID: party,
				MARK_SELECTED: _touches(selected_band, sender, receiver),
			})
	return out

func _draw_route_arrow(from: Vector2, to: Vector2, selected: bool, radius: float) -> void:
	if _is_wrap_artifact(from, to):
		return
	# Inset both ends by the ring's radius: the shaft leaves the sender's ring and the head lands on
	# the receiver's, outside the token it would otherwise be drawn under.
	var inset := radius * RING_RADIUS_FACTOR
	var head := maxf(ROUTE_ARROWHEAD_MIN, radius * ROUTE_ARROWHEAD_FACTOR)
	# Two camps too close for both insets and a head have no shaft left to draw.
	if from.distance_to(to) <= 2.0 * inset + head:
		return
	var direction := (to - from).normalized()
	var start := from + direction * inset
	var end := to - direction * inset
	var color := Color(HudStyle.INK, ROUTE_SELECTED_OPACITY if selected else ROUTE_OPACITY)
	var casing := Color(HudStyle.GROUND, ROUTE_CASING_OPACITY)
	var shaft_end := end - direction * head
	# The casing is one solid line under the dashes, so the gaps read as dark rather than as terrain.
	_view.draw_line(start, shaft_end, casing, ROUTE_WIDTH + ROUTE_CASING_EXTRA_WIDTH, true)
	_view._draw_arrowhead(start, end + direction * ROUTE_CASING_EXTRA_WIDTH, casing,
		head + ROUTE_CASING_HEAD_GROWTH)
	# The dashes stop at the head's base, so the head is a clean solid triangle.
	_view.draw_dashed_line(start, shaft_end, color, ROUTE_WIDTH, ROUTE_DASH)
	_view._draw_arrowhead(start, end, color, head)

## One ring per occupied tile, on the tile's ACTIVE band — the one `BandMarkerRenderer` draws as the
## full-brightness top card (the selected band when it is here, else the first in snapshot order) —
## so the ring always describes the camp the stack is showing. Only a player camp in a pooling
## network is ringed, and only when its net `pooled` food this turn does not read `even`
## (`HudTradeVocab.EVEN_FLOOR`, the Trade tab's own dead band). Out of the pool (a negative net) is a
## giver — `HudStyle.TRADE`, the links' own gold, so the layer tells ONE story: gold is goods flowing
## out. Into it is a taker — cool, `HudStyle.READY`. **Not `SIGNAL` for the cool side**: `SIGNAL` is
## cream on ember, orange on kiln and pale blue on loam, while `READY` is blue or teal on all four and
## sits clear of every theme's `TRADE` gold. **Not `WARN` for the giver**: sharing food is not a
## warning.
func _collect_rings() -> Array:
	var active_by_tile: Dictionary = {}
	for unit in _view.units:
		if _view._unit_hidden_by_fog(unit):
			continue
		var pos: Array = Array(unit.get("pos", []))
		if pos.size() != 2:
			continue
		var tile := Vector2i(int(pos[0]), int(pos[1]))
		if not active_by_tile.has(tile) or int(unit.get("entity", -1)) == _view.selected_unit_id:
			active_by_tile[tile] = unit
	var out: Array = []
	for tile in active_by_tile:
		var unit: Dictionary = active_by_tile[tile]
		if not HudConst.is_player_unit(unit) or TradeLedger.network_id(unit) == HudTradeVocab.NO_NETWORK:
			continue
		var band_id := TradeLedger.band_id_of(unit)
		if band_id == HudConst.NO_BAND_ID:
			continue
		var net := TradeLedger.cause_net(unit, HudTradeVocab.COMMODITY_FOOD, [HudTradeVocab.CAUSE_POOLED])
		if TradeLedger.is_even(net):
			continue
		out.append({MARK_BAND: band_id, MARK_NET: net, MARK_GIVER: net < 0.0})
	return out
