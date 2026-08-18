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

## The POOLS block's head (`docs/plan_standing_upkeep.md` §4.7) — the three band-level pools that
## staff what this tab is about, moved here from the Band tab because the pool was on one tab and its
## consequences (the sources it pays for, the queue it funds) on another.
##
## **ITS READOUT COUNTS ALL THREE ROLES, which is a different question from the retired
## `%d on keeping`.** That head deliberately excluded the builders — a build is a job rather than a
## standing charge — and it could, because the block held the keeping pair alone. This block holds all
## three cards, so a head naming two of them would be a key that does not add up to what is under it.
const ZONE_HEADER_POOLS := "Pools"

const POOLS_ZONE_READOUT_FORMAT := "%d of %d on work"

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

## **THE BUILDING ROLE** (`docs/plan_standing_upkeep.md` §2.5) — the third band-level pool, and the
## card that replaced the per-source BUILDERS stepper the compose sheet used to carry.
const ROLE_NAME_BUILDERS := "Builders"

## **IT NAMES THE QUEUE, BECAUSE THE QUEUE IS WHERE THE HANDS GO.** Unlike the keeping pair this pool
## is NOT split across everything the band holds: the whole of it goes on the HEAD of the band's build
## queue until that entry's meter fills, then on the next. A hint that said "keeps every build" would
## promise a spread the model deliberately does not offer — and *"builders with nothing to do"* needs
## no warning, a build demand ending when its meter fills.
const BUILDERS_ROLE_HINT := "Raises whatever this band has queued, one job at a time, head of the queue first."

## **THE FUND-MODE CONTROL** — how this band splits a keeping pool it cannot stretch, `spread` or
## `priority` (`upkeep_mode <faction> <band> …`). It renders under the three pool cards and ONLY
## where the band holds something on either web: the choice is meaningless with nothing to fund, and
## a control offered there would read as a setting the player had failed to make.
##
## **IT HAD A `Short of keepers` SECTION TITLE AND IT IS RETIRED** (§4.7). The head said nothing the
## two buttons under it and the arithmetic line under those did not already say — a label over a
## control whose whole content is two words — and it cost a line on a zone that clips. The buttons and
## the line share ONE row now; `UPKEEP_MODE_TITLE` went with the head.
const UPKEEP_MODE_SPREAD_LABEL := "Spread"

const UPKEEP_MODE_PRIORITY_LABEL := "Priority"

## The two modes stated as what they DO to the band's own sources, since that is the choice. Ride
## each button's tooltip, so the pair of one-word faces stays narrow enough for the dock's flanks.
const UPKEEP_MODE_SPREAD_HINT := "Fund every source in proportion — everything degrades a little."

const UPKEEP_MODE_PRIORITY_HINT := "Fund the biggest investments in full and let the marginal ones rot."

## RETIRED — **`UPKEEP_MODE_SHORT_FORMAT` AND `UPKEEP_MODE_COVERED_TEXT`, the line under the pair.**
## It stated `Short 2 work of 4 this turn.` — **a SUM across both webs**, since both of its terms were
## the plant pool's plus the animal pool's, so it could not name the web that was short even in
## principle: a band whose Agriculture is fine and whose Husbandry is starving read one number
## belonging to neither card. The shortfall belongs to a POOL, so it is on that pool's own card now
## (`UPKEEP_POOL_SHORT_MARK` + `UPKEEP_POOL_SHORT_*_FORMAT`).
##
## The covered sentence went for a second reason of its own: it announced that nothing is wrong, using
## a noun — *keeper* — that appears on no control in this game. The band staffs `Agriculture` and
## `Husbandry`; a sentence naming neither cannot be acted on, and *"everything is fine"* is what an
## unmarked card already says.

## **THE POOL CARD'S MARK — a bare `⚠`, no figure** (the tile card's own rule one surface over). A card
## is a role name and a stepper; a number wedged into it would be the retired line again, in less room.
const UPKEEP_POOL_SHORT_MARK := HudSelectionVocab.RUNG_HAZARD_GLYPH

## …and the figure it stands for, on the card's own `tooltip_text` — **ONE SENTENCE, whose numbers are
## what the pool SUPPLIES against what it is ASKED FOR.**
##
## **THERE WERE TWO SENTENCES AND THEY WERE ONE STATEMENT ALL ALONG.** A live shortfall said *"Short 2
## of 2"* and the declare-time warning said *"a queued job will need 3 work a turn, and nobody is on
## this pool"* — different words, different subjects, the same glyph in the same slot — which read as
## one warning misbehaving rather than as two facts. Both are now this shape at different numbers.
##
## **Per web, because the whole defect of the retired summed line was that it could not say which** —
## the plant pool keeps ground and the animal pool keeps animals, and those are different decisions
## with different remedies. Both name the band's own holdings rather than "sources", which is the noun
## the player sees on the map, and both name the QUEUE beside them because a job not yet started is
## exactly what the demand figure includes before the sim bills anything for it.
##
## **IN WORK, NEVER IN HANDS** — `DetailFormat.build_price_clause`'s rule, for its reason: the model is
## denominated in work units end to end, and how many hands a rate takes depends on what they carry.
## **And it never says *rung***, a word that appears on no control the player uses.
const UPKEEP_POOL_COVERAGE_PLANT_FORMAT := "This pool supplies %s work a turn; this band's tended ground and queued jobs need %s."

const UPKEEP_POOL_COVERAGE_ANIMAL_FORMAT := "This pool supplies %s work a turn; this band's tamed animals and queued jobs need %s."

## Which of the pair a card takes, off the role it staffs — one picker, for `under_kept_note`'s reason:
## a card that reached for the wrong web's sentence would be a wrong answer that looks like a right one.
static func upkeep_pool_coverage_format(role_name: String) -> String:
    return UPKEEP_POOL_COVERAGE_ANIMAL_FORMAT if role_name == ROLE_NAME_HUSBANDRY \
        else UPKEEP_POOL_COVERAGE_PLANT_FORMAT

## **THE POOL CARD'S ONE LINE, or `""` where the pool covers what it is asked for** — the ONE composer,
## so the card's mark and its hover cannot disagree about whether there is anything to say.
##
## **THE TRIGGER IS COVERAGE, AND IT WAS *UNSTAFFED* FOR ONE SLICE.** Reported from play: a Cultivate
## queued at a demand of 2.0 work a turn marked the card, ONE keeper cleared the mark, and the next
## turn brought it back — because *adequate* had been spelled *has at least one body on it*. A keeper
## supplies 1.0 bare (1.5 with the derived tillage kit), so one was never enough and the mark should
## never have cleared. **Adequate means the pool COVERS the demand**: adding one hand to a 2.0 bill
## does not clear it, adding enough does, and that is what makes the mark mean something.
static func upkeep_pool_coverage_line(role_name: String, cover: Dictionary) -> String:
    var asked := maxf(float(cover.get(POOL_COVERAGE_ASKED_KEY, SourceForecast.NO_UPKEEP_DEMAND)),
        SourceForecast.NO_UPKEEP_DEMAND)
    var supply := maxf(float(cover.get(POOL_COVERAGE_SUPPLY_KEY, SourceForecast.NO_UPKEEP_DEMAND)),
        SourceForecast.NO_UPKEEP_DEMAND)
    # Nothing to hold, or held with room to spare. The second test is the coverage one and uses the
    # same floor every work rate on this panel is stated at, so a pool short by less than the readout
    # can print is not marked for a difference nobody could see.
    if asked < SourceForecast.UPKEEP_WORK_MIN \
            or asked - supply < SourceForecast.UPKEEP_WORK_MIN:
        return ""
    return upkeep_pool_coverage_format(role_name) % [
        DetailFormat.format_work_units(supply), DetailFormat.format_work_units(asked)]

## The two keys of the coverage dict the pool card composes and this line reads. **Named**, because
## producer and reader are different scripts and a typo in a `get` there is a silent zero — which on
## the supply side would mark every card in the game and on the demand side would mark none.
const POOL_COVERAGE_SUPPLY_KEY := "supply"
const POOL_COVERAGE_ASKED_KEY := "asked"

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
##
## **THIS IS ONE LINE, AND A SOURCE ROW ON THE BOARD IS TWO OF THEM** — see
## `WORK_ROW_TWO_LINE_HEIGHT` below, which the capacity arithmetic divides by. What still draws at
## exactly this height is the BUILD QUEUE's row, which is one line and stays one: the two lists read
## at one line-height rather than at one row-height, and the queue has no accounts to state.
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

## **THE INSPECTOR STRIP'S FIXED SKELETON — a BASE, never the whole height.** It covers the head row,
## the one-line sentence every model states, the inline-link row, the two gaps between them and the
## card's own padding. Four more children draw CONDITIONALLY on what the MODEL says, each with its own
## `ZONE_BLOCK_SEPARATION` gap, and `BandPanelController._work_inspector_height` adds them per model.
##
## **IT WAS THE WHOLE ANSWER FOR A WHILE AND THAT WAS THE DEFECT.** The reservation forked on ONE
## panel-state bool (is the picker open) and never read the model at all, so the overdraw line, the
## slipping `note`, the `muted_note` and the `ArrivalStrip` each drew with nothing reserved for them —
## and the zone `clip_contents`, so the difference came off the bottom of the strip silently.
##
## **IT WAS 118, AND THE 40px OF SLACK IN THAT NUMBER IS WHAT MADE THE FORK SURVIVE SO LONG.** A flat
## constant covering four conditional children has to carry room for them, so the model-blind form was
## adequate by **2px** at the worst case a model can state (186 reserved against 184 drawn) — and every
## pixel of that cushion was charged to the ZONE, on every dock, whether or not a row was even
## selected. With the conditional terms stated on their own the base is measured as a base:
## `band_panel_inspector` draws **58px** of head + links + gaps + card padding.
##
## **IT WAS 78 UNTIL THE BOARD ROW GREW A SECOND LINE.** The base carried a one-sentence readout
## (*accounts · 50% left standing · ● Working · 3 assigned*) whose every clause the ROW now states —
## the accounts and the floor on line two, the crew on the stepper, the pending state on the stripe —
## so the whole `Label` and its `ZONE_BLOCK_SEPARATION` went, which is the 20px that pays for the
## taller row. **Re-measure this after touching the strip's base children**; the conditional terms
## below are stated separately for exactly that reason and must not be folded back in.
const WORK_INSPECTOR_EXTENT := 58.0

## The slack that base carries over its measurement. **A measurement tolerance, not padding** — the
## same statement `BandCityPanel.BAND_ZONE_TWO_COLUMN_SLACK` makes about the flank's extent, and the
## same unit: one `ZONE_BLOCK_SEPARATION`, the smallest quantity of vertical air this zone already
## spends, rather than a fresh number of its own.
const WORK_INSPECTOR_SLACK := float(ZONE_BLOCK_SEPARATION)

const WORK_INSPECTOR_HEIGHT := WORK_INSPECTOR_EXTENT + WORK_INSPECTOR_SLACK

## What ONE conditional `HudWidgets.build_status_part` line costs the strip — the overdraw line, the
## slipping `note`, the `muted_note`. The label is a bare `Label` at `ALLOC_SECTION_FONT_SIZE` with no
## autowrap, so it is exactly ONE line whatever it says; **14px is measured at that size**, not
## guessed, and the gap is the one the column puts above every block.
const WORK_INSPECTOR_NOTE_LINE_HEIGHT := 14.0

const WORK_INSPECTOR_NOTE_HEIGHT := WORK_INSPECTOR_NOTE_LINE_HEIGHT + float(ZONE_BLOCK_SEPARATION)

## **WHAT ONE BOARD ROW COSTS, and it is TWO LINES.** A source row holds a name, a variable-length
## ACCOUNT list, four affordances and a stepper, and 356px of narrow-shell zone does not hold them on
## one line — the accounts either elide to a fragment or take the name's pixels, and the name is the
## one thing a row may never yield. So the accounts get a line of their own, full width, and this is
## what the pair reserves AND draws at: line one at `WORK_ROW_HEIGHT`, the two-line stepper's own gap,
## and the accounts line.
##
## **THE SECOND LINE IS `HudWidgets.build_two_line_stepper`'S, TERM FOR TERM** — its
## `TWO_LINE_STEPPER_SEPARATION` gap under a part at `ALLOC_SECTION_FONT_SIZE`, whose measured line
## height is the one `WORK_INSPECTOR_NOTE_LINE_HEIGHT` already states. The board's own 13px type has
## no measured line height anywhere in this file, and inventing one is how a row comes to draw taller
## than the capacity arithmetic paid for.
##
## **IT IS DECLARED HERE RATHER THAN BESIDE `WORK_ROW_HEIGHT` BECAUSE A GDScript `const` MAY NOT READ
## ONE DECLARED BELOW IT**, and the note line's height is declared on the line above this.
const WORK_ROW_TWO_LINE_HEIGHT := WORK_ROW_HEIGHT + float(TWO_LINE_STEPPER_SEPARATION) \
    + WORK_INSPECTOR_NOTE_LINE_HEIGHT

## …and what the `ArrivalStrip` costs when the model's schedule has a gap worth drawing.
##
## **A TWIN of `ArrivalStrip.STRIP_HEIGHT`, deliberately not a read of it.** A `const` initializer
## evaluates at class load, so a vocab leaf referencing a `class_name`d Control script is a load cycle
## waiting to happen (`hud-modules.md` → `const` direction). The two cross-reference each other in
## prose instead; change one and change the other.
const WORK_INSPECTOR_ARRIVALS_STRIP_HEIGHT := 8.0

const WORK_INSPECTOR_ARRIVALS_HEIGHT := WORK_INSPECTOR_ARRIVALS_STRIP_HEIGHT \
    + float(ZONE_BLOCK_SEPARATION)

## …and the policy picker, when the row's `Set floor` link has it open — the four rungs plus the gap
## above them.
##
## **STATED AS A DELTA, where it used to be a whole alternative strip height** (`186`, i.e. a second
## total). A second total cannot compose with the conditional lines above it: the fork that chose
## between the two totals is exactly what made the strip's reservation model-blind.
##
## **MEASURED like the base, and it is HALF what the two totals implied** — `band_panel_work_policy_
## investment` draws 110px against the base's 78, i.e. the three preset cells plus their gap. The 68 the
## old pair implied was that 32 plus the base's own cushion, counted twice.
##
## There is no taller variant of the picker any more: `WORK_INSPECTOR_STANDING_LINE_HEIGHT` reserved
## room for a WARN line naming a rung the picker could not show, and issue #442 removed the state — a
## `policy` is always one of the four, so every open picker draws the same height.
const WORK_INSPECTOR_POLICY_PICKER_HEIGHT := 32.0

## **THE CEILING THESE TERMS ADD UP TO, stated because it is UNMEASURED rather than because it is
## reserved.** A model carrying every conditional child at once — the overdraw line, the `note`, the
## `muted_note`, the `ArrivalStrip` and an open policy picker — reserves **190px** (84 + 3×20 + 14 +
## 32) against the 104 the tallest row any fixture produces asks for. `BandCityPanel.PANEL_HEIGHT_WIDE`
## is sized against that 104, so a row reaching this ceiling would take the work zone 86px past its
## box on a horizontal dock.
##
## **NOTHING PADS FOR IT, DELIBERATELY.** No fixture produces the combination and it is not known to be
## reachable in play — `warn` and `note` are near-exclusive on the board's own rows, and the picker is
## panel state a player opens — so the zone is not made 86px taller for a state nobody has seen. A
## KNOWN unmeasured worst case is the cheaper thing to carry; if one is ever observed, this is the
## figure both of that constant's levers move by.
## `band_panel_preview._assert_work_inspector_worst_case_fits` builds it and pins the strip's own
## arithmetic, which is what keeps the number above honest even though no zone reserves it.
const WORK_INSPECTOR_CEILING_HEIGHT := WORK_INSPECTOR_HEIGHT \
    + 3.0 * WORK_INSPECTOR_NOTE_HEIGHT \
    + WORK_INSPECTOR_ARRIVALS_HEIGHT + WORK_INSPECTOR_POLICY_PICKER_HEIGHT

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

## **THE ACCOUNTS LINE IS INDENTED ONTO THE NAME'S OWN COLUMN**, so the board reads as a column of
## names with each row's products hanging under its own name rather than under the severity stripe.
## It is exactly what line one spends before the name begins — the icon slot and its separation — and
## it is DERIVED from those two rather than measured off a render, since the stripe lives outside both
## lines' container and costs neither.
const WORK_ROW_ACCOUNTS_INDENT := int(WORK_ROW_ICON_WIDTH) + WORK_ROW_SEPARATION

## The stable handle on a row's ACCOUNTS line, the `WORK_ROW_RUNG_META` treatment one control down: it
## is a `Label` in its own margin under line one, and a harness that found it by text would be
## asserting the string it had already composed. **A RETIRED `WORK_ROW_RATE_WIDTH` (46px) IS WHAT IT
## REPLACED** — the accounts used to be a fixed slot on line one, where a four-cash-crop patch's
## `+0.06 fibre · +0.07 grape · +0.06 tea · +0.07 tobacco` measured 583px of a 356px zone and the row's
## NAME, its only expanding child, was allocated Godot's 1px floor.
const WORK_ROW_ACCOUNTS_META := &"work_row_accounts"

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

## **THE `⌃` IS THE CONTROL THAT QUEUES THE JOB** (`docs/plan_standing_upkeep.md` §4.7a ①), so its
## hover says the rung's word AND what the click does. Spelled out here, where there is room for
## words; the mark itself is two glyphs.
##
## **IT REPLACED `WORK_ROW_READY_TOOLTIP_FORMAT`, whose remedy was `open this row to start`** — that
## pointed at the inspector strip, which never carried a way to start anything (the declaration was
## the TILE sheet's checkbox, three surfaces away). The sentence outlived its mechanism twice over
## and goes with the mark's promotion to a button.
##
## **ONE CONST FOR THE BUTTON AND FOR THE ROW LINE.** The row's own tooltip states the same offer
## when the pointer is anywhere else on the row, and two spellings of one sentence is how the two
## come to describe different controls.
##
## **AND THE RUNG'S PRICE RIDES BENEATH IT** — `50 work · 2 work a turn from Agriculture to hold`,
## composed by `DetailFormat.build_price_clause`, on its own line under this sentence. Ray took the
## price off the compose sheet (*"That information should be on the work tab. No need to have it here,
## it is useless."*); this is the Work tab, and a cost is actionable on the surface that queues, funds
## and orders jobs. **It costs the height-capped zone nothing**, being a hover on a control that is
## already there. The TURN COUNT is deliberately not in it: the BUILD QUEUE row's date is the sim's
## own chained answer and the one a reorder is judged against, so quoting a second estimate here would
## be two producers for one number.
## **RETIRED — it promised a one-click queue, and the press opens a TRACK now**
## (`docs/plan_standing_upkeep.md` §2.8). A queue entry names a DESTINATION and climbs every rung on
## the way, so *"click ⌃ to queue it"* named the next rung as though it were the whole decision.
## `WORK_ROW_READY_TRACK_TOOLTIP` says what the press does; everything above about the PRICE riding
## beneath it, and about the turn count deliberately not being in it, is unchanged and still true.

## A rung UNDER WAY: the verb glyph and how far in. No chevron — `⌃` offers, this reports.
const WORK_ROW_BUILDING_FORMAT := "%s%d%%"

const WORK_ROW_BUILDING_TOOLTIP_FORMAT := "%s in progress — %d%% done."

## …and the SAME rung under way while nobody is raising it, or while it is losing ground: the verb
## glyph and a `⚠`, with **the percentage dropped**. A percent on a build nobody is staffing implies
## progress that is not happening, and a percent that is falling is the same lie one state over — so
## the slot still says WHICH rung is promised here and stops saying it is being worked.
##
## **IT IS THE MAP BADGE'S FACE, RESTATED RATHER THAN SHARED.** `BandOverlayRenderer`'s
## `BADGE_UNSTAFFED_FORMAT` is the identical `%s⚠` plus the trailing space its plate needs for the
## crew count that follows it, which this row's own reserved slot does not. A HUD controller must not
## import a MAP renderer, so the two faces are deliberate twins — but the VERDICT behind them is
## genuinely one function: both fork on `SourceForecast.build_is_stalled`, neither re-derives it from
## a crew count or a percentage, and that is what stops the map showing an alert the WORK tab does not.
##
## **A PARKED BUILD IS NOT THIS.** `BUILD_PACE_HELD` — a `-2` with nobody on it and its keeping
## covered — keeps its number and its ordinary ink, because that number is honest and the state is a
## decision rather than a failure (`labor-ui.md` → "THE BUILD LINE'S STATE IS ITS COLOUR").
const WORK_ROW_BUILDING_UNSTAFFED_FORMAT := "%s⚠"

## …and its tooltip, which drops the percent for the reason the face does and names both remedies —
## the two ways a meter comes to a stop have two different fixes, and the row cannot tell which
## without stating a number it has just refused to state.
const WORK_ROW_BUILDING_UNSTAFFED_TOOLTIP_FORMAT := "%s in progress — it is NOT advancing. Staff the band's Builders role, or cover this source's keeping."

## **THE ROW'S RUNG-AXIS SLOT, VALUED THE FACE IT RENDERED.** Every claim about that slot is a STRING
## composed at render time, so a harness that found it by its text would only confirm the string it
## had already assumed — and the three build states differ by one glyph at a thumbnail's size. The
## meta is what lets an assertion find the Label first and read it second.
const WORK_ROW_BUILD_STATE_META := "work_row_build_state"

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
## **THE WORD `rung` IS OURS AND NOT THE PLAYER'S.** These two tooltips read *"the top plant rung"* /
## *"the top animal rung"*, which is exact and means nothing to anyone who has not read the ladder
## config: it appears on no control, in no menu and in no other sentence the game shows. So they say
## what the player can check instead — there is nothing further to do to this ground / this herd. The
## word stays in comments and identifiers, where it is the right term.
##
## The glyphs are each rung's EXISTING mark, reused (`DetailFormat.CULTIVATION_GLYPH` /
## `field_glyph` / `pastoral_glyph` / `CORRAL_GLYPH`) — see the block above them for why the pastoral
## rung has to borrow the `tame` verb's ◎.
const WORK_ROW_RUNG_TENDED_TOOLTIP := "Tended Patch — this ground has been cultivated."

## …and the committed crop when the patch carries one (`committed_display_name`, e.g. "Wild Emmer").
const WORK_ROW_RUNG_TENDED_CROP_FORMAT := "Tended Patch — %s. This ground has been cultivated."

const WORK_ROW_RUNG_FIELD_TOOLTIP := "Field — this ground has been sown, and there is no improving it further."

const WORK_ROW_RUNG_FIELD_CROP_FORMAT := "Field — %s sown, and there is no improving this ground further."

const WORK_ROW_RUNG_PASTORAL_TOOLTIP := "Pastoral herd — tamed, and it keeps to your camp."

const WORK_ROW_RUNG_PENNED_TOOLTIP := "Penned herd — corralled, and there is no taming it further. It eats from your larder every turn."

## The under-contained managed-herd note (fauna neglect-escape arc): fewer keepers staffed than the
## herd needs, so it sheds whole animals into a nearby wild herd. Drives the row's amber stripe + the
## inspector's WARN line, and rides the same `note` slot as the overstaff note — which it WINS. The
## two could not co-occur while containment came off the hunting crew (a herd cannot be short of
## hunters and overstaffed with them at once); with the crews split they can, and an animal walking
## off outranks a hunter bringing nothing home.
##
## **THIS SOURCE'S KEEPING CAME UP SHORT — one note, keyed by WEB** (`docs/plan_standing_upkeep.md`
## §4.6a). Maintenance left the tile: a managed source is held out of the band's `agriculture` or
## `husbandry` POOL, and what the row reports is that this source's SHARE of that pool did not cover
## it. So each note names the one control that can move the number — the pool card in the POOLS block
## directly above this board since §4.7 — and pointing at a per-source keeper stepper would send the
## player looking for a control that no longer exists.
##
## **IT WAS TWO NOTES AND THE SECOND ONE LIED.** `WORK_ROW_UNBUILT_NOTE` said *"Nobody is building
## this — staff its BUILDERS"* for a source whose meter was still going up, on the premise that an
## unbuilt rung was owed its build crew. **One pool owes both now**, at every fullness, so the two
## states were one state and the builder wording sent the player to the wrong lever. What the merge
## costs is that the note no longer distinguishes a rung being RAISED from one being HELD — it does
## not need to, because the player does the same thing either way, and the source's own card still
## says which on the rung row that carries the meter.
const WORK_ROW_UNDER_HERDED_NOTE := "Animals drifting off — raise this band's Husbandry role."

## The plant web's twin. The consequence is the ground going back to wild rather than a flock
## shedding, and the pool is `agriculture` — the same sentence about a different web, which is why the
## picker below is one function and not a branch at each call site.
const WORK_ROW_UNDER_KEPT_NOTE := "This ground is slipping — raise this band's Agriculture role."

## RETIRED — **`WORK_ROW_UNDER_KEPT_TOOLTIP` AND `WORK_ROW_UNDER_HERDED_TOOLTIP`**, a four-sentence
## hover each (*"Under-kept — an improved patch is held out of the band's AGRICULTURE pool, not by its
## gatherers, so this row's + will not stop the slide. …"*). They explained the MODEL — which pool
## owes the keeping, why the stepper beside them is the wrong lever, that a half-built rung is billed
## like a finished one — where the player's question is *what do I do and how long have I got*. The
## note under the row already answers the first; the second is the countdown below, and everything
## else was prose nobody had a reason to read twice.

## **THE HOVER, AND IT IS ONE PRODUCER WITH A FLAG** — the note's own sentence, plus the countdown
## **only where a caller supplies one**. Both surfaces that state an under-kept source call this:
##
## - the WORK BOARD passes the rung and its grace, so its row reads
##   *"This ground is slipping — raise this band's Agriculture role. / Tended is lost in 3 turns."*
## - the SOURCE'S CARD (the tile card's rung row, the herd drawer's) passes neither, so its row hover
##   is the first line alone.
##
## **THAT ASYMMETRY IS DELIBERATE.** The board is where staffing is decided this turn, so *how long you
## have* is actionable there; the card is where you look at the ground, and a figure you cannot act on
## from it is noise. **One producer, never two** — a second sentence-builder for the card is exactly how
## the two surfaces come to phrase one hazard differently, which is the failure `under_kept_note`'s own
## picker exists to prevent one line up.
##
## `rung_word` is the rung's badge word (`DetailFormat.rung_badge_word`) — `""` for a rung the badge
## table cannot name, which drops the countdown rather than counting down an unnamed thing.
const UNDER_KEPT_NO_COUNTDOWN := -1

## The countdown's three forms. `grace` is `neglectGraceRemaining` read through its own flag, so `0` on
## an at-risk source means the penalty is biting THIS turn — opposite news from `0` on a source that is
## not at risk, which never reaches here.
const UNDER_KEPT_LOST_FORMAT := "%s is lost in %d turns."

const UNDER_KEPT_LOST_ONE_TURN := "%s is lost next turn."

const UNDER_KEPT_LOST_NOW := "%s is being lost now."

## Which of the pair this row takes, off the row's own labor kind — one picker, so the note and the
## tooltip can never end up describing two different webs.
static func under_kept_note(kind: String) -> String:
    return WORK_ROW_UNDER_HERDED_NOTE if kind == SourceForecast.LABOR_KIND_HUNT \
        else WORK_ROW_UNDER_KEPT_NOTE

static func under_kept_tooltip(kind: String, rung_word: String = "",
        grace: int = UNDER_KEPT_NO_COUNTDOWN) -> String:
    var note := under_kept_note(kind)
    if rung_word == "" or grace == UNDER_KEPT_NO_COUNTDOWN:
        return note
    var countdown := UNDER_KEPT_LOST_NOW % rung_word
    if grace == 1:
        countdown = UNDER_KEPT_LOST_ONE_TURN % rung_word
    elif grace > 1:
        countdown = UNDER_KEPT_LOST_FORMAT % [rung_word, grace]
    return "%s\n%s" % [note, countdown]

## **THE SAME PAIR ASKED WITH A *SOURCE* KIND** (`SOURCE_KIND_HERD` / `SOURCE_KIND_FORAGE`), which is
## what the card-side producers hold. `SOURCE_KIND_HERD` is `"herd"` and `LABOR_KIND_HUNT` is
## `"hunt"`, so handing one straight to the labor-keyed pickers above silently answers with the PLANT
## web's sentence on an animal source — a wrong answer that looks like a right one. It delegates
## rather than re-spelling the pair, so the two webs' wording still has exactly one home.
static func under_kept_note_for_source(source_kind: String) -> String:
    return under_kept_note(_labor_kind_of(source_kind))

## …and the HOVER asked the same way, which is the form both source CARDS use. It supplies no
## countdown by construction: the card states no figure at all, and the one surface that does states
## it by passing the pair above.
static func under_kept_tooltip_for_source(source_kind: String) -> String:
    return under_kept_tooltip(_labor_kind_of(source_kind))

## **WHICH BAND ROLE PAYS THIS SOURCE'S KEEPING** — `Husbandry` on the animal web, `Agriculture` on
## the plant one. It is the role NAME the two notes above already name in prose, pulled out so the
## surfaces that need the bare word (the compose sheet's standing price, the blocked-queue remedy)
## read the same table rather than spelling the pair a third time.
static func keeping_role_name(source_kind: String) -> String:
    return ROLE_NAME_HUSBANDRY if source_kind == SourceForecast.SOURCE_KIND_HERD \
        else ROLE_NAME_AGRICULTURE

static func _labor_kind_of(source_kind: String) -> String:
    return SourceForecast.LABOR_KIND_HUNT if source_kind == SourceForecast.SOURCE_KIND_HERD \
        else SourceForecast.LABOR_KIND_FORAGE

const WORK_EMPTY_HINT := ALLOC_NO_SOURCES_HINT

## The inspector strip (the row's second/third lines, relocated to one place).
##
## **THE STRIP'S STABLE HANDLE, VALUED THE HEIGHT IT RESERVED.** The block has nothing findable by
## text — its head is the row's own label and its lines are composed per model — and the claim a
## harness has to make about it is arithmetic rather than words: does the reservation cover what the
## strip DREW? Carrying the reserved number on the meta is what lets that be asked without the harness
## re-deriving it and agreeing with the builder by construction, the `POOLS_BLOCK_META` idiom.
const WORK_INSPECTOR_META := "work_inspector"

const INSPECTOR_CLOSE_GLYPH := "✕"

const INSPECTOR_CLOSE_TOOLTIP := "Close detail"

const WORK_INSPECT_JUMP := "Jump to source"

const WORK_INSPECT_POLICY := "Change policy"

const WORK_INSPECT_UNASSIGN := "Unassign"

const WORK_INSPECT_OVERDRAW_LINE := "⚠ Overdraws the source at this policy."

const WORK_INSPECT_ASSIGNED_FORMAT := "%d assigned"

## **RETIRED — `WORK_INSPECT_BUILDERS_FORMAT`, the `N building` clause**
## (`docs/plan_standing_upkeep.md` §2.5). It named the SOURCE's own build crew, and a source has none:
## a verb declares and the hands stand on the band's `builders` pool, which funds the head of its
## queue. The count belongs to the queue, which is a list of its own.

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

# ---- The POOLS block (`docs/plan_standing_upkeep.md` §4.7) ----------------------------------------
#
# The band's three standing pools — Agriculture, Husbandry and Builders — at the top of the WORK zone,
# between `_build_work_head` and the BUILD QUEUE. They were the Band tab's KEEPING block until this
# slice; what moved them is that the pool was on one tab and everything it pays for on another.
#
# **THE BLOCK ALWAYS RENDERS, including for a band with an empty board.** Three steppers at 0 is a
# live control rather than furniture explaining an absence — which is the opposite of the queue block
# one down, where nothing queued genuinely means nothing to show.

## The POOL CARD is the role card with everything but the CONTROL taken off: the role name and the
## stepper, no kit picker (none of the three has one — the keeping pair mounts none by rule and the
## Builders card's was deleted in §4.6b) and no prose. Each card's hint becomes its `tooltip_text`:
## the words survive, they stop costing vertical space on a zone that clips.
##
## Measured on the drawn card — the stylebox's 6px top and bottom padding, the name at
## `ROLE_CARD_NAME_FONT_SIZE`, one `ROLE_CARD_SEPARATION` and the compact stepper's own button height.
const POOL_CARD_HEIGHT := 56.0

## …and the fund-mode row's own drawn height: ONE row, the two buttons and the shortfall/covered line
## side by side. Reserved only where that row renders — `_build_upkeep_mode_row` returns nothing on a
## band with nothing to keep.
##
## **IT WAS 67, AND THREE LINES** (§4.7) — a `Short of keepers` section head over the buttons over the
## note. The head said nothing the two words and the number beneath it did not, and this zone clips, so
## it went and the other two shared a row at 22.
##
## **THE NOTE HAS SINCE GONE TOO, AND THE NUMBER DID NOT MOVE — re-measured, not assumed.** The
## buttons were always the taller element of that row (`HudWidgets.compact` at `WORK_CHIP_FONT_SIZE` /
## `WORK_CHIP_PADDING_V`), so with the arithmetic line retired the row draws **exactly 22.0px** of the
## 22.0 reserved — `band_panel_preview._assert_upkeep_mode_row_fits` prints both figures beside the
## claim, which is what makes a re-derivation a measurement rather than a guess. `pools_block_height`
## is therefore unchanged: **110px with the fund-mode row, 82 without.**
const UPKEEP_MODE_ROW_HEIGHT := 22.0

## **THE BLOCK'S STABLE HANDLE, valued whether its fund-mode row is present** — the harnesses assert
## the reserved height against the drawn one and need the same `has_fund_mode` the builder used.
const POOLS_BLOCK_META := "pools_block"

## **THE HEIGHT THE BLOCK RESERVES *AND* DRAWS AT — one function, two callers**, exactly as
## `build_queue_block_height` is. The work zone `clip_contents`, so a block that drew without being
## paid for in `_work_board_capacity`'s chrome term would silently slice board rows off the bottom of
## the zone; reserving and drawing from one expression is what makes the two unable to disagree.
static func pools_block_height(has_fund_mode: bool) -> float:
    var height := ZONE_HEAD_HEIGHT + float(ZONE_BLOCK_SEPARATION) + POOL_CARD_HEIGHT
    if has_fund_mode:
        height += float(ZONE_BLOCK_SEPARATION) + UPKEEP_MODE_ROW_HEIGHT
    return height

# ---- The BUILD QUEUE block (`docs/plan_standing_upkeep.md` §4.6b) ---------------------------------
#
# The band's ordered build queue, above the filter chips in the WORK zone. **Above them deliberately:**
# the chips filter the BOARD, and the queue is the band's own list rather than a view of that board —
# a block below them would read as a filtered subset of it.
#
# **NO QUEUE MEANS NO BLOCK AT ALL** — zero nodes, zero height, zero chrome. That is the common
# early-game state and it must cost nothing, which is also why `build_queue_block_height` answers 0
# there rather than reserving a bare header.

## The block's own head, uppercased by `HudWidgets.alloc_section_label` like every other zone head.
const ZONE_HEADER_BUILD_QUEUE := "Build queue"

## **ENTRIES DRAWN BEFORE THE OVERFLOW ROW TAKES OVER — a CEILING, and the BOX is the other term**
## (`docs/plan_standing_upkeep.md` §4.7). Three is what a zone with room shows; what a zone without
## room shows is `build_queue_rows_max`'s answer, which is never above this.
##
## **IT STOPPED BEING A LONE CONSTANT WHEN THE POOLS BLOCK LANDED, AND THE NUMBERS SAY WHY.** The
## block is paid for out of the WORK zone's own clipped box, and that box is **300px on a horizontal
## dock** against **761–1013px in the narrow shell's swapped host**. Measured on the 1920 bottom dock
## with the pools block above it, a four-entry queue at this ceiling wants **342px of the 300px box**;
## lowering the ceiling to 2 leaves **314**, and only 1 fits — while the tall LEFT dock has ~450px
## spare at 3 and would be paying for a horizontal dock's shortage. One number cannot answer both, so
## the ceiling stays authored and the RESOLUTION reads the room.
const BUILD_QUEUE_ROWS_MAX := 3

## …and the floor. A block that drew NO entry row would be a head over an overflow line, which says
## less than the `+N more` row alone; "no queue means no block" is the empty-queue rule and is a
## different statement from "this zone is too short for the list".
const BUILD_QUEUE_ROWS_MIN := 1

## The gaps the queue's own room has to clear before it may claim a row: head→pools, pools→queue,
## queue→chips, chips→board, board→pager, and the inspector gap `_work_board_capacity` reserves
## unconditionally. Named rather than spelled, since it is the one term of the reservation that is a
## COUNT rather than a height.
const BUILD_QUEUE_ROOM_GAP_COUNT := 6.0

## **AND THE INSPECTOR'S OWN HEIGHT BESIDE THAT GAP.** The reservation below budgeted the STRIP'S GAP
## and not the strip, so the queue claimed rows the zone could only afford while nothing was selected
## — and selecting a row is one click, after which the board (floored at one row) has nothing left to
## give back. It is the BASE height rather than a worst case on purpose: the conditional lines and the
## policy picker are the board's to pay for, and a queue cap sized on the tallest strip a model could
## ever produce would shrink the block on every dock for a state most bands never reach.
const BUILD_QUEUE_ROOM_INSPECTOR_HEIGHT := WORK_INSPECTOR_HEIGHT

## **HOW MANY ENTRY ROWS THIS ZONE CAN AFFORD**, clamped into `[BUILD_QUEUE_ROWS_MIN,
## BUILD_QUEUE_ROWS_MAX]`. It reserves everything the zone owes whatever the queue does — its head,
## the chips, the POOLS block, one board row, the pager, the block's own head and the gaps between
## them — and divides what is left by the row height.
##
## **THE OVERFLOW ROW IS TAKEN OFF THE ANSWER, NOT ADDED TO THE COST.** `build_queue_block_height`
## draws a `+N more` row BESIDE the capped entries rather than in place of one, so a zone that affords
## two rows and is handed four entries shows ONE entry and the overflow — the drawn count is the same
## either way, and computing it here is what stops the reservation and the render disagreeing.
static func build_queue_rows_max(box_height: float, pools_fund_mode: bool, entries: int) -> int:
    # The board row this leaves room for is a SOURCE row, so it is the two-line height; the rows this
    # divides for are QUEUE rows, which are one line each.
    var reserved := ZONE_HEAD_HEIGHT + WORK_CHIPS_HEIGHT + pools_block_height(pools_fund_mode) \
        + ZONE_HEAD_HEIGHT + WORK_ROW_TWO_LINE_HEIGHT + WORK_PAGER_HEIGHT \
        + BUILD_QUEUE_ROOM_INSPECTOR_HEIGHT \
        + float(ZONE_BLOCK_SEPARATION) * BUILD_QUEUE_ROOM_GAP_COUNT
    var afforded := int((box_height - reserved) / WORK_ROW_HEIGHT)
    if entries > afforded:
        afforded -= 1
    return clampi(afforded, BUILD_QUEUE_ROWS_MIN, BUILD_QUEUE_ROWS_MAX)

## The HEAD marker — the one entry the whole builders pool is standing on. Its slot is reserved on
## EVERY row (`BUILD_QUEUE_MARKER_WIDTH`) so the job faces line up down the block; a conditionally
## omitted Label would shift every row behind the head.
const BUILD_QUEUE_HEAD_MARKER := "▸"

const BUILD_QUEUE_MARKER_WIDTH := 10.0

## The date column. Fixed and CLIPPING: the widest values this column takes — the `∞`-carrying
## sentinels and the `<verb> N% · turn N` completion form — would squeeze the job face to nothing on a
## left dock if they sized the row. The Label's `text` still carries the full value (clipping is visual
## only) and the row tooltip repeats it, so nothing is unreachable.
##
## **IT IS THE WIDEST VALUE THE COLUMN CAN BE HANDED, MEASURED, and that is why it went 118 → 168**
## (`docs/plan_standing_upkeep.md` §2.8). The completion form leads with the leg in flight's
## participle now, so the value is `Cultivating 100% · turn 999` at its longest — 168px at
## `WORK_ROW_FONT_SIZE`, printed by `band_panel_preview._report_queue_row_columns` rather than
## guessed. **Under-sizing it is not a cosmetic loss here**: the Label trims from the END whatever its
## alignment, so a column short of the value cuts the DATE off a row whose whole remaining job is to
## state one. That is also why the Corral's participle was shortened to `Penning` — see
## `HudComposeVocab.IMPROVEMENT_RUNNING_LABELS`.
##
## The 50px comes out of the job FACE, which is the row's only expanding child: at the tall dock it
## leaves ~106px, which holds a plant face (`▦ Sow (66, 25)` needs 89) and ellipsises a long animal one
## (`🐄 Corral Thunder Mammoths` needs 189 — it was already ellipsised at the old width). The hover
## carries face and date in full.
const BUILD_QUEUE_DATE_WIDTH := 168.0

## `3 builders · Tillage kit` — the head's readout, naming the pool that funds the queue and the kit
## it is holding. The kit comes from the SAME resolution the Builders role card's gear line states
## (`BandPanelController._role_kit_id`), so the card and this header cannot disagree about which web's
## tool the pool is carrying.
const BUILD_QUEUE_BUILDERS_FORMAT := "%d builders · %s"

## …and the same slot when nobody is on the role. **This branch is the direct answer to a playtest
## report** — a Cultivate that was not progressing, with nothing on any surface saying why — so it
## names the remedy rather than merely stating the zero, and it takes the WARN ink.
const BUILD_QUEUE_NO_BUILDERS_NOTE := "⚠ No builders — staff the Builders role"

## …and the head's tooltip. **It names where the pool is staffed, so it moved with the card** (§4.7):
## the Builders card is in the POOLS block directly above this head now, not in the band tab's
## WORKFORCE zone, and a remedy pointing at a control that is no longer there is worse than none.
const BUILD_QUEUE_BUILDERS_TOOLTIP := "The band's builders pool funds the HEAD of this queue until its meter fills, then the next. Staff it on the Builders card just above."

## A queued PLANT entry's face — the declared verb (glyph and word, `HudFormat.policy_face`) plus the
## tile it stands on. The verb's vocabulary is the board row's in-progress axis's own, so a rung under
## way reads the same word here and there.
const BUILD_QUEUE_PLANT_FACE_FORMAT := "%s (%d, %d)"

## …and its animal twin, naming the herd rather than a tile.
const BUILD_QUEUE_ANIMAL_FACE_FORMAT := "%s %s"

## The truncation row. **A truncated list with nothing under it reads as the whole list**, which is
## the faction page's standing rule for a capped list, applied to the band's own.
const BUILD_QUEUE_OVERFLOW_FORMAT := "+%d more"

const BUILD_QUEUE_OVERFLOW_TOOLTIP := "More entries are queued than this zone can show. Reorder from the command line with `build_order`."

## The withdrawal. Same `✕` and same steady DANGER ink the parties zone's recall control wears — a
## destructive control reads as one — and, like that one, it asks nothing first: `unqueue` withdraws a
## DECLARATION, the banked meter survives it, and re-declaring is one tick of the compose control.
const BUILD_QUEUE_UNQUEUE_GLYPH := "✕"

const BUILD_QUEUE_UNQUEUE_WIDTH := 22.0

const BUILD_QUEUE_UNQUEUE_TOOLTIP := "Withdraw this build. The work already banked is kept, and the source keeps its crew."

## **THE JOB'S SETTINGS ARE A ROW EXPANSION, NOT COLUMNS ON THE ROW**
## (`docs/plan_standing_upkeep.md` §4.7a ②, ③). Ray, from play: *"The CROP TO TEND shouldn't be a
## selection here as the user can't do the cultivate here."* — so the crop left the compose sheet for
## the entry it belongs to. It then shipped for a pass as a picker IN the row, and the tall LEFT dock
## (the SHIPPED DEFAULT edge) rendered `▸ 🌱 Cultiv…  [Sim pic ⌄]  turn 82 (0%)  ✕`: two of the five
## columns ellipsised into fragments, one of them a truncated crop name that reads as a word.
##
## **THE ROW HAD THREE COLUMNS WHEN CLIPPING-PLUS-TOOLTIP WAS DECIDED FOR IT, AND A LIST IS SCANNED.**
## A tooltip answers a question a player already has; it cannot repair a list they are reading down.
## So the row goes back to marker · face · date · `✕` (it kept a SOURCE mark between the two until the
## date column learned a verb), and the settings open BENEATH it — the
## WORK BOARD's own inspector pattern (`_build_work_inspector`), one open at a time, clicked to
## toggle.
##
## **AND IT IS WHAT MAKES THE KIT PICKER POSSIBLE.** §4.7a ② gives every queue entry its own builders
## kit — the override the sim resolves per entry and the Builders card could not express — and that
## control lands in THIS strip beside the crop. On the row it would have been a sixth column.
##
## **PLANT ENTRIES ONLY.** `tame` and `corral` commit no species, so an animal entry has nothing to
## configure YET: it does not expand, and its row does not offer to. One predicate
## (`_queue_crop_choices`) answers both the row's clickability and the strip's existence, so a row
## cannot invite a click that opens nothing.
const BUILD_QUEUE_CROP_WIDTH := 168.0

## The open strip's height — ONE row of controls, and the number BOTH the strip draws at and
## `build_queue_block_height` reserves. The zone `clip_contents`, so a strip that drew taller than it
## was paid for would take the difference off the bottom of the board with nothing to show for it.
const BUILD_QUEUE_SETTINGS_HEIGHT := 30.0

## The strip's key label — the word the retired compose-sheet picker's own header carried, so a player
## who learned it there reads it here.
const BUILD_QUEUE_SETTINGS_CROP_KEY := "CROP"

## `""` on the wire is a REAL instruction — *pick the tile's dominant legal plant for me* — so the
## picker states it as an entry rather than as an empty face. It leads the list, being the state a
## patch starts in.
const BUILD_QUEUE_CROP_DEFAULT_LABEL := "Sim picks"

const BUILD_QUEUE_CROP_TOOLTIP := "Which crop this job commits the patch to. Leave it to the sim and it takes the patch's dominant legal plant."

## The expandable row's own hover, appended to the face/date pair — a row that opens has to say so,
## the board row's `WORK_ROW_OPEN_HINT` being the pattern.
const BUILD_QUEUE_ROW_OPEN_HINT := "Click to set this job's crop."

## `Wild Emmer 56%` — the entry's face, the crop basket's own pairing of a plant with its share.
const BUILD_QUEUE_CROP_ENTRY_FORMAT := "%s %d%%"

## The row's own tooltip: the job face and its date in full, since both columns clip.
const BUILD_QUEUE_ROW_TOOLTIP_FORMAT := "%s — %s"

## The indent a queue row's blocked-cause line takes: NONE. `DetailFormat.build_blocked_lines` hangs
## its sentence under a rung row on the source's card and indents it to say so; a tooltip has no row
## above it, and a leading run of spaces in one reads as a typo rather than as structure.
const BUILD_QUEUE_TOOLTIP_UNINDENTED := ""

## **…AND ON A REAL COUNT IT CARRIES BOTH READINGS**, which is what actually kills the ambiguity
## (`docs/plan_standing_upkeep.md` §4.7): `Cultivate (71, 18) — turn 82 (0%) · 42 turns from now`.
## The column states the DATE, because a chained countdown read as a per-entry span is what the
## playtest tripped over; the span is still the number a player reasons with when deciding whether to
## reorder, so the hover states it rather than making them subtract. A SENTINEL keeps its single face
## — there is no span to state — and so does a pending row, whose date slot is a status glyph.
const BUILD_QUEUE_ROW_SPAN_FORMAT := "%s · %d turns from now"

# ---- A DECLARATION THE WIRE HAS NOT PLACED YET ---------------------------------------------------
#
# `buildQueuePosition` is a WIRE field, so an entry the player declared this turn has no position
# until the sim resolves the turn — and the block, derived from that field alone, stayed empty until
# the next tick. Reported from play as *"it is very confusing if it doesn't show up the moment I
# create it."* The optimistic overlay already carries the declaration (`record_pending_assign` takes
# the `improvement`, and `effective_worker_map` merges it), so a source whose EFFECTIVE improvement
# is a live verb while its WIRE position is `NOT_IN_ANY_BUILD_QUEUE` is exactly a pending entry.
#
# **THREE THINGS IT DOES NOT DO, each because it would state a fact the sim has not made:**
#   • it does not INTERLEAVE — pending rows sort to the TAIL, after every confirmed entry, because
#     the sim APPENDS and the tail is the only honest position for an entry with none;
#   • it states NO DATE — there is no chained answer for an entry that is not in the chain, and a
#     number there would be invented;
#   • it wears NO HEAD MARKER, even when the queue is otherwise empty — the head is the entry the
#     pool is actually funding, which the sim decides.
#
# **ITS `✕` STILL WORKS**, and needs nothing of its own: `unqueue` names a SOURCE, so withdrawing a
# declaration made a second ago is the same command as withdrawing one placed ten turns back — which
# is also the most likely thing a player wants from this row.
#
# **IT COSTS A FULL ROW**, so it goes into the SAME list `build_queue_block_height` counts and
# `_work_board_capacity` subtracts. There is exactly one expression for the drawn height and the
# reserved height, and a pending row that was drawn outside it would slice the board silently.
#
# **IT RECONCILES AWAY FOR FREE**: `reconcile_pending` drops the overlay entry on the first snapshot
# with a newer turn, by which time the wire carries a real position — so nothing has to remove it.

## The pending row's date slot carries the CLIENT'S ONE SPELLING OF PENDING instead of a countdown —
## `FoodIcons.for_status(FoodIcons.STATUS_PENDING)`, the `○` in amber that the work rows' status
## clause and the map's dashed-amber overlays already use. Named here only so the rule is stated
## where the block is; the GLYPH itself is never re-typed, and the tooltip is
## `HudFormat.status_tooltip_line`'s own words ("Pending — starts when you advance the turn").
const BUILD_QUEUE_PENDING_STATUS := FoodIcons.STATUS_PENDING

## **THE BLOCK'S STABLE HANDLES.** Every claim this block owes is a STRING composed at render time —
## the head marker, the job face, the date — so a harness that found them by their text would only
## confirm the string it had already assumed. The metas are what let it find the controls first and
## read them second; `BUILD_QUEUE_BLOCK_META` additionally makes the block's ABSENCE assertable,
## which is the no-queue-no-block rule's whole claim and one no picture can carry.
const BUILD_QUEUE_BLOCK_META := "build_queue_block"

## Valued the entry's own queue POSITION, so a harness can tell the head from the rest without
## reading the marker it is trying to assert.
const BUILD_QUEUE_ROW_META := "build_queue_row"

const BUILD_QUEUE_OVERFLOW_META := "build_queue_overflow"

const BUILD_QUEUE_MARKER_META := "build_queue_marker"

const BUILD_QUEUE_FACE_META := "build_queue_face"

const BUILD_QUEUE_DATE_META := "build_queue_date"

const BUILD_QUEUE_UNQUEUE_META := "build_queue_unqueue"

## The open SETTINGS strip, valued the entry KEY it belongs to — so an assertion can say *this* row's
## strip is the one that opened rather than *a* strip exists somewhere in the block.
const BUILD_QUEUE_SETTINGS_META := "build_queue_settings"

## **THE HEIGHT THE BLOCK RESERVES *AND* DRAWS AT — one function, two callers.** The work zone
## `clip_contents`, so a block that drew without being paid for in `_work_board_capacity`'s chrome
## term would silently slice board rows off the bottom of the zone. Reserving and drawing from one
## expression is what makes the two unable to disagree.
##
## `0` for an empty queue, which is the no-block-at-all rule stated in arithmetic.
##
## `rows_max` is `build_queue_rows_max`'s answer for the zone being drawn into, so both callers hand
## over the SAME number rather than each reading the ceiling.
##
## **`settings_legs` / `settings_crop` ARE THE ROW EXPANSION, AND IT COSTS NOTHING CLOSED**
## (§4.7a ②, ③). The strip is open-only and one-at-a-time, so it adds its height exactly when it
## draws — the work board's own inspector term (`_work_board_capacity`'s `inspector_h`) in this
## block's arithmetic.
##
## **THEY ARE THE STRIP'S TWO INPUTS RATHER THAN ITS HEIGHT, so the number still lives in one place.**
## It was a lone BOOL for exactly that reason — a caller passing a float could pass a different one
## from the strip's own — and a strip that also lists an entry's LEGS has a height that varies, so
## what a caller states is the CONTENT and `build_queue_settings_height` remains the one arithmetic
## both the reservation and the render read.
static func build_queue_block_height(entries: int, rows_max: int,
        settings_legs: int = 0, settings_crop: bool = false) -> float:
    if entries <= 0:
        return 0.0
    var rows := mini(entries, rows_max)
    if entries > rows_max:
        rows += 1
    return ZONE_HEAD_HEIGHT + float(rows) * WORK_ROW_HEIGHT \
        + build_queue_settings_height(settings_legs, settings_crop)

## **WHAT AN OPEN SETTINGS STRIP DRAWS AT — one expression, and `0.0` when there is nothing to open.**
## The crop picker costs its control row; each LEG of the entry's climb costs a line.
##
## **THE LEGS ARE IN THE STRIP BECAUSE A MULTI-LEG ENTRY IS ONE ROW** (`docs/plan_standing_upkeep.md`
## §2.8). A `sow` declared on untended ground is a two-leg climb, and splitting it into two queue rows
## would offer two `✕`s for one withdrawal and two places to drag for one reorder. So the entry stays
## one unit and its legs are what the row opens into.
## **THE LEG LIST'S OWN KEY COSTS A LINE, and forgetting it is how a strip draws taller than it was
## paid for** — which this zone answers by clipping the bottom of the BOARD, silently.
static func build_queue_settings_height(legs: int, has_crop: bool) -> float:
    var height := BUILD_QUEUE_SETTINGS_HEIGHT if has_crop else 0.0
    if legs > 0:
        height += float(legs + 1) * BUILD_QUEUE_LEG_HEIGHT
    return height

# ---- THE ENTRY'S LEGS, inside its row's expansion (`docs/plan_standing_upkeep.md` §2.8) ----------

## One leg line. Shorter than a queue ROW because it is a readout rather than a control — nothing on
## it is pressable, and a leg at the row's own 28px would read as a second queue.
const BUILD_QUEUE_LEG_HEIGHT := 18.0

const BUILD_QUEUE_LEG_FONT_SIZE := 11

## The strip's key for the leg list, in the CROP key's own register.
const BUILD_QUEUE_LEGS_KEY := "CLIMB"

## **THE LEG IN FLIGHT WEARS THE QUEUE HEAD'S OWN MARKER**, and every other leg reserves its slot —
## the block's standing rule one level in. The first published leg IS the one in flight (the wire
## lists them first-incomplete first), so nothing here decides which.
const BUILD_QUEUE_LEG_MARKER := BUILD_QUEUE_HEAD_MARKER

## `▦ Field · 75 work · turn 96` — a leg's rung, what it still owes FROM WHERE THE SOURCE STANDS, and
## its own chained date. **The work figure is the wire's `workRemaining` and not the rung's span**: a
## patch thirty units into a Cultivate owes twenty on that leg, because a previous improvement is a
## RECEIPT, NOT A DISCOUNT.
const BUILD_QUEUE_LEG_FORMAT := "%s · %s work · %s"

## …and the form for a leg the wire dates with a sentinel. **A leg cannot be dated when the entry
## carrying it cannot**, and printing the work alone is the honest half rather than a fabricated turn.
const BUILD_QUEUE_LEG_UNDATED_FORMAT := "%s · %s work"

const BUILD_QUEUE_LEG_META := "build_queue_leg"

# ---- THE DESTINATION PICKER — the `⌃`'s ladder track (`docs/plan_standing_upkeep.md` §2.8) --------
#
# **IT IS AN OVERLAY, NEVER A BLOCK IN THE ZONE.** The work zone reads 396 of 396 in height and 354
# of 356 in width with a row selected, and both budgets ASSERT rather than clip — so a track drawn
# inside it would fail the harness at best and slice the board at worst. A `PopupPanel` is a WINDOW
# and cannot change any zone's height, which is the same reason the detail breakdowns are popovers
# and the destructive confirms are `ConfirmationDialog`s.

## The track card's fixed width — wide enough for a rung's name beside its two figures, narrow enough
## to float beside a 354px dock column without covering it.
const RUNG_TRACK_WIDTH := 292.0

const RUNG_TRACK_PADDING := 10

const RUNG_TRACK_GAP := 4.0

const RUNG_TRACK_TITLE := "TAKE IT TO…"

const RUNG_TRACK_TITLE_FONT_SIZE := ZONE_HEAD_FONT_SIZE

const RUNG_TRACK_ROW_FONT_SIZE := 12

const RUNG_TRACK_REASON_FONT_SIZE := 11

const RUNG_TRACK_ROW_SEPARATION := 2

## The rung NAME column's share of the card, so the figures on the right line up down the track and a
## long rung name ellipsises rather than pushing them off the edge.
const RUNG_TRACK_NAME_WIDTH := 150.0

## `🌾 Tended Patch` — the rung's own glyph and word, the pair the work row's rung mark and the
## source card's badge already use. **A second table of rung names is how one rung comes to be called
## two things on one screen**, so this composes `DetailFormat.rung_badge_word` and the glyph beside
## it rather than spelling either.
const RUNG_TRACK_NAME_FORMAT := "%s %s"

## The branch's FLOOR — the rung every source starts on. It has no verb and is never a destination,
## and naming it is what makes the track a ladder rather than a list of purchases.
const RUNG_TRACK_WILD_NAME := "Wild"

## **THE FIVE STATES, AS WORDS.** A track is read once, in a hurry, and a glyph vocabulary invented
## for it would be five more marks to learn beside the three the work row already carries.
const RUNG_TRACK_STATE_BANKED := "banked"
const RUNG_TRACK_STATE_STANDING := "where you are"
const RUNG_TRACK_STATE_PATH := "on the way"
const RUNG_TRACK_STATE_TARGET := "the target"
const RUNG_TRACK_STATE_LOCKED := "locked"

## `75 work · ≈12 turns` — what a selectable destination's own leg still owes and when the sim says it
## lands. **The turns half renders only where the wire dates the leg**, which is when an entry is
## already climbing this branch; a rung nobody has queued has no chained date and states the work
## alone rather than a number this surface has no right to.
const RUNG_TRACK_COST_FORMAT := "%s work · %s"

const RUNG_TRACK_COST_UNDATED_FORMAT := "%s work"

## The card's stable handles. Every claim the track owes is a string composed at render time, so a
## harness that found a row by its text would only confirm the string it had already assumed.
const RUNG_TRACK_META := "rung_track"
## Valued the rung's own improvement VERB, which is also what a press emits — so an assertion reads
## the destination the row would send rather than the words it happens to print.
const RUNG_TRACK_ROW_META := "rung_track_row"
## …and valued that row's STATE, on the same node, so *which* row is the target is assertable without
## parsing the figures beside it.
const RUNG_TRACK_STATE_META := "rung_track_state"

## **WHICH OF THE THREE THE BUILD SLOT IS**, on the slot itself beside the face it drew. It exists
## because the slot's NODE TYPE stopped answering: a running build's face is a `Button` now (it opens
## the same track), so *is this an offer?* can no longer be read off `control is Button` — which is
## how a harness would come to count a climb as an offer without either side changing.
const WORK_ROW_BUILD_KIND_META := "work_row_build_kind"
const WORK_ROW_BUILD_KIND_OFFER := "offer"
const WORK_ROW_BUILD_KIND_BUILDING := "building"
const WORK_ROW_BUILD_KIND_STALLED := "stalled"
const WORK_ROW_BUILD_KIND_NONE := ""

## The `⌃` mark's own hover, once the mark opens a track instead of declaring outright. It replaces
## `WORK_ROW_READY_QUEUE_TOOLTIP_FORMAT`'s promise of a one-click queue with what the press actually
## does; the PRICE line beneath it is unchanged, and is still `DetailFormat.build_price_clause`'s.
const WORK_ROW_READY_TRACK_TOOLTIP := "Choose how far to take this source — every rung on the way is queued as one job."

## …and the hover on a RUNNING build's face, which opens the same track to re-aim the climb. A build
## in flight is where *how far are we taking this?* is most often asked, and the answer used to be
## reachable only by withdrawing the entry and declaring again.
const WORK_ROW_BUILDING_TRACK_TOOLTIP := "Change where this climb ends — the work already banked is kept."

