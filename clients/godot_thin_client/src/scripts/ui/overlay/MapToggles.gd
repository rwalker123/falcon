class_name MapToggles
extends RefCounted

## **THE MAP-LAYER TOGGLES — a registry, one row per switchable map layer** (issue #624).
##
## The `☰` button on the minimap's border (`OverlayPicker`) opens a `MAP LAYERS` popover with one
## checkbox per row here; `ClientSettings` persists each row's state under its `key`; the renderer
## that owns the layer asks `ClientSettings.is_map_toggle_on(key)`. **Neither the popover nor the
## settings store names a toggle** — adding a layer is a row here plus the renderer's own check.
##
## A toggle is NOT an overlay channel. A channel is a RASTER that replaces the tile fill, and exactly
## one is painted at a time (`OverlayChannels`); a toggle is an independent layer of marks drawn over
## whatever channel is on, and any number can be on at once.
##
## Row fields:
##   `key`     — the stable id: the `ClientSettings` key and what a renderer asks for. Never renamed,
##               or every player's saved choice for it is orphaned.
##   `label`   — the checkbox's text.
##   `tooltip` — what the layer shows, on hover.
##   `default` — the state before the player has ever touched it (and after Restore Defaults).

const KEY := "key"
const LABEL := "label"
const TOOLTIP := "tooltip"
const DEFAULT := "default"

## The exchange network — the lines between the player's pooling bands, the giver/taker ring on each,
## and this turn's trade-route shipments (`ExchangeNetworkRenderer`).
const TRADE_NETWORK := "trade_network"

const ROWS: Array[Dictionary] = [
	{
		KEY: TRADE_NETWORK,
		LABEL: "Trade network",
		TOOLTIP: "Lines between your camps that pool goods, a ring on each camp that gave (warm) or "
			+ "took (cool) food this turn, and an arrow for each trade shipment",
		DEFAULT: true,
	},
]

## The row for `key`, or an empty Dictionary for a key the registry does not hold.
static func row_for(key: String) -> Dictionary:
	for row in ROWS:
		if String(row[KEY]) == key:
			return row
	return {}

## `key`'s default state. A key outside the registry is OFF: nothing can be drawn for a layer the
## client does not know.
static func default_for(key: String) -> bool:
	var row := row_for(key)
	return bool(row.get(DEFAULT, false))
