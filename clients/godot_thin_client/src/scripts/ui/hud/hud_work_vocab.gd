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


## The FACTION page's other two band-zone blocks. **Settling is the sedentarization score under the
## player-facing word the manual uses.** Both lived in the retired KNOWLEDGE zone and neither is
## knowledge — neither is earned by practice and neither unlocks a verb, which is why they did not
## follow the craft tracks out to the knowledge screen (`docs/plan_knowledge_screen.md` §2). What they
## state is what the faction has BECOME and what it has FOUND, which is the band zone's own question.
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
## **PROVISIONAL, pending playtest.**
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

## **THE FACTION PAGE'S BAND-ZONE HEIGHT TIER.** Its blocks — the PEOPLE bar, the vitals rows,
## SETTLING and DISCOVERIES — do not all fit the ~300px a horizontal dock's zone offers, and the zone
## CLIPS, so a box below this drops DISCOVERIES and keeps the rest (`FactionRollup.build_band_zone`).
##
## **IT IS A REAL "CAN THIS BOX HOLD THE FULL BLOCK?" TEST, not a round number between two docks**,
## and it was MEASURED when Settling and Discoveries were rehomed here from the retired KNOWLEDGE zone
## (`docs/plan_knowledge_screen.md` §4) rather than carried over: the full block reads **461px** at its
## worst case (`band_panel_faction`, the PEOPLE bar + four vitals rows + Settling + the sites list at
## its cap plus its `+N more`), and the two boxes the panel actually offers are **396** on a wide
## horizontal dock and **941** on a tall side one. So this sits above the block and below the tall box,
## with 19px of margin over the first — and that margin, not the gap between the two docks, is what
## protects a box that is neither and what shrinks as the block grows.
##
## **IT WAS GUESSED AT 400 FIRST AND THAT WAS WRONG IN BOTH DIRECTIONS**: it sat BELOW the 461 the
## block needs, so a box between the two would have taken the full branch and clipped — and it cleared
## the wide dock's own 396 by only 4px, which is not a threshold, it is a coincidence.
##
## **Re-measure before adding a row to this zone**; `band_panel_preview._report_zone_content_extent`
## prints the full block's extent on `band_panel_faction` and the tiered one on
## `band_panel_faction_wide`, and this threshold must stay above the first.
const FACTION_BAND_FULL_MIN_HEIGHT := 480.0

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

## **THE FACTION `Kit` ROW'S VOCABULARY IS RETIRED** (`docs/plan_standing_upkeep.md` §4.9 item 12) —
## `FACTION_KIT_ALL_EQUIPPED`, `FACTION_KIT_DRY_NOTE` and `FACTION_KIT_SHORT_NOTE` went with the row
## that was their only reader. The durabilities never aggregated, so the row was an alert and a
## drill-down; the crafting panel's kit ledger states the items in full, and the event dock's
## `kit_life` line pushes the two `life_readout` seams the row could only be read for.

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

## **ONE CONTROL LINE — a `compact` `OptionButton` at this zone's type size, MEASURED.** Two blocks
## draw it: the BUILD QUEUE row's settings strip (`BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT`) and the work
## inspector's kit pair (`WORK_INSPECTOR_KIT_LINE_HEIGHT`), and they name this rather than each
## carrying a literal — two measurements of one control are two answers to one question, free to drift
## by a pixel this zone pays for by clipping the board.
##
## **IT IS NOT THE PLAN'S 38.** `docs/plan_standing_upkeep.md` §4.9 item 12c costs a picker at
## `32 + 6`; that is the COMPOSE SHEET's, a free-standing form with a whole column to spend. In this
## zone the shipped control is 22 and the block gap is charged once per BLOCK rather than per line.
const WORK_COMPACT_PICKER_LINE_HEIGHT := 22.0

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

## …and the PRIORITY picker, when the row's `Priority` link has it open
## (`docs/plan_standing_upkeep.md` §4.9 item 9b). **The floor picker's three cells plus ONE hint
## line**: the two pickers are built from the same three-cell grid, and this one carries
## `WORK_PRIORITY_HINT` beneath it at the same `ALLOC_SECTION_FONT_SIZE` and the same block gap every
## other conditional line in this strip is measured at — so it is stated as the pair rather than as a
## fresh number of its own, and moving the grid moves both.
##
## **THE TWO PICKERS ARE MUTUALLY EXCLUSIVE, so the strip pays for AT MOST ONE** — `_work_picker_open`
## is a three-valued state precisely so that this is true by construction rather than by discipline.
const WORK_INSPECTOR_PRIORITY_PICKER_HEIGHT := WORK_INSPECTOR_POLICY_PICKER_HEIGHT \
    + WORK_INSPECTOR_NOTE_HEIGHT

## **THE CEILING THESE TERMS ADD UP TO, and the DIALOG CAN SIMPLY BE THAT TALL**
## (`docs/plan_standing_upkeep.md` §4.9 item 12d). A model carrying every conditional child at once —
## the overdraw line, the `note`, the `muted_note`, the `ArrivalStrip` and an open picker — reserves
## **210px** (84 + 3×20 + 14 + 52), and that figure is now the `WorkInspectorDialog`'s own
## `min_height` at that model rather than a debt anybody owes the work zone. The dialog is measured
## against the VIEWPORT, whose shortest shipped height is 720, so the ceiling is comfortably inside its
## room on every configuration this client renders — and where a window ever is too short, the card's
## own scroll carries the remainder instead of a zone clipping the board.
##
## **The picker term is the TALLER of the two**, the priority one: they cannot both be open
## (`_work_picker_open`), so the ceiling is a max rather than a sum, and taking the floor picker's 32
## here would understate it by the hint line.
##
## ⛔ **THE RETIRED REASONING, QUOTED RATHER THAN DELETED — it was about a strip inside the zone, and
## there is no such strip.** *"THE CEILING THESE TERMS ADD UP TO, stated because it is UNMEASURED
## rather than because it is reserved. … against the 104 the tallest row any fixture produces asks
## for. `BandCityPanel.PANEL_HEIGHT_WIDE` is sized against that 104, so a row reaching this ceiling
## would take the work zone 106px past its box on a horizontal dock. NOTHING PADS FOR IT,
## DELIBERATELY. No fixture produces the combination and it is not known to be reachable in play —
## `warn` and `note` are near-exclusive on the board's own rows, and the picker is panel state a
## player opens — so the zone is not made 106px taller for a state nobody has seen. A KNOWN unmeasured
## worst case is the cheaper thing to carry; if one is ever observed, this is the figure both of that
## constant's levers move by."* Every measurement in it is still correct; what expired is the
## conclusion. The zone budget no longer carries an inspector term at all, so this height cannot take
## it past its box by any number, and `PANEL_HEIGHT_WIDE` — *"both of that constant's levers"* being
## its 456 and the `MAX_WIDE_HEIGHT_FRACTION` clamp over it — is not the lever a taller strip would
## move any more. It is no longer an unmeasured RISK; it is simply the tallest card the dialog draws.
##
## `band_panel_preview._assert_work_inspector_worst_case_fits` builds it and pins the strip's own
## arithmetic against the DIALOG that hosts it, which is what keeps the number above honest.
## ⛔ **THE KIT PAIR IS NOT A TERM HERE, AND THAT IS THE POINT OF THE THIRD PICKER**
## (`docs/plan_standing_upkeep.md` §4.9 item 12c). It rides the `max` with the other two —
## `WORK_INSPECTOR_KITS_PICKER_HEIGHT` is 44 against the priority picker's 52 — so it **cannot** be the
## worst case and this constant does not move for it.
##
## **IT WAS AN UNCONDITIONAL TERM FOR ONE SLICE, and the retired reasoning is quoted rather than
## deleted**: *"AND THE KIT PAIR IS COUNTED AT ITS STACKED HEIGHT, which INVERTS this file's usual wrap
## rule. A wrapped line normally costs back the row it saved invisibly; here the wrap is the INTENDED
## behaviour — the pair rides one line where there is room and drops the second picker onto its own
## line where there is not — so the height to reserve is the STACKED one and the single line is the
## saving. Measured: at every shipped dock width it stacks, the wide dock's work zone being 382px
## against the 484 one line needs."* Every measurement in that paragraph is correct and the conclusion
## it supports is the one that killed the shape: **the pair NEVER rode one line on any dock the game
## ships**, so the "saving" was hypothetical and the 50px was charged to every strip, open picker or
## not. A picker body has the strip's whole width, stacks unconditionally, and is paid for only when
## the player asks for it.
const WORK_INSPECTOR_CEILING_HEIGHT := WORK_INSPECTOR_HEIGHT \
    + 3.0 * WORK_INSPECTOR_NOTE_HEIGHT \
    + WORK_INSPECTOR_ARRIVALS_HEIGHT + WORK_INSPECTOR_PRIORITY_PICKER_HEIGHT

## Gaps the work column always spends: head→chips, chips→board.
##
## ⛔ **IT WAS 3, AND THE THIRD WAS THE INSPECTOR'S** (`docs/plan_standing_upkeep.md` §4.9 item 12d).
## The retired reading was *"head→chips, chips→board, board→(inspector | nothing)"* — a gap charged to
## every render whether or not a row was selected, because the strip could appear on any of them. The
## strip is a viewport-centred dialog now (`WorkInspectorDialog`), so there is no board→inspector seam
## in this column at all and the gap retires with the term beside it in `_work_board_capacity`.
const WORK_ZONE_GAP_COUNT := 2.0

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

## ---- THE RING'S CARET, ON THE STANDING-RUNG MARK (`docs/plan_standing_upkeep.md` §4.9 item 12c) ---
##
## **THE MARK WEARS THE SAME `⌃` THE READY SLOT DOES, AND IT MUST MEAN THE SAME THING** — press it and
## a card opens stating what the job eats, what it costs to hold, and where it will stall. That is why
## the ring opens a PRICE rather than committing on the click: a caret that sometimes declared outright
## and sometimes opened a card would be one glyph with two meanings.
##
## ⛔ **IT IS NOT IN THE READY SLOT, and that slot's own four-way is the reason.** `⌃▦` offers a rung,
## `▦45%` reports one climbing, `⚠▦` reports one stuck and a fourth state reports one lapsed — and
## `RungLadder.has_track` is FALSE on a corralled herd (`animal:pen` is the top of the animal branch),
## so that slot renders NOTHING at all on the very row this control belongs to. Extending a pen is
## what you do AFTER the ladder is finished; the mark is what the job acts on, a ring widening the pen
## the mark denotes.
const WORK_ROW_RING_FORMAT := "%s⌃"

## …and its hover, which has to say what the mark alone cannot: that the caret is about the PEN's size
## rather than about climbing anything.
const WORK_ROW_RING_TOOLTIP := "Penned — press to price another fenced ring around this pen."

## **A RING IN FLIGHT WEARS NO CARET, so a second cannot be declared over the first.** The mark falls
## back to its plain glyph and states the ring on its hover; the PERCENTAGE lives on the build queue
## row, which is the surface that dates and withdraws it. The tile card's `Fencing N%` badge retired
## with the move for exactly that reason — it was a third statement of one meter.
const WORK_ROW_RING_BUILDING_TOOLTIP_FORMAT := "Penned — another ring is going up, %d%% done. The build queue carries its date."

## The ring card's heading, in `RUNG_TRACK_TITLE`'s register because it is the same kind of card one
## mark over.
const RING_CARD_TITLE := "Extend the pen"

## The ring's own line on that card. It names the THING rather than a rung, because a ring is not one:
## the track's rows are positions on a branch and this is a repeatable increment with no position.
const RING_CARD_ROW_NAME := "Another ring"

## The stable handle on the ring card, and on the mark that opens it — read by name, never by glyph,
## for `WORK_ROW_RUNG_META`'s own reason (a site icon can be the same character as a rung mark).
const RING_CARD_META := &"ring_card"

const WORK_ROW_RING_META := &"work_row_ring"

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

# **THE PLANT ROW'S VERB, AT EVERY RUNG** (`docs/plan_standing_upkeep.md` §4.9 item 12c). Keyed by
# the crew label `HudFormat.plant_crew_label` resolves, so the board row and the sheet it opens cannot
# disagree about what the people on that tile are doing. DISPLAY ONLY — the row's `kind` is still
# `forage`.
#
# ⛔ **IT WAS A PAIR — `WORK_ROW_FORAGE_FORMAT` (`"Forage (%d, %d)"`) AND `WORK_ROW_TEND_FORMAT`
# (`"Tend (%d, %d)"`), THE SECOND OF WHICH CLAIMED**, verbatim: *"The MANAGED plant row's twin. A
# Tended Patch or a Field is never gather-drawn, so its crew tends it; the board says so in the same
# two nouns the compose sheet uses."* Still true of the SIM, and item 12c retired the second word
# anyway: a Field's sheet read `ASSIGN TENDERS` and then offered the *Gathering* kit, the tending
# being the Agriculture pool's rather than the harvest crew's. The rung MARK on the row still says
# which ground it is; what stops changing is the verb, because the job never changed.
const WORK_ROW_PLANT_FORMAT := "Harvest (%d, %d)"

const WORK_ROW_PLANT_FORMATS := {
    HudComposeVocab.HARVEST_CREW_LABEL: WORK_ROW_PLANT_FORMAT,
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

## ---- THE THIRD ARM: WHEN THE MISSING THING IS A GOOD (`docs/plan_standing_upkeep.md` §2.7) --------
##
## ⛔ **THE ROLE SENTENCE IS WRONG ADVICE THE MOMENT THE MISSING THING IS A MATERIAL.** Twelve keepers
## do not mend a fence with no hurdles — *"raise this band's Agriculture role"* points the player at a
## stepper that cannot help. **A dead kit makes a job want more hands; a missing material stops the
## work outright**, and the two must not read alike.
##
## **THREE SHORTFALLS, THREE REMEDIES, AND THREE REGISTERS**:
##   • a MISSING GOOD — the DANGER ink, and this sentence. No stepper fixes it; the remedy is the
##     bench, or fewer things standing.
##   • MISSING HANDS — the WARN ink and the role sentence above. The stepper IS the lever.
##   • a DEAD KIT — the FAINT ink, quiet. It costs hands and takes nothing away, and it is the event
##     dock's `kit_life` line rather than a note on this row at all.
##
## **THE REMEDY, IN THE BLOCKED-BUILD FAMILY'S OWN WORDS**, and its own const so the two families'
## wording has ONE visible relationship: `HudSelectionVocab.BUILD_BLOCKED_MATERIALS_FORMAT` reads
## *"Short of %s — the bench or a trade, not more builders."* — the same two levers, refusing the same
## lever.
##
## **`hands`, NOT `builders`.** Upkeep is staffed by KEEPERS (`agriculture` / `husbandry`), not by the
## builders pool, so naming builders here would send the player to the one role card that cannot move
## this number. Its two siblings on this row — `WORK_ROW_UNDER_KEPT_NOTE` / `WORK_ROW_UNDER_HERDED_NOTE`
## — name a role to RAISE; this one exists to say no head count helps, which is what the row's own
## `SourcePriority` rank is for instead (the sim's `settle_scarce_store` decides which pen the hurdles
## reach when there are not enough).
##
## **IT IS WEB-INDEPENDENT BY DESIGN.** A bench and a trade are the answer on both webs, which is why
## the sentence needs no source kind at all.
const MATERIAL_SHORT_REMEDY := "The bench or a trade, not more hands."

## **IT STATES BOTH TERMS, NEVER THEIR DIFFERENCE** — `Short of hurdles — 0.40 of the 0.58 a turn it
## needs. The bench or a trade, not more hands.` — because the sim publishes both precisely so this
## sentence needs no client arithmetic.
##
## ⛔ **IT SAID `a turn this pen eats` AND A PEN GENUINELY DOES EAT** (`docs/plan_standing_upkeep.md`
## §4.9 item 12c). It ate grass and hay, and §2.7's whole argument is that **hay is FEED, not
## upkeep** — #578 retired a defect that billed a pen's shortfall to the keepers' food larder — so the
## retired tail (`"%s — %s of the %s a turn this %s eats."`, filled from a `pen`/`patch` noun pair) put
## feed and upkeep back under one verb, two lines below a `Fed: 100% — all pasture` row. The readout
## undoing the model is a defect, not a preference. `it needs` names the obligation without naming an
## appetite, and the source noun retires with the clause that consumed it — see
## `MATERIAL_SHORT_REMEDY`.
##
## **THE LEAD-IN IS SHARED WITH THE BLOCKED-BUILD CAUSE, and that is what makes both red.**
## `HudSelectionVocab.BUILD_BLOCKED_MATERIAL_SHORT_LEAD` is the one prefix in this client that means
## *a good is missing*, and `DetailFormat.detail_bbcode` tints an indented sub-line DANGER on it — so
## the work row's note and the queue's stuck reason take one ink from one string, and neither can drift
## into the amber that means *missing hands*. ⛔ It may not carry BBCode: both hosts draw this note as a
## plain `Label`.
const WORK_ROW_MATERIAL_SHORT_FORMAT := HudSelectionVocab.BUILD_BLOCKED_MATERIAL_SHORT_LEAD \
    + "%s — %s of the %s a turn it needs. " + MATERIAL_SHORT_REMEDY

## **THE NOTE'S SEVERITY, CARRIED ON THE MODEL RATHER THAN GUESSED AT THE RENDER SITE.** The work
## inspector and the drawer's standing summary both drew this note in a hard-coded `HudStyle.WARN`,
## which is right for a staffing shortfall and wrong for a missing good — so the producer that knows
## which shortfall it is says so, and neither renderer sniffs the sentence for a hazard word.
##
## ⛔ **AND IT IS NOT BBCODE SMUGGLED INTO THE STRING.** Both hosts render the note as a plain `Label`
## with a `font_color` override; a `[color=…]` run in the text would print its own markup.
const NOTE_SEVERITY_WARN := "warn"
const NOTE_SEVERITY_DANGER := "danger"

## The ink each severity draws in — the ONE table, read by both render sites, so the two surfaces
## cannot colour one note two ways.
static func note_color(severity: String) -> Color:
    return HudStyle.DANGER if severity == NOTE_SEVERITY_DANGER else HudStyle.WARN

## **THE GOOD-SHORTFALL SENTENCE FOR ONE ROW, OR `""`** — `""` meaning *this row went short of no
## good*, which is every row on the shipped ladder but a pen's.
##
## `demand` / `supplied` are the row's own published pair (`LaborAssignment.materialUpkeepDemand` /
## `materialUpkeepSupplied`); the WORST good is the one named, because a note has one sentence and the
## good furthest behind is the one to act on. **Never a total across goods** — that is the currency
## this model does not have.
##
## ⛔ **IT TOOK A `kind` AND NO LONGER DOES.** That argument filled the retired `a turn this %s eats`
## tail from the `MATERIAL_SHORT_NOUN_HERD`/`_PATCH` pair and was used for nothing else, so the web is
## no longer a term of this sentence at all — see `MATERIAL_SHORT_REMEDY`, which is the same answer on
## both. `material_short_note_for_source` retired with it: its whole job was translating a SOURCE kind
## into the labor kind this function no longer wants.
static func material_short_note(demand: Array[Dictionary],
        supplied: Array[Dictionary]) -> String:
    var worst := _worst_material_shortfall(demand, supplied)
    if worst.is_empty():
        return ""
    return WORK_ROW_MATERIAL_SHORT_FORMAT % [
        String(worst[SourceForecast.MATERIAL_PAYOFF_ID_KEY]),
        DetailFormat.format_trimmed(float(worst[SourceForecast.MATERIAL_UPKEEP_SUPPLIED_KEY]),
            RUNG_TRACK_MATERIAL_DECIMALS),
        DetailFormat.format_trimmed(float(worst[SourceForecast.MATERIAL_UPKEEP_DEMAND_KEY]),
            RUNG_TRACK_MATERIAL_DECIMALS)]

## **IS THIS SOURCE SHORT OF A GOOD AT ALL** — the BOOLEAN the card side's ⚠ gate asks, over the same
## `_worst_material_shortfall` walk and therefore the same `MATERIAL_FLOW_MIN` threshold the sentence
## uses. `DetailFormat.rung_is_at_risk` used to ask by testing the SENTENCE for emptiness; once the
## card stopped printing that sentence (§4.9 item 12c, §4.7's *the board is where staffing is decided*)
## composing prose in order to test it would be the one coupling holding the retired readout in place.
static func has_material_shortfall(demand: Array[Dictionary],
        supplied: Array[Dictionary]) -> bool:
    return not _worst_material_shortfall(demand, supplied).is_empty()

## The good furthest behind its bill, by the SHARE paid rather than the raw gap: a rung wanting 6 of
## one good and 0.05 of another is not worse off for the larger number, it is worse off for the one it
## covered least of. `{}` when every good was covered.
static func _worst_material_shortfall(demand: Array[Dictionary],
        supplied: Array[Dictionary]) -> Dictionary:
    var worst := {}
    var worst_share := 1.0
    for row in SourceForecast.material_upkeep_shortfalls(demand, supplied):
        var wanted := float(row[SourceForecast.MATERIAL_UPKEEP_DEMAND_KEY])
        if wanted < SourceForecast.MATERIAL_FLOW_MIN:
            continue
        var share := float(row[SourceForecast.MATERIAL_UPKEEP_SUPPLIED_KEY]) / wanted
        if worst.is_empty() or share < worst_share:
            worst_share = share
            worst = row
    return worst

## Which of the pair this row takes, off the row's own labor kind — one picker, so the note and the
## tooltip can never end up describing two different webs.
##
## **A GOOD-SHORTFALL SENTENCE SUPERSEDES BOTH** where the caller has one to pass: it is the arm that
## names a remedy the stepper cannot reach, so a row short of hands AND of hurdles is told about the
## hurdles. Callers with no material pair in hand pass nothing and get the staffing pair, which is
## every caller that existed before this arm.
static func under_kept_note(kind: String, material_note: String = "") -> String:
    if material_note != "":
        return material_note
    return WORK_ROW_UNDER_HERDED_NOTE if kind == SourceForecast.LABOR_KIND_HUNT \
        else WORK_ROW_UNDER_KEPT_NOTE

## **AND ITS SEVERITY, ASKED THE SAME WAY** — DANGER for a missing good, WARN for missing hands. One
## producer for the pair, so a note and its ink can never describe different shortfalls.
static func under_kept_note_severity(material_note: String = "") -> String:
    return NOTE_SEVERITY_DANGER if material_note != "" else NOTE_SEVERITY_WARN

## **THE COUNTDOWN RIDES THE GOOD-SHORTFALL ARM TOO**, unchanged: a material shortfall drives the same
## decay through the same `grace_turns` and the same neglect counter as a staffing one — one grace,
## one counter, the decay riding the worst of the two fractions — so the hover says how long you have
## whichever term came up short.
static func under_kept_tooltip(kind: String, rung_word: String = "",
        grace: int = UNDER_KEPT_NO_COUNTDOWN, material_note: String = "") -> String:
    var note := under_kept_note(kind, material_note)
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
static func under_kept_note_for_source(source_kind: String,
        material_note: String = "") -> String:
    return under_kept_note(_labor_kind_of(source_kind), material_note)

## ⛔ **RETIRED — `material_short_note_for_source(source_kind, demand, supplied)`**, the card-side
## entry point onto the sentence above. It existed for one reason — `SOURCE_KIND_HERD` is `"herd"`
## while `LABOR_KIND_HUNT` is `"hunt"`, so a card handing its own kind straight to the labor-keyed
## producer would silently word an animal source's note for the plant web — and BOTH halves of that
## reason are gone: the sentence takes no kind any more, and the card does not state it at all
## (`docs/plan_standing_upkeep.md` §4.9 item 12c). §4.7 set that shape and item 12 regressed against
## it by shipping the full sentence to both entry points: **the board is where staffing is decided
## this turn, and on the tile card it is a number you cannot act on.** The staffing half of that
## argument does not carry over — no head count fixes a missing good — and a better one replaces it:
## what this UI offers against a scarce good is the row's own `SourcePriority` rank, and that control
## sits in the work row's strip. The tile card has nothing to press.
##
## What the card keeps is the ⚠ and its short state word, which `DetailFormat.rung_is_at_risk` still
## routes through `has_material_shortfall` above — the mark, not the sentence.

## …and the HOVER asked the same way, which is the form both source CARDS use. It supplies no
## countdown by construction: the card states no figure at all, and the one surface that does states
## it by passing the pair above.
static func under_kept_tooltip_for_source(source_kind: String,
        material_note: String = "") -> String:
    return under_kept_tooltip(_labor_kind_of(source_kind), "", UNDER_KEPT_NO_COUNTDOWN,
        material_note)

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

## …and the DIALOG that hosts it (`docs/plan_standing_upkeep.md` §4.9 item 12d). Its own handle rather
## than the strip's, because the two claims a harness makes about them are opposites: the strip must
## be found OUTSIDE every zone now, and the card around it must be found centred in the viewport. One
## meta answering both would make "the zone no longer contains the strip" and "the dialog is up" the
## same question.
const WORK_INSPECTOR_DIALOG_META := "work_inspector_dialog"

const INSPECTOR_CLOSE_GLYPH := "✕"

const INSPECTOR_CLOSE_TOOLTIP := "Close detail"

## **WHAT JOINS THE HEAD LINE'S TWO FACTS** — `Harvest (28, 16) · ▦ Field 100%`
## (`docs/plan_standing_upkeep.md` §4.9 item 12c). The row's own second line already separates its
## accounts with this character (`STANDING_SUMMARY_SEPARATOR`'s register), so the strip joining *what
## is being done here* to *what this ground IS* reads as one more clause of the same kind rather than
## as a new punctuation to learn.
const WORK_INSPECT_RUNG_SEPARATOR := " · "

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

# ---- THE PLAYER'S OWN RANK ON A WORKED ROW (`docs/plan_standing_upkeep.md` §4.9 item 9b) --------
#
# **THE MARK IS A WORD ON EVERY LAYER IT CROSSES.** `LaborAssignment.priority` is a FlatBuffers enum
# whose ordinals are `Normal = 0, High = 1, Low = 2` — Normal first because a scalar equal to its
# default costs no wire bytes — while the order the band actually sheds in runs Low, Normal, High. The
# two are not the same sequence, so the native decoder converts the ordinal to one of the three tokens
# below and no GDScript reader is ever handed a number it could sort on. These are also exactly the
# tokens `work_priority <faction> <band> <source…> <level>` takes, so the picker echoes back the word
# it was shown and nothing between the button and the socket re-spells it.

const WORK_PRIORITY_HIGH := "high"

const WORK_PRIORITY_NORMAL := "normal"

const WORK_PRIORITY_LOW := "low"

## The picker's order, and the only place it is stated: **High · Normal · Low**, the band's own
## shedding order read from the top so the leftmost button is the one that is kept longest. It is NOT
## the wire ordering (`Normal, High, Low`) and must never be rebuilt from `SourcePriority`'s numbering.
const WORK_PRIORITY_LEVELS: Array[String] = [WORK_PRIORITY_HIGH, WORK_PRIORITY_NORMAL,
    WORK_PRIORITY_LOW]

## The one-word faces the three picker buttons wear — the level itself, capitalised, with no verb and
## no consequence clause. The consequence is stated ONCE, under the row of buttons
## (`WORK_PRIORITY_HINT`), rather than three times across three faces that would then have to agree.
const WORK_PRIORITY_FACES := {
    WORK_PRIORITY_HIGH: "High",
    WORK_PRIORITY_NORMAL: "Normal",
    WORK_PRIORITY_LOW: "Low",
}

## **WHAT A MARKED ROW PUTS AT THE HEAD OF ITS LINE TWO.** Normal is deliberately ABSENT rather than
## mapped to `""`: the default is the overwhelming majority of rows, and a prefix on it would spend the
## board's scarcest resource — line width — saying nothing. `work_row_priority_prefix` reads this
## table, so an absent key and a blank face are one answer.
const WORK_ROW_PRIORITY_PREFIXES := {
    WORK_PRIORITY_HIGH: "High priority",
    WORK_PRIORITY_LOW: "Low priority",
}

## The sentence under the picker. **ONE sentence, and it names no resource on purpose**: the rank
## orders the shedding walk's workers today and the pen-feed split as of the same slice, and a list of
## the consumers it governs would have to grow every time another scarcity handler learns to read it.
const WORK_PRIORITY_HINT := "When something runs short, the band spends it on high priority first."

## The inspector strip's fourth inline link, between `Change policy` and `Unassign` — and the face the
## CRAFTING panel's bench link wears too, read back as `HudWorkVocab.WORK_INSPECT_PRIORITY`. One word
## for one control kind: the two links open the same `build_work_priority_picker`, and a second
## spelling in `hud_crafting_vocab.gd` would be free to drift from this one.
const WORK_INSPECT_PRIORITY := "Priority"

## The strip's FIFTH inline link, between `Priority` and `Unassign` — the kit pair, opened on demand
## (`docs/plan_standing_upkeep.md` §4.9 item 12c). The three pickers are the same kind of control (a
## standing property of this row) and `Unassign` stays last, being the destructive one.
##
## **PLURAL, because it opens TWO pickers.** `Kit` would name the take crew's alone, which is the half
## a player already meets on the compose sheet — and the whole point of the pair is that the SITE has
## one too.
const WORK_INSPECT_KITS := "Kits"

## The stable handle on ONE picker button, valued the LEVEL it would send — `POLICY_RUNG_META`'s twin
## one control over, and for its reason: the face is presentation (`WORK_PRIORITY_FACES`), so a
## harness identifying a button by `text` would be asserting the string it had already composed.
const WORK_PRIORITY_RUNG_META := &"work_priority_rung"

## …and the handle on a row's PREFIX label, valued the level it states. Its own node rather than a
## splice into the accounts string, so the accounts keep their `OVERRUN_TRIM_ELLIPSIS` + tooltip
## treatment untouched (`BandPanelController._build_work_row_accounts`).
const WORK_ROW_PRIORITY_META := &"work_row_priority"

## The gap between that prefix and the accounts beside it, and it is **ZERO ON PURPOSE**: the prefix
## carries `WORK_INSPECT_SENTENCE_SEPARATOR` inside its own text, exactly as the accounts carry the
## one before the floor clause, so the spacing of line two is stated in one place instead of half in
## a string and half in a container constant — where the two would drift by a pixel and the line would
## read as two different sentences depending on which clause you looked at.
const WORK_ROW_PRIORITY_SEPARATION := 0

## **WHICH EXPANSION THE OPEN INSPECTOR IS SHOWING — ONE state with FOUR values, never three bools.**
## The pickers are mutually exclusive: bools would admit combinations that reserve no height at all,
## and the strip's tallest state is what the work zone's box is sized against. Opening any of them
## therefore SETS this, which closes the others by construction rather than by discipline — and
## `_work_inspector_height` adds ONE picker term through a matching `if/elif` chain, so the strip pays
## the MAX and never the sum.
##
## **THAT PROPERTY IS WHAT MADE THE KIT PAIR FREE** (`docs/plan_standing_upkeep.md` §4.9 item 12c). The
## pair was drawn as a permanent block first and cost the zone 50px unconditionally, on top of whichever
## picker was open — measured at **442 into a 396px box** on a wide dock. As a picker it costs
## `WORK_INSPECTOR_KITS_PICKER_HEIGHT` (44) **instead of**, not beside, the priority picker's 52 — so
## the worst case does not move at all.
const WORK_PICKER_NONE := &"none"

const WORK_PICKER_FLOOR := &"floor"

const WORK_PICKER_PRIORITY := &"priority"

## The KIT PAIR — the take crew's tool and the SITE's keeping tool, opened on demand from the links row
## exactly as the two above are. They are the same kind of control: a standing property of the row that
## the strip has no room to draw permanently.
const WORK_PICKER_KITS := &"kits"

## The level a value that is not one of the three tokens reads as — `upkeep_fund_mode`'s rule, and for
## its reason: the control that renders this offers exactly three choices, so a fourth token would
## light none of them and leave the row looking unset. It is a NORMALIZATION, not a missing-field
## fallback — the decoder always inserts the key.
static func work_priority_of(value: Variant) -> String:
    var level := String(value).strip_edges().to_lower()
    return level if WORK_PRIORITY_FACES.has(level) else WORK_PRIORITY_NORMAL

## A row's line-two PREFIX, its separator included — `""` on a Normal row, whose line two must stay
## byte-identical to what it printed before the mark existed.
static func work_row_priority_prefix(level: String) -> String:
    var face := String(WORK_ROW_PRIORITY_PREFIXES.get(work_priority_of(level), ""))
    return "" if face == "" else face + WORK_INSPECT_SENTENCE_SEPARATOR

## The ink a marked row's prefix wears. **`SIGNAL` for High and `DANGER` for Low** — this HUD's two
## ends of "the player has singled this out": SIGNAL is what a surface wears when it is the thing being
## attended to, DANGER what it wears when it is the thing that gets given up. A Normal row has no
## prefix to tint and answers the quiet ink its accounts already use, so a caller that asks anyway is
## never handed a colour that would make the default shout.
##
## Resolved in a FUNCTION rather than a `const` table: `HudStyle`'s palette entries are `static var`
## (the theme is swappable), so a `const` initialised from one is a parse error — `hud_event_vocab.gd`
## makes the same statement about its own rung table.
static func work_priority_ink(level: String) -> Color:
    match work_priority_of(level):
        WORK_PRIORITY_HIGH:
            return HudStyle.SIGNAL
        WORK_PRIORITY_LOW:
            return HudStyle.DANGER
        _:
            return HudStyle.INK_DIM

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
## queue→chips, chips→board, board→pager. Named rather than spelled, since it is the one term of the
## reservation that is a COUNT rather than a height.
##
## ⛔ **IT WAS 6, AND THE SIXTH WAS THE INSPECTOR'S GAP** (`docs/plan_standing_upkeep.md` §4.9 item
## 12d), beside a `BUILD_QUEUE_ROOM_INSPECTOR_HEIGHT` term that is retired with it. **The retired
## reasoning is quoted rather than deleted**: *"AND THE INSPECTOR'S OWN HEIGHT BESIDE THAT GAP. The
## reservation below budgeted the STRIP'S GAP and not the strip, so the queue claimed rows the zone
## could only afford while nothing was selected — and selecting a row is one click, after which the
## board (floored at one row) has nothing left to give back. It is the BASE height rather than a worst
## case on purpose: the conditional lines and the policy picker are the board's to pay for, and a
## queue cap sized on the tallest strip a model could ever produce would shrink the block on every
## dock for a state most bands never reach."* Every word of that was true while the strip lived in
## this column. It does not: the inspector is a viewport-centred `WorkInspectorDialog` and no
## selection can take a pixel off this zone, so **the whole point is that NO inspector term survives
## anywhere in the zone's budget** — one left here would go on charging the queue for a strip that
## cannot appear.
const BUILD_QUEUE_ROOM_GAP_COUNT := 5.0


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
        + BUILD_QUEUE_ROOM_SETTINGS_HEIGHT \
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

# ---- DRAG-TO-REORDER — the marker column IS the handle (`docs/plan_standing_upkeep.md` §4.7b ③) ---
#
# **THE ROW HAS NO WIDTH FOR A COLUMN OF ITS OWN, and that is measured rather than felt.** At the tall
# LEFT dock the row's content line is ~356px, of which the marker, the date, the reorder column and the
# separations take everything but the job face — and the face is ALREADY ellipsised at its widest
# shipped value (`🐄 Corral Thunder Mammoths` needs 189 of the ~126 it gets). A handle column would
# come straight out of that face, which is the one column with an unclipped-name guarantee on it.
#
# So the handle is the marker slot, which is reserved on every row already (so the faces line up) and
# holds nothing at all on a non-head row. **The drag is no longer the ONLY reorder** — the row grew
# explicit `▲`/`▼` arrows in the `✕`'s old column, because a handle that only reveals itself under a
# press is not a control a player finds — but it costs the row nothing, so it stayed. It keeps `▸` on the HEAD — that marker is load-bearing, it
# says which entry the builders pool is funding — and draws a faint grab glyph everywhere else.
#
# **A PENDING ROW IS NOT DRAGGABLE**, and its slot stays empty: the wire has not placed it, so there
# is no position for `build_order` to move it to and nothing below it to move it above.

## The grab glyph a confirmed non-head row wears in the marker column.
const BUILD_QUEUE_DRAG_HANDLE := "⠿"

## …at the same width as the head marker (the column is one column) and in the quiet ink, because a
## handle is an affordance rather than a statement: the `▸` is the only mark in this column that says
## something about the queue.
const BUILD_QUEUE_DRAG_HANDLE_FONT_SIZE := WORK_ROW_FONT_SIZE

const BUILD_QUEUE_DRAG_TOOLTIP := "Drag to reorder. The builders fund the top entry until its meter fills, then the next — so the order IS the funding decision."

## The drag preview's face: the job it is carrying, so a list of near-identical rows still says which
## one is in flight.
const BUILD_QUEUE_DRAG_PREVIEW_ALPHA := 0.75

## **THE DROP INDICATOR IS DRAWN INSIDE THE TARGET ROW'S OWN 28px.** The block sets `separation` to 0
## so the rows are flush, and an indicator drawn BETWEEN two rows would need a new term in
## `build_queue_block_height` — on the reservation side as well as the render side, in a zone that
## clips. An edge line on the row's own stylebox costs the block nothing at all.
const BUILD_QUEUE_DROP_EDGE_WIDTH := 2

## Which edge of the hovered row the entry would land on — the row's own handle for the indicator, so
## a harness can assert the drop TARGET without reading a colour off a picture.
const BUILD_QUEUE_DROP_MARK_META := "build_queue_drop_mark"

## The drag payload's own type tag. A `Variant` drop target has to be able to refuse everything that
## is not one of these rows, and a bare Dictionary from some other control would otherwise satisfy a
## key check.
const BUILD_QUEUE_DRAG_TYPE := "build_queue_entry"

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

## **AND IT IS A DOOR NOW, NOT A NOTICE** (`docs/plan_standing_upkeep.md` §4.9 item 9c). The sentence that
## sent the player to the command line went with the drag handle (§4.7b ③), and the one that said the
## hidden entries were out of reach went with the EXPANSION — pressing this row opens the whole queue
## over the Work zone, where every entry has a row, both arrows and its own settings strip. The head
## above is the same door and the only way back.
const BUILD_QUEUE_OVERFLOW_TOOLTIP := "Show the whole queue. The Work board makes way for it; press the BUILD QUEUE header to come back."

## The withdrawal. Same `✕` and same steady DANGER ink the parties zone's recall control wears — a
## destructive control reads as one — and, like that one, it asks nothing first: `unqueue` withdraws a
## DECLARATION, the banked meter survives it, and re-declaring is one tick of the compose control.
const BUILD_QUEUE_UNQUEUE_GLYPH := "✕"

## **32 BECAUSE `HudWidgets.compact` LEAVES THE GHOST BUTTON ITS HORIZONTAL PADDING.** The trim
## squeezes the type size and the VERTICAL padding — that is what keeps the control inside a 28px row
## — and the button's own left/right margins survive it, so a 22px reservation was 10px under what the
## `✕` actually draws and the row's expanding face paid the difference. It is the withdrawal's column
## rather than the glyph's: what has to fit is the styled BUTTON.
const BUILD_QUEUE_UNQUEUE_WIDTH := 32.0

const BUILD_QUEUE_UNQUEUE_TOOLTIP := "Withdraw this build. The work already banked is kept, and the source keeps its crew."

# ---- UP/DOWN REORDER — the arrows take the `✕`'s column (`docs/plan_standing_upkeep.md` §4.7b ③) --
#
# **THE DRAG WAS INVISIBLE, AND THAT IS WHAT THESE ANSWER.** Ray, from play, once the gesture worked
# at all: a handle that only reveals itself on a press is not a control a player finds. So the queue
# grows an EXPLICIT reorder — two arrows on every confirmed row — and the drag stays beside it,
# because it now works and costs the row nothing.
#
# **THEY COST ZERO PIXELS, WHICH IS WHY THIS PLACEMENT WON.** The row's ~356px content line is
# already spoken for (marker 10, face expanding, date 168, this column 32, four separations) and the
# face is ellipsised at its widest shipped value — so the only placement that adds no column is one
# that TAKES a column. The `✕` is the slot with somewhere else to go: the row expands into a settings
# strip on every entry, and a withdrawal is a rarer act than a reorder.
#
# **AND THE ARROWS KEEP THE FULL ROW HEIGHT** — a stacked pair inside 28px would be two 13px targets,
# which is the placement this one was measured against and beat.

## The reorder column IS the withdrawal's old one, stated as arithmetic rather than as a second 32.
const BUILD_QUEUE_REORDER_WIDTH := BUILD_QUEUE_UNQUEUE_WIDTH

## The gap between the pair, so two adjacent buttons read as two targets rather than as one wide one.
const BUILD_QUEUE_REORDER_SEPARATION := 2

## …and what each arrow gets from the split. **NEITHER BUTTON IS GIVEN A HEIGHT**: they fill the row,
## which is the whole reason a side-by-side pair beat a stacked one.
const BUILD_QUEUE_REORDER_BUTTON_WIDTH := \
    (BUILD_QUEUE_REORDER_WIDTH - float(BUILD_QUEUE_REORDER_SEPARATION)) / 2.0

## **THE GHOST BUTTON'S SIDE PADDING IS 11px EACH, AND TWO OF THOSE DO NOT FIT IN 15.**
## `HudWidgets.compact` deliberately leaves the horizontal margins alone — a zone row is short on
## height, not on width — which is exactly why the `✕` needed 32 for a 9px glyph. A pair sharing that
## same 32 is the one control on this row that IS short on width, so it trims the sides too
## (`HudWidgets.compact`'s `padding_h`) and the glyph is what the column then holds.
const BUILD_QUEUE_REORDER_PADDING_H := 1

## …and the type size that leaves the glyph room inside the trimmed box. Smaller than
## `WORK_ROW_FONT_SIZE` because a solid triangle reads at a size a letterform would not, and because
## the arrows are an affordance beside the row's information rather than part of it.
const BUILD_QUEUE_REORDER_FONT_SIZE := 11

const BUILD_QUEUE_PROMOTE_GLYPH := "▲"

const BUILD_QUEUE_DEMOTE_GLYPH := "▼"

## Both tooltips say what the ORDER decides, in `BUILD_QUEUE_DRAG_TOOLTIP`'s own words — the two
## controls do one thing and a player who learned it from the handle must read it again here.
const BUILD_QUEUE_PROMOTE_TOOLTIP := "Move this build UP one place. The builders fund the top entry until its meter fills, then the next — so the order IS the funding decision."

const BUILD_QUEUE_DEMOTE_TOOLTIP := "Move this build DOWN one place. The builders fund the top entry until its meter fills, then the next — so the order IS the funding decision."

## How far one press moves an entry: one place, in either direction. The `build_order` position is
## `rank ∓ this`, and the command's own semantics are *remove, then insert at* — so a single step
## swaps the entry with its neighbour rather than needing an index of its own.
const BUILD_QUEUE_REORDER_STEP := 1

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
## **AND IT IS WHAT MADE THE KIT PICKER POSSIBLE.** §4.7a ② gives every queue entry its own builders
## kit — the override the sim resolves per entry and the Builders card could not express — and that
## control lands in THIS strip beside the crop. On the row it would have been a sixth column.
##
## **THE CROP IS THE PLANT WEB'S ALONE; THE KIT IS EVERY ENTRY'S.** `tame` and `corral` commit no
## species, so an animal entry offers no crop at all — and it still expands, because it is still
## raised with a tool. `_queue_settings_content` is the ONE predicate answering both the row's
## clickability and the strip's existence, so a row cannot invite a click that opens nothing, and what
## differs between the webs is the strip's CONTENT rather than whether there is one.
const BUILD_QUEUE_CROP_WIDTH := 168.0

## **ONE CONTROL LINE INSIDE THE STRIP** — the compact picker's own drawn height, which
## `_build_queue_settings_line` also declares as its minimum so the drawn line and the reserved one
## are one number. It is the term the WRAP multiplies, and the reason the strip's height below is a
## SUM rather than a literal: a second line costs another control and no more chrome.
const BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT := WORK_COMPACT_PICKER_LINE_HEIGHT

## **THE STRIP'S OWN CHROME, CHARGED EXACTLY ONCE** — `HudStyle.ROLE_CARD_PADDING` above and below
## (6 + 6), from the single `work_inspector_stylebox` the strip wears however many lines open inside
## it. Reserving it per LINE is what made a wrapped strip cost 68 where it draws 56.
const BUILD_QUEUE_SETTINGS_CHROME := 12.0

## The open strip's height at ONE LINE of controls, and the number BOTH the strip draws at and
## `build_queue_block_height` reserves. The zone `clip_contents`, so a strip that drew taller than it
## was paid for would take the difference off the bottom of the board with nothing to show for it —
## and a strip that drew SHORTER costs the board a row it could have drawn, for dead space.
##
## **34 = the control plus the chrome, and the 30 it replaces was a live 4px UNDER-RESERVE**
## (`docs/plan_standing_upkeep.md` §4.7b ②): a reservation that counted only the control was short by
## the chrome around it every time a strip opened.
const BUILD_QUEUE_SETTINGS_HEIGHT := BUILD_QUEUE_SETTINGS_CHROME \
    + BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT

## **THE HEADROOM `build_queue_rows_max` KEEPS FOR THIS STRIP, and it is what the retired
## `BUILD_QUEUE_ROOM_INSPECTOR_HEIGHT` was paying for by accident** (`docs/plan_standing_upkeep.md`
## §4.9 item 12d). An open settings strip is charged to the BOARD, and the board is floored at
## `maxi(1, …)` — so once the queue has claimed enough rows to leave the board at that floor, an
## opened strip has nothing to come out of and the zone simply overflows. It never showed while the
## queue's reservation carried 84px of inspector the queue could not use: the strip fitted in its
## shadow. Taking the inspector out of that reservation is exactly what exposed it — measured on a
## 1920 bottom dock, the queue claimed 3 rows instead of 1 and `Zone_work` drew **414 into its 396px
## box** the moment a strip opened.
##
## **STATED AS THE STRIP'S OWN WORST CASE rather than as a cushion**: the WRAPPED control pair, which
## is what an entry carrying both a crop and a kit draws wherever the strip is too narrow for one
## line. **LEGS ARE DELIBERATELY NOT COUNTED** — a multi-leg climb is the rarer entry, and reserving
## for it would shrink the block on every dock for a state most bands never reach, which is the same
## trade the retired constant's own comment made.
##
## ⛔ **IT IS DECLARED HERE, NOT BESIDE `BUILD_QUEUE_ROOM_GAP_COUNT` WHERE IT IS READ**, because a
## GDScript `const` may not read one declared below it and both of its terms are on the lines above.
const BUILD_QUEUE_ROOM_SETTINGS_HEIGHT := BUILD_QUEUE_SETTINGS_HEIGHT \
    + BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT

## **THE KEY COLUMN BOTH SETTINGS KEYS DECLARE — `CROP` and `KIT` alike.** One constant, because the
## whole point of a stacked layout is that the two keys line up: two independently-measured widths
## would put the pickers on two different left edges the moment the strip wraps, and a reader would
## see that as a misdrawn panel rather than as two rows.
const BUILD_QUEUE_SETTINGS_KEY_WIDTH := 30.0

## The kit control's declared width, the crop's own (`BUILD_QUEUE_CROP_WIDTH`) — the two pickers are
## the same kind of control naming the same kind of thing, and a pair of unequal columns beside two
## equal keys reads as an accident.
const BUILD_QUEUE_KIT_WIDTH := 168.0

## The kit half's key, in the CROP key's register.
const BUILD_QUEUE_SETTINGS_KIT_KEY := "KIT"

## ---- THE INSPECTOR STRIP'S KIT PAIR (`docs/plan_standing_upkeep.md` §4.9 item 12c) --------------
##
## **THE STRIP IS THE SURFACE BECAUSE IT IS THE ONE PLACE THE RUNG IS KNOWN.** That is item 12's own
## phrase, used when it declined a kit line on the Agriculture and Husbandry cards. The board's
## inspector already knows which source is selected and stated neither kit.
##
## **IT IS A PICKER, OPENED ON DEMAND — not a block the strip draws all the time.** That is the whole
## of why it is free: `_work_picker_open` is a four-valued state, so the strip pays for AT MOST ONE
## expansion and `_work_inspector_height` takes the MAX rather than the sum. At 44 the kit picker is
## SHORTER than the priority picker already is (52), so the strip's worst case does not move.
##
## ⛔ **IT WAS A PERMANENT BLOCK FIRST, AND THAT COST 50px UNCONDITIONALLY** — on top of whichever
## picker was open. Measured: **442 into a 396px box on a wide dock, over by 46**, and over at every
## viewport where the narrow shell is height-clamped too. The dead reasoning, quoted because it is
## exactly the kind that gets written back if it merely vanishes: *"THE PAIR IS ONE FLEX-WRAP ROW, NOT
## TWO HAND-PLACED ONES, and that INVERTS the usual trap. A wrapped line normally costs back the row it
## saved invisibly; here the wrap is the INTENDED behaviour, so the height to reserve is the STACKED
## one and the single line is the saving."* The inversion was real and the reservation was honest; what
## was wrong was drawing the pair at all times. **Measured against it**: the pair needs 472px to ride
## one line and the WIDEST shipped work zone on a horizontal dock is 382 — the wide shell gives the
## board ONE 380px column at 1920, the flanks and the lateral bounds having taken the rest — so the
## "single line is the saving" case never occurred on any dock the game ships.
##
## **ONE CONTROL LINE IS THE QUEUE SETTINGS STRIP'S OWN MEASURED HEIGHT.** The two hosts draw the same
## control — a `compact` `OptionButton` in this zone — so a second measurement would be a second answer
## to one question, free to drift by a pixel this zone pays for by clipping the board. **The plan's
## arithmetic said 38px** (`32 + 6`); that is the COMPOSE SHEET's picker, a free-standing form with a
## whole column to spend. In this zone the shipped figure is 22.
##
## **TWO LINES, ONE PER KIT, AND NO WRAP PREDICATE.** A picker body has the strip's full width and two
## controls to place, so it stacks them unconditionally — which removes the width branch, the
## `one_line` argument and the drift surface between a predicate and a container that the block form
## needed. `WORK_INSPECTOR_KIT_KEY_WIDTH` still lines the two keys up.
const WORK_INSPECTOR_KIT_LINES := 2.0

## **AND THE PICKER'S HEIGHT, WHICH IS THE TERM THE `max` COMPETES ON** — two control lines and no
## chrome of its own, the block gap being the column's. 44 against the priority picker's 52, so this
## never becomes the strip's worst case and `WORK_INSPECTOR_CEILING_HEIGHT` does not move for it.
##
## ⛔ **NO HINT LINE, and the arithmetic is why.** The priority picker's 52 is 32 + a 20px hint; two kit
## lines plus a hint would be 64 — 12 over the current max, which busts the wide shell by 8. What the
## hint would have said (`none` is a real choice, and is how a site is worked bare-handed to conserve
## the tool) is in each picker's TOOLTIP instead.
const WORK_INSPECTOR_KITS_PICKER_HEIGHT := WORK_INSPECTOR_KIT_LINES \
    * WORK_COMPACT_PICKER_LINE_HEIGHT

## The crew key's declared width — wider than the queue strip's `CROP`/`KIT` because this key is a
## crew NOUN (`Harvesters` / `Hunters` / `Herders`) rather than a three-letter tag, and both keys in
## the picker take it so the two rows share a left edge.
const WORK_INSPECTOR_KIT_KEY_WIDTH := 62.0

## ⛔ RETIRED — **`WORK_INSPECTOR_KIT_PICKER_WIDTH`**, a declared 168px column borrowed from the queue
## settings strip. That strip places two controls SIDE BY SIDE and must column them; a picker body has
## the strip's whole width and one control per row, so the picker expands into whatever the key leaves
## and a fixed column would be dead space on a wide dock and a clipped kit name on a narrow one.

## The RIGHT key. One word, and deliberately not the role's name (`Agriculture` / `Husbandry`): the
## pair is *what this crew carries* beside *what holds the site*, and naming a band ROLE here would
## read as a control over that role's pool — which this is not, the kit being per SITE since §2.5.
const WORK_INSPECT_UPKEEP_KEY := "Upkeep"

## ⛔ **THE `none` RULE LIVES IN BOTH TOOLTIPS BECAUSE THE PICKER HAS NO HINT LINE.** The priority
## picker's 52px is 32 + a 20px hint; two kit rows plus a hint would be 64, which is 12 over the
## strip's current worst case and busts the wide dock by 8 — so the sentence a hint line would have
## carried rides the controls themselves. It is on BOTH, deliberately: `none` means the same thing on
## either kit and a player who opens one picker must not have to open the other to learn it.
const WORK_INSPECT_TAKE_KIT_TOOLTIP := "What this crew carries when it works the source. `none` is a real choice, not an empty one — it is how a site is worked bare-handed to conserve the tool."

const WORK_INSPECT_UPKEEP_KIT_TOOLTIP := "What this SITE is held with, turn after turn — the keeping tool, not the take one. Set per site, so a pick here moves this row and no other. `none` is a real choice, not an empty one — it is how a site is held bare-handed to conserve the tool."

## ⛔ RETIRED — **`work_inspector_kits_one_line_width` / `work_inspector_kits_one_line` /
## `work_inspector_kits_height(has_kits, one_line)`**, the block form's width predicate and its
## always-on height term. A picker body has the strip's full width and stacks its two rows
## unconditionally, so there is no width branch left to state — and the height is a plain constant in
## the `max` above rather than a term added to every strip whether or not anyone asked for it.

## The stable handle on each picker, valued the kit id it currently states — `BUILD_QUEUE_KIT_PICKER_META`'s
## twin one host over, and for its reason: the face is presentation, so a harness identifying the
## control by `text` would be asserting the string it had already composed.
const WORK_INSPECT_TAKE_KIT_META := &"work_inspect_take_kit"

const WORK_INSPECT_UPKEEP_KIT_META := &"work_inspect_upkeep_kit"


## **WHY THE JOB HAS A TOOL AT ALL, AND HOW FAR THIS PICK REACHES**
## (`docs/plan_standing_upkeep.md` §4.7a ②). Both sentences are load-bearing: the derivation is the
## thing the player is overriding, and *this job alone* is the promise the retired `builders`-row
## picker could not keep — one click there pinned a web's tool onto every later build with no way back.
const BUILD_QUEUE_KIT_TOOLTIP := "Which tools this build is raised with. Left alone, the job derives its own from the food web it is on — hoes for a crop, a crook for stock. A pick here changes THIS job alone."

## **DOES THE SETTINGS STRIP FIT ITS TWO CONTROLS ON ONE LINE?** — the flow, computed rather than
## discovered (`docs/plan_standing_upkeep.md` §4.7b ②). Ray, on the layout: *"make it flow, so on
## horizontal layouts it would be 1 line and vertical 2, most likely because of space available."*
##
## ⛔ **IT IS A WIDTH PREDICATE AND NOT A CONTAINER BEHAVIOUR, and the zone's clipping is why.** The
## strip's height is RESERVED before it is drawn — `build_queue_settings_height` is the one arithmetic
## both `_work_board_capacity`'s chrome term and the strip's own `custom_minimum_size` read — so a
## Godot flow container that wrapped at LAYOUT time would leave the reservation unable to know how
## many lines were drawn, and the difference would come silently off the bottom of the board. Both
## sides read this one answer instead.
##
## **NEITHER PICKER EVER SHRINKS.** The whole objection to fitting the pair into a narrow dock by
## trimming them was that a truncated crop name reads as a word; so the widths are fixed and the LINE
## COUNT is what gives.
##
## `line_width` is the width the strip actually gets — `BandPanelController._queue_settings_line_width`
## derives it once from the zone box less the strip's own chrome and feeds the same answer to the
## reservation and the builder.
static func queue_settings_one_line(line_width: float) -> bool:
    return line_width >= queue_settings_one_line_width()

## **WHAT ONE LINE COSTS — the two keys, the two pickers, AND THE WITHDRAWAL RIDING THE LAST LINE.**
## One expression, exported so nothing re-spells it: the predicate above and every report of *how far
## short this dock is* read the same number, and a harness that added the terms up itself would be
## asserting its own arithmetic.
##
## ⛔ **THE `✕`'s WIDTH IS A TERM HERE BECAUSE THE `✕` IS ON THIS LINE** (§4.7b ③). It left the row
## when the reorder arrows took that column, and it lands right-aligned on the strip's LAST control
## line — so a predicate that still priced two keys and two pickers alone would say ONE LINE at a
## width where the pair plus the button does not fit, and the withdrawal would be squeezed off the
## right edge of a zone that clips. The separations are one per gap: 3 between the four settings
## controls, and a fourth before the button.
static func queue_settings_one_line_width() -> float:
    return BUILD_QUEUE_SETTINGS_KEY_WIDTH + float(WORK_ROW_SEPARATION) \
        + BUILD_QUEUE_CROP_WIDTH + float(WORK_ROW_SEPARATION) \
        + BUILD_QUEUE_SETTINGS_KEY_WIDTH + float(WORK_ROW_SEPARATION) + BUILD_QUEUE_KIT_WIDTH \
        + float(WORK_ROW_SEPARATION) + BUILD_QUEUE_UNQUEUE_WIDTH

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
##
## **IT NAMES THE KIT FIRST BECAUSE EVERY ENTRY HAS ONE** (`docs/plan_standing_upkeep.md` §4.7a ②).
## The crop is the plant web's alone — a `Tame` commits no species — so a hint promising only the crop
## was false on every animal row the moment the kit made those rows expandable.
const BUILD_QUEUE_ROW_OPEN_HINT := "Click to set this job's tools and crop."

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

## Valued the entry's RANK IN THE BAND'S OWN QUEUE — its index in the list as drawn — so a harness
## can tell the head from the rest without reading the marker it is trying to assert.
## `NOT_IN_ANY_BUILD_QUEUE` on a row the band's wire queue does not carry (a declaration still
## crossing the round trip), which is the client's one meaning of *pending* here.
##
## ⛔ **IT IS NOT `ForagePatchState.buildQueuePosition`, AND IT WAS** (`docs/plan_standing_upkeep.md`
## §4.9 item 9a). That field is published per SOURCE and rides the WINNING band, so on a source two
## bands hold it is another band's place in another band's line — a value every reader of this meta
## was reading as *this* band's rank. It is a readout of the source, never a rank, and the block no
## longer puts it on a node.
const BUILD_QUEUE_ROW_META := "build_queue_row"

const BUILD_QUEUE_OVERFLOW_META := "build_queue_overflow"

const BUILD_QUEUE_MARKER_META := "build_queue_marker"

const BUILD_QUEUE_FACE_META := "build_queue_face"

const BUILD_QUEUE_DATE_META := "build_queue_date"

## **THE `✕` LIVES IN THE SETTINGS STRIP NOW**, not on the row — the arrows took its column. The meta
## did not move with it: it is still the withdrawal control's handle, still valued the entry's rank,
## and every harness that finds it by this name finds the same button in a new host.
const BUILD_QUEUE_UNQUEUE_META := "build_queue_unqueue"

## The row's two reorder arrows, each valued the POSITION its press would send — so an assertion can
## read what the button would do without pressing it, and a disabled one still states its intent.
const BUILD_QUEUE_PROMOTE_META := "build_queue_promote"

const BUILD_QUEUE_DEMOTE_META := "build_queue_demote"

## The settings strip's two controls, each valued its own selected id — so an assertion can name the
## picker it wants rather than taking whichever `OptionButton` the strip happens to build first.
const BUILD_QUEUE_CROP_PICKER_META := "build_queue_crop_picker"

const BUILD_QUEUE_KIT_PICKER_META := "build_queue_kit_picker"

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
## draws — the shape the work board's own inspector term used to have in this block's arithmetic,
## before §4.9 item 12d took that strip out of the zone entirely and left this the only expansion in
## the column that costs it anything.
##
## **THEY ARE THE STRIP'S TWO INPUTS RATHER THAN ITS HEIGHT, so the number still lives in one place.**
## It was a lone BOOL for exactly that reason — a caller passing a float could pass a different one
## from the strip's own — and a strip that also lists an entry's LEGS has a height that varies, so
## what a caller states is the CONTENT and `build_queue_settings_height` remains the one arithmetic
## both the reservation and the render read.
static func build_queue_block_height(entries: int, rows_max: int,
        settings_legs: int = 0, settings_crop: bool = false, settings_kit: bool = false,
        settings_one_line: bool = true) -> float:
    if entries <= 0:
        return 0.0
    var rows := mini(entries, rows_max)
    if entries > rows_max:
        rows += 1
    return ZONE_HEAD_HEIGHT + float(rows) * WORK_ROW_HEIGHT \
        + build_queue_settings_height(settings_legs, settings_crop, settings_kit, settings_one_line)

# ---- THE EXPANSION — the whole queue over the whole Work zone (§4.9 item 9c) ---------------------------
#
# The block above is a SUMMARY: what the builders pool is funding, and what is next. It draws three
# entry rows and a `+N more`, and the queue itself has no cap — so a fourth job was funded with no
# row, and nothing past the third could be seen, reordered or withdrawn from the UI at all. The
# expansion is that list, in full, and it is a MODE rather than a widening: the 3-row block is
# untouched, and the mode spends NOTHING while it is closed.
#
# **WHAT IT DOES NOT DRAW, and why a stub board was rejected.** The source board, the filter chips
# that filter it, the pager that pages it and the work inspector that inspects a row of it all go —
# the chips and the pager say nothing without the list they act on, and a board squeezed to one or
# two rows is neither a list a player can use nor free. The work HEAD stays (the player must know
# which zone this is) and the POOLS block stays directly above the list it funds, which is the whole
# reason §4.7 moved keeping onto this tab.

## The disclosure glyph on the BUILD QUEUE head, which is the toggle BOTH ways — `+N more` is a second
## door IN only, the expanded view having no overflow row left to press.
##
## **`▾` / `▴` RATHER THAN THIS FILE'S OTHER CARET PAIR.** `DetailFormat.BREAKDOWN_CARET_*` and
## `hud_crafting_vocab.GROUP_HEAD_CARET_*` fold with `▾`/`▸`, and `▸` is already
## `BUILD_QUEUE_HEAD_MARKER` two rows below this head — the entry the builders pool is standing on.
## One glyph meaning *folded* on the head and *funded* on a row of the same block is a collision this
## block cannot afford, so the pair is `hud_event_vocab`'s `CARET_DOWN` / `CARET_UP` instead: down to
## open the list downward, up to fold it back.
const BUILD_QUEUE_DISCLOSURE_COLLAPSED := "▾"

const BUILD_QUEUE_DISCLOSURE_EXPANDED := "▴"

## It rides the head's own type size — it is part of the title, not a control beside it — and takes
## its width out of the head's EXPANDING spacer rather than off the right-hand readout, which states
## the builders count and their kit and may not give up a character.
const BUILD_QUEUE_DISCLOSURE_FONT_SIZE := ZONE_HEAD_FONT_SIZE

const BUILD_QUEUE_DISCLOSURE_TOOLTIP := "Show the whole queue over the Work board, or fold it back to the top three and the sources."

## The head row's own meta, so a frame can find the toggle and press it where the player presses it.
## Valued the EXPANDED flag, so the glyph and the state cannot be asserted apart.
const BUILD_QUEUE_DISCLOSURE_META := "build_queue_disclosure"

## **THE THIRD AND LAST SANCTIONED `ScrollContainer` IN THIS PANEL** (`PARTIES_LIST_NAME` under
## `ZONE_PARTIES`, `BAND_ZONE_SCROLL_NAME` under `ZONE_BAND`, this one under `ZONE_WORK`).
##
## It is safe for the identical reason the other two are: a `ScrollContainer` reports no minimum on
## its scrolling axis, so what the list holds never reaches the zone's reservation, and what it DOES
## report is a fixed number `build_queue_expanded_scroll_height` declares from the zone's own BOX —
## geometry the panel states rather than anything the snapshot says.
##
## ⛔ **ITS SANCTION IS CONDITIONAL WHERE THE OTHER TWO ARE UNCONDITIONAL.** It must exist EXACTLY
## when the queue is expanded and never otherwise — the collapsed zone is the paged, no-scroll board
## the whole zone model is built on, and "no stray scroll" is a claim satisfied by a panel that never
## expands. Both halves are asserted.
const BUILD_QUEUE_EXPANDED_SCROLL_NAME := "BuildQueueList"

## **WHAT THE EXPANDED LIST'S VIEWPORT IS DECLARED AT — the zone's box less everything above it.**
## One arithmetic, in the same shape `_work_board_capacity` charges its chrome: the work head, the
## POOLS block, the queue's own head, and the two block separations the column puts between the three
## blocks (the head and the list share one block at separation 0, so there is no gap inside it).
##
## ⛔ **IT IS NOT CLAMPED UP TO A FLOOR.** A dock too short to hold this mode must FAIL the zone-fit
## assertion loudly, which is this zone's standing contract; a floor would turn that into a silent
## clip of the bottom row, since the zone `clip_contents`.
static func build_queue_expanded_scroll_height(box_height: float, pools_fund_mode: bool) -> float:
    return box_height - ZONE_HEAD_HEIGHT - pools_block_height(pools_fund_mode) - ZONE_HEAD_HEIGHT \
        - float(ZONE_BLOCK_SEPARATION) * BUILD_QUEUE_EXPANDED_GAP_COUNT

## The gaps above that viewport: work head → pools, pools → queue block. Named rather than spelled,
## the same way `BUILD_QUEUE_ROOM_GAP_COUNT` is, because it is a COUNT and not a height.
const BUILD_QUEUE_EXPANDED_GAP_COUNT := 2.0

# ---- EDGE AUTO-SCROLL, so a drag can reach past the viewport (§4.9 item 9c) ------------------------------
#
# A scrolling list whose drag cannot leave the viewport is a reorder that only works inside one
# screenful. The arrows do not care — they name a rank — but the drag does.

## The hot band at each edge of the expanded list's viewport. ONE ROW, so a pointer holding the row it
## is dragging over the last visible row is already in it: any smaller and the player has to aim at a
## strip thinner than the thing they are holding.
const BUILD_QUEUE_AUTOSCROLL_MARGIN := WORK_ROW_HEIGHT

## …and the speed, stated in ROWS PER SECOND rather than pixels per frame — the row is this list's
## unit, and a frame is not a quantity the design has an opinion about. Six rows a second crosses a
## nine-entry queue in a second and a half and is slow enough to stop on the row meant.
##
## ⛔ **A FRACTIONAL RATE NEEDS AN ACCUMULATOR.** `ScrollContainer.scroll_vertical` is an INT and
## 6 rows/s at 60fps is 2.8px a frame, so truncating per frame loses most of the travel;
## `_queue_autoscroll_carry` holds the remainder and is zeroed whenever the direction stops or flips.
const BUILD_QUEUE_AUTOSCROLL_ROWS_PER_SECOND := 6.0

## ⛔ **AND ONE TICK MAY NEVER MOVE THE LIST MORE THAN ONE ROW**, which is what this cap is stated as
## rather than as a number of seconds: at the rate above, one row IS `1 / rows-per-second` of a
## second. A hitch — a stalled frame, a resize, a harness capturing a PNG mid-gesture — hands the pump
## an arbitrarily long elapsed time, and a step of several rows teleports the drop target past the row
## the player was aiming at.
const BUILD_QUEUE_AUTOSCROLL_MAX_TICK_SECONDS := 1.0 / BUILD_QUEUE_AUTOSCROLL_ROWS_PER_SECOND

## The pump's clock is `Time.get_ticks_usec`, which is UNSCALED — see
## `BandPanelController._queue_autoscroll_tick` for why a frame delta is unusable — so it needs the
## one conversion. `SnapshotLoader` / `TurnProfile` carry a `USEC_PER_MSEC` for their own millisecond
## readouts; this rate is stated per SECOND, so it states its own.
const MICROSECONDS_PER_SECOND := 1_000_000.0

## **WHAT AN OPEN SETTINGS STRIP DRAWS AT — one expression, and `0.0` when there is nothing to open.**
## The crop picker costs its control row; each LEG of the entry's climb costs a line.
##
## **THE LEGS ARE IN THE STRIP BECAUSE A MULTI-LEG ENTRY IS ONE ROW** (`docs/plan_standing_upkeep.md`
## §2.8). A `sow` declared on untended ground is a two-leg climb, and splitting it into two queue rows
## would offer two `✕`s for one withdrawal and two places to drag for one reorder. So the entry stays
## one unit and its legs are what the row opens into.
## **THE LEG LIST'S OWN KEY COSTS A LINE, and forgetting it is how a strip draws taller than it was
## paid for** — which this zone answers by clipping the bottom of the BOARD, silently.
## **AND THE CONTROL ROWS COST ONE LINE OR TWO — never a measured height**
## (`docs/plan_standing_upkeep.md` §4.7b ②). The pair flows: where the strip is wide enough for both
## keys and both pickers (`queue_settings_one_line`) they sit side by side, and where it is not they
## stack. **A LONE CONTROL IS ALWAYS ONE LINE whatever the width** — an ANIMAL entry has a kit and no
## crop, so it has nothing to wrap against, and letting the predicate answer for it would reserve a
## second line for a strip that draws one.
##
## ⛔ **THE SECOND LINE COSTS A CONTROL, NOT A WHOLE STRIP.** There is ONE stylebox around the pair
## however they stack, so the wrapped height is `chrome + 2 × control` = 56 and never
## `2 × BUILD_QUEUE_SETTINGS_HEIGHT` = 68 — which reserved 12px the strip never draws, rendered as
## dead space inside it and cost the work board a row wherever that 12px straddled a boundary.
static func build_queue_settings_height(legs: int, has_crop: bool, has_kit: bool = false,
        one_line: bool = true) -> float:
    var controls := 0
    if has_crop:
        controls += 1
    if has_kit:
        controls += 1
    var height := 0.0
    if controls > 0:
        height = BUILD_QUEUE_SETTINGS_HEIGHT if (controls <= 1 or one_line) \
            else BUILD_QUEUE_SETTINGS_HEIGHT + BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT
    elif legs > 0:
        # **THE WITHDRAWAL RIDES THE LAST CONTROL LINE, so a strip with no pickers still buys one**
        # (§4.7b ③). Every queued entry has a KIT, so this branch does not fire on anything the sim
        # publishes today — but a strip that opened on legs alone and then drew a `✕` with no line
        # under it would draw taller than it was paid for, in a zone that answers that by clipping
        # the board. The reservation and the builder take the same branch.
        height = BUILD_QUEUE_SETTINGS_HEIGHT
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

## **THE SIX STATES, AS WORDS — every one of them, which is the invariant.** A track is read once, in
## a hurry, and a glyph vocabulary invented for it would be six more marks to learn beside the three
## the work row already carries. **A state with no word here renders as an EMPTY face**, and on the
## three selectable states that is a control with nothing on it that still sends a command, so a state
## added to `RungLadder` adds its word in this block.
const RUNG_TRACK_STATE_BANKED := "banked"
const RUNG_TRACK_STATE_STANDING := "where you are"
const RUNG_TRACK_STATE_PATH := "on the way"
const RUNG_TRACK_STATE_TARGET := "the target"
const RUNG_TRACK_STATE_LOCKED := "locked"
## …and its MIRROR, which is the pair this word is chosen for: a destination the branch admits reads
## `open` exactly where a refused one reads `locked`, one column apart on the same card. It is the
## face a selectable rung falls back to where the wire prices no such job on this source — the state
## being the whole of what is known there — and without it that row rendered as a **blank button that
## still issued a `tame`/`corral` declaration on press.**
const RUNG_TRACK_STATE_OPEN := "open"

## `75 work · ≈12 turns` — what a selectable destination's own leg still owes and when the sim says it
## lands. **The turns half renders only where the wire dates the leg**, which is when an entry is
## already climbing this branch; a rung nobody has queued has no chained date and states the work
## alone rather than a number this surface has no right to.
const RUNG_TRACK_COST_FORMAT := "%s work · %s"

const RUNG_TRACK_COST_UNDATED_FORMAT := "%s work"

# ---- THE PRICE ASIDES — the material half of a rung's price (`docs/plan_standing_upkeep.md` §2.7) --
#
# **WORK WAS NEVER THE WHOLE PRICE, AND THE RIGHT-HAND FACE HAS NO ROOM TO SAY SO.** A rung's face is
# already `75 work · ≈12 turns` in a 292px card; a fence's six hurdles and its 0.05-a-turn mending
# bill are two more statements about the same rung, so they take the ASIDE shape a locked rung's
# reason and a crop row's payoff face already use — beneath the row, wrapped, in the quiet ink.
#
# ⛔ **THEY ARE NOT REFUSALS AND DO NOT RIDE `ROW_REASONS_KEY`.** That array means *why this rung is
# refused*; a price is not a refusal. A LOCKED rung keeps its reason **and** states its price, which
# was settled deliberately: hiding the price behind the refusal means a player who has never made a
# hurdle cannot see what a pen would cost, so they cannot plan toward one.
#
# ⛔ **AND THEY ARE THE `⌃` TRACK'S, NOT THE COMPOSE SHEET'S.** Foraging and hunting have no hold cost
# to state; the improvement is chosen from the work row's `⌃`, which is why this whole readout lives
# on one surface.

## `+ 6 hurdles to raise it` — the WHOLE pile the rung swallows, at full coverage, drawn as the meter
## climbs. Rendered only where the wire quotes a pile, which is the rung DIRECTLY ABOVE where the
## source stands and no other: `buildMaterialCost` prices one rung, so a row two rungs up states
## nothing rather than repeating the rung below it.
const RUNG_TRACK_BUILD_MATERIAL_FORMAT := "+ %s to raise it"

## `you have 2 hurdles — it will stall at about a third` — the store against the pile, on the good
## that binds. **A short store STALLS a build and never refuses it** (the ladder's own rule): the
## coverage fraction scales the work banked and the materials drawn together, so the honest warning is
## *how far this gets* rather than *you may not*.
##
## **THE BINDING GOOD IS NAMED, NEVER A TOTAL.** With two goods in a pile the fraction is the WORST of
## them and the clause names that one — summing across goods is the currency this model does not have.
const RUNG_TRACK_STALL_FORMAT := "you have %s — it will stall at about %s"

## The fraction in words, and the ladder is the words' OWN values — the rung chosen is simply the
## nearest of them, so there is no threshold table to tune and no boundary to get wrong. Below the
## lowest rung the answer is `RUNG_TRACK_STALL_BARELY`, because *a tenth* overstates a store that
## covers a fortieth.
const RUNG_TRACK_STALL_STEPS := [
    [0.10, "a tenth"],
    [0.25, "a quarter"],
    [0.33, "a third"],
    [0.50, "half"],
    [0.67, "two thirds"],
    [0.75, "three quarters"],
]

## What the fraction reads as under the lowest named rung — a store that would buy almost none of the
## pile. Half the smallest named fraction is the cut, which is that fraction's own nearest-neighbour
## boundary rather than a dial: a store under it is nearer to nothing than to a tenth.
const RUNG_TRACK_STALL_BARELY := "barely at all"

## `then 1 work · 0.05 hurdles a turn to hold` — the STANDING price of the rung being offered, beside
## the one-off pile above it. **Every rung above the standing one states it, materials or not**: what
## a player is agreeing to is a one-off cost AND a bill for as long as the thing stands, and the
## one-off figure on the row cannot say the second half.
##
## **THE WORK TERM IS THE RUNG'S OWN RATE** (`SourceForecast.build_upkeep_demand`) — the per-rung pair
## the compose sheet's `BUILD_PRICE_UPKEEP_FORMAT` quotes, at every fullness, not the bill this source
## was handed this turn. It is rendered through `DetailFormat.format_work_units`, so the track and the
## sheet print one rate one way.
const RUNG_TRACK_HOLD_FORMAT := "then %s a turn to hold"

const RUNG_TRACK_HOLD_WORK_TERM := "%s work"

## ONE MATERIAL TERM — `0.05 hurdles`, `6 hurdles`. **A material names itself** (the rule
## `SourceForecast.PICKER_MATERIAL_PRODUCT_FORMAT` states: the catalogue ships no display name, so the
## id IS the word), and the amount is trimmed rather than fixed-width so a whole pile reads `6` while
## a mending rate reads `0.05`.
const RUNG_TRACK_MATERIAL_TERM := "%s %s"

## What separates two goods in one clause — the client's standing clause joiner, so `6 hurdles · 2
## rope` reads exactly as every other multi-account run in this HUD does.
const RUNG_TRACK_PRICE_SEPARATOR := " · "

## The decimals a material amount is printed to before its trailing zeros come off. Two, which is what
## every other material figure in this client prints at (`FLORA_CROP_MATERIAL_CLAUSE_FORMAT`, the
## picker's product line) and is one step finer than the shipped pen's 0.05-a-turn fence bill.
const RUNG_TRACK_MATERIAL_DECIMALS := 2

# ---- THE CROP STEP — the second page of the same card (`docs/plan_standing_upkeep.md` §2.8) --------
#
# **A PLANT RUNG DOES NOT COMMIT UNTIL A CROP IS NAMED.** The `⌃` used to declare in one click and
# send no species token, so every Sow took the sim's default — the HIGHEST-SHARE legal plant, which
# considers neither what it pays nor the player's take selection — and that is how fertile ground got
# committed to a zero-food cash crop. Picking the rung now opens this step and the CROP is the
# declaration.
#
# ⛔ **THE ROWS STATE WHAT EACH CROP PAYS, and that is the actual repair.** Forcing the choice only
# relocates the trap if the list is names and shares: the player picks the dominant plant again,
# because it looks like the obvious answer. Nothing on the path from *this ground is fertile* to
# *this field feeds nobody* states the zero unless a row does.
## ⛔ **CULTIVATE GROWS NOTHING, AND THE TITLE MAY NOT SAY IT DOES.** The tended rung **weeds**: the
## favored species' share rises toward `tended_weeding_gain` and the volunteers standing beside it
## are still wild, so nothing is planted and the choice is which plant this band gets good at —
## `tended_conversion_gain` multiplies that one species' whole yield vector and nothing else's. Only
## the FIELD rung plants, forcing the favored share to 1.0 and every other to 0, which is where
## *grow* becomes the true word. Reported from play as a nit on exactly that distinction, and it is
## the model rather than the wording: a player told they are choosing what to GROW on a Cultivate has
## been told the rung does something it does not do.
const RUNG_CROP_TITLE_TEND := "WHAT TO TEND"

const RUNG_CROP_TITLE_GROW := "WHAT TO GROW"

## **`Sim picks` STAYS AN OPTION AND STOPS BEING THE DEFAULT.** `""` is a real instruction on the wire
## — *take the tile's dominant legal plant* — and choosing it deliberately is fine. It is rendered
## LAST rather than first (a leading default is the thing a hurried player takes) and its aside names
## the plant it would actually resolve to, so it is no quieter about the consequence than any other
## row.
const RUNG_CROP_SIM_PICKS_LABEL := "Sim picks"

## …and what that pick would land on, stated in the row's own aside slot.
const RUNG_CROP_SIM_PICKS_NOTE_FORMAT := "→ %s"

## Back to the rung list. The card is a Window and dismisses on a click outside, but a step the player
## cannot leave without losing the rung they picked is a step they will avoid using.
const RUNG_CROP_BACK_LABEL := "‹ Back"

## A patch whose basket carries no plant this rung may legally take. The rung is still offered — the
## sim accepts a Sow with no species token and settles it itself — so this states the fact rather than
## refusing the climb.
const RUNG_CROP_NONE_NOTE := "No plant here can climb this rung — the sim will settle it."

## The card's stable handles. Every claim the track owes is a string composed at render time, so a
## harness that found a row by its text would only confirm the string it had already assumed.
const RUNG_TRACK_META := "rung_track"
## The crop step's own row handle, valued the SPECIES key the press would send (`""` for `Sim picks`,
## which is a real instruction and not an absent one). Spelled apart from `RUNG_TRACK_ROW_META` so a
## harness asking *which rung* can never be answered by a crop row that happens to be on screen.
const RUNG_CROP_ROW_META := "rung_crop_row"
## …and the step itself, so *is the card showing rungs or crops* is one read.
const RUNG_CROP_STEP_META := "rung_crop_step"
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

