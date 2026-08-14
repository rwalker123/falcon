class_name HudWorkVocab

## Band/City WORK-board + zone vocabulary — the paged work board, zone chrome, People/Workforce
## bars, standing-role cards, the allocation sections, the pager and the inspector strip.

# Leading label on the assign controls' band-picker dropdown ("which band supplies the workers").
const BAND_PICKER_LABEL := "Band:"

# Worker-stepper row chrome: the fixed-width −/+ buttons, the centered count column,
# and the row separation.
const WORKER_STEPPER_BUTTON_WIDTH := 28.0

const WORKER_STEPPER_VALUE_WIDTH := 32.0

const WORKER_STEPPER_SEPARATION := 6

# The two stepper FACES. One spelling, because two stepper families now draw them — the worker/party
# steppers (`HudWidgets.add_stepper_controls`) and the shipment manifest's per-row cargo stepper,
# which counts a FLOAT quantity of goods and so cannot share that builder's integer count. The minus
# is U+2212 MINUS SIGN, not a hyphen: it matches the `+`'s stroke weight and optical width, which a
# hyphen at this size visibly does not.
const STEPPER_MINUS_FACE := "−"

const STEPPER_PLUS_FACE := "+"

# Policy-picker layout: the two-line rung buttons (name over product line) wrap at most 3 per row, so
# the six-rung forage/local-hunt pickers read as two tidy rows of three and the four extractive rungs
# read as 3 + 1 with Eradicate alone on the second row. THREE IS A CEILING, NOT A TARGET — a picker
# with fewer rungs fills what it has, and a caller passing an explicit `columns` (a zone's narrower
# budget) is clamped to it rather than overriding it upward, so the wrap SHAPE is the same wherever
# the player meets these rungs. Four abreast was tried and read wrong: it made the expedition launch
# picker a different creature from the local hunt beside it, and it set the widest compose card's
# width off a row that never needed to be that wide.
const POLICY_PICKER_COLUMNS := 3

# **A THIRD ACCOUNT ON LINE 2 DID NOT MOVE THIS** (#426), and that is a measurement rather than an
# assumption: a wide-face ceiling of 2 was written for the three-account forage face and the rendered
# frame refuted it — `0.60 food · 0.20 fodder` three abreast comes out 555px against the
# deer hunt picker's long-standing 546, with nothing clipped, and 3 + 3 reads better than 2 + 2 + 2.

# The inset between a rung's box and its two-line face, vertical and horizontal. Sized well under
# `HudStyle._button_stylebox`'s authored 9/11 — that pair was sized to give ONE line of text vertical
# presence and to keep a lone word off the border, and a two-line face already has both. The height is
# not free: the FORAGE compose card is capped by the room left below it in the VIEWPORT
# (`ComposeSheet.refit` — there is no fixed pixel cap any more) and already spends most
# of itself on the rung gates and the crop list, and the commit button below is what falls off the fold
# when the picker grows (the `forage_crop_picker` guard in ui_preview asserts exactly that). The width
# is not free either — the face's longest LINE sets the rung's width, so 22px of side chrome per button
# is width the grid spends on nothing, and in the Band panel's fixed-width zone column (the
# PARTIES-zone launch picker, in a ~300px dock) the authored box pushed the picker 18px past its zone,
# where the zone's `clip_contents` ate the end of every metric line. Trim the box, never the type.
const POLICY_PICKER_PADDING_V := 4

const POLICY_PICKER_PADDING_H := 6

# The gap between the two lines of a rung's face. One pixel: they are one utterance ("this rung, and
# what it pays"), so they must read as a stacked pair rather than as two rows of a list.
const POLICY_PICKER_FACE_SEPARATION := 1

# **A PRESET IS A SHORTCUT TO A VALUE, NOT THE SUBJECT OF THE PANEL.** The rung name used to carry no
# size override at all, so it rendered at Godot's default control size (this project ships no
# replacement theme) — which made three buttons the LOUDEST thing on a sheet whose subject is the
# chart below them and the numbers below that. Reported from play. Both lines are stepped down
# together, keeping the one-step gap between them: the name still LEADS and the numbers still SUPPORT,
# but the pair now sits under the readout's answer rather than over it.
const POLICY_PICKER_NAME_FONT_SIZE := 12

# The metric line's type, ONE STEP under the rung name's. The name LEADS and the numbers SUPPORT: at a
# single size `0.32 food · 0.08 fodder` competed with `♻ Sustain` for the same glance instead of
# answering it.
const POLICY_PICKER_METRIC_FONT_SIZE := 11

# …and one step QUIETER, as an alpha on whatever colour the rung's state gives line 1
# (`HudStyle.button_font_color`), never a colour of its own. Deriving it is what keeps the two lines
# tinting as a unit — the single property the one-`Button.text` face had and the reason it was written
# that way: a selected, disabled, or (in future) warned rung moves BOTH lines by construction, because
# there is only ever one source colour to move.
const POLICY_PICKER_METRIC_ALPHA := 0.72

# Passed for `columns` to keep `HudWidgets.build_policy_picker`'s option-count default — a caller that only wants
# to set a LATER argument must still name this one, and a bare 0 there reads as "no columns".
const POLICY_PICKER_AUTO_COLUMNS := 0

# Two-line worker-stepper form (opt-in via `status_line`, used by the Forage/Hunt Current-actions
# rows): the title + stepper ride line 1, the yield/policy/status/notes drop to an indented, smaller
# secondary line 2 so the row reads narrow. `STATUS_LINE_INDENT` ≈ the leading resource-icon width, so
# line 2 sits under the title TEXT rather than under the icon; the flow separation is the gap between
# the status parts (which wrap to the next line rather than widening the panel); the two-line gap is
# the vertical space between line 1 and line 2.
const STATUS_LINE_INDENT := 18.0

const STATUS_LINE_SEPARATION := 6

const TWO_LINE_STEPPER_SEPARATION := 2

# Allocation-panel section headers + role hints (make the panel read as a "current actions"
# report and make the standing Scout/Warrior roles discoverable — the −/+ steppers ARE how
# you staff a scout mission now; there is no targeted map action).
const ALLOC_SECTION_FONT_SIZE := 10

# Vertical gap between the rows within one allocation section block (Workers / Current actions /
# Band roles / Orders / Send expedition). Matches the pre-section-block flat-list spacing so the
# tall stack reads unchanged; the Band/City panel spaces the blocks THEMSELVES apart (tall) or flows
# them into columns (wide).
const ALLOC_BLOCK_SEPARATION := 6

# The merged larder projection's section header (see `_build_food_outlook_block`). Its own block, not
# a line inside the summary RichTextLabel — BBCode cannot host a drawn chart.
const ALLOC_HEADER_FOOD_OUTLOOK := "Food outlook"

const ALLOC_NO_SOURCES_HINT := "No sources worked yet — select a tile or herd to assign foragers/hunters."

const SCOUT_ROLE_HINT := "Posts scouts that see around obstacles — more scouts range farther."

const WARRIOR_ROLE_HINT := "Guards the band — matters once threats arrive."

# Predators Phase 3 — the LIVE Warrior-card hint when a visible, camp-threatening predator sits within
# raid range of the band (see `BandPanelController._band_predator_threat_present`). Replaces the static
# hint above with a crimson alert; `%d` is the band's on-guard warrior count. The threat is derived
# CLIENT-side from the herd telemetry (fog-filtered → only visible predators) + the sim's echoed
# `raid_radius`; nothing new is asked of the wire beyond those two cohort fields.
const WARRIOR_THREAT_ALERT_FORMAT := "⚠ Predator nearby — %d on guard"

# Appended to a clickable Current-actions row's tooltip: the row's LABEL is an inline link that jumps
# the map to the source being worked (a forage tile, or a hunted herd's CURRENT tile). Scout/Warrior
# are band-wide roles with no tile, so their rows stay plain labels and never carry this.
const SOURCE_ROW_FOCUS_HINT := "Click to show this source on the map."

# ---- Band/City panel zones (docs/band_panel_ux_proposal.html) ---------------
## The tighter gap between the parts of one zone SECTION (bar → key → cards). The gap between the
## sections themselves travelled to `HudWidgets.ZONE_SECTION_SEPARATION` with `make_zone_column`, its
## only reader; this one has readers on both sides (the work board's capacity maths), so it stays.
const ZONE_BLOCK_SEPARATION := 6

## The zone box assumed when no dock is injected (the HUD-only ui_preview host), so the work board
## still pages against a sane measure instead of collapsing to one row.
const ZONE_FALLBACK_SIZE := Vector2(340.0, 360.0)

## A zone section head reserves exactly this height, so the work board's capacity maths and what the
## head actually draws are the same number.
const ZONE_HEAD_HEIGHT := 20.0

const ZONE_HEAD_SEPARATION := 6

const ZONE_HEAD_FONT_SIZE := 10

## Section-menu affordance (`⋯`) — a MenuButton, so its popup is a Window and cannot move any layout.
const SECTION_MENU_GLYPH := "⋯"

const SECTION_MENU_WIDTH := 22.0

const CONFIRM_DIALOG_TITLE := "Confirm"

## Zone section headers (uppercased by `HudWidgets.alloc_section_label`).
const ZONE_HEADER_PEOPLE := "People"

const ZONE_HEADER_WORKFORCE := "Workforce"

## The KEEPING block's head (`docs/plan_standing_upkeep.md` §2.5) — the two standing roles that hold
## what the band has built, and how their pools split when short. Its readout is the hands on the two
## roles together, the WORKFORCE head's `n idle of m` shape one scope down.
const ZONE_HEADER_KEEPING := "Keeping"

const KEEPING_ZONE_READOUT_FORMAT := "%d on keeping"

const ZONE_HEADER_WORK := "Work"

const ZONE_HEADER_PARTIES := "Parties"

## THE NARROW SHELL'S TAB LABELS, declared per SUBJECT (`BandCityPanel.set_zone_layout`). A tab picks
## a ZONE, and a zone's name states the scope its content is at — so a band's first tab reads `Band`
## and the faction page's reads `Faction`. They are their own words rather than the `ZONE_HEADER_*`
## section heads above: a head titles a block INSIDE a zone and a tab names the zone, and the two
## coinciding on `Work` is a coincidence rather than a shared fact.
const ZONE_TAB_BAND := "Band"

const ZONE_TAB_WORK := "Work"

const ZONE_TAB_PARTIES := "Parties"

## THE FACTION PAGE — the cycler's pinned first entry (issue #450), whose zones answer the
## band zones' own questions one rung up: who the faction IS, what it is DOING, what it KNOWS, and
## who is OUT.
const ZONE_TAB_FACTION := "Faction"

## Abbreviated deliberately: the narrow shell fits four tabs across a 354px strip, and `Knowledge`
## is the longest word of the set. Pending playtest.
const ZONE_TAB_KNOWLEDGE := "Know"

const FACTION_HEADER_KNOWLEDGE := "Knowledge"

## The KNOWLEDGE zone's other two blocks (issue #450, the four-zone body). **Settling is the
## sedentarization score under the player-facing word the manual uses**, and it lands here rather than
## beside the stores for the same reason the craft tracks do: it is not a stock and not a population —
## it is what the faction has BECOME, and what it unlocks is what its hands may attempt.
const FACTION_HEADER_SETTLING := "Settling"

const FACTION_HEADER_DISCOVERIES := "Discoveries"

## **THE SIM'S SEDENTARIZATION STAGE IS A WIRE TOKEN, AND THIS TABLE IS THE ONLY PLACE IT BECOMES A
## WORD.** `SedentarizationStage::as_str()` spells the three stages `none` / `soft` / `hard`, and
## `FactionReadouts.SEDENTARIZATION_STAGE_NONE` says outright that they are "not a word anyone sees" —
## so the SETTLING row rendered its key as a lowercase enum (`soft  ▰▰▰▱▱  62/100`) beside the
## capitalised `Nomadic` the below-threshold case already had. The words come from the sim's own
## prompts: the soft threshold asks whether to "establish a seasonal base", the hard one whether to
## "invest in storehouses and settle".
##
## **PROVISIONAL, pending playtest** — the same footing `ZONE_TAB_KNOWLEDGE`'s abbreviation is on.
const FACTION_SETTLING_STAGE_LABELS := {
    "none": "Nomadic",
    "soft": "Seasonal base",
    "hard": "Ready to settle",
}

## What an absent, `none` or UNRECOGNISED stage reads as — never the raw token, which is the whole
## point of the table above. It reads the table rather than restating the word, so `Nomadic` has one
## home and the fallback cannot drift from the `none` row.
const FACTION_SETTLING_NOMADIC: String = FACTION_SETTLING_STAGE_LABELS["none"]

## `▰▰▱▱▱  62/100` — the SETTLING row's VALUE: the meter and the score against the scale the sim
## reports it on. The stage word is the row's KEY, not part of this.
const FACTION_SETTLING_VALUE_FORMAT := "%s  %d/%d"

const FACTION_SETTLING_SCALE := 100

## **THE FACTION PAGE'S ROW SIZE — the `band` zone's vitals rows, which every other zone matches.**
## Those rows are a bare `RichTextLabel` carrying no size override, so this is Godot's stock default
## written down; the harness asserts the two are equal at render time rather than trusting the
## number. See `FactionRollup.STAT_ROW_FONT_SIZE` for why this is NOT the work board's 13.
const FACTION_STAT_ROW_FONT_SIZE := 16

## A discovery row's value: how many INSTANCES of that site kind the faction has found. The head's
## readout is the instance TOTAL, so a kind found three times reads `3` on one row rather than as
## three rows — the top bar's own "N is instances, the strip is kinds" split, stated in full here
## because this page has the room the strip does not.
const FACTION_DISCOVERY_COUNT_FORMAT := "%d"

## **THE KNOWLEDGE ZONE'S HEIGHT TIER.** All three of its blocks measured **336px** at the page's row
## size against the ~300px a horizontal dock offers, and the zone CLIPS — so a box below this drops
## DISCOVERIES and keeps Settling + the craft tracks (`FactionRollup.build_knowledge_zone`).
##
## **IT IS A REAL "CAN THIS BOX HOLD THE FULL BLOCK?" TEST, not a round number between two docks.**
## The full block measures **452px** at its worst case (five craft tracks, the sites list at its cap
## plus its `+N more`), so a box that only just clears this threshold still holds it with 28px to
## spare. The two boxes the panel actually offers are nowhere near either side — ~300 and ~1055 — so
## the margin is what protects a box that is not one of those two, and it is what shrinks as the
## block grows.
## **Re-measure before adding a row to this zone**; `band_panel_preview._report_zone_content_extent`
## prints the full block's extent on `band_panel_faction_knowledge` and the tiered one on
## `band_panel_faction_wide`, and this threshold must stay above the first.
const FACTION_KNOWLEDGE_FULL_MIN_HEIGHT := 480.0

## A discovered site whose catalog row carries no display name — the site_id is a worse name than
## none at all is a lie, so the id stands.
const FACTION_DISCOVERY_UNNAMED := "Unnamed site"

## **THE ALERT CLAUSE — what a faction row says where an aggregate would lie.** A runway is one larder
## against one band's drain and a kit condition is three durabilities per band; neither has a faction
## value, so those rows state HOW MANY bands are in trouble and the drill-down states which. The count
## and nothing more: naming the band here would make the row as long as the list it replaces.
const FACTION_ALERT_GLYPH := "⚠"

const FACTION_ALERT_ONE := "1 band"

const FACTION_ALERT_MANY := "%d bands"

## The Kit row's two states. It carries NO durabilities — a mean of three per band describes no band
## that exists — so it is the alert, or the word saying there is none.
const FACTION_KIT_ALL_EQUIPPED := "all equipped"

const FACTION_KIT_DRY_NOTE := "a kit has run out"

## The SHORTFALL note (issue #520) — a band whose gear works and does not go round. It is worded away
## from `FACTION_KIT_DRY_NOTE`'s finality on purpose: running out is permanent and a shortfall is the
## band outgrowing its gear, which crafting can answer.
const FACTION_KIT_SHORT_NOTE := "a kit does not go round"

## **THE SUMMARY TABS' VOCABULARY.** Work and Parties are one idea in two scopes — a row per thing,
## flagged when it wants attention — so they share every word below rather than each growing its own.
## The flag is the ORB's glyph and the orb's two severities; this page invents no third.
const FACTION_FLAG_GLYPH := "●"

## A zone head that has something to report: `2 of 5`. It reads as a fraction rather than a bare count
## because the count alone (`2`) is indistinguishable from the roster size on a small faction.
const FACTION_FLAGGED_FORMAT := "%d of %d"

## A band's one-line work summary. Counts rather than names: a row is one line and a band works up to
## 34 sources.
## Singular is spelled out rather than left to a `%d sources` that reads "1 sources" — which is the
## COMMON case here, not an edge one: a young band works one patch, and that is the first thing a new
## player sees on this tab.
const FACTION_SOURCES_FORMAT := "%d sources"

const FACTION_SOURCES_ONE := "1 source"

const FACTION_PENS_FORMAT := "%d pens"

const FACTION_PENS_ONE := "1 pen"

const FACTION_SUMMARY_SEPARATOR := " · "

## The faction's own alerts — the unworked-rung producer's, whose patches belong to the FACTION and so
## have no band's row to sit on.
const FACTION_LAND_ROW := "The land"

## An expanded band's worked sources, and an expanded party's own facts.
const FACTION_SOURCE_FORAGE_FORMAT := "Forage (%d, %d)"

const FACTION_SOURCE_HUNT_FORMAT := "Hunt %s"

const FACTION_SOURCE_CREW_FORMAT := "%d · %s"

const FACTION_PARTY_MISSION := "Mission"

const FACTION_PARTY_CREW := "Crew"

const FACTION_PARTY_PHASE := "Phase"

## A bare percentage in a drill-down row, where the row's own key already says what it is a percentage
## OF (the band page's `GROWTH_ROW_FORMAT` spells `of normal` because it stands alone; here the
## faction row above the list has already said it).
const FACTION_PERCENT_FORMAT := "%d%%"

## **`FACTION_TRADE_STOCK_FORMAT` IS RETIRED** (arc #527) with the faction page's Trade row.

## The PEOPLE key's trailing chip on the faction page, where a band's page carries its dependency
## count: how many bands the bar is summed over, so a total is never read as one band's.
const FACTION_BANDS_CHIP_FORMAT := "%d bands"

const FACTION_BANDS_CHIP_ONE := "1 band"

## The faction page's per-band and per-party lists are CAPPED — the zones clip rather than scroll, and
## neither list has a pager (the work BOARD's pager belongs to a band's own sources). What the cap
## drops is stated, never silently truncated.
const FACTION_LIST_ROWS_MAX := 6

const FACTION_LIST_MORE_FORMAT := "+%d more"

## A faction with no party out. The band page's parties zone says this with a disabled footer button;
## this page has no footer, so it says it in words.
const FACTION_PARTIES_EMPTY := "No parties out"

## A knowledge track the faction has finished. `HudFormat.meter_bar` would draw a full bar, which
## reads as "still climbing, nearly there" — the top-bar strip's own reasoning for its `✔ known`.
const FACTION_KNOWLEDGE_KNOWN := "known"

## The composition KEY's chip gap and type size. The bar/swatch geometry travelled to `HudWidgets`
## with `build_composition_bar` / `build_composition_key`; these two stay because the parties zone's
## link row and the dependency chip read them outside those builders.
const COMPOSITION_KEY_SEPARATION := 12

const COMPOSITION_KEY_FONT_SIZE := 11

## PEOPLE key glyphs + words (the words live in the tooltips the glyphs replaced).
const PEOPLE_GLYPH_CHILDREN := "👶"

const PEOPLE_GLYPH_WORKING := "🛠"

const PEOPLE_GLYPH_ELDERS := "🧓"

const PEOPLE_LABEL_CHILDREN := "children"

const PEOPLE_LABEL_WORKING := "working age"

const PEOPLE_LABEL_ELDERS := "elders"

## Above this many dependents per 100 workers the band carries more mouths than hands → WARN. Stays
## here because the chip's own tint reads it beside the tooltip that `HudFormat.dependency_tooltip`
## now writes; the ratio BASE and the two tooltip strings travelled with that formatter.
const PEOPLE_DEPENDENCY_HEAVY := 100

## The chip says the COUNT, not the ratio. `dep 88/100` was the analyst's framing of a number the
## player has to act on — it reads as a score out of 100 (and the game's designer could not tell what
## it meant), while the bar beside it already shows the split. "14 dependents" is the fact; the ratio
## and what it implies live in the tooltip, which is where the teaching belongs.
const PEOPLE_DEPENDENCY_FORMAT := "%d dependents"

## The band zone renders at DENSER TIERS as its box shrinks. **A TIER NEVER DROPS A BLOCK** — the zone
## scrolls (`BAND_ZONE_SCROLL_NAME`), so content that outgrows the box is reached rather than lost, and
## the only thing a tier decides is how tightly what is there is drawn.
## At/above TALL: the full-height food-outlook chart.
## Below TALL: the COMPACT chart — same series, same empty marker, less height.
## All measured against the zone BOX, never against the dock edge.
##
## **They USED to omit** — below CHART_MIN the zone built no chart at all and the role cards dropped
## their hint line to a tooltip. That is the behaviour this pair of thresholds no longer expresses:
## silently losing content is never an acceptable outcome, and the parties list had already answered
## the same requirement by scrolling.
const BAND_ZONE_TALL_MIN_HEIGHT := 420.0

const BAND_ZONE_CHART_MIN_HEIGHT := 340.0

const FOOD_CHART_COMPACT_HEIGHT := 42.0

## The three tiers as an ordinal, so `zones_resized` can tell a mere re-page (the work board) from a
## band-zone tier change (which needs the zone rebuilt, not re-paged — the density is authored at build
## time and the split across columns with it).
const BAND_ZONE_TIER_SHORT := 0

const BAND_ZONE_TIER_COMPACT := 1

const BAND_ZONE_TIER_TALL := 2

## WORKFORCE readout + segment keys.
const WORKFORCE_IDLE_FORMAT := "%d idle of %d"

## Workers out with a party, appended to the idle readout and shown ONLY when there are any.
## **They are a HEADER CLAUSE, not a bar segment.** The sim removes a party's members from the parent
## band's working-age cohort the turn it launches, so they are not inside the `working_age` the bar's
## segments partition — drawing them as a slice made the segments sum past their own denominator
## ("4 idle of 16" over a bar totalling 22). The fact still has to be reachable, so it reads here.
const WORKFORCE_AWAY_FORMAT := " · %d away"

const WORKFORCE_AWAY_TOOLTIP := "Out with a party — no longer part of this band's workforce, and not counted in the bar below."

const WORKFORCE_KEY_FORAGE := "Forage"

const WORKFORCE_KEY_HUNT := "Hunt"

const WORKFORCE_KEY_ROLES := "Roles"

## The hands on a source's BUILD (`docs/plan_standing_upkeep.md` §2.2) — the second allocation, which
## the Forage and Hunt segments deliberately do not absorb. Those two name what a crew TAKES; a builder
## takes nothing, so folding them in would have a band gathering from a patch nobody is gathering.
##
## **It exists for exactly the reason the bench's does, one allocation over.** `effective_idle` nets
## builders out (they are staffed labor), so without a segment of their own three builders vanished
## from a bar whose segments are supposed to partition the same `working_age` the header counts
## against — `Forage 9 · Hunt 6 · Idle 3` accounting for 18 with the builders invisible as well as
## miscounted.
const WORKFORCE_KEY_BUILD := "Build"

## The crafting bench's crew. **The segments PARTITION the workforce**, so the bench needs one of its
## own the moment idle stops counting it: netting the crew out of idle without naming it here would
## drop those hands off the bar entirely, and the key beneath would no longer add up to the head count
## the zone's own header states.
const WORKFORCE_KEY_BENCH := "Bench"

const WORKFORCE_KEY_IDLE := "Idle"

## Standing-role CARDS (the fix for roles reading as one more worked source in a list).
const ROLE_NAME_SCOUT := "Scout"

const ROLE_NAME_WARRIOR := "Warrior"

## **THE TWO KEEPING ROLES** (`docs/plan_standing_upkeep.md` §2.5) — cards in the same family as the
## two above, because they ARE the same family: a band-wide count of hands, set by `assign_labor`.
## One per food web, which is the split the two intensification ladders already have.
const ROLE_NAME_AGRICULTURE := "Agriculture"

const ROLE_NAME_HUSBANDRY := "Husbandry"

## **THE HINTS NAME THE POOL, NOT A TILE.** The hands are measured against the SUM of what the band
## holds on that web and split across every source of it, so a player reading the card must not go
## looking for a per-source stepper — there is none, and this sentence is where that is said.
const AGRICULTURE_ROLE_HINT := "Keeps every tended patch and Field this band works. Short of the sum, they rot."

const HUSBANDRY_ROLE_HINT := "Keeps every tamed herd and pen this band works. Short of the sum, animals drift off."

## **THE FUND-MODE CONTROL** — how this band splits a keeping pool it cannot stretch, `spread` or
## `priority` (`upkeep_mode <faction> <band> …`). It renders under the two keeping cards and ONLY
## where the band holds something on either web: the choice is meaningless with nothing to fund, and
## a control offered there would read as a setting the player had failed to make.
const UPKEEP_MODE_TITLE := "Short of keepers"

const UPKEEP_MODE_SPREAD_LABEL := "Spread"

const UPKEEP_MODE_PRIORITY_LABEL := "Priority"

## The two modes stated as what they DO to the band's own sources, since that is the choice. Ride
## each button's tooltip, so the pair of one-word faces stays narrow enough for the dock's flanks.
const UPKEEP_MODE_SPREAD_HINT := "Fund every source in proportion — everything degrades a little."

const UPKEEP_MODE_PRIORITY_HINT := "Fund the biggest investments in full and let the marginal ones rot."

## The line under the pair. It states the POOL's own arithmetic, so the mode reads as an answer to a
## live shortfall rather than as an abstract preference; the covered form is the reassuring half and
## renders in the same slot, which is what keeps the control from appearing only when it is too late.
const UPKEEP_MODE_SHORT_FORMAT := "Short %s work of %s this turn."

const UPKEEP_MODE_COVERED_TEXT := "Your keepers cover everything this band holds."

## …and the messages the command feed shows for the press. Keyed by the token so the two can never
## drift from what was actually sent; the fallback exists only for a mode the sim gains before this
## table does, and says the token verbatim rather than describing the wrong behaviour.
const UPKEEP_MODE_COMMAND_MESSAGES := {
	HudConst.UPKEEP_FUND_MODE_SPREAD: "Short of keepers, everything this band holds degrades a little.",
	HudConst.UPKEEP_FUND_MODE_PRIORITY: "Short of keepers, this band holds its biggest investments and lets the rest go.",
}

const UPKEEP_MODE_COMMAND_MESSAGE_FALLBACK := "Keeping split set to %s."

## Trimmed to what the SHORT tier affords: at 8/8 the band zone stood 5px past a 360px T/B dock
## (measured by `band_panel_preview`'s zone-bounds assertion, which is why it exists).
const ROLE_CARD_SEPARATION := 6

const ROLE_CARD_NAME_FONT_SIZE := 12

## Two lines of hint at ALLOC_SECTION_FONT_SIZE, so the two cards stay the same height whatever the
## hint wraps to.
const ROLE_CARD_HINT_HEIGHT := 28.0

## WORK BOARD geometry. Every one of these heights is BOTH what the element reserves in
## `_work_board_capacity` and what it actually draws at, so the page can never overflow its zone.
const WORK_ROW_HEIGHT := 28.0

## The node name of the PARTIES zone's scrolling LIST — the party rows plus whichever inspector strip
## is open, between that zone's fixed head and its fixed Scout/Hunt/Deny footer.
##
## **IT IS ONE OF THE BAND/CITY PANEL'S TWO SANCTIONED `ScrollContainer`s, and the NAME is how that
## stays true.** The panel is no-scroll by default: a zone whose content height fed back into a FIXED
## reservation is the map flicker the fixed cross-axis size exists to prevent. This one cannot, because
## the scroll's whole contribution to the zone's minimum is `PARTIES_LIST_MIN_HEIGHT` and nothing else —
## the list's real height never reaches the panel. `band_panel_preview` asserts that every
## `ScrollContainer` in the panel is a NAMED one under the zone that sanctions it, so the invariant
## still holds everywhere else rather than having been deleted.
const PARTIES_LIST_NAME := "PartiesList"

## The least room that scrolling list is ever given, in board rows. **A floor that never binds any
## shipped layout** — head + footer + this sits far under the band flank's two-column extent, which is
## what `BandCityPanel.PANEL_HEIGHT_WIDE_TWO_COLUMN` is derived from — and its only job is to stop a
## future shorter box collapsing the list into a bare scrollbar with no visible row.
##
## THREE, because a list showing fewer than the row being read plus one either side has stopped reading
## as a list: at two there is nothing to say the bar beside it is for scrolling.
const PARTIES_LIST_MIN_ROWS := 3

## That floor in pixels. `WORK_ROW_HEIGHT` is this client's one row unit — the work board's capacity
## maths divides by it — so the parties list states its floor in the same unit rather than in a number
## of its own. A party row is taller than a board row (42 against 28), which makes this a floor of
## about two party rows: deliberately conservative, since the point is a non-degenerate list and not a
## promise about how many parties are visible.
const PARTIES_LIST_MIN_HEIGHT := float(PARTIES_LIST_MIN_ROWS) * WORK_ROW_HEIGHT

## The node name of the BAND zone's scrolling stack — the vitals / PEOPLE / food-outlook / WORKFORCE
## blocks, whether they are stacked flat or split across the widened flank's two columns.
##
## **IT IS THE SECOND SANCTIONED `ScrollContainer`, and it exists because THE TIERS STOPPED DELETING
## CONTENT.** The zone used to answer a box it could not fill by building no food-outlook chart and
## hint-less role cards; at a 1920 logical viewport on a horizontal dock that is one column packing
## 299px into a 300px box, so the chart and the hints were simply gone until roughly a 2250 viewport
## earned the flank a second column. Losing content is never an acceptable outcome — the parties list
## had already answered the identical requirement — so the blocks are all built and the stack scrolls.
##
## It is safe for the same reason the parties list is: a `ScrollContainer` reports no minimum on its
## scrolling axis, so what the stack holds never reaches the panel and the strip's cross-axis size
## stays a pure function of dock / collapse / window. What it DOES report is declared by the builder
## from the zone's own BOX, which is geometry the panel states rather than anything the snapshot says.
const BAND_ZONE_SCROLL_NAME := "BandZoneScroll"

## Sized so a TYPICAL label — `Forage (nn, nn)`, `Hunt Woolly Mammoth` — fits whole beside the row's
## fixed furniture. At 300 a 1920 bottom dock took 4 columns and cut the labels mid-coordinate
## (`Forage (73, 20`), which costs the row the one thing it is for: naming WHICH source. Three
## readable columns beat four unreadable ones — the page loses ~7 rows, the row keeps its identity.
const WORK_COLUMN_MIN_WIDTH := 380.0

const WORK_MAX_COLUMNS := 4

const WORK_CHIPS_HEIGHT := 26.0

const WORK_PAGER_HEIGHT := 24.0

const WORK_INSPECTOR_HEIGHT := 118.0

## The inspector with its policy picker open (an extra rung row + its hint).
const WORK_INSPECTOR_POLICY_HEIGHT := 186.0

## There is no taller variant of that any more: `WORK_INSPECTOR_STANDING_LINE_HEIGHT` reserved room
## for a WARN line naming a rung the picker could not show, and issue #442 removed the state — a
## `policy` is always one of the four, so every open picker draws the same height.

## Gaps the work column always spends: head→chips, chips→board, board→(inspector | nothing).
const WORK_ZONE_GAP_COUNT := 3.0

const WORK_COLUMN_RULE_WIDTH := 1.0

const WORK_COLUMN_SEPARATION := 10

const WORK_ROW_STRIPE_WIDTH := 2.0

## The row is a fixed budget: everything but the label is fixed-width, so the label gets whatever a
## `WORK_COLUMN_MIN_WIDTH` column has left. These are trimmed to the smallest legible size so the
## label's share stays as wide as possible; past it the label ellipsises and the inspector strip
## spells the row out in full.
const WORK_ROW_SEPARATION := 4

const WORK_ROW_ICON_WIDTH := 16.0

const WORK_ROW_RATE_WIDTH := 46.0

const WORK_ROW_MARKS_WIDTH := 20.0

## The SOURCE-RUNG slot, immediately left of the policy/⚠ marks. Sized like `WORK_ROW_ICON_WIDTH` —
## it holds the same one-emoji family (🌾 / 🐄) at the same `WORK_ROW_FONT_SIZE`, and a Label's
## `custom_minimum_size` is a FLOOR, so a wider glyph still renders whole. It is reserved on EVERY
## row, wild ones included: the marks and rate columns are right-anchored behind the expanding label,
## so a slot that appeared only on tended rows would shift the rate column row-to-row and the board
## would read ragged. Costs the label ~20px (slot + `WORK_ROW_SEPARATION`) out of the
## `WORK_COLUMN_MIN_WIDTH` budget.
const WORK_ROW_RUNG_WIDTH := 16

## The ready slot is WIDER than the rung slot: it carries two glyphs (`⌃` + the verb) where the rung
## slot carries one. Reserved even when empty, so the right-anchored furniture lines up down the board.
const WORK_ROW_READY_WIDTH := 26

## The stable handle on a rung mark, mirroring `HudWidgets.POLICY_RUNG_META`'s job for a picker rung.
## `band_panel_preview` identifies the mark by THIS, never by its glyph: `FoodIcons.SITE_ICONS`
## already spends 🌾 on `savanna_grassland`, so a text match finds the row's SOURCE icon too and the
## harness would assert against the wrong Label.
const WORK_ROW_RUNG_META := &"work_row_rung"

## A board row must be EXACTLY `WORK_ROW_HEIGHT` — the capacity maths divides by it, so a row that
## renders taller silently overflows the page off the bottom of the zone. The default button chrome
## (`HudStyle._button_stylebox`, 9px of vertical padding) makes a stepper ~42px tall on its own, so a
## work row's stepper takes a COMPACT treatment: these are the paddings and type sizes that fit.
const WORK_ROW_FONT_SIZE := 13

const WORK_STEPPER_FONT_SIZE := 12

const WORK_STEPPER_PADDING_V := 2

## The same squeeze for the zone chrome, each sized to its own reserved height.
const ZONE_MENU_PADDING_V := 2

const WORK_CHIP_PADDING_V := 3

const WORK_PAGER_PADDING_V := 2

const INSPECTOR_CLOSE_PADDING_V := 2

const WORK_CHIP_SEPARATION := 4

const WORK_CHIP_FONT_SIZE := 11

## Board filters + sorts. The chips ARE the summary and the filter (they replace group headers).
const WORK_FILTER_ALL := &"all"

const WORK_FILTER_FORAGE := &"forage"

const WORK_FILTER_HUNT := &"hunt"

const WORK_FILTER_ATTENTION := &"attention"

## The rung-ready filter (issue #412) — its own chip and its own count, NOT folded into `attention`.
## Attention means TROUBLE (overdrawing, wasted workers, an unacknowledged edit); a rung on offer is an
## OPPORTUNITY. One control that means both is a control that finds neither.
const WORK_FILTER_READY := &"ready"

const WORK_SORT_YIELD := &"yield"

const WORK_SORT_NAME := &"name"

## Every legal work sort. The persisted preference is validated against this, so an unknown or retired
## value in the prefs file falls back to the default. The failure this prevents is NOT a broken board:
## `_sort_work_models` branches on `== WORK_SORT_NAME` and treats everything else as yield, so an
## unvalidated value would silently reinstate the yield sort — the behaviour issue #460 removed.
const WORK_SORTS: Array[StringName] = [WORK_SORT_NAME, WORK_SORT_YIELD]

const WORK_CHIP_ALL_FORMAT := "All %d"

const WORK_CHIP_KIND_FORMAT := "%s %d · %s"

const WORK_CHIP_ATTENTION_FORMAT := "⚠ %d"

## The ready chip. `⌃` is the same chevron the map badge and the overflow chip use, so the three
## surfaces share one mark for one idea and none of them needs a legend.
const WORK_CHIP_READY_FORMAT := "⌃ %d ready"

## A row's ready mark: the chevron plus the offered rung's own policy glyph (`⌃▦`, `⌃🐄`). The chevron
## is load-bearing — the verb and standing-rung glyphs COLLIDE (▦ is both "Sow" and "this is a Field"),
## so the glyph alone would read as *done* rather than *available*.
const WORK_ROW_READY_FORMAT := "⌃%s"

## Spelled out in the row tooltip, where there is room for words.
const WORK_ROW_READY_TOOLTIP_FORMAT := "Ready to %s — open this row to start."

## A rung UNDER WAY: the verb glyph and how far in. No chevron — `⌃` offers, this reports.
const WORK_ROW_BUILDING_FORMAT := "%s%d%%"

const WORK_ROW_BUILDING_TOOLTIP_FORMAT := "%s in progress — %d%% done."

const WORK_CHIP_TOOLTIP := "Filter the board to these sources."

const WORK_SOURCES_FORMAT := "%d sources"

const WORK_TOTAL_TOOLTIP := "Total food per turn from every worked source."

# **`WORK_TRADE_TOTAL_TOOLTIP` IS RETIRED** (arc #527) with the head's trade total.

# The FODDER total's tooltip (issue #449), a SIBLING of the food total rather than part of it: fodder credits the band's fodder store to feed its penned animals and never the
# larder, so folding it into the food figure would break the identity the Food line is denominated in.
# Shown only when a worked source actually pays fodder, so a band growing no feed reads exactly as it
# did before the third account.
const WORK_FODDER_TOTAL_TOOLTIP := "Total fodder per turn from every worked source. Fodder feeds penned animals — it is not food for people, so it is counted beside the food total, not in it."

# The band's PRODUCTIVITY, as a head item beside the two totals. It is the multiplier every rate on
# this board is ALREADY scaled by, which is why it reads here rather than as a band-zone vitals row:
# the head is where its consequence is. Rendered ONLY below full output (`SourceForecast.OUTPUT_FULL`)
# — a permanent "Output 100%" is noise on a row that is otherwise live summary, the same rule the
# fodder total follows.
const WORK_OUTPUT_FORMAT := "Output %d%%"

const WORK_OUTPUT_TOOLTIP := "Discontent is holding this band below full productivity, and every rate on this board is already scaled by it. Raise morale to restore full output."

const WORK_MENU_TOOLTIP := "Sort and bulk actions for worked sources."

const WORK_MENU_SORT_YIELD := "Sort by yield"

const WORK_MENU_SORT_NAME := "Sort by name"

const WORK_MENU_UNASSIGN_FORMAT := "Unassign all work (%d)"

const WORK_UNASSIGN_CONFIRM_FORMAT := "Return all %d sources' workers to idle? Standing roles and parties are untouched."

const WORK_UNASSIGN_CONFIRM_OK := "Unassign all"

const WORK_ROW_FORAGE_FORMAT := "Forage (%d, %d)"

# The MANAGED plant row's twin. A Tended Patch or a Field is never gather-drawn, so its crew tends it;
# the board says so in the same two nouns the compose sheet uses. Keyed by the crew label
# `HudFormat.plant_crew_label` resolves, so the board row and the sheet it opens cannot disagree about
# what the people on that tile are doing. DISPLAY ONLY — the row's `kind` is still `forage`.
const WORK_ROW_TEND_FORMAT := "Tend (%d, %d)"

const WORK_ROW_PLANT_FORMATS := {
    HudComposeVocab.FORAGE_CREW_LABEL: WORK_ROW_FORAGE_FORMAT,
    HudComposeVocab.TEND_CREW_LABEL: WORK_ROW_TEND_FORMAT,
}

const WORK_ROW_HUNT_FORMAT := "Hunt %s"

const WORK_ROW_OPEN_HINT := "Click the row for detail and actions."

## THE SOURCE-RUNG MARK — what the source IS, beside the policy glyph's what-the-band-is-DOING.
## The two are ORTHOGONAL and the row carries both: a Tended Patch being Sustained and a Tended Patch
## being Depleted are different situations, and one mark cannot say which. The row's policy glyph
## tracks the verb IN FLIGHT — a patch under construction wears 🌱 Cultivate and, the turn the build
## lands, reverts to ♻ Sustain — so without this mark ~25 turns of investment become invisible on the
## board where labor is actually managed.
##
## WILD IS THE ABSENCE OF A MARK: rung 1 is the default every source starts on, and glyphing it would
## put a mark on every row in the game to say "nothing has happened here yet".
##
## The glyphs are each rung's EXISTING mark, reused (`DetailFormat.CULTIVATION_GLYPH` /
## `field_glyph` / `pastoral_glyph` / `CORRAL_GLYPH`) — see the block above them for why the pastoral
## rung has to borrow the `tame` verb's ◎.
const WORK_ROW_RUNG_TENDED_TOOLTIP := "Tended Patch — this ground has been cultivated."

## …and the committed crop when the patch carries one (`committed_display_name`, e.g. "Wild Emmer").
const WORK_ROW_RUNG_TENDED_CROP_FORMAT := "Tended Patch — %s. This ground has been cultivated."

const WORK_ROW_RUNG_FIELD_TOOLTIP := "Field — this ground has been sown, the top plant rung."

const WORK_ROW_RUNG_FIELD_CROP_FORMAT := "Field — %s sown, the top plant rung."

const WORK_ROW_RUNG_PASTORAL_TOOLTIP := "Pastoral herd — tamed, and it keeps to your camp."

const WORK_ROW_RUNG_PENNED_TOOLTIP := "Penned herd — corralled, the top animal rung. It eats from your larder every turn."

## The under-contained managed-herd note (fauna neglect-escape arc): fewer keepers staffed than the
## herd needs, so it sheds whole animals into a nearby wild herd. Drives the row's amber stripe + the
## inspector's WARN line, and rides the same `note` slot as the overstaff note — which it WINS. The
## two could not co-occur while containment came off the hunting crew (a herd cannot be short of
## hunters and overstaffed with them at once); with the crews split they can, and an animal walking
## off outranks a hunter bringing nothing home.
##
## **IT NAMES THE BAND'S HUSBANDRY ROLE, BECAUSE THIS ROW'S `+` IS NOT THE REMEDY**
## (`docs/plan_standing_upkeep.md` §2.5). Containment used to be read off the HUNTING crew, so the
## board's own stepper answered the warning; then it was a per-source KEEPERS stepper on the herd's
## compose sheet. **Maintenance has since left the tile**: a managed herd is held from the band's
## `husbandry` POOL, and what this row is reporting is that the herd's SHARE of that pool did not
## cover it. So the note names the one control that can move the number — the Husbandry role card in
## this panel's WORKFORCE zone — and pointing at a per-source keeper stepper would now send the
## player looking for a control that no longer exists.
const WORK_ROW_UNDER_HERDED_NOTE := "Animals drifting off — raise this band's Husbandry role."

## …and the row tooltip carries the part the one-line note has no room for: WHY the `+` on this row
## does not answer the ⚠, and where the hands that do come from. It is a tooltip rather than a second
## strip line because `_work_inspector_height` reserves ONE open height for every row.
const WORK_ROW_UNDER_HERDED_TOOLTIP := "Under-herded — a managed herd is held out of the band's HUSBANDRY pool, not by its hunters, so this row's + will not stop the drift. Raise Husbandry in the WORKFORCE zone, or set the band's keeping split so this herd is funded first."

## **THE OTHER WAY A SOURCE BLEEDS: a half-built rung nobody is building** (`SourceForecast.
## is_unbuilt_and_unpaid`). An at-risk meter is owed the crew that OWNS it, and a rung still going up
## owes its BUILDERS — so a Tame whose crew was re-tasked slides back and, on the animal web, sheds
## animals, while every keeper-shaped reading on the row says nothing is wanted. It is the same
## silent-loss class as the shed and it wears the same ⚠.
##
## It shares the `note` slot with the under-herded note and CANNOT collide with it: that one needs a
## positive keeper demand and this one needs a zero. The two nouns are the whole point of having both
## — telling a player to staff KEEPERS on a rung that wants BUILDERS is the mistake this pair exists
## to stop making.
const WORK_ROW_UNBUILT_NOTE := "Nobody is building this — staff its BUILDERS."

const WORK_ROW_UNBUILT_TOOLTIP := "This rung is part-built and nobody is paying for it, so it slides back toward wild — and a half-tamed herd sheds animals while it does. A rung still going up is owed its BUILD crew, not keepers: open the source (Jump to source) and put BUILDERS on the improvement. The source's own card states what the neglect costs and how many turns are left."

const WORK_EMPTY_HINT := ALLOC_NO_SOURCES_HINT

## The inspector strip (the row's second/third lines, relocated to one place).
const INSPECTOR_CLOSE_GLYPH := "✕"

const INSPECTOR_CLOSE_TOOLTIP := "Close detail"

const WORK_INSPECT_JUMP := "Jump to source"

const WORK_INSPECT_POLICY := "Change policy"

const WORK_INSPECT_UNASSIGN := "Unassign"

const WORK_INSPECT_OVERDRAW_LINE := "⚠ Overdraws the source at this policy."

const WORK_INSPECT_ASSIGNED_FORMAT := "%d assigned"

## …and the source's OTHER crew beside it (`docs/plan_standing_upkeep.md` §2.2), rendered only where
## there are builders on it. A row admitted to the board on its builders alone would otherwise read
## `0 assigned` with three hands standing on its meter.
const WORK_INSPECT_BUILDERS_FORMAT := "%d building"

const WORK_INSPECT_SENTENCE_SEPARATOR := " · "

## **THE STANDING-INVESTMENT LINE AND ITS DISCARD CONFIRM ARE GONE** (issue #442). Three consts
## lived here — the WARN sentence, the confirm prompt and its OK label — and all three existed to
## handle a work row standing on a rung the picker did not offer, which is a state a `policy` that
## is always a stance cannot reach. A stance re-pick no longer discards anything: `assign_labor`
## leaves the improvement axis alone, so the pick is an ordinary change of take on every row.

const PAGER_PREV_GLYPH := "‹"

const PAGER_NEXT_GLYPH := "›"

const PAGER_PREV_TOOLTIP := "Previous page"

const PAGER_NEXT_TOOLTIP := "Next page"

const PAGER_FORMAT := "Page %d / %d"

const PAGER_RANGE_FORMAT := "%d–%d of %d"

