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

# Policy-picker layout: the compacted glyph+metric buttons wrap 3 per row so the six-rung
# forage/local-hunt pickers read as two tidy rows of three instead of one over-wide row. A picker
# with at most POLICY_PICKER_MAX_SINGLE_ROW rungs (the 4-rung expedition launch/compose picker) stays
# a single row instead — a 3+1 grid would strand a lone one-third-width button on a second row.
const POLICY_PICKER_COLUMNS := 3

const POLICY_PICKER_MAX_SINGLE_ROW := 4

# Passed for `columns` to keep `HudWidgets.build_policy_picker`'s width-driven default — a caller that only wants
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

const SCOUT_ROLE_HINT := "Posts scouts that see around obstacles — more scouts range farther. Staff with −/+."

const WARRIOR_ROLE_HINT := "Guards the band — matters once threats arrive."

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

const ZONE_HEADER_WORK := "Work"

const ZONE_HEADER_PARTIES := "Parties"

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

## The band zone yields by TIERS as its box shrinks — the zone height is fixed, so the CONTENT gives
## way, never the layout (nothing here scrolls, and a clipped chart teaches nothing).
## At/above TALL: the full-height food-outlook chart and hinted role cards.
## Between CHART_MIN and TALL: a compact chart.
## Below CHART_MIN (a 360px T/B dock): no chart at all, and the role cards drop their hint line to a
## tooltip — the two biggest blocks, given up in the order they are least missed.
## All measured against the zone BOX, never against the dock edge.
const BAND_ZONE_TALL_MIN_HEIGHT := 420.0

const BAND_ZONE_CHART_MIN_HEIGHT := 340.0

const FOOD_CHART_COMPACT_HEIGHT := 42.0

## The three tiers as an ordinal, so `zones_resized` can tell a mere re-page (the work board) from a
## band-zone tier change (which needs the zone rebuilt, not re-paged).
const BAND_ZONE_TIER_SHORT := 0

const BAND_ZONE_TIER_COMPACT := 1

const BAND_ZONE_TIER_TALL := 2

## WORKFORCE readout + segment keys.
const WORKFORCE_IDLE_FORMAT := "%d idle of %d"

const WORKFORCE_KEY_FORAGE := "Forage"

const WORKFORCE_KEY_HUNT := "Hunt"

const WORKFORCE_KEY_ROLES := "Roles"

const WORKFORCE_KEY_PARTIES := "Parties"

const WORKFORCE_KEY_IDLE := "Idle"

## Standing-role CARDS (the fix for roles reading as one more worked source in a list).
const ROLE_NAME_SCOUT := "Scout"

const ROLE_NAME_WARRIOR := "Warrior"

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

## …plus the standing-investment line (`WORK_INSPECT_STANDING_INVESTMENT_FORMAT`), which only renders
## on a source standing on an investment rung. One `ALLOC_SECTION_FONT_SIZE` line and its separation.
const WORK_INSPECTOR_STANDING_LINE_HEIGHT := 22.0

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

const WORK_SORT_YIELD := &"yield"

const WORK_SORT_NAME := &"name"

const WORK_CHIP_ALL_FORMAT := "All %d"

const WORK_CHIP_KIND_FORMAT := "%s %d · %s"

const WORK_CHIP_ATTENTION_FORMAT := "⚠ %d"

const WORK_CHIP_TOOLTIP := "Filter the board to these sources."

const WORK_SOURCES_FORMAT := "%d sources"

const WORK_TOTAL_TOOLTIP := "Total food per turn from every worked source."

const WORK_MENU_TOOLTIP := "Sort and bulk actions for worked sources."

const WORK_MENU_SORT_YIELD := "Sort by yield"

const WORK_MENU_SORT_NAME := "Sort by name"

const WORK_MENU_UNASSIGN_FORMAT := "Unassign all work (%d)"

const WORK_UNASSIGN_CONFIRM_FORMAT := "Return all %d sources' workers to idle? Standing roles and parties are untouched."

const WORK_UNASSIGN_CONFIRM_OK := "Unassign all"

const WORK_ROW_FORAGE_FORMAT := "Forage (%d, %d)"

const WORK_ROW_HUNT_FORMAT := "Hunt %s"

const WORK_ROW_OPEN_HINT := "Click the row for detail and actions."

const WORK_EMPTY_HINT := ALLOC_NO_SOURCES_HINT

## The inspector strip (the row's second/third lines, relocated to one place).
const INSPECTOR_CLOSE_GLYPH := "✕"

const INSPECTOR_CLOSE_TOOLTIP := "Close detail"

const WORK_INSPECT_JUMP := "Jump to source"

const WORK_INSPECT_POLICY := "Change policy"

const WORK_INSPECT_UNASSIGN := "Unassign"

const WORK_INSPECT_OVERDRAW_LINE := "⚠ Overdraws the source at this policy."

const WORK_INSPECT_ASSIGNED_FORMAT := "%d assigned"

const WORK_INSPECT_SENTENCE_SEPARATOR := " · "

## The inspector's picker offers the four EXTRACTIVE rungs only — the INVESTMENT rungs are ladder
## COMMITMENTS made at the source's own compose control, where their gates and payoff forecasts live.
## So a source STANDING on an investment rung highlights none of the four, which without a word reads
## as an unset control on a very-much-set assignment. These two say what is actually true: the rung
## it stands on, and that picking here ENDS it (a part-built pen/field is discarded, not paused).
const WORK_INSPECT_STANDING_INVESTMENT_FORMAT := "Currently %s — picking a rung here ends it."

const WORK_INSPECT_END_INVESTMENT_CONFIRM_FORMAT := "End %s on %s and take at %s instead? The work done toward it is lost."

const WORK_INSPECT_END_INVESTMENT_CONFIRM_OK := "End it"

const PAGER_PREV_GLYPH := "‹"

const PAGER_NEXT_GLYPH := "›"

const PAGER_PREV_TOOLTIP := "Previous page"

const PAGER_NEXT_TOOLTIP := "Next page"

const PAGER_FORMAT := "Page %d / %d"

const PAGER_RANGE_FORMAT := "%d–%d of %d"

## `cancel_order` scopes (the server grammar: `cancel_order <faction> [band] [all|work|roles]`).
## `work` clears Forage + Hunt only — standing roles, parties and an in-progress move survive.
## A policy picker rendered INSIDE a zone wraps to this many columns — four rungs abreast do not fit
## a 380px L/R dock, and a picker wider than its zone drags the whole zone column past its host.
const ZONE_POLICY_PICKER_COLUMNS := 2
