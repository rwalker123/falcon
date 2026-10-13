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
##   * GROWING — it saw contact THIS turn (`last_contact_turn == current turn`).
##   * FADING — no contact this turn, strength still above `HudConst.TIE_STRENGTH_NONE`.
##   * PARKED — strength at `TIE_STRENGTH_NONE`: "we know such a people exist and have no current
##     dealings". Shown, never hidden.
const STATE_GROWING := 0
const STATE_FADING := 1
const STATE_PARKED := 2
## **STATIONED** (issue #774) — THIS band has people posted with the subject and standing there. It
## replaces growing / fading on the row, because a post gets contact every turn and the tie cannot fade.
const STATE_STATIONED := 3

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
	STATE_PARKED: "No tie left. Nothing can flow until they are met again.",
	STATE_STATIONED: "Your people are posted with them — the tie gets contact every turn.",
}

## Strength as a whole percent of a full tie (`0..1` on the wire).
const STRENGTH_FORMAT := "%d%%"

## **WHAT A PARKED TIE NEEDS, WITH WHERE TO GO** — line 2 of a parked row, in amber beside the state it
## explains. It carries the remembered position because meeting them again starts there, and words
## it as a sighting ("were last seen"), never as a live position: a connection only grants
## `Discovered`.
const PARKED_HINT_FORMAT := "No tie — meet them again. Last seen at (%d, %d), turn %d."

## …and the same row when no position was ever recorded.
const PARKED_HINT := "No tie — meet them again before anything can flow."

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
const STATION_LINK_TOOLTIP := "Post some of your people with them. It keeps the tie from fading."
const STATION_LINK_DONE_TOOLTIP := "Your people are already posted with them. Recall them from the Parties tab."
## A PARKED tie has nothing to post people at — the link stays so the row teaches that the tie is the gate.
const STATION_LINK_PARKED_REASON := "No tie — meet them again first."
const STATION_LINK_NO_FREE_REASON := "No free workers — take someone off a work row to station them."

## The hover line added to a stationed row: how many are posted, and how stale their news is.
const STATION_ROW_HOVER_FORMAT := "%d of your people are stationed with them · news about %d %s old"
const STATION_TURN_ONE := "turn"
const STATION_TURN_MANY := "turns"

## Metadata the harness finds the link by (value: the subject's `band_id`).
const STATION_LINK_META := &"connections_station_link"
