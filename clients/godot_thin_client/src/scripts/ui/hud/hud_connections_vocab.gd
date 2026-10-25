class_name HudConnectionsVocab
extends RefCounted

## **THE "PEOPLES WE KNOW" ROSTER'S VOCABULARY** (arc #527, issue #549) — the band page's list of the
## ties one band holds: its head, its three tie states and their words, and the lines that say what a
## parked tie needs. `ConnectionsRoster` is its one reader.
##
## A LEAF: it reads nothing, so it can be read from a `const` initializer anywhere without a
## load-order cycle (`.claude/rules/client/hud-modules.md`). The remembered-position sentence is NOT
## here — it is `HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT`, shared with the shipment
## picker so the two surfaces word the same memory the same way.

## The head, on the band page's Peoples tab (or its section under Parties on a wide shell).
const HEAD := "Peoples we know"

## The head's readout: how many ties the band holds, parked ones included.
const HEAD_COUNT_FORMAT := "%d"

## **THE EMPTY STATE**, one line, under the head.
const EMPTY := "This band has met no other people yet."

## **THE THREE STATES A TIE IS IN**, read off the published row alone (`ConnectionsRoster.tie_state`).
##   * GROWING — it saw contact in the turn that just resolved (`resolved_turn`, see below).
##   * FADING — no contact this turn, strength still above `HudConst.TIE_STRENGTH_NONE`.
##   * PARKED — strength at `TIE_STRENGTH_NONE`: "we know such a people exist and have no current
##     dealings". Shown, never hidden.
const STATE_GROWING := 0
const STATE_FADING := 1
const STATE_PARKED := 2
## **STATIONED** (issue #774) — THIS band has people posted with the subject and standing there. It
## replaces growing / fading on the row: a post's tie rises when a runner's report reaches home, not every
## turn, so the state word says where the people are rather than which way the tie moves.
const STATE_STATIONED := 3

## **THE FRAME'S TURN RUNS ONE AHEAD OF EVERY CONTACT STAMP.** The sim stamps contact
## (`advance_connections`, `TurnStage::Visibility`) with the tick of the turn being resolved, and
## `advance_tick` then increments it in the Snapshot stage BEFORE the frame is captured — so the turn a
## frame publishes (`snapshot["turn"]`, `HudBandLaborState.current_turn()`) is one past the turn whose
## contact it carries. "This turn" for a tie, a sighting or a heart reading means the turn that just
## RESOLVED: `resolved_turn(frame_turn)`. Every reader compares against it, never the raw frame turn.
const RESOLVED_TURN_LAG := 1

## The turn that just resolved, given the frame's published turn. `NO_TURN` stays `NO_TURN`.
static func resolved_turn(frame_turn: int) -> int:
	if frame_turn == NO_TURN:
		return NO_TURN
	return frame_turn - RESOLVED_TURN_LAG

## How many turns old a sighting stamped `stamp_turn` is, floored at 0 (a sighting from the turn that
## just resolved is 0 old).
static func age_of(stamp_turn: int, frame_turn: int) -> int:
	return maxi(0, resolved_turn(frame_turn) - stamp_turn)

## The turn before any is known (`HudBandLaborState`'s own initial `_current_turn`). Never equal to a
## real turn, so it never reads as contact.
const NO_TURN := -1

const STATE_WORDS := {
	STATE_GROWING: "growing",
	STATE_FADING: "fading",
	STATE_PARKED: "parked",
	STATE_STATIONED: "stationed",
}

## What each state's word says on hover.
const STATE_TOOLTIPS := {
	STATE_GROWING: "In contact this turn — the tie is strengthening.",
	STATE_FADING: "Not seen this turn — the tie weakens every turn without contact.",
	STATE_PARKED: "This band has no tie to them. Another of your bands can still send to them.",
	STATE_STATIONED: "Your people are posted with them — their news keeps the tie strong.",
}

## Strength as a whole percent of a full tie (`0..1` on the wire).
const STRENGTH_FORMAT := "%d%%"

## **WHAT A PARKED TIE NEEDS, WITH WHERE TO GO** — line 2 of a parked row, in amber beside the state it
## explains. It carries the remembered position because meeting them again starts there, and words
## it as a sighting ("were last seen"), never as a live position: a connection only grants
## `Discovered`.
const PARKED_HINT_FORMAT := "No tie — meet them again. Last seen at (%d, %d), turn %d."

## …and the same row when no position was ever recorded.
const PARKED_HINT := "No tie — meet them again."

## Line 2 of a row whose band was seen in the turn that just resolved: it is in sight, so no "may have
## moved". Older sightings keep the remembered sentence (`HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT`).
const IN_SIGHT_FORMAT := "In sight at (%d, %d)"

## The map tooltip line for a remembered band's ghost marker (name, sighting turn).
const REMEMBERED_HOVER_FORMAT := "%s — last seen turn %d, may have moved"

## The row's hover: when the tie began and when it last saw contact.
const ROW_TOOLTIP_FORMAT := "First met turn %d · last contact turn %d"

## The gap between a row's two lines: none — they are one row, and the block's own separation is
## what spaces one tie from the next.
const ROW_LINE_SEPARATION := 0

## The separator between line 1's name and its strength / state cells.
const ROW_SEPARATOR := "·"

## Metadata the harness identifies a roster row by (value: the subject's `band_id`), the tab's (or the
## wide section's) node name, and the rows box inside it.
const ROW_META := &"connections_roster_row"
const BLOCK_NAME := "ConnectionsRoster"
const ROWS_NAME := "ConnectionsRosterRows"

## The node name of the Peoples tab's scrolling list — one of the Band/City panel's SANCTIONED
## `ScrollContainer`s, under `BandCityPanel.ZONE_PEOPLES`. The name is how `band_panel_preview` tells a
## sanctioned scroll from a stray.
const LIST_NAME := "PeoplesList"
## The scrollbar gutter between that scroll and the rows box.
const GUTTER_NAME := "PeoplesListGutter"

# ---- STATION PEOPLE (issue #774) -----------------------------------------------------------------
# The inline link on a live tie's row, and the words on the row of a band this one has people posted
# with. The compose sheet it opens is `BandPanelController._fill_station_compose_sheet`.

## The link on line 2 of a row, and what it reads once people are posted (or walking) there.
const STATION_LINK := "Station people"
const STATION_LINK_DONE := "Stationed"
const STATION_LINK_TOOLTIP := "Post some of your people with them. Their news keeps the tie strong."
const STATION_LINK_DONE_TOOLTIP := "Your people are already posted with them. Recall them from the Parties tab."
const STATION_LINK_NO_FREE_REASON := "No free workers — take someone off a work row to station them."

## The hover line added to a stationed row: how many are posted, and how stale their news is.
const STATION_ROW_HOVER_FORMAT := "%d of your people are stationed with them · news about %d %s old"
const STATION_TURN_ONE := "turn"
const STATION_TURN_MANY := "turns"

## Metadata the harness finds the link by (value: the subject's `band_id`).
const STATION_LINK_META := &"connections_station_link"

# ---- TRADE LINK -------------------------------------------------------------------------------
# The `Trade` link beside `Station people` on a row: it opens the band's Trade sheet with this row's band
# already its destination. Its face is the Trade band verb's own mark (`HudComposeVocab.BAND_VERBS`).
const TRADE_LINK := "Trade"
const TRADE_LINK_TOOLTIP := "Open a trade shipment to them."
## Max width of the verb's mark on the link, so a 256px source cannot size the row around itself.
const TRADE_LINK_ICON_MAX_WIDTH := 14
## The callable's key in the `diplomacy` bundle the roster is handed beside the Station press.
const TRADE_PRESS_KEY := "trade_press"
## **A BAND THE PEOPLE KNOWS ONLY THROUGH A SIBLING** — this band holds no row to it, another of the
## player's bands does. Line 2 of its row names that sibling (the one holding the freshest sighting) and
## the sighting.
const KNOWN_THROUGH_FORMAT := "Known through %s · last seen at (%d, %d), turn %d"
## …and the same when that row never recorded a position.
const KNOWN_THROUGH_NO_SIGHTING_FORMAT := "Known through %s"
## The state cell on such a row, in place of a strength and a state word this band does not have.
const KNOWN_THROUGH_WORD := "known"
## Metadata the harness finds the link by (value: the subject's `band_id`).
const TRADE_LINK_META := &"connections_trade_link"
