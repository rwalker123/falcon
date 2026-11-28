class_name HudTribeVocab

## **THE TRIBE ON SCREEN — wire keys, words and sizes** (`docs/plan_labor_at_scale.md` → "The tribe on
## screen", issue #863). One home for the phrases the tribe page, the `Band:` picker's fold, the
## Materials & Crafting entry and the orb's proposal row all say, so a phrase is typed once.

# ---- wire keys: the cohort dict of an own member band -----------------------------------------------
const KEY_TRIBE_ID := "tribe_id"
const KEY_TRIBE_NAME := "tribe_name"
## Member band ids, parallel to `tribe_member_names`, largest first.
const KEY_MEMBER_BANDS := "tribe_member_bands"
const KEY_MEMBER_NAMES := "tribe_member_names"
const KEY_LEVY_CEILING := "tribe_levy_ceiling"
const KEY_WORKS := "tribe_works"
const KEY_PROPOSAL := "tribe_proposal"
const KEY_SCOUTING_RANGE := "tribe_scouting_range"
const KEY_HAS_ANCHOR := "has_belief_anchor"
const KEY_ANCHOR_X := "belief_anchor_x"
const KEY_ANCHOR_Y := "belief_anchor_y"
const KEY_STAGE_ID := "tribe_settlement_stage_id"
const KEY_STAGE_ICON := "tribe_settlement_stage_icon"

# ---- wire keys: a `labor_assignments` row ----------------------------------------------------------
const ROW_FOR_TRIBE := "for_tribe"
const ROW_SCOUTING_STRETCH := "scouting_stretch"
const ROW_SCOUTING_REACH := "scouting_reach"
const ROW_KIND := "kind"

# ---- wire keys: one `tribe_works` entry ------------------------------------------------------------
const WORK_X := "x"
const WORK_Y := "y"
const WORK_BRANCH := "branch"
const WORK_TARGET_RUNG := "target_rung"
const WORK_PRIORITY := "priority"
const WORK_BANDS := "bands"
const WORK_WORKERS := "workers"

# ---- wire keys: `tribe_proposal` --------------------------------------------------------------------
const PROPOSAL_KEY := "key"
const PROPOSAL_KIND := "kind"
const PROPOSAL_TEXT := "text"
const PROPOSAL_TILE_X := "tile_x"
const PROPOSAL_TILE_Y := "tile_y"
const PROPOSAL_FOR_BAND := "for_band"

## `tribe_id` of a band in no tribe.
const NO_TRIBE := 0

# ---- the levy ceiling: wire value, command token, on-screen word ------------------------------------
## `tribe_levy_ceiling` is 0 for a band in no tribe and 1..4 for the four settings.
const LEVY_NOT_IN_TRIBE := 0
const LEVY_LEVELS: Array[Dictionary] = [
	{"ceiling": 1, "token": "none", "label": "None"},
	{"ceiling": 2, "token": "low", "label": "Low"},
	{"ceiling": 3, "token": "normal", "label": "Normal"},
	{"ceiling": 4, "token": "high", "label": "High"},
]
const LEVY_HEAD := "Levy"
const LEVY_TOOLTIP := "How much of each band's spare hands the tribe may take for its works and its scouts. A band's own sources are always kept up first."

# ---- command shapes (formatted by `Main.format_tribe_command`) --------------------------------------
const COMMAND_WORK := "work"
const COMMAND_STOP := "stop"
const COMMAND_LEVY := "levy"
const ACTION_KEY := "action"

# ---- the page's words -------------------------------------------------------------------------------
const TITLE_FORMAT := "%s"
const SUBTITLE_FORMAT := "Tribe · %d bands · gathering place (%d, %d)"
const SUBTITLE_NO_ANCHOR_FORMAT := "Tribe · %d bands"
const ZONE_TAB_TRIBE := "Tribe"
const BANDS_CHIP_FORMAT := "%d bands"
const FOOD_ROW_FORMAT := "Food: %d  (%s)"
const FOOD_NET_FORMAT := " · [color=#%s]%s[/color]"
const SCOUT_HEAD := "Scouting range"
const SCOUT_EDGE_FORMAT := "%d scouts ride the tribe's edge"
const SCOUT_EDGE_ONE := "1 scout rides the tribe's edge"
const SCOUT_SHORT_FORMAT := "%s's stretch sees +%d of +%d tiles"
const SCOUT_READONLY_FORMAT := "+%d tiles · set by the tribe"
const SCOUT_READONLY_TOOLTIP := "One range for the whole tribe, set on the tribe's page."
const FOR_TRIBE_MARK := "for the tribe"
const FOR_TRIBE_TOOLTIP := "These hands were levied by the tribe, not set by this band."

# ---- the Work tab -----------------------------------------------------------------------------------
const BANDS_HEAD := "Bands"
const WORKS_HEAD := "Shared works"
const NO_WORKS_LINE := "No shared works yet. Start one from a tile's card."
const JOBS_FORMAT := "%d jobs"
const JOBS_ONE := "1 job"
const FOR_TRIBE_FORMAT := "%d for the tribe"
const IDLE_FORMAT := "%d idle"
const SUB_LINE_SEPARATOR := " · "
const NO_JOBS_LINE := "Nothing yet"
const CREW_HOME := "at home"
const CREW_TRIBE := "for the tribe"
const CREW_IDLE := "idle"
const CREW_TIP_FORMAT := "%s: %d at home, %d for the tribe, %d idle"
const FOLD_OPEN_TIP := "Show this band's work"
const FOLD_CLOSED_TIP := "Hide this band's work"
const OPEN_BAND_TIP := "Open %s's own panel"
const WORK_NAME_FORMAT := "%s (%d, %d)"
const HANDED_ONE_FORMAT := "handed to %s"
const HANDED_MANY_SEPARATOR := ", then "
const WORKERS_FORMAT := "%d workers"
const WORKERS_ONE := "1 worker"
const STOP_LABEL := "✕ Stop"
const STOP_TIP := "Stop this shared work"
## `stop` rows address the work by its tile and branch.
const BREADCRUMB_TIP := "Back to the tribe's page"
## Larger than the header's 10px subtitle, so the crumb reads as a link rather than a caption.
const BREADCRUMB_FONT_SIZE := 13
## Room a closed option picker spends beside its text: the arrow icon and the side margins.
const PICKER_TEXT_PADDING := 44.0
const BREADCRUMB_BACK_FORMAT := "‹ %s"
const BREADCRUMB_TAIL_FORMAT := "› %s"

# ---- widths and sizes (no bare literals in the builders) ---------------------------------------------
const CREW_BAR_WIDTH := 56.0
const FOLD_BUTTON_WIDTH := 22.0
const FOLD_BUTTON_HEIGHT := 22.0
const FOLD_TRIANGLE_SIDE := 8.0
const BAND_LINE_SEPARATION := 6
const BAND_LINE_FONT_SIZE := 13
const SUB_LINE_FONT_SIZE := 11
const MARK_FONT_SIZE := 10
const BAND_ROWS_INDENT := 22
const NAME_UNDERLINE_WIDTH := 1
const NAME_UNDERLINE_GAP := 1.0
const WORK_ROW_PADDING := 4
const WORK_ROW_SEPARATION := 2

# ---- the road card -----------------------------------------------------------------------------------
const ROAD_BRANCH := "route"
## The shipped route rungs, for a world whose catalog names no verb (`SourceForecast.IMPROVEMENT_GRADE` / `_PAVE`).
const ROAD_VERB_RUNGS := {
	"grade": "route:dirt_road",
	"pave": "route:paved_road",
}

# ---- the `Band:` picker ----------------------------------------------------------------------------
const PICKER_FOLD_OPEN_TIP := "Hide the tribe's bands"
const PICKER_FOLD_CLOSED_TIP := "Pick one of the tribe's bands"
const PICKER_MEMBER_INDENT := 1
const PICKER_FOLD_WIDTH := 24.0
const PICKER_TRIBE_LABEL_FORMAT := "%s"
const PICKER_ASIDE := "Joins the tribe's shared works."
const PICKER_FOLD_META := "tribe_picker_fold"
const PICKER_META := "tribe_band_picker"

# ---- Materials & Crafting --------------------------------------------------------------------------
## #864's read-only view of what each member's bench makes for the tribe:
## `static func build(members: Array[Dictionary]) -> Control` (the parameter is TYPED).
const CRAFTING_VIEW_PATH := "res://src/scripts/ui/CraftingTribeView.gd"
const CRAFTING_NO_BENCH := "The tribe has no bench. Each band's bench marks what it makes for the tribe."
const CRAFTING_TRIBE_VIEW_META := "crafting_tribe_view"
const CRAFTING_PLACEHOLDER_META := "crafting_tribe_placeholder"
## Set on the band handed to `CraftingPanelController.open_for` when the `\u2692` was pressed on the tribe's page.
const CRAFTING_OPEN_TRIBE_KEY := "open_tribe_entry"

# ---- the orb row -----------------------------------------------------------------------------------
const ORB_KIND := "tribe_proposal"
const ORB_LABEL_FORMAT := "%s — %s"
const ORB_DETAIL := "The tribe's leader proposes"
const ORB_PROPOSAL_TRIBE_FLAG := "tribe"
const ORB_PROPOSAL_TRIBE_ID := "tribe_id"
const ORB_PROPOSAL_APPROVE_TOOLTIP := "The tribe's leader raises this at once."
const ORB_PROPOSAL_NOT_NOW_TOOLTIP := "Hide this proposal for a while."
const ORB_APPROVE := "approve"
const ORB_DEFER := "defer"

# ---- scroll-carry keys -----------------------------------------------------------------------------
const WORK_SCROLL_NAME := "TribeWorkScroll"
const BAND_SCROLL_NAME := "TribeBandScroll"
const TRIBE_PAGE_META := "tribe_page"
const BAND_LINE_META := "tribe_band_line"
const WORK_ROW_META := "tribe_work_row"
const FOLD_META := "tribe_band_fold"
const SCOUT_LINE_META := "tribe_scout_line"
const LEVY_PICKER_META := "tribe_levy_picker"
const FOOD_LABEL_META := "tribe_food_label"
const NAME_LINK_META := "tribe_band_name"
const SUB_LINE_META := "tribe_band_sub"
const CREW_BAR_META := "tribe_crew_bar"
const BREADCRUMB_META := "tribe_breadcrumb"
const FOR_TRIBE_MARK_META := "for_tribe_mark"
const SCOUT_READONLY_META := "scout_readonly"
