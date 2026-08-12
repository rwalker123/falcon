class_name HudSelectionVocab

## Selection-card vocabulary — roster row chrome, condition chips, the LAND/HERD row meta, the
## subject drawer geometry, the Move-band button + Band/City pointer, and the activity glyphs.

# Band-status alert types, ordered high → low priority (rendered in this order).
const BAND_ACTIVITY_IDLE := "idle"

# Occupants roster row chrome.
const ROSTER_DOT_SIZE := 9.0

const ROSTER_ROW_MIN_HEIGHT := 30.0

const ROSTER_ROW_SEPARATION := 8

const ROSTER_ROW_H_PADDING := 10.0

const ROSTER_ACCENT_WIDTH := 3.0

const ROSTER_HEADER_FONT_SIZE := 10

# The leading MARK on a land / herd row — the species or site's bundled art where the client has any
# (issue #439), the emoji where it does not. A square box, sized to sit inside `ROSTER_ROW_MIN_HEIGHT`
# (30) with the row's vertical padding intact rather than to fill it.
const ROSTER_ROW_ICON_BOX := 18.0

# The emoji FALLBACK's size. It matches the size the row's own name label renders at — the stock
# default, this client applying no `Theme` — because the glyph used to live INSIDE that label, and
# splitting it out must not resize it.
const ROSTER_ROW_ICON_FONT_SIZE := 16

# Fallback glyph for the land row on a tile carrying no food module. Text-presentation (the line-art
# policy in `FoodIcons`), so unlike an emoji it DRAWS in whatever `font_color` its label carries.
# That colour is now applied EXPLICITLY — `SelectionCardController._roster_row_ink` hands it to
# `HudWidgets.build_marker_icon` at build time and re-applies it on the in-place patch path — because
# the mark is its own bare `Label` since issue #439 and inherits nothing (this client applies no
# `Theme`). Left un-set it would render stock near-white beside an `INK_DIM` name.
const LAND_ROW_GLYPH := "◈"

# Land-row meta, shortest true form: workers on it · else the module it offers · else nothing.
const LAND_META_WORKERS_FORMAT := "%d %s"

const LAND_META_NO_FORAGE := "No forage"

# Herd-row meta: the same `<count> <activity glyph>` form the land row uses, so a hunted herd
# (`1 🏹`) and a foraged hex (`2 🌾`) state their staffing identically down the subject list.
const HERD_META_WORKERS_FORMAT := "%d %s"

# Chip strip font: one notch under the row labels — a chip is a standing condition, not a heading.
const CHIP_FONT_SIZE := 11

# Tag chips are skipped when the tile reports this literal (the `tags_text` "no tags" value): an
# absent condition earns no chip, exactly as it earns no row.
const CHIP_TAGS_NONE := "none"

# The drawer's floor. Below this a compose block is unreadable, so the card is allowed to push the
# dock into its own scroll rather than crushing the controls the player came here to use.
const SUBJECT_DRAWER_MIN_HEIGHT := 180.0

const SUBJECT_DRAWER_BOTTOM_MARGIN := 12.0

# The list ↔ drawer rule: one hairline, the same weight `header_stylebox` draws under a card title.
const SUBJECT_DIVIDER_HEIGHT := 1.0

# A selected PLAYER band's detail lives in the dockable Band/City panel, so its drawer here would
# otherwise be a blank gap. Say where it went instead.
const BAND_PANEL_POINTER_TEXT := "Labor allocation is in the Band / City panel."

# …but REPOSITIONING is a map action, and the player is already on the map with this hex open, so
# Move stays in the drawer beside the pointer (§18). Same words as the Band/City panel's own Orders
# Move — one order, one name.
const MOVE_BAND_BUTTON_TEXT := "Move"

const MOVE_BAND_BUTTON_TOOLTIP := "Relocate the band, then click a destination tile."

# Per-activity glyph for a player band's roster row. `activity` is the kind with the
# most workers (Early-Game Labor): idle | forage | hunt | scout | warrior.
const ACTIVITY_GLYPHS := {
    "idle": "·",
    "forage": "🌾",
    "hunt": "🏹",
    "scout": "🧭",
    "warrior": "🛡",
}

# ---- THE BUILD READOUT — a rung's price in WORK, and the turns that price buys -------------------
# `docs/plan_unit_costed_work.md` §11. An improvement declares a fixed size in WORK UNITS, a crew
# produces work units per turn, and **TURNS ARE THE OUTPUT** — so the same percentage fills at
# different speeds on different rungs, and fills faster as you add hands, hold a higher escapement
# floor or carry better tools. A bare `Cultivation: 42%` was a complete statement while every rung
# cost the same 25 turns; it is not one now, and none of that is visible without these lines.
#
# The copy lives HERE, with the selection card and its subject drawer, because that is where both
# build meters render — the tile card's plant rungs and the herd drawer's animal ones. The compose
# sheet's pre-commit quote is `HudComposeVocab`'s, composed through the same shared formatters.

# `Preparing 18 / 50 work (42%)` — the verb the row already led with, then the absolutes, then the
# percentage the meter has always shown. **The percentage is NOT re-derived from the absolutes**: the
# wire ships the fraction and the pair separately, and dividing here would be a second opinion about
# one meter.
const BUILD_METER_WORK_FORMAT := "%s %s / %s work (%d%%)"

# The row as it always was, for a source the wire prices no such job on. Not a missing-field path: a
# source can carry a meter for a rung it states no cost for, and `18 / 0 work` reads as a defect
# where a bare percentage reads as an unpriced job.
const BUILD_METER_PERCENT_FORMAT := "%s %d%%"

# Work units read WHOLE where they are whole (`50`) and to one place where they are not (`17.6`).
# The shipped costs are integers and a `50.0` beside them claims a precision the config lacks — the
# rule `_format_danger_scalar` already follows for the same reason.
const BUILD_WORK_DECIMALS := 1

# `≈11 turns at this crew` — the readout that makes "turns are an OUTPUT" legible: add hands and
# watch it fall. **The trailing clause is load-bearing.** Without it the number reads as a property
# of the rung, which is the fixed-turn model this arc exists to replace.
#
# **Both forms are SPELLED FROM THE TAIL** rather than each carrying its own copy of it, so a needle
# for "any turn estimate at all" is one string that cannot go stale — which is what the `-1` rule
# needs asserting: a row reading `≈-1 turns` is exactly the failure, and a needle built from a
# specific count passes straight over it (measured — the first cut of that assertion did).
const BUILD_TURNS_ROW_TAIL := "at this crew"

const BUILD_TURNS_ROW_FORMAT := "≈%d turns " + BUILD_TURNS_ROW_TAIL

# …and its singular, so a build one turn out does not read `≈1 turns at this crew`.
const BUILD_TURNS_ROW_ONE := "≈1 turn " + BUILD_TURNS_ROW_TAIL

# `your gear: −8.5 work off this job` — what the crew's tools took off the COST, in the units the
# cost is quoted in. It is the only way a player can tell a tool is worth carrying to a garden and
# not to a farm: the contribution is a fixed number of units against a job whose size is not, so its
# share shrinks as the job grows. Rendered only above zero — a `−0` advertises nothing. The minus is
# U+2212 MINUS SIGN, the stepper faces' own rule.
const BUILD_GEAR_WORK_ROW_FORMAT := "your gear: −%s work off this job"
