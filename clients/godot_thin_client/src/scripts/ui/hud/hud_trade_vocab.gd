class_name HudTradeVocab
extends RefCounted

## **THE BAND TRADE TAB'S VOCABULARY** (issue #731, `docs/band_trade_tab_ux_proposal.html`) — the wire's
## codes for a transfer crossing, the tab's thresholds, its words and its sizes, in ONE place so no
## reader spells a cause as a bare int and no row states a word the rest of the tab words differently.
##
## A LEAF: it reads `HudConst` and nothing else, so it can be read from a `const` initializer anywhere
## without a load-order cycle (`.claude/rules/client/hud-modules.md`).

# ---- THE WIRE --------------------------------------------------------------------------------------
## The cohort keys the native decoder publishes (`native/src/dict/population.rs`). All three are
## inserted unconditionally, so a band that crossed nothing carries an empty list, never a missing key.
const CROSSINGS_KEY := "transfer_crossings"
const POOLING_LINKS_KEY := "pooling_links"
const NETWORK_SPAN_KEY := "supply_network_span_tiles"
## The supply network this band pools in — `0` means none (`MapView.SUPPLY_NETWORK_SOLO`).
const NETWORK_ID_KEY := "supply_network_id"
const NO_NETWORK := 0

## One crossing's fields.
const CROSSING_COMMODITY := "commodity"
const CROSSING_READINGS := "readings"
const CROSSING_DIRECTION := "direction"
const CROSSING_LINK := "link"
const CROSSING_CAUSE := "cause"
const CROSSING_COUNTERPARTY_ID := "counterparty_band_id"
const CROSSING_COUNTERPARTY_NAME := "counterparty_name"
const CROSSING_COUNTERPARTY_FACTION := "counterparty_faction"
const CROSSING_PARTY_ID := "party_id"
const CROSSING_AMOUNT := "amount"
## `counterparty_band_id` / `party_id` when the crossing names none.
const NO_BAND := 0

## One pooling link's fields. `rung_id` is `RouteRungState.rungKey` (`"route:trail"`), `""` for open
## ground — some tile on the run has no kept road.
const LINK_BAND_ID := "band_id"
const LINK_DISTANCE := "distance_tiles"
const LINK_RUNG_ID := "rung_id"
const OPEN_GROUND_RUNG := ""

## **THE SCHEMA'S CODES** (`TransferCrossingState` in `snapshot.fbs`). Named here and nowhere else.
const DIRECTION_IN := 0
const DIRECTION_OUT := 1
const LINK_LOCAL := 0
const LINK_ROUTE := 1
const CAUSE_POOLED := 0
const CAUSE_DOWRY_OUT := 1
const CAUSE_DOWRY_IN := 2
const CAUSE_SHIPMENT_OUT := 3
const CAUSE_SHIPMENT_IN := 4
const CAUSE_PARTY_HOME := 5
const CAUSE_PARTY_PROVISIONS := 6

## **WHAT COUNTS AS TRADE.** The Local arm is `pooled`; the Route arm is a shipment either way. A
## band's own party coming home, a party's launch larder and a split's dowry are NEVER on the tab —
## they are the band's own hunting and foraging (the Food popover's rows) and the split's own event.
const TRADE_CAUSES: Array[int] = [CAUSE_POOLED, CAUSE_SHIPMENT_IN, CAUSE_SHIPMENT_OUT]
const SHIPMENT_CAUSES: Array[int] = [CAUSE_SHIPMENT_IN, CAUSE_SHIPMENT_OUT]

## The store keys of the two larders on a crossing. Food's is the sim's store key, `provisions`.
const COMMODITY_FOOD := HudConst.STORE_ITEM_PROVISIONS
const COMMODITY_FODDER := HudConst.CARGO_ITEM_FODDER

# ---- THRESHOLDS ------------------------------------------------------------------------------------
## **A NET SMALLER THAN THE READOUT'S ONE DECIMAL CAN STATE READS `even`**, never an arrow on `0.0`.
## Half the last printed digit — the smallest amount the one-decimal figure can show as non-zero.
const EVEN_FLOOR := 0.05
## A direction with MORE than this many shipments folds its tail behind a `N more shipments` row…
const SHIPMENT_FOLD_TRIGGER := 4
## …keeping this many, the largest by total amount. Under the trigger a direction is stated whole.
const SHIPMENT_FOLD_KEEP := 3

# ---- WORDS -----------------------------------------------------------------------------------------
const NETWORK_KEY_WORD := "Network"
const NETWORK_CAMPS_FORMAT := "%d camps ›"
const NETWORK_SPAN_FORMAT := "within %d tiles"
## A network whose camps share one tile publishes a span of 0 — it is still a network.
const NETWORK_SPAN_SHARED := "one camp's ground"
const NETWORK_NONE := "not pooling"
const NETWORK_CAMPS_TOOLTIP := "Every camp this band pools with"

const GOOD_SINGULAR_FORMAT := "%d good"
const GOOD_PLURAL_FORMAT := "%d goods"
const SHIPMENT_SINGULAR_FORMAT := "%d shipment"
const SHIPMENT_PLURAL_FORMAT := "%d shipments"
const IMPORT_SINGULAR_FORMAT := "%d import"
const IMPORT_PLURAL_FORMAT := "%d imports"
const EXPORT_SINGULAR_FORMAT := "%d export"
const EXPORT_PLURAL_FORMAT := "%d exports"
const MORE_SHIPMENT_SINGULAR_FORMAT := "%d more shipment"
const MORE_SHIPMENT_PLURAL_FORMAT := "%d more shipments"
const SPLIT_JOIN := " · "
const NONE_WORD := "none"

const IMPORTS_WORD := "Imports"
const EXPORTS_WORD := "Exports"
const IN_GLYPH := "▲"
const OUT_GLYPH := "▼"
## A shipment's arrow: `←` for what came in, `→` for what left.
const IMPORT_ARROW := "←"
const EXPORT_ARROW := "→"
const OPENS_CARET := "›"

## One good's row: its net this turn, and how many rating piles stand behind it when more than one.
const AMOUNT_FORMAT := "%s %.1f"
const RATINGS_FORMAT := "%d ratings"
const EVEN_WORD := "even"
## A pile's SIGNED amount on a hover card (`+0.8` / `-0.4`).
const SIGNED_AMOUNT_FORMAT := "%+.1f"
## A shipment's cargo clause: `12.0 food · 1.1 wood`.
const CARGO_TERM_FORMAT := "%.1f %s"

const EMPTY_HEAD := "Nothing crossed this turn."
const EMPTY_BODY := "Camps pool when one is short of what another holds; shipments appear here the turn they land or leave."

# ---- THE OVERFLOW PANEL ----------------------------------------------------------------------------
const CAMPS_TITLE := "The pooling network"
const CAMPS_COUNT_FORMAT := "%d camps"
const GOOD_SCOPE_TITLE_FORMAT := "%s across the network"
const GOOD_SCOPE_SUMMARY_FORMAT := "%d camps moved it; %d sat even"
const LOCAL_TITLE := "Pooled this turn"
const ROUTE_TITLE_BOTH := "Trade this turn"
const ROUTE_TITLE_IN := "Imports this turn"
const ROUTE_TITLE_OUT := "Exports this turn"
const THIS_BAND_WORD := "this band"
const VIA_FORMAT := "via %s"
const DISTANCE_FORMAT := "%d tiles"
const LOST_IN_TRANSIT := "lost in transit — friction"

# ---- THE LINK RUNG ---------------------------------------------------------------------------------
const OPEN_GROUND_WORD := "open ground"
const OPEN_GROUND_TIP := "within the free reach, no road — pools at full friction"
const RUNG_HOLDS_FORMAT := "holds a link to %d tiles"
const RUNG_FRICTION_FORMAT := "friction ×%.2f"
## The icon a rung draws, keyed on the rung id's own name (the part after the branch's `route:`).
const RUNG_ICON_PATH := "path"
const RUNG_ICON_TRAIL := "trail"
const RUNG_ICON_DIRT_ROAD := "dirt_road"
const RUNG_ICON_PAVED_ROAD := "paved_road"
const RUNG_KEY_SEPARATOR := ":"

# ---- THE FACTION MARK ------------------------------------------------------------------------------
const FACTION_OURS := "your people"
const FACTION_THEIRS := "another people"
const FACTION_NAME_FALLBACK_FORMAT := "Faction %d"
const FACTION_TIP_FORMAT := "%s — %s"

# ---- SIZES -----------------------------------------------------------------------------------------
## The tab's row text and its fainter sub-text (a good's `6 ratings`, a shipment's cargo).
const ROW_FONT_SIZE := 12
const SUB_FONT_SIZE := 10
## The section heads (`⇄ LOCAL EXCHANGE`) and the network line's key.
const HEAD_FONT_SIZE := 10
## The list popover's title.
const TITLE_FONT_SIZE := 13
const ROW_SEPARATION := 6
const SECTION_SEPARATION := 8
const ROWS_SEPARATION := 2
## How far a direction sub-head and a shipment line step in under the TRADE ROUTE head
## (section › direction › shipment), and how far a shipment's goods step in again in the overflow list.
const DIRECTION_INDENT := 10
const SHIPMENT_INDENT := 22
const CARGO_INDENT := 34
## The network line's box padding.
const NETWORK_PADDING_H := 8
const NETWORK_PADDING_V := 5
const NETWORK_CORNER_RADIUS := 2
const NETWORK_BORDER_WIDTH := 1
## A rating chip's padding and rim.
const CHIP_PADDING_H := 4
const CHIP_PADDING_V := 0
const CHIP_CORNER_RADIUS := 2
const CHIP_BORDER_WIDTH := 1
const CHIP_SEPARATION := 3
## The hover card's width floor and its gap from the row it describes.
const HOVER_MIN_WIDTH := 230.0
const HOVER_GAP := 10.0
const HOVER_EDGE_MARGIN := 8.0
## The camps panel's two right-hand columns — the link (its rung, or the relay's first hop) and the
## distance — fixed so they line up down the list.
const CAMP_LINK_COLUMN_WIDTH := 120.0
const CAMP_DISTANCE_COLUMN_WIDTH := 48.0

# ---- THE LIST POPOVER ------------------------------------------------------------------------------
## Node names a harness finds it by.
const POPOVER_NAME := "TradeListPopover"
const POPOVER_SCROLL_NAME := "TradeListScroll"
## How far off the anchor row the popover floats — the disclosure popover's own gap.
const POPOVER_GAP := 4.0
## Clearance kept between the popover and the edge of the visible screen.
const POPOVER_EDGE_MARGIN := 8.0
const POPOVER_PADDING := 10
## The four sides `POPOVER_PADDING` is applied to.
const POPOVER_MARGIN_SIDES := ["left", "top", "right", "bottom"]
