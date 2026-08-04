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
