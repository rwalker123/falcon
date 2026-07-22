extends CanvasLayer
class_name HudLayer

## Map-zoom rail (bottom-left nav cluster). `map_zoom_step` carries +1 (in) / -1 (out);
## `map_zoom_fit` fits the map to the view. Main wires both to the single MapView zoom path.
signal map_zoom_step(direction: int)
signal map_zoom_fit
## Emitted when the player clears ALL of a band's labor assignments (the "Clear all"
## affordance); carries the band dict so Main can extract faction + entity bits for the
## repurposed `cancel_order` command (now a clear-all → fully idle).
signal cancel_order_requested(band: Dictionary, scope: String)
## Early-Game Labor (docs/plan_early_game_labor.md, slice 3b): assign/unassign
## working-age workers to a source or band-wide role. Payload keys:
## { faction, band, kind ("forage"|"hunt"|"scout"|"warrior"), workers,
##   x, y (forage/hunt readout), herd_id, policy (hunt) }. Main formats the
## `assign_labor …` text command. workers==0 removes/zeroes the assignment.
signal assign_labor_requested(payload: Dictionary)
## The Telling (docs/plan_the_telling.md): the player answered a pending narrative fork.
## Payload keys: { faction, beat_id, choice_id }. Main formats the `answer_fork …` command.
signal answer_fork_requested(payload: Dictionary)
## Emitted after the player picks a destination tile for the selected band's move.
## Payload keys: { faction, band, x, y }. Main formats the `move_band …` command.
signal move_band_requested(payload: Dictionary)
## Scouting expedition (docs/plan_exploration_and_sites.md §2). Sent after the player outfits a
## party on a resident band (a party-size stepper) and clicks a target tile. Payload keys:
## { faction, band, party_workers, x, y }. Main formats the `send_expedition …` command.
signal send_expedition_requested(payload: Dictionary)
## Hunting expedition (docs/plan_exploration_and_sites.md §2b). Sent after the player outfits a party
## on a resident band and clicks a target herd. Payload keys: { faction, band, party_workers,
## fauna_id, fauna_label }. `fauna_id` is the DATABASE KEY the command line addresses the herd with;
## `fauna_label` is its player-facing species name (via `_herd_display_name`), which is what the
## command-feed note must read — a feed line naming `game_deer_07` is a key leaking into the game UI.
## Main formats the `send_hunt_expedition …` command.
signal send_hunt_expedition_requested(payload: Dictionary)
## Emitted when the player recalls the selected in-flight expedition (folds it home). Payload
## keys: { faction, expedition }. Main formats the `recall_expedition …` command.
signal recall_expedition_requested(payload: Dictionary)
## Emitted when the player extends a built pen by one fenced ring (Grazing 2d-γ). Payload keys:
## { faction, x, y } — the pen's anchor tile. Main formats the `extend_pen <faction> <x> <y>` command.
signal extend_pen_requested(payload: Dictionary)
## Optimistic pending-labor state changed (Early-Game Labor slice 3b UX): carries the
## per-band pending map so MapView can draw the pending-action hex highlights. Main forwards
## it to `MapView.set_labor_pending`.
signal labor_pending_changed(pending: Dictionary)
signal next_turn_requested(steps: int)
## Emitted whenever the active command-targeting state changes. Carries a dict
## ({} when inactive) that Main forwards to MapView so the map can draw the
## reticle / valid-target glow / hover ETA.
signal targeting_changed(info: Dictionary)
## Emitted when the player clicks a band alert; Main forwards it to
## MapView.focus_on_tile so the map pans to the band that raised the alert.
signal alert_focus_requested(x: int, y: int)
## Emitted when a roster row (band or wildlife) is selected in the Occupants card.
## `kind` is "unit" (id = entity_id int) or "herd" (id = herd_id String). Main
## forwards it to MapView.select_occupant so the map selection ring follows the
## chosen occupant without a hex click.
signal roster_occupant_selected(kind: String, id: Variant)

## PURE FALLBACK build identifier of THIS client — used only when no git stamp is present.
## The real build id is the git stamp `scripts/run_stack.sh` writes to `res://build_stamp.txt`
## (`<commit-date>-<short-hash>[-dirty]`, mirroring the server's `CORE_SIM_BUILD_ID`), read via
## `ClientBuild.current()`. **No more hand-bumping** — the git stamp is the source of truth, and
## this const matches the server's own `dev-unknown` fallback. Shown in the bottom-centre overlay
## beside the server build so the running client+server builds can be confirmed at a glance.
const CLIENT_BUILD := "dev-unknown"
const ClientBuild := preload("res://src/scripts/ClientBuild.gd")
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var sedentarization_label: Label = %SedentarizationLabel
@onready var demographics_label: Label = %DemographicsLabel
@onready var discoveries_row: HBoxContainer = %DiscoveriesRow
@onready var discoveries_label: Label = %DiscoveriesLabel
@onready var discoveries_strip: HBoxContainer = %DiscoveriesStrip
@onready var intensification_label: Label = %IntensificationLabel
@onready var nav_backing: PanelContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking
@onready var zoom_rail: VBoxContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail
@onready var zoom_in_button2: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomInButton
@onready var zoom_out_button2: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomOutButton
@onready var zoom_fit_button: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomFitButton
@onready var zoom_level_label: Label = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomLevelLabel
@onready var terrain_legend_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel as PanelCard
@onready var terrain_legend_scroll: ScrollContainer = %LegendScroll
@onready var terrain_legend_list: VBoxContainer = %LegendList
@onready var terrain_legend_description: Label = %LegendDescription
@onready var victory_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel
@onready var victory_status_label: RichTextLabel = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel/Margin/VictoryLabel
@onready var command_feed_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel as PanelCard
@onready var command_feed_scroll: ScrollContainer = %CommandFeedScroll
@onready var command_feed_label: RichTextLabel = %CommandFeedLabel
@onready var telling_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TellingPanel as PanelCard
@onready var telling_scroll: ScrollContainer = %TellingScroll
@onready var telling_label: RichTextLabel = %TellingLabel
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var tile_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/TilePanel as PanelCard
@onready var tile_detail: RichTextLabel = %TileDetail
@onready var occupant_detail: RichTextLabel = %OccupantDetail
# ONE card, ONE list, ONE drawer (docs/plan_tile_panel_layout.md). The chip strip carries the
# tile's standing condition and never scrolls; `%SubjectList` is the selectable list of subjects on
# this hex — the LAND first, then the bands and wildlife — and `%SubjectScroll` is the single,
# height-capped drawer every one of them fills. Only one drawer is ever open, which is what bounds
# the card: rows are ~30px, a compose block is 300+.
@onready var tile_chips: HFlowContainer = %TileChips
@onready var subject_list: VBoxContainer = %SubjectList
@onready var subject_scroll: ScrollContainer = %SubjectScroll
@onready var subject_body: VBoxContainer = %SubjectBody
# The 1px rule marking where the LIST ends and the DRAWER begins — without it the drawer's first
# row runs straight on from the last wildlife row and the two blocks read as one list.
@onready var subject_divider: Panel = %SubjectDivider
# Early-Game Labor allocation UI (slice 3b), all runtime-populated containers:
# the band's allocation panel (Working/Idle + assignment rows + Scout/Warrior + Move/Clear),
# the herd "assign hunters" controls, and the tile "assign foragers" controls.
@onready var allocation_panel: VBoxContainer = %AllocationPanel
@onready var herd_assign_controls: VBoxContainer = %HerdAssignControls
@onready var forage_assign_controls: VBoxContainer = %ForageAssignControls
@onready var stockpile_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel
@onready var stockpile_title: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileTitle
@onready var stockpile_list: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileList
@onready var left_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack
@onready var right_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack
@onready var right_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll
@onready var turn_orb: TurnOrb = $LayoutRoot/RootColumn/BottomBar/TurnCluster
@onready var minimap_container: MarginContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/MinimapContainer

var tooltip_panel: PanelContainer
var tooltip_label: Label

# The legend card + its terrain-only Name/Count sort header now live in
# ui/hud/LegendController.gd; the command feed card in ui/hud/CommandFeedController.gd.
# These two aliases keep `HudLayer.LEGEND_SORT_FIELD_*` resolvable for external
# callers (e.g. tools/ui_preview.gd) with the controller as the single source of truth.
const LEGEND_SORT_FIELD_NAME := LegendController.SORT_FIELD_NAME
const LEGEND_SORT_FIELD_COUNT := LegendController.SORT_FIELD_COUNT
const STACK_ADDITIONAL_MARGIN := 16.0
const PLAYER_FACTION_ID := 0
# Turn-orb attention contract (see TurnOrb.gd). The folded-in Alerts panel became
# three producers here: starving (critical), losing_population (warn), idle_workers (warn) —
# plus a fourth, awaiting_orders (warn): an expedition parked at its objective, burning provisions
# until the player acts. That is structurally the SAME class as idle workers (a demand on the
# player, an efficiency loss, not a crisis), so it shares their WARN severity and, like them, must
# be discoverable from the orb rather than only by having the right band panel open.
const ATTENTION_KIND_STARVING := "starving"
const ATTENTION_KIND_LOSING_POPULATION := "losing_population"
const ATTENTION_KIND_IDLE_WORKERS := "idle_workers"
const ATTENTION_KIND_AWAITING_ORDERS := "awaiting_orders"
# A pen whose keeper could not pay this turn's feed: the herd is SHRINKING every turn, and with it
# the yield a 25-turn investment was built for. It recovers if fed again, so this is a reversible
# loss the player must be told about WHILE it is reversible — exactly what the orb is for.
#
# SEVERITY IS DELIBERATELY WARN, NOT CRITICAL, and that is a framing decision about DOUBLE-REPORTING:
# a pen only goes unfed when the keeper's larder came up short, so the SAME empty larder normally
# also trips `starving` (critical) on that band. The two are not one alert twice — they are two
# different LOSSES from one cause (the people are dying / the herd is dying), with two different
# subjects, two different jumps (the band's tile / the herd's tile) and two different remedies. But
# only ONE of them gets to shout: the band's `starving` row stays the critical headline, and the pen
# row rides below it as the consequence the player would otherwise never see coming.
const ATTENTION_KIND_STARVING_PEN := "starving_pen"
const ATTENTION_PEN_LABEL_FORMAT := "%s pen starving"
# The detail carries the fed fraction and the consequence — and NOTHING else. It deliberately does
# NOT name the keeper band: the orb's rows CLIP at POPOVER_WIDTH (sized to the widest producer), and
# appending "· Band 1" pushed this row past it (rendered, looked at, cut). The row already names the
# herd, and its Jump lands on that herd — the band adds nothing the player can act on here.
const ATTENTION_PEN_DETAIL_FORMAT := "%d%% fed — the herd is shrinking"
## The Telling (docs/plan_the_telling.md): a narrative fork awaiting the player's answer.
##
## CRITICAL and, uniquely, `blocking` — it is the one producer that holds the end-turn. That is a
## deliberate asymmetry with every other row: a starving band is a loss you can choose to accept,
## but a fork is the game asking who your people ARE, and letting it scroll past unanswered is the
## one outcome the arc cannot afford. The out is not "ignore it" but the DEFER choice, which the
## panel always offers and always keeps enabled.
##
## It is NON-LOCATING (x/y = -1): the question lives in a panel, not on a hex, so the orb row reads
## `Open ▸` and routes through `panel_requested` rather than a map jump.
const ATTENTION_KIND_DECISION := "decision"
const ATTENTION_NON_LOCATING := -1
## The orb's rows CLIP at POPOVER_WIDTH, and a fork's narration is a paragraph — so the row carries
## only a fixed prompt and the fork's own first clause; the QUESTION itself belongs in the panel.
const ATTENTION_DECISION_LABEL := "A question awaits an answer"
const ATTENTION_DECISION_DETAIL_MAX_CHARS := 64
const ATTENTION_DECISION_DETAIL_ELLIPSIS := "…"
const UNANSWERED_FORK_LABEL := "A question went unanswered"
const UNANSWERED_FORK_DETAIL := "The turn advanced past a pending fork — it will settle as if nothing was said."
const ATTENTION_SEVERITY_CRITICAL := "critical"
const ATTENTION_SEVERITY_WARN := "warn"
# Awaiting expeditions are listed ONE ROW EACH (not one aggregate like idle workers): each parked
# party is a SEPARATE decision with its own destination, so an aggregate row would have nowhere to
# jump. The popover is positioned ABOVE the orb (`TurnOrb._position_popover`), so an unbounded list
# would climb off the top of the screen and take the `Advance ▸` footer with it — hence a cap, past
# which the remainder folds into a single overflow row that jumps to the first party beyond it.
const ATTENTION_AWAITING_MAX_ROWS := 3
const ATTENTION_AWAITING_OVERFLOW_LABEL_FORMAT := "+%d more awaiting orders"
const ATTENTION_AWAITING_OVERFLOW_DETAIL := "Jump to the next parked party"
# The row's context line: "<mission> · <objective>" (the objective is the herd for a hunt party, the
# party's own tile for a scout). Mission words come from EXPEDITION_MISSION_LABELS, the demand
# headline from EXPEDITION_PHASE_LABELS — neither is retyped here.
const ATTENTION_AWAITING_DETAIL_FORMAT := "%s · %s"
const ATTENTION_TILE_FORMAT := "(%d, %d)"
# Top-bar glyph for the discovered-Wondrous-Sites readout (a faceted-gem marker).
const DISCOVERIES_GLYPH := "◈"
# Drawn for a discovered site whose catalog row has NEITHER bundled art nor a non-empty `glyph`.
# A site the player has found must never render as blank space in the strip, so this is the
# last-resort mark: an outline of DISCOVERIES_GLYPH, reading as "a site, kind unillustrated".
const DISCOVERIES_UNKNOWN_GLYPH := "◇"
# Gap between the "◈ Discoveries N" text and the site strip, and between strip entries. Matches the
# double-space the glyphs used to be joined with, now that they are mixed Labels and TextureRects.
const DISCOVERIES_STRIP_SEPARATION := 6
# Separator between the four knowledge tracks in the top-bar intensification readout.
const INTENSIFICATION_SEGMENT_SEP := "  ·  "
# Leads the top-bar knowledge strip. This strip is the FACTION half of the two-meter split (§4.1) —
# what your PEOPLE have learned, permanently, from cumulative practice — as opposed to the per-source
# build meters in a herd's/patch's own drawer. The prefix exists precisely so the strip cannot be read
# as a stat of whatever happens to be selected; ⚒ (a hammer-and-pick — craft/skill) is bold line art,
# legible at top-bar size (the 🪙/💰 lesson), and used nowhere else in the HUD.
const KNOWLEDGE_STRIP_PREFIX := "⚒ Your people know:  "
# Block-glyph meter widths. `METER_BAR_CELLS` is the standard (Sedentarization). The knowledge strip
# runs NARROWER because it carries four tracks on one top-bar line — at the standard width the line
# overflowed and clipped its last track off-screen (caught in the preview, not in review).
const METER_BAR_CELLS := 10
const KNOWLEDGE_METER_CELLS := 5
# Tracks per row in the "⚒ Your people know" strip. Four tracks on ONE line overflowed the top bar and
# clipped the last one off-screen (the Penning playtest report); grouping into rows of two keeps every
# line short enough to fit common window widths — a top-bar strip is fine on two rows, and no track is
# ever lost off the edge, regardless of how many rungs the ladder grows.
const KNOWLEDGE_STRIP_TRACKS_PER_LINE := 2
# A fully-learned track. Bare, because `KNOWLEDGE_STRIP_PREFIX` already supplies the verb.
const KNOWLEDGE_KNOWN_BADGE := "✔"
const FOOD_MODULE_LABELS := {
    "coastal_littoral": "Coastal Littoral",
    "riverine_delta": "Riverine Delta",
    "savanna_grassland": "Savanna Grassland",
    "temperate_forest": "Temperate Forest",
    "boreal_arctic": "Boreal Arctic",
    "montane_highland": "Montane Highland",
    "wetland_swamp": "Wetland Swamp",
    "semi_arid_scrub": "Semi-Arid Scrub",
    "coastal_upwelling": "Coastal Upwelling",
    "mixed_woodland": "Mixed Woodland",
}
const FOOD_ACTION_FORAGE := "forage"
const FOOD_ACTION_HUNT := "hunt"
# Band-status alert types, ordered high → low priority (rendered in this order).
const BAND_ACTIVITY_IDLE := "idle"
# Verb prefixes for the optimistic in-flight label on the disabled cancel button,
# composed with the task action phrase as "<verb> <phrase>…" (e.g. "Cancelling
# Market Hunt…", "Starting Foraging…"). Shown from dispatch until the snapshot
# confirms the band's `activity` CHANGED from its value at dispatch.
const CANCEL_ORDER_PENDING_VERB := "Cancelling"
const START_ORDER_PENDING_VERB := "Starting"
# Why a band is losing population — appended to the losing_population alert label.
const DECLINE_REASON_STARVING := "starving"
const DECLINE_REASON_LOW_MORALE := "low morale"
# Morale-driven loss is now emigration/relocation (people don't die of low morale —
# see docs/plan_civ_wellbeing.md), so a shrink with emigrants last turn reads this.
const DECLINE_REASON_PEOPLE_LEAVING := "people leaving"
# Per-cohort morale cause (snapshot PopulationCohortState.moraleCause; 0 = None).
const MORALE_CAUSE_NONE := 0
const MORALE_CAUSE_TERRAIN := 1
const MORALE_CAUSE_COLD := 2
const MORALE_CAUSE_UNREST := 3
# Plain-language cause labels, shared by the drawer morale line and the alert reason.
# Cold reads "harsh climate" because the server penalty fires on hot OR cold deviation.
const MORALE_CAUSE_LABEL_TERRAIN := "harsh terrain"
const MORALE_CAUSE_LABEL_COLD := "harsh climate"
const MORALE_CAUSE_LABEL_UNREST := "unrest"
# Morale-trend arrow glyphs; |morale_delta| below this (0.5%/turn) reads as flat (no
# arrow), so trivial drift — nearly every tile bleeds a hair today — isn't shown as a decline.
const MORALE_TREND_EPSILON := 0.005
const MORALE_TREND_FALLING_GLYPH := "▼"
const MORALE_TREND_RISING_GLYPH := "▲"
# Civilization Wellbeing (docs/plan_civ_wellbeing.md). Productivity readout: output is the
# modifier-stack result (1.0 = full); the Output row only appears below full, tinted by the
# output.{warn,critical} buckets in BandFoodStatus.
const OUTPUT_FULL := 1.0
# Itemized morale breakdown — the four signed Layer-1 contributions (their sum IS
# morale_delta) rendered as indented sub-lines under the Morale headline when morale is
# concerning or declining. Tinted by sign (▲ positive = healthy, ▼ negative = amber).
const MORALE_BREAKDOWN_INDENT := "    "
const MORALE_CONTRIB_POSITIVE_GLYPH := "▲"
const MORALE_CONTRIB_NEGATIVE_GLYPH := "▼"
const MORALE_CONTRIB_LABEL_SETTLING := "settling"
# Positive unrest contribution reads as "culture" (cohesion), negative as "unrest".
const MORALE_CONTRIB_LABEL_CULTURE := "culture"
# Recovery guidance — a dim line naming the real levers (NOT harvest) when morale is concerning.
const RECOVERY_GUIDANCE_GLYPH := "↑"
const RECOVERY_GUIDANCE_TEXT := "↑ Recover: move to Hospitable ground · Scout · Hunt"
# Positive-lever morale hints on the action buttons (tooltip suffixes).
const MORALE_HINT_SCOUT := "Scout unknown ground — reveals nearby tiles and lifts the band's spirits (+morale)."
const MORALE_HINT_PERSISTENT := "  Hunting a herd also lifts morale each turn (+morale/turn)."
# Occupants roster row chrome.
const ROSTER_DOT_SIZE := 9.0
const ROSTER_ROW_MIN_HEIGHT := 30.0
const ROSTER_ROW_SEPARATION := 8
const ROSTER_ROW_H_PADDING := 10.0
const ROSTER_ACCENT_WIDTH := 3.0
const ROSTER_HEADER_FONT_SIZE := 10
# ---- The subject list: the land is the FIRST ROW, not a card above the occupants ---------------
# The land is the same KIND of thing a band or a herd is — a subject on this hex you can put
# workers on — so it is a row, and selecting it fills the same one drawer. `_selected_subject` says
# only which KIND of row is lit; `_selected_unit` / `_selected_herd` stay authoritative for WHICH.
const SUBJECT_LAND := "land"
const SUBJECT_UNIT := "unit"
const SUBJECT_HERD := "herd"
# `roster_occupant_selected`'s id for the LAND kind: the land has no entity, and the signal's id is
# a Variant, so it carries the same "no occupant" sentinel the rest of the client uses.
const LAND_SUBJECT_ID := -1
# Fallback glyph for the land row on a tile carrying no food module. Text-presentation (the
# line-art policy in `FoodIcons`): it inherits the row label's colour, so it dims with the row.
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
# Provisions is the food item under a band's larder `stores`.
const STORE_ITEM_PROVISIONS := "provisions"
const FOOD_UNLIMITED_GLYPH := "∞"
# The UNIT the larder runway is spelled in, shared by the ONE renderer (`_food_turns_text`) and the
# ONE reader (`_format_detail_bbcode`'s Food/Provisions/Carried threshold tint, which recognizes the
# row by looking for this word). They must never drift: the tint went dead once already because the
# renderer switched from "days" to "turns" while the guard kept testing for the old literal.
const FOOD_RUNWAY_UNIT := "turn"
const UI_BALANCE_CONFIG_PATH := "res://src/config/ui_balance.json"
# Dock-card visibility preferences. Reuses the file `NarrativeForkPanel` already writes the voice
# register into — one prefs file, its own section; the path/section constants are borrowed.
const HUD_PANELS_CONFIG_SECTION := "hud_panels"
const CONFIG_KEY_LEGEND_SUPPRESSED := "legend_suppressed"
const CONFIG_KEY_VICTORY_SUPPRESSED := "victory_suppressed"
const CONFIG_KEY_COMMAND_FEED_SUPPRESSED := "command_feed_suppressed"
# Both reference cards start HIDDEN: the right dock is the narrative surface's home, and Victory /
# Terrain Types are look-it-up readouts the player opens on demand (V / L) rather than standing
# furniture competing with the telling for dock height.
const PANEL_SUPPRESSED_BY_DEFAULT := true
const DEFAULT_TRAVEL_SPEED := 3.0
const DEFAULT_TRAVEL_PREVIEW_LIMIT := 12
# The legend card (rows + sort header + suppress state) is owned by _legend; the
# command feed card by _command_feed; the narrative panel by _telling. Hud delegates to all three.
var _legend: LegendController = null
var _command_feed: CommandFeedController = null
var _telling: TellingPanel = null
# Victory's counterpart to the legend's `legend_suppressed` — the player-hidden state of a dock
# card, distinct from "no victory data to show".
var _victory_suppressed: bool = PANEL_SUPPRESSED_BY_DEFAULT
var localization_store = null
var campaign_label: Dictionary = {}
var victory_state: Dictionary = {}
# Previous per-band size (entity id -> size) so we can detect population loss
# across snapshots for the "losing population" alert.
var _prev_band_sizes: Dictionary = {}
var _selected_tile_info: Dictionary = {}
var _selected_unit: Dictionary = {}
var _selected_herd: Dictionary = {}
# Which KIND of subject row is lit (SUBJECT_LAND | SUBJECT_UNIT | SUBJECT_HERD). The two dicts
# above stay authoritative for WHICH unit/herd; this only picks the drawer.
var _selected_subject: String = SUBJECT_LAND
# The hex the player last EXPLICITLY chose a subject on (a row click), so the auto-select rule can
# tell "a fresh hex, pick a default" from "the player already decided here". Without it, choosing
# the LAND row on an occupied hex cleared both occupant dicts and the next snapshot's
# `reapply_selection("tile", …)` re-ran the default and stole the selection back to the first band.
# `(-1, -1)` = no choice yet, which no real hex matches.
var _subject_choice_tile: Vector2i = Vector2i(-1, -1)
# One drawer fit in flight at a time — see `_fit_subject_drawer`.
var _subject_fit_pending: bool = false
# The assembled Occupants roster for the current hex: full unit markers and herd
# dicts (from `_selected_tile_info`, plus the selected occupant if the tile_info
# doesn't list it — e.g. an inspector-driven herd selection). Rebuilt each render.
var _roster_units: Array = []
var _roster_herds: Array = []
# Every herd in the snapshot (`snapshot["herds"]`, pushed by Main each turn). The roster above only
# holds the SELECTED hex's herds, so it can't answer "where is the herd this band hunts?" — herds
# MIGRATE, so a hunt assignment's `target_x/target_y` is a stale launch position. This is the live
# position + label source for the Current-actions Hunt row (label + jump), mirroring
# `MapView.herds` / `MapView._herd_by_id`, which the hunted-herd ring already resolves through.
var _world_herds: Array = []
var _selected_food_module: String = ""
var _selected_food_is_hunt: bool = false
# Days-of-food of the currently-selected band's larder, so the detail formatter
# can threshold-tint the Food row. NAN when no band is selected.
var _selected_band_food_turns: float = NAN
# Set by `_band_food_line`: the current player band carries real food flow, so the Food row becomes a
# clickable disclosure (net rate on the line + a Gathered/Hunted/Eaten breakdown).
var _food_flow_present: bool = false
# Disclosure context for the Food + Morale summary rows, rebuilt each render in `_unit_summary_lines`:
# row-label → {key, open, concerning}. `_format_detail_bbcode` reads it to render the caret + meta.
var _disclosure_state: Dictionary = {}
# The breakdown rows each disclosure would show, keyed `"<kind>:<entity>"` → Array[String]. Written
# every render by `_register_disclosure` and read by the popover, so the popover never recomputes a
# number and a click needs no band lookup — the meta carries the key. Deliberately NOT cleared with
# `_disclosure_state`: that one is per-render and per-host, and the other host's render must not be
# able to empty the payload behind an open popover.
var _breakdown_payloads: Dictionary = {}
# The disclosure key whose popover is currently up, `""` = none. It is what "open" means now, so it
# is also what flips the row's caret.
var _breakdown_popover_key: String = ""
# The one popover both hosts share, built lazily on the first disclosure click.
var _breakdown_popover: PopupPanel = null
var _breakdown_popover_label: RichTextLabel = null
# Morale (0–1) of the currently-selected player band, so the detail formatter can
# threshold-tint the Morale row. NAN when no player band is selected.
var _selected_band_morale: float = NAN
# Output multiplier (0–1) of the currently-selected player band, so the detail formatter
# can threshold-tint the Output row. NAN when no band with a below-full output is selected.
var _selected_band_output: float = NAN
# Early-Game Labor (docs/plan_early_game_labor.md, slice 3b). Assignment kinds mirror
# the sim's LaborAssignment.kind; the source-centric allocation targets the single
# player band captured from each snapshot (there is exactly one player band today).
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"
const LABOR_KIND_SCOUT := "scout"
const LABOR_KIND_WARRIOR := "warrior"
# EXTRACTIVE take policies — the four rungs that take from a wild source without changing it. Shared
# by forage + hunt (and the only ones a hunting EXPEDITION can carry: a detached party builds no pen).
const LABOR_HUNT_POLICIES := ["sustain", "surplus", "market", "eradicate"]
# The Sustain rung by name: the default compose policy. It is also the rung the ladder's KNOWLEDGE
# is learned from — but only knowledge: since slice 3a **Sustain no longer tames anything**. Working
# a source under a stewardship policy teaches the faction the knowledge that unlocks the NEXT rung's
# verb (§4), and the per-source build meter is then filled by that verb itself.
const LABOR_POLICY_SUSTAIN := "sustain"
const DEFAULT_HUNT_POLICY := LABOR_POLICY_SUSTAIN
# INVESTMENT rungs (the Intensification Ladder, docs/plan_intensification_ladder.md §2): an up-front
# cost — the source pays only its dip ceiling (the patch's `ceiling_cultivate` / `ceiling_sow`
# scalars; for a herd, the `tame` / `corral` rows of its `hunt_policy_ceilings` list) while the
# workers prepare it, then flips to the much higher managed yield. Kind-specific, and the sim REJECTS the cross pairing: Cultivate + Sow are forage-only, Tame
# + Corral are hunt-only. Each ladder now runs its verb TWICE — one verb per rung-transition:
#   plants:  wild --cultivate--> Tended Patch --sow--> Field
#   animals: wild --tame------> Pastoral herd --corral--> Pen
const LABOR_POLICY_CULTIVATE := "cultivate"
const LABOR_POLICY_SOW := "sow"
const LABOR_POLICY_TAME := "tame"
const LABOR_POLICY_CORRAL := "corral"
# The full picker option sets per source kind (the four extractive rungs + that kind's TWO investment
# rungs, in ladder order so the picker reads bottom-of-the-ladder → top).
const FORAGE_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "cultivate", "sow"]
const HUNT_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "tame", "corral"]
# Forage take policies reuse the hunt picker, but carry forage-appropriate behaviour hints
# (gathering a plant patch's regrowth, not culling a herd).
const FORAGE_POLICY_HINTS := {
    "sustain": "Sustain — gather at the patch's regrowth; it stays healthy.",
    "surplus": "Surplus — gather more now; the patch declines.",
    "market": "Market — gather for trade goods; faster decline.",
    "eradicate": "Eradicate — strip the patch bare.",
    "cultivate": "Cultivate — prepare this patch: low yield while you work it, then a much higher tended yield. It must stay staffed or it goes feral.",
    # Sow is plant RUNG 3 — the twin of Corral. Its hint must carry the two things that make it a
    # different bargain from Cultivate: it pays ~nothing while the crop is in the ground (there is no
    # standing stand to take a fraction of), and it out-yields a tended patch ~2×. The "goes feral"
    # warning is one rule for the whole plant web — an abandoned patch bleeds BOTH meters, so a
    # neglected Field reverts to WILD, not to a free tended patch.
    "sow": "Sow — plant a Field on this ground: almost no food while the crop grows, then twice a tended patch's yield. It must stay staffed or it goes feral all the way back to wild.",
}
# GATES on the investment rungs. The option stays VISIBLE but disabled with its reasons, so the player
# learns the prerequisite BEFORE acting rather than never discovering the rung exists. Both gates
# mirror the sim's `assign_labor` validation (faction knowledge complete + the source ready).
#
# Each reason states WHAT'S MISSING + HOW FAR ALONG IT IS + THE ACTION THAT CLOSES IT — naming the
# prerequisite alone ("Herd must be domesticated") tells the player a door is locked without saying
# where the key is.
#
# THIS IS WHERE THE TWO-METER SPLIT IS TAUGHT (docs/plan_intensification_ladder.md §4.1). A gated
# verb has at most two kinds of reason, and they are DIFFERENT KINDS OF THING:
#   • a KNOWLEDGE reason — "your PEOPLE haven't learned this craft yet". Faction-wide, permanent,
#     earned by cumulative practice on the rung BELOW. Its meter lives in the top-bar knowledge
#     strip, never in this source's drawer, and the remedy names the PRACTICE that fills it.
#   • a SOURCE reason — "you haven't done it to THIS herd/patch yet". Local, decays if abandoned.
#     Its meter is the source's own drawer row, and the remedy names the VERB that fills it.
# One line teaches the whole ladder: practise this rung → fill that knowledge meter → unlock that
# verb. The remedies therefore name a glyph pulled from the shared `FoodIcons.POLICY_ICONS` map, so
# each is literally the icon on a button beside it.
#
# The KNOWLEDGE reasons. Practice teaches the NEXT rung up (§4), and the rule keys off the rung the
# source STANDS on, not the verb — so the same Sustain hunt teaches Herding on a wild herd and
# Penning on a tamed one. Format args: %d = the live faction progress percent, %s = the Sustain glyph.
const GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT := "Your people know Cultivation %d%% — %s Sustain-forage a wild patch to learn it"
const GATE_REASON_HERDING_KNOWLEDGE_FORMAT := "Your people know Herding %d%% — %s Sustain-hunt a wild herd to learn it"
# The two knowledges slice 4 added. The §4.3 reshuffle put ONE knowledge on each transition, so these
# gate the rung-3 verbs and their remedies point at working the rung-2 source — the ladder's
# "practise this rung to unlock the next" rule, stated in the place the player is blocked.
const GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT := "Your people know Seed Selection %d%% — %s Sustain-forage a Tended Patch to learn it"
const GATE_REASON_PENNING_KNOWLEDGE_FORMAT := "Your people know Penning %d%% — %s Sustain-hunt a tamed herd to learn it"
# The SOURCE reasons — this one animal/patch's own build meter. `Corral`'s remedy now names the
# `Tame` VERB (glyph %s), not "Sustain-hunt this Thriving herd": since slice 3a, Sustain tames
# nothing. That correction is the single most load-bearing copy fix in this slice — the old sentence
# is the exact hidden rule the arc exists to kill.
const GATE_REASON_HERD_DOMESTICATED_FORMAT := "This herd is %d%% tamed — %s Tame it to finish"
# The patch-ecology gate is a STOCK condition, not a policy one, so its remedy is the opposite advice:
# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever. The
# patch only climbs back to Thriving when the take is LESS than the growth — fewer workers, or none.
# %s = the live `patch_ecology_phase`, capitalized.
const GATE_REASON_PATCH_THRIVING_FORMAT := "Patch is %s — ease workers off and let it regrow to Thriving"
# A COMPLETED investment rung is a dead-end no-op — the build is DONE, so re-running the verb only pays
# the low prep dip forever. The rung is greyed (like Sow is greyed when gated) and the reason points the
# player at the ♻ Sustain that now HARVESTS the finished ground, where the real payoff lives. Mirrors the
# SOURCE-reason voice ("This herd is 40% tamed — ◎ Tame it to finish") for a state that is already there.
const GATE_REASON_ALREADY_TENDED_FORMAT := "Already a Tended Patch — %s Sustain-forage it to harvest"
const GATE_REASON_ALREADY_FIELD_FORMAT := "Already a Field — %s Sustain-forage it to harvest"
# THE SOW SITE GATE — "why can't I sow HERE?" is *the* question rung 3 provokes, because only ~1% of
# the map will take seed (46 of 4160 tiles on the standard map: alluvial plain + river delta). The
# client cannot re-derive this — it holds neither the per-biome capacity table nor the hydrology — so
# the sim ships the VERDICT as a stable key and these turn it into the manual's voice. Never show a
# Sow button that just fails, and never answer with a bare "you can't": each line names the fault AND
# points at the rung that lifts it (Worked Land — irrigation and the plough — is a future arc, so the
# promise is deliberately "not yet", not a date).
#
# Rung 3 moves seed but cannot FERTILIZE, so the land itself must do it: the ground has to be rich
# already and near fresh water. Salt coast does not count.
const SOW_REFUSAL_TOO_POOR := "too_poor"
const SOW_REFUSAL_TOO_DRY := "too_dry"
const SOW_REFUSAL_TOO_POOR_AND_TOO_DRY := "too_poor_and_too_dry"
const SOW_REFUSAL_REASONS := {
    "too_poor": "This ground is too thin to take a crop — your people can carry seed, but not yet feed the soil. Look to the river valleys, until they learn to work poorer land.",
    "too_dry": "This ground is rich but too dry to farm — your people can carry seed, but not yet carry water to it. Sow beside fresh water, until they learn to bring it here.",
    "too_poor_and_too_dry": "This ground is both too thin and too dry to take a crop — your people can carry seed, but neither feed the soil nor water it yet. The river valleys will take it; this ground will not, until they learn to work the land.",
}
# An unrecognized refusal key still refuses (fail CLOSED — the sim gates the command regardless, so a
# button offered here would simply fail), and says the one thing we do know.
const SOW_REFUSAL_FALLBACK := "This ground will not take seed — your people cannot yet work land like this."
# Taming pauses (it does not fail, and it does not lose progress) while the herd is not Thriving. The
# verb is deliberately NOT gated on that — a herd's phase swings as you hunt it — so this line is the
# only thing standing between the player and a hidden rule. %s = the herd's live `ecology_phase`.
const TAME_STALLED_HINT_FORMAT := "⚠ Taming is paused — the herd is %s, and it only gentles while Thriving. Progress is not lost: ease your hunters off and it resumes as the herd recovers."
# A patch with no streamed phase (redacted remembered tile) still fails the Thriving
# test; it reads as unknown rather than asserting a phase we don't have.
const GATE_PHASE_UNKNOWN_LABEL := "not Thriving"
# A single-reason gate reads as a compact one-liner under the picker row ("🌱 Cultivate — <reason>").
const GATE_REASON_LINE_FORMAT := "%s — %s"
# Two or more reasons are far too long for one line, so they render as a header + one bullet each
# ("🌱 Cultivate needs:" / "   · <reason>").
const GATE_REASON_HEADER_FORMAT := "%s needs:"
const GATE_REASON_BULLET_FORMAT := "   · %s"
# The disabled button's tooltip carries every reason, one per line.
const GATE_REASON_TOOLTIP_SEPARATOR := "\n"
# Every policy button's tooltip leads with this — the policy name + its full metric ("Sustain — up to
# +0.90/turn"), since the compact button face no longer carries the name. A gated button appends its
# gate reasons below (one per line), so a hover names the rung AND explains any lock.
const POLICY_TOOLTIP_NAME_FORMAT := "%s — %s"
# 0..1 progress tracks (knowledge, domestication) render as whole percents.
const PROGRESS_PERCENT_SCALE := 100.0
# A knowledge track (0..1) is usable only once fully learned; a domestication track likewise.
const KNOWLEDGE_COMPLETE := 1.0
const DOMESTICATION_COMPLETE := 1.0
# Herd drawer "Corral" row: the pen-build meter (0..1) reads "Building N%" until it completes, then
# the penned badge — the herd twin of the tile card's "Cultivation N%" → "🌾 Tended Patch" row.
const CORRAL_PROGRESS_COMPLETE := 1.0
const CORRAL_BUILDING_LABEL := "Building"
# The build-verb for the in-progress Cultivate rung — the plant twin of Husbandry's "Domesticating".
const CULTIVATION_PREPARING_LABEL := "Preparing"
const CORRAL_GLYPH := "🐄"
# Tile card "Field" row — plant RUNG 3, the patch twin of the herd's "Corral" row and the rung above
# "Cultivation". Its own row (never merged with Cultivation): a patch carries BOTH meters, and a Field
# may stand on ground that was never tended. "Sowing N%" follows the pen's "Building N%" / the fence's
# "Fencing N%" build-verb convention; the completed badge is a Field — deliberately a different WORD
# and a different glyph from "🌾 Tended Patch", because rung 3 is a different thing, not a bigger number.
const FIELD_ROW := "Field"
# Tile card "What grows here" row (flora roster F1) — the named plants this tile's forage capacity is
# MADE OF. Naming DECOMPOSES, it never adds: the shares sum to 1, so this says what the Forage number
# already on the card consists of. Derived from the biome, so it is descriptive, not a state.
const FLORA_COMPOSITION_ROW := "What grows here"
const FLORA_SHARE_SEPARATOR := " · "
const FLORA_SHARE_FORMAT := "%s %d%%"
# Whole-percent scale for a 0..1 share. The displayed numbers must ALWAYS sum to this: naive rounding
# can land on 99 or 101, and the remainder is absorbed into the largest share (the first entry — the
# wire list is share-descending), which is the one where a ±1 is least visible.
const FLORA_SHARE_PERCENT_TOTAL := 100
const FIELD_PROGRESS_COMPLETE := 1.0
const FIELD_SOWING_LABEL := "Sowing"
const FIELD_BADGE_LABEL := "Field"
# The pen as a managed POPULATION (docs/plan_corral_managed_population.md). A penned herd cannot
# graze: its keeper hauls it `pen_upkeep` food/turn off the band larder. `pen_fed_fraction` is the
# share of that demand the keeper actually paid last turn — anything below fully-fed means the herd
# is SHRINKING and its yield with it, so the Corral row swaps its penned badge for a loud starving
# state and the herd's map glyph tints red. `PenStatus` owns that test (shared with MapView).
const PEN_STARVING_LABEL := "⚠ Starving — %d%% fed"
# The pen's feed row in the herd drawer — what THIS pen demands per turn, and whether it is being
# paid. The band's own ledger row is the sim-summed `pen_feed_upkeep` across all its pens; this is
# the per-herd demand (`pen_upkeep`), which is why the two are never added together.
const PEN_FEED_ROW := "Pen feed"
# `_format_yield` already carries the "/turn" suffix — these only add the shortfall.
const PEN_FEED_STARVING_FORMAT := "%s — only %d%% paid"
# Grazing 2d-γ — the pen is fenced LAND that grazes itself. Two herd-drawer rows state it:
#   • the FOOTPRINT — "Pen: radius R · N tiles" (`pen_radius` + the SERVER's in-bounds
#     `pen_footprint_tiles` count, displayed VERBATIM — the closed-form hex-disk count is wrong at map
#     edges, so the client never recomputes it).
#   • the FEED SPLIT — "Fed by pasture NN% · larder N.N food/turn" (`pen_pasture_fraction` × 100 and
#     `pen_upkeep`, the OFFSET larder bill). A self-feeding pen on lush land reads "100% · larder 0.0";
#     a scrub pen "0% · larder N.N". The Pen-feed row below still carries the debit + starving detail.
const PEN_FOOTPRINT_ROW := "Pen"
const PEN_FOOTPRINT_FORMAT := "radius %d · %d tiles"
const PEN_FEED_SPLIT_ROW := "Fed by pasture"
const PEN_FEED_SPLIT_FORMAT := "%d%% · larder %.1f food/turn"
# The Extend-pen affordance (Grazing 2d-γ; command `extend_pen <faction> <x> <y>` at the pen anchor).
# On a built pen with no ring in flight it offers "Extend pen"; while a ring is being worked off
# (`pen_extend_progress > 0`) it is replaced by a "Fencing N%" badge — the pen twin of the corral-build
# "Building N%" meter. The server rejects an extend at max radius / unowned / Herding-unknown with a
# feed message, so the client does not pre-gate on those (max radius is not on the wire).
const PEN_EXTEND_LABEL := "Extend pen"
const PEN_EXTEND_TOOLTIP := "Fence another ring around the pen: the keeper works it off over ~25 turns at a reduced take, then the pen grazes more land and feeds itself further. Rejected at the pen-radius maximum."
const PEN_FENCING_LABEL := "Fencing %d%%"
# Grazing 2d-δ — the per-species HUSBANDRY CEILING (`HerdTelemetryState.husbandryCeiling`): how far up
# the ladder a species can climb. "wild" = hunt-only (no husbandry track at all); "pastoral" =
# tameable + roams but can NEVER be penned (hide Corral + Extend); "pen" (or empty/absent) = the full
# ladder, everything as today. The herd drawer + assign controls gate their husbandry affordances on it.
const HUSBANDRY_CEILING_WILD := "wild"
const HUSBANDRY_CEILING_PASTORAL := "pastoral"
const HUSBANDRY_CEILING_PEN := "pen"
# In place of the whole husbandry section on a wild-ceiling herd, and where the corral affordance would
# sit on a pastoral one — so the missing controls read as intentional, not a bug. Colon-free, so
# `_format_detail_bbcode` renders them as dim informational sentences (the `kv.is_empty()` path).
const HUSBANDRY_WILD_HINT := "Wild game — hunt only"
const HUSBANDRY_PASTORAL_HINT := "Herdable, not pennable"
# Herd drawer "Herders" row — a MANAGED herd's staffing (intensification ladder). A domesticated herd
# needs `herders_needed` herders every turn to HOLD its tameness; understaffed (`herded_fraction < 1`)
# it DECAYS out of the pastoral rung, slips back to wild, and stops earning Penning — the silent stall
# a playtest hit ("🐄 Domesticated" with no signal that Penning had stopped). The row makes the deficit
# visible; the under-herded value is WARN-tinted via `_herders_value_hex`, and the slipping consequence
# is spelled out below it so the player knows WHY Penning stalled and how to fix it.
# `FULLY_HERDED` is the `herded_fraction` wire default (1.0 = fully staffed, also unmanaged/vanished
# herds) — treated as "no problem". The staffed label reads "N / N" (calm); under-herded "A / N —
# under-herded" (amber).
const FULLY_HERDED := 1.0
const HERDERS_ROW := "Herders"
const HERDERS_STAFFED_FORMAT := "%d / %d"
const HERDERS_UNDER_FORMAT := "%d / %d — under-herded"
const HERDERS_SLIPPING_FORMAT := "Tameness slipping — teaching Herding, not Penning. Staff all %d herders to hold it."
# Herd drawer grazing range (Grazing Phase 2b-iii): the ground the herd grazes (tile count of its hex
# range, so it pairs with the map ring) — a SEPARATE fact from the biomass/cap pair, which the `Biomass`
# row now carries as a `current / max` pair (`11636 / 11636`). The `Range` key stays ≤ 16 chars so
# `_split_detail_kv` renders it as an aligned table row beside Biomass.
const HERD_RANGE_ROW := "Range"
# Herd drawer size class: the `<size> game` class the roster row used to carry as its meta. The row's
# meta slot now states the herd's STAFFING (`1 🏹`, parallel to the land row), so the size class moved
# to the drawer — where the facts that don't fit the row live. The key stays ≤ 16 chars so
# `_split_detail_kv` renders it as an aligned table row above Biomass.
const HERD_SIZE_ROW := "Size"
const HERD_SIZE_CLASS_FORMAT := "%s game"
# Overgrazing is a TRIVIAL honest comparison of two sim-provided numbers — biomass exceeds what the
# range can sustainably feed, so the herd is drawing the range down and will shrink. NOT a re-derivation
# of the ecology model (K and graze flow are the sim's). The epsilon keeps a herd sitting exactly at K
# from flickering the warning. WARN-tinted via `_format_detail_bbcode` (the Ecology/Corral rows' path).
const OVERGRAZE_EPSILON := 0.05
const OVERGRAZING_WARNING := "⚠ Overgrazing — range can't sustain this herd"
# The one ecology phase a patch can be cultivated from (matches `EcologyPhase::as_str`).
const ECOLOGY_PHASE_THRIVING := "thriving"
# The FOUR intensification knowledge tracks (the `intensification_knowledge[]` row's field names) —
# the FACTION-WIDE half of the two-meter split (§4.1). One per rung-transition, so the list IS the
# ladder, and §4.3 pins "no two rungs share an unlock gate":
#   plant:  wild --cultivation--> tended --seed_selection--> field
#   animal: wild --herding------> pastoral --penning-------> pen
# `seed_selection`/`penning` were appended by slice 4 (discovery ids 2005/2006).
const KNOWLEDGE_TRACK_CULTIVATION := "cultivation"
const KNOWLEDGE_TRACK_HERDING := "herding"
const KNOWLEDGE_TRACK_SEED_SELECTION := "seed_selection"
const KNOWLEDGE_TRACK_PENNING := "penning"
# The player-facing name of each track, from the manual's vocabulary (§2a is authoritative). Also the
# order the top-bar knowledge strip renders them in: each web's own ladder, bottom rung first, so the
# strip reads as two ladders climbing rather than four unrelated numbers.
const KNOWLEDGE_TRACK_LABELS := {
    "cultivation": "Cultivation",
    "seed_selection": "Seed Selection",
    "herding": "Herding",
    "penning": "Penning",
}
# Command-feed nudge fired ONCE when a track completes: the rung it unlocks is a new verb the player
# has never seen, so learning the discovery has to say what it bought — and, since the verb is only
# HALF the story, what the verb then asks of them (a per-source meter to fill).
const KNOWLEDGE_UNLOCK_LABELS := {
    "cultivation": "Cultivation learned",
    "seed_selection": "Seed Selection learned",
    "herding": "Herding learned",
    "penning": "Penning learned",
}
# NOTE: `herding` used to read "The Corral policy is now available on domesticated herds." Both
# halves were wrong after the §4.3 reshuffle — Herding gates **Tame** (rung 2) and it is **Penning**
# that gates Corral (rung 3).
const KNOWLEDGE_UNLOCK_NOTES := {
    "cultivation": "The Cultivate policy is now available on Thriving wild patches.",
    "seed_selection": "The Sow policy is now available — but only on rich, well-watered ground.",
    "herding": "The Tame policy is now available on wild herds that can be domesticated.",
    "penning": "The Corral policy is now available on herds you have tamed.",
}
# The policy hint under a LOCAL (resident-band) hunt's picker. The live yield line above it already
# carries the NUMBER; these carry the CONSEQUENCE, which is otherwise invisible — above all Sustain's,
# because a resident Sustain hunt on a thriving herd accrues HUSBANDRY toward livestock (and feeds
# Sedentarization's `domestication` input), the single most under-communicated payoff in the system.
#
# These are the BAND's payoffs and must not be reused for an expedition: the Hunting expedition arm
# credits FOOD ONLY — no husbandry accrual, no trade goods — so `SEND_HUNT_POLICY_HINTS` below
# deliberately promises neither. Do not merge the two sets; the asymmetry is real (a known v1 gap,
# tracked server-side), and a hint that claims a payoff the sim never pays is a lie to the player.
#
# Corral (the herd-side INVESTMENT rung) lives HERE and only here — it is a LOCAL-hunt policy: a
# detached party follows the herd and builds no pen, so the expedition set has no Corral entry (and
# the sim rejects a Corral expedition outright). This is also the local set `_policy_hint` spells out
# on a worked Hunt row's tooltip — those rows are always a resident band's.
const LOCAL_HUNT_POLICY_HINTS := {
    # Sustain USED to claim it tamed the herd ("on a thriving herd the hunt also tames it… livestock
    # that pays food every turn without being hunted down"). BOTH halves of that are now false and
    # the sentence is the reason this whole arc exists: slice 3a split the conflated branch, so
    # Sustain TEACHES the faction Herding but tames nothing (the `tame` verb fills the herd's own
    # meter), and slice 3b retired passive-free pastoral, so a tamed herd pays only through workers.
    # What Sustain honestly does is teach — which is exactly the ladder's first rung, so it says so.
    "sustain": "Sustain — takes only the herd's renewable yield, so it stays healthy forever. Working a herd this way is also how your people learn the next rung's craft: Herding on a wild herd, Penning on a tamed one.",
    "surplus": "Surplus — more food now; the herd slowly declines. The fuller larder pushes the band toward settling.",
    "market": "Market — sells the take as trade goods rather than eating it; the herd declines fast. Trade has little effect yet.",
    "eradicate": "Eradicate — hunts the herd toward extinction. No food, no craft learned, no trade — denial only.",
    # Tame is animal RUNG 2 — the verb that replaced the hidden Sustain side effect. Its payoff is
    # NOT "free food": 3b retired the passive rung, so the honest promise is yield PER WORKER (~1.5×
    # off the same crew) plus proximity (the herd drifts to the band instead of being chased).
    "tame": "Tame — gentle this herd into livestock: a reduced take while you work it, then it keeps to your band instead of roaming, and the same hunters bring back about half again as much. Your people still work it every turn.",
    # Corral is the ladder's best yield AND its only rung with a running cost. The hint has to carry
    # all three halves of that bargain — the ~25-turn investment dip, the top payoff, and the fact
    # that a penned herd is a POPULATION YOU FEED: its food comes off your larder every turn, and an
    # underfed herd shrinks (and takes its yield down with it). It also still escapes if unstaffed.
    "corral": "Corral — pen this herd: half yield for ~25 turns while you build, then the best yield of any herd. But penned animals can't graze: you feed them from your larder every turn, and an underfed herd shrinks. It must stay staffed or the herd goes wild again.",
}
# One worker per −/+ stepper press.
const WORKER_STEP := 1
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
# A food module whose kind is a game trail is HUNTED, not gathered — `FoodIcons.for_site` swaps in the
# hunt glyph for it. Mirrors `MapView._draw_food_site`'s `kind == "game_trail"` test.
const FOOD_SITE_KIND_GAME_TRAIL := "game_trail"
# Appended to a clickable Current-actions row's tooltip: the row's LABEL is an inline link that jumps
# the map to the source being worked (a forage tile, or a hunted herd's CURRENT tile). Scout/Warrior
# are band-wide roles with no tile, so their rows stay plain labels and never carry this.
const SOURCE_ROW_FOCUS_HINT := "Click to show this source on the map."
# The same affordance on an Active-expeditions row (the whole row is the button there).
const EXPEDITION_ROW_FOCUS_HINT := "Click to show this expedition on the map."
# Per-source food yield readout on the allocation rows. Yields are food/turn floats; render to
# 2 decimals with an explicit sign ("+0.31 /turn").
const YIELD_DECIMALS := 2
const YIELD_PER_TURN_SUFFIX := " /turn"
# Overhunting flag: a worked source whose actual take exceeds its renewable-sustainable ceiling by
# more than this epsilon is overdrawing (depletable herds only — forage is renewable, actual ==
# sustainable, so it never trips). Shown as a WARN-tinted ⚠ on the row + spelled out in the tooltip.
const OVERHUNT_EPSILON := 0.001
const OVERHUNT_FLAG := "⚠"
const YIELD_TOOLTIP_RENEWABLE := " · renewable"
const YIELD_TOOLTIP_OVERDRAW := " — overdrawing"
# Overstaffing (wasted labor) — DISTINCT from the ⚠ overdraw flag above. Every policy caps a
# source's take at its ceiling (policy ceiling / resource biomass), so past `workers_needed`
# extra workers produce nothing HERE and should move elsewhere. A source can be overstaffed while
# perfectly sustainable (and overdrawn while fully used), so this reads as its own WARN-tinted note
# on the row rather than borrowing the ⚠. `workers_needed == 0` (rehydrated save)
# means "unknown" ⇒ no note, never a wrong one.
const OVERSTAFF_NOTE_FORMAT := " · only %d of %d working"
const OVERSTAFF_TOOLTIP := "Overstaffed — this source's yield is capped at its sustainable/policy ceiling; the extra workers produce nothing here. Reassign them to another source."
# Joins the yield readout and the overstaffing explanation into one row tooltip.
const TOOLTIP_LINE_SEPARATOR := "\n"
# UNDERSTAFFING (`LaborAssignment.wastedYield`): provisions the source OFFERED that the crew could not
# collect — the party is under-crewed for the kill (an animal too big to fully carry, or an
# over-abundant pulse) and food is left standing. Muted (INK_FAINT), the low-key mirror of the
# WARN-amber overstaff note. Below FOOD_FLOW_MIN ⇒ hidden (0 on a rehydrated save).
const WASTED_NOTE_FORMAT := " · %s wasted"
const WASTED_TOOLTIP := "Under-crewed — this source offered %s the party couldn't carry home. Add workers to collect it."
# A MANAGED hunt source's crew are HERDERS, not a hunt party (`workersNeeded` = max(herders, haulers),
# scaling with herd size). The local stepper labels them so a pen needing several keepers doesn't read
# as a hunt-party bug. See `_is_managed_hunt_source`.
const HUNT_CREW_LABEL := "Hunters"
const HERD_CREW_LABEL := "Herders"
# A policy button carries its per-policy metric TWICE: a bare COMPACT string on the one-line button face
# (glyph + metric, no name — so all six rungs fit one docked row) and the VERBOSE full string in the
# tooltip (led by the policy name). Each `*_policy_takes` helper emits both as a `{compact, full}` pair.
#
# EXTRACTIVE (Sustain/Surplus/Market/Eradicate) on the LOCAL-hunt AND forage pickers: the source's CAP
# for that policy (worker-independent), NOT the crew's carry-aware delivered take (that's the preview
# line). The compact face is the bare signed rate (`+0.90`); the full tooltip frames it as "up to X/turn"
# so it reads as the ceiling it is, distinct from the honest per-crew line below, and so the four
# extractive rungs read as ASCENDING (Sustain < Surplus < Market < Eradicate). FOOD units (the
# cross-source comparison axis), so it keeps `_format_signed`, exactly as the expedition metric does.
# The compact form is just `_format_signed(rate)`; only the verbose tooltip form needs a format string.
const POLICY_CAP_FORMAT := "up to %s/turn"
# The INVESTMENT rungs (Cultivate/Sow, Tame/Corral) wear a metric too, but it is not an immediate take
# like the extractive rate — it is the PAYOFF the preparation builds TOWARD (the tended/field/pastoral/
# corral yield). A leading arrow marks it on the compact face (`→+1.20`, distinct from an extractive
# rate and never a rung you'd out-earn today); the full tooltip spells it "builds toward X/turn".
const POLICY_PAYOFF_COMPACT := "→%s"
const POLICY_PAYOFF_FULL_FORMAT := "builds toward %s/turn"
# The EXPEDITION picker wears the SAME "up to X/turn" cap metric as the local hunt + forage pickers
# (`POLICY_CAP_FORMAT` via `_extractive_take`): each policy's MAX obtainable food/turn, computed in
# `_expedition_policy_takes` as the max over party sizes of delivered_food / trip_turns. No bespoke
# raid-animals face any more — the three pickers read identically.
# PRE-COMMIT YIELD FORECAST on the assign controls (%ForageAssignControls / %HerdAssignControls).
# The overstaffing note above is POST-HOC — it tells you a turn later that workers were wasted. The
# forecast is the same truth shown WHILE COMPOSING: the sim exports, for the forage patch and the
# herd alike, a `per_worker_yield` plus one take ceiling per policy (the patch as scalar fields, the
# herd as its `hunt_policy_ceilings` list) — all food/turn at the source's CURRENT biomass and at
# output_multiplier 1.0:
#     expected(workers, policy) = min(workers × per_worker_yield, ceiling[policy]) × band output
#     max_useful_workers(policy) = ceil(ceiling[policy] / per_worker_yield)
# The ceilings are already biomass-clamped, so that `min` IS the take. The worker stepper caps at
# max-useful (the `+` goes dead there, explained by MAX_USEFUL_NOTE_FORMAT) so over-assignment is
# impossible up front; the post-hoc note still covers a source whose biomass FELL after staffing.
# max_useful is independent of the band's output multiplier — it scales both terms linearly.
const FORECAST_PER_WORKER_KEY := "per_worker_yield"
const FORECAST_CEILING_KEYS := {
    "sustain": "ceiling_sustain",
    "surplus": "ceiling_surplus",
    "market": "ceiling_market",
    "eradicate": "ceiling_eradicate",
    # The INVESTMENT rungs' ceiling is the DIP yield paid while the patch is being prepared — so the
    # same expected(workers, policy) math shows the cost of the investment while composing.
    "cultivate": "ceiling_cultivate",
    # Plant rung 3. Its OWN field rather than reusing `ceiling_cultivate`: the two plant rungs' dips
    # are independently tunable, and folding them onto one number would pass every forecast==actual
    # test by coincidence and lie the moment either rung is retuned.
    "sow": "ceiling_sow",
    # NOTE — this dict is the FORAGE PATCH's ceiling map, and ONLY that. A patch carries no policy
    # list, so a scalar field per rung is its whole representation. Every HERD policy — the four
    # extractive rungs plus `tame` and `corral` — resolves instead through the `hunt_policy_ceilings`
    # LIST via `_hunt_policy_ceiling`; the herd's matching scalars are deprecated schema slots and are
    # no longer decoded. That's why `tame` and `corral` are absent here (their payoffs, `pastoral_yield`
    # / `corral_yield`, ARE real scalars and live in FORECAST_PAYOFF_KEYS). Adding a herd rung here
    # would read a field the wire no longer carries and quote a 0 dip.
}
# The PAYOFF the investment buys — the food/turn the source pays once prepared (one worker suffices).
# Only the investment rungs have one; an extractive rung's forecast is a single number.
#
# `tame` → `pastoral_yield`: the sim now exports the Tame rung's payoff (the pastoral MSY once the herd
# is tamed), the pastoral twin of `corral_yield`, so Tame renders the same dip→payoff pair as its three
# siblings. Tame's DURING-BUILDING dip has no scalar ceiling field (there is no `ceilingTame`); it rides
# the `hunt_policy_ceilings` LIST, so `_forecast_inputs` resolves Tame's dip through `_hunt_policy_ceiling`
# rather than a `FORECAST_CEILING_KEYS` scalar (adding a key there would silently quote Sustain's ceiling).
const FORECAST_PAYOFF_KEYS := {
    "cultivate": "tended_yield",
    "corral": "corral_yield",
    "sow": "field_yield",
    "tame": "pastoral_yield",
}
# The INVESTMENT rungs by name — "does this rung trade a dip now for a better source later?". This is
# the test for *which yield row a rung gets*, and it is deliberately NOT `policy in
# FORECAST_PAYOFF_KEYS`: `tame` is an investment rung that has no quotable payoff (above), so the
# payoff map cannot answer this question. An investment rung must never render the extractive
# "renewable / ⚠ overdraws the herd" preview — it is drawn sustainably by construction, and the
# verdict would argue with the dip row.
const INVESTMENT_POLICIES := ["cultivate", "sow", "tame", "corral"]
# The RUNNING COST the payoff is paid against. Only the pen has one: a corralled herd is a managed
# population that eats from the keeper's larder every turn (`pen_upkeep`), and `corral_yield` is the
# GROSS take with that feed NOT deducted — so advertising the payoff bare would promise a number the
# player never banks. A tended patch has no running cost, hence no entry.
const FORECAST_FEED_KEYS := {
    "corral": "pen_upkeep",
}
# The investment forecast states the DEAL, not a single yield: "Preparing: +0.09 /turn → then +1.20 /turn".
# Tame renders through it too (dip from `hunt_policy_ceilings["tame"]`, payoff = `pastoral_yield`), with
# no feed term (Tame has no running cost).
const INVESTMENT_FORECAST_FORMAT := "Preparing: %s → then %s"
# The same deal for a rung that also carries a running feed cost:
#   "Preparing: +0.75 /turn → then +5.40 /turn − 1.74 feed"
# `pen_upkeep` answers "what would this pen cost?" for an UNPENNED herd too (a projection at the
# herd's current biomass, on the SAME basis `corral_yield` uses — see `fauna::pen_upkeep`), so the
# pre-commit row quotes the real running cost at the moment the player actually decides. The
# subtraction is a pure difference of two numbers the sim exported for THIS herd; the client models
# no ecology. (Before the sim exported that projection this row said "before feed"; it no longer
# has to.) A herd with no `pen_upkeep` (no pen feed to charge) degrades to the plain no-feed format
# above rather than printing a fabricated "− 0.00 feed".
const INVESTMENT_FORECAST_FEED_FORMAT := "Preparing: %s → then %s − %s feed"
# A ZERO PAYOFF IS DATA, NOT A MISSING NUMBER — and it is the single most valuable thing this row can
# say. The pen's harvest is constant ESCAPEMENT (take only the biomass standing above `K/2`), so a
# herd at or below the MSY point honestly pays **0.00** until it rebuilds: penning it would eat feed
# every turn and pay nothing. That must never be suppressed, blanked, or em-dashed away — a player
# who pens a depleted herd because the UI declined to show them a zero has been actively misled. So
# the zero renders in full, and the row EMPHASIZES it: WARN-amber instead of income-green, plus this
# note naming the remedy (let it rebuild). The feed line still shows, because the feed is what makes
# a zero payoff a net LOSS rather than merely a nothing.
# (The "is it zero" floor is the shared `FOOD_FLOW_MIN` — one definition of "below this, there is no
# flow here", used by the band ledger's rows and by this row alike.)
const INVESTMENT_FORECAST_DEPLETED_NOTE := "⚠ Too depleted to pen — it would eat feed and pay nothing until the herd rebuilds."
# How a forecast dict SPELLS its field keys — a key spelling, nothing more.
#
# Two dict shapes carry them BARE and so share one prefix: a herd dict, and the RAW wire
# forage-patch dict (decoded in native `forage_patches_to_array`, stored in `_forage_patch_lookup`,
# and read by the Current-actions Forage row). Only `tile_info` carries the patch's fields under a
# `patch_` prefix, because that is a cross-ref MapView stamps on in `_tile_info_at`.
#
# ⚠ A PREFIX CANNOT IDENTIFY A SOURCE KIND — that is why the bare case is ONE const and not two
# same-valued ones. It used to be two (a `HERD_*` and a `WIRE_FORAGE_PATCH_*`, both `""`), and
# having a herd-sounding name for the empty string invited `prefix == HERD_…` as an "is this a
# herd?" test; it read as discriminating and was not, so it silently routed forage patches down the
# herd branch and left the `+` button dead on every Current-actions Forage row. Pass `SOURCE_KIND_*`
# when you need the kind; a prefix only ever tells you how to spell a key.
const BARE_FORECAST_PREFIX := ""
const FORAGE_FORECAST_PREFIX := "patch_"
# WHICH KIND OF SOURCE a forecast dict describes. Stated explicitly by every `_forecast_inputs`
# caller because it CANNOT be recovered from the dict: the two kinds share a key prefix (see the
# warning above), and a shape test (`has("hunt_policy_ceilings")`) would misread a herd whose
# snapshot omitted the list as a forage patch. The caller knows what it fetched; it says so.
const SOURCE_KIND_HERD := "herd"
const SOURCE_KIND_FORAGE := "forage"
# Below this a worker produces nothing here (a dead-season forage tile with no forecast fields).
# Dividing by it would blow max-useful up to infinity, so instead: no forecast row,
# and the stepper keeps its plain idle-worker cap.
const FORECAST_MIN_PER_WORKER := 0.0001
# Sentinel for "no forecast data" → the stepper is not forecast-capped.
const MAX_USEFUL_UNBOUNDED := -1
# A whole-animal hunt's kill-credit bank accumulates the smoothed take, then discharges a WHOLE animal
# when it holds a full body. Worst case the turn's rate lands with just under one body already banked,
# so one extra whole animal drops that turn beyond floor(rate / body) — this is that +1.
const HUNT_PEAK_DROP_BANK_BONUS := 1
# A tended patch / corralled herd collapses max-useful to exactly 1, so this note has to read
# "max 1 worker" — pluralize the noun rather than shipping "max 1 workers".
const MAX_USEFUL_NOTE_FORMAT := "max %d %s useful here — more would be idle"
const MAX_USEFUL_NOUN_ONE := "worker"
const MAX_USEFUL_NOUN_MANY := "workers"
# A Current-actions row's `+` disabled because the SOURCE is fully staffed (not because idle ran out):
# spelled out in the row tooltip rather than as a visible note, to keep the compact row uncluttered.
const MAX_USEFUL_CAPPED_TOOLTIP := "Fully staffed — this source can use at most %d %s; more would idle here."
# The OTHER binding cap: idle workers run out BELOW the usefulness ceiling, so the `+` caps at labor,
# not usefulness. Named in the "N of M" spirit (N = the labor cap you're at, M = the useful ceiling),
# so a capped `+` reads as "fixable by reassigning labor" rather than as a silent bug.
const LABOR_BOUND_NOTE_FORMAT := "%d of %d useful — free up idle workers to send more"
# The expedition sub-case where freeing idle workers WOULD NOT help: the party-size cap binds
# (idle >= max party), so the advice is wrong — say we're at the party limit instead.
const PARTY_SIZE_BOUND_NOTE_FORMAT := "%d of %d useful — at the max party size"
# Band food flow lives on the Food summary line: `Food 15 (19 turns) · −0.77 /turn` (net =
# food_income − food_consumption, sign-tinted), with a click-to-expand category breakdown
# (Gathered/Hunted/Eaten) underneath — mirroring the morale breakdown. `FOOD_FLOW_MIN` gates both
# the net readout and each breakdown category (below it → absent, not shown as a zero).
const FOOD_FLOW_MIN := 0.001
# Click-to-open disclosure shared by the Food + Morale summary rows: a ▸/▾ caret on the row label and
# a clickable `[url]` meta = `<prefix><kind>:<entity>` dispatched by `_on_detail_meta_clicked`.
#
# THE BREAKDOWN OPENS IN A POPOVER, NEVER INLINE. Expanding it in place grew the vitals label — a
# `fit_content` RichTextLabel — by several lines AFTER `_build_band_zone_content` had already chosen
# its height tier from the zone box, and the zone box is fixed by design with `clip_contents` hosts,
# so the extra lines silently sliced the WORKFORCE row and ate the role cards. A Window cannot change
# a zone's height, which is the same reason the section `⋯` menus are `MenuButton`s and the
# destructive confirms are `ConfirmationDialog`s. The work board's budgeted inline inspector strip is
# the other idiom and does not apply here: in the SHORT tier the chart is already dropped and the role
# cards are already hint-less, so there is nothing left to spend but PEOPLE/WORKFORCE — the content.
const BREAKDOWN_CARET_OPEN := "▾"
const BREAKDOWN_CARET_CLOSED := "▸"
const BREAKDOWN_TOGGLE_META_PREFIX := "breakdown:"
const BREAKDOWN_KIND_FOOD := "food"
const BREAKDOWN_KIND_MORALE := "morale"
# The detail-row labels the disclosure attaches to (must equal the `Key` in `_split_detail_kv`).
const DETAIL_ROW_FOOD := "Food"
const DETAIL_ROW_MORALE := "Morale"
# The breakdown popover's card: wide enough for the longest breakdown row (`▼ −1.74  🐄 Pen feed
# (animals)` / `▲ +1.0%  harsh terrain (Karst Cavern Mouth)`) without wrapping every line, and
# padded like the cards it borrows its stylebox from.
const BREAKDOWN_POPOVER_WIDTH := 300.0
const BREAKDOWN_POPOVER_PADDING := 10
# The gap between the clicked row and the popover's top edge, so the caret stays readable under it.
const BREAKDOWN_POPOVER_GAP := 4.0
# ---- Band/City panel identity grid ---------------------------------------------------------------
# The panel's own header already states the band's name + settlement stage, so the summary rows there
# drop the `Unit: <name>` row (a THIRD copy of the same name) and replace `Size: <n>` (population
# under another name) with the labor line — same numbers, one row, in the identity grid where they
# belong. The Occupants-card drawer (FOREIGN bands, and the no-panel ui_preview fallback) keeps
# Unit/Size: it has no panel header naming the band, and a foreign band exposes no worker breakdown.
# The population/workers LINE is gone from the summary entirely: the band zone's People and
# Workforce bars state the same numbers as two readable charts, and a text restatement above them
# was the third telling of one fact.
# Category breakdown rows under Food reuse the morale breakdown's indent + ▲/▼ glyphs, so they flow
# through the SAME `_format_detail_bbcode` indented-sub-line path (sign-tinted: ▲ income green, ▼
# eaten amber) — no inline color tags, which mis-layout between the KV table segments.
const FOOD_LABEL_GATHERED := "Gathered"
const FOOD_LABEL_HUNTED := "Hunted"
# The two DEBIT rows, deliberately separate: the people eat (`food_consumption`), and the ANIMALS in
# the band's pens eat (`pen_feed_upkeep` — a confined herd cannot graze, so its keeper hauls it food
# every turn). Both come straight off the same larder, and telling them apart is the entire readout
# of the corral-as-a-managed-population arc: a band whose larder drains because it is feeding its
# herd must be able to SEE that, not just watch the number fall.
const FOOD_LABEL_EATEN := "Eaten (people)"
const FOOD_LABEL_PEN_FEED := "%s Pen feed (animals)" % CORRAL_GLYPH
# Scouting expedition (docs/plan_exploration_and_sites.md §2). A detached party is a cohort
# tagged Expedition flowing through the same populations[] array as a band; it carries no labor
# in v1, so its drawer shows a dedicated mission/phase/party/provisions readout + Recall/Move
# instead of the labor-allocation panel. The outfit affordance (party stepper + send) lives on a
# resident band's allocation panel.
const EXPEDITION_MISSION_SCOUT := "scout"
const EXPEDITION_MISSION_HUNT := "hunt"
const EXPEDITION_PHASE_OUTBOUND := "outbound"
# One source: the phase key `awaiting` is also the status-glyph key + the orb producer's key.
const EXPEDITION_PHASE_AWAITING := FoodIcons.STATUS_AWAITING
const EXPEDITION_PHASE_HUNTING := "hunting"
const EXPEDITION_PHASE_DELIVERING := "delivering"
const EXPEDITION_PHASE_RETURNING := "returning"
const EXPEDITION_MISSION_LABELS := {
	"scout": "Scouting expedition",
	"hunt": "Hunting expedition",
}
const EXPEDITION_PHASE_LABELS := {
	"outbound": "Outbound",
	"awaiting": "Awaiting orders",
	"returning": "Returning",
	"hunting": "Hunting",
	"delivering": "Delivering",
}
# Band/City panel "Active expeditions" section — mission glyphs mirror the map markers
# (MapView EXPEDITION_GLYPH / EXPEDITION_HUNT_GLYPH).
const PANEL_EXPEDITION_SCOUT_GLYPH := "⚑"
const PANEL_EXPEDITION_HUNT_GLYPH := "🏹"
const SEND_EXPEDITION_HINT := "Detach a party to scout distant territory, then click a target tile."
const SEND_EXPEDITION_BUTTON := "Send scouting party…"
# Hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party that follows a
# migratory herd, accumulates food, and drops it at the band. Launched from a resident band by
# picking a herd (herd-target click, not a tile), and Recalled like a scout expedition.
const SEND_HUNT_EXPEDITION_HINT := "Detach a party to follow a migratory herd, then click on the herd."
# Distance-aware herd-hunt affordance (docs/plan_exploration_and_sites.md §2b): clicking a herd
# offers a LOCAL hunt when it's within the SELECTED band's hunt_reach, or a hunting EXPEDITION when
# it's beyond. One compose control (worker/party stepper + policy), two labels/commands keyed off the
# wrap-aware hex distance from the selected band's own tile.
const ASSIGN_LOCAL_HUNT_BUTTON := "Assign Local Hunt"
## The ONE label for a hunting party's send, at BOTH entry points (the herd drawer's beyond-reach
## branch and the Parties sheet) — `_style_send_hunt_button` owns the button's text in every branch,
## so a second const would only ever be the placeholder it overwrites. **No ellipsis, deliberately:**
## both callers now COMMIT on the press. The scout send keeps its `…` because it still opens tile
## targeting — the ellipsis is the form's one signal that more input is coming.
const SEND_HUNTING_EXPEDITION_BUTTON := "Send hunting party"
# Range-aware forage assign: foraging is stationary gathering (NO expedition fallback), so a tile
# beyond the selected band's `work_range` disables the button rather than offering an alternative.
const FORAGE_ASSIGN_BUTTON := "Forage"
# ---- THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§15) -------------------------------
# Composing is modal by nature — open, decide, commit, done — so the two ~270px compose blocks live
# in a floating sheet (`ui/hud/ComposeSheet.gd`) rather than permanently in the drawer. The drawer
# keeps the detail rows, gains a one-line STANDING-ASSIGNMENT summary, and ends in the button below.
const FORAGE_CREW_LABEL := "Foragers"
# `Assign foragers ▸` / `Assign hunters ▸` / `Assign herders ▸` — the noun is the same one the
# sheet's stepper uses, so the drawer and the sheet can never disagree about who is being staffed.
const COMPOSE_OPEN_BUTTON_FORMAT := "Assign %s ▸"
const COMPOSE_SHEET_EYEBROW_FORMAT := "Assign %s"
# The standing staffing being edited, shown INSIDE the sheet (the header carries verb + subject).
const COMPOSE_NOW_STAFFED_FORMAT := "Now %d%s"
const COMPOSE_PENDING_SUFFIX := " · pending"
# The drawer's one-line summary of what is ALREADY standing on this source: `♻ 3 foragers · +2.74
# /turn`. The rate comes from `_source_yield_readout` — never recomputed here.
const STANDING_SUMMARY_FORMAT := "%s %d %s"
const STANDING_SUMMARY_SEPARATOR := " ·"
const COMPOSE_KIND_NONE := ""
const COMPOSE_KIND_FORAGE := "forage"
const COMPOSE_KIND_HERD := "herd"
# Generic section header for the outfit block (hosts both the scout + hunt send verbs).

# ---- Band/City panel zones (docs/band_panel_ux_proposal.html) ---------------
## Gap between a zone's top-level sections (vitals / people / outlook / workforce), and the tighter
## gap between the parts of one section (bar → key → cards).
const ZONE_SECTION_SEPARATION := 12
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

## Zone section headers (uppercased by `_alloc_section_label`).
const ZONE_HEADER_PEOPLE := "People"
const ZONE_HEADER_WORKFORCE := "Workforce"
const ZONE_HEADER_WORK := "Work"
const ZONE_HEADER_PARTIES := "Parties"

## Stacked-bar geometry, shared by the People and Workforce bars.
const COMPOSITION_BAR_HEIGHT := 9.0
const COMPOSITION_BAR_SEPARATION := 2
## A present-but-tiny segment still needs a visible sliver, so the stretch ratio never falls to 0.
const COMPOSITION_MIN_RATIO := 1.0
const COMPOSITION_KEY_SEPARATION := 12
const COMPOSITION_KEY_FONT_SIZE := 11
const COMPOSITION_SWATCH_SIZE := Vector2(8.0, 8.0)
const COMPOSITION_SWATCH_SEPARATION := 4

## PEOPLE key glyphs + words (the words live in the tooltips the glyphs replaced).
const PEOPLE_GLYPH_CHILDREN := "👶"
const PEOPLE_GLYPH_WORKING := "🛠"
const PEOPLE_GLYPH_ELDERS := "🧓"
const PEOPLE_LABEL_CHILDREN := "children"
const PEOPLE_LABEL_WORKING := "working age"
const PEOPLE_LABEL_ELDERS := "elders"
## Dependency ratio: dependents per this many working-age adults.
const PEOPLE_DEPENDENCY_BASE := 100
## Above this many dependents per 100 workers the band carries more mouths than hands → WARN.
const PEOPLE_DEPENDENCY_HEAVY := 100
## The chip says the COUNT, not the ratio. `dep 88/100` was the analyst's framing of a number the
## player has to act on — it reads as a score out of 100 (and the game's designer could not tell what
## it meant), while the bar beside it already shows the split. "14 dependents" is the fact; the ratio
## and what it implies live in the tooltip, which is where the teaching belongs.
const PEOPLE_DEPENDENCY_FORMAT := "%d dependents"
## SHORT on purpose: the chip's face already carries the count, so the tooltip only has to say what a
## dependent IS and who carries them. The long version (which also quoted the ratio) explained the
## jargon without making it any more useful — the ratio itself is gone from the UI entirely.
const PEOPLE_DEPENDENCY_TOOLTIP := """Children and elders — they eat from the larder but cannot be put to work.
%d working-age adults support them."""
## Appended when dependents outnumber workers — the reason the chip is WARN-tinted.
const PEOPLE_DEPENDENCY_HEAVY_TOOLTIP := "\nMore mouths than hands."

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
const ROLE_CARD_PADDING := 6
const ROLE_CARD_CORNER_RADIUS := 4
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
const WORK_ROW_PADDING_H := 4
const WORK_ROW_PADDING_V := 2
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

const PAGER_PREV_GLYPH := "‹"
const PAGER_NEXT_GLYPH := "›"
const PAGER_PREV_TOOLTIP := "Previous page"
const PAGER_NEXT_TOOLTIP := "Next page"
const PAGER_FORMAT := "Page %d / %d"
const PAGER_RANGE_FORMAT := "%d–%d of %d"

## PARTIES zone.
const PARTIES_HEADER_FORMAT := "%d out · %d workers"
const PARTIES_EMPTY_HINT := "No parties in the field."
const PARTY_MENU_TOOLTIP := "Bulk actions for parties in the field."
const PARTY_RECALL_GLYPH := "✕"
const PARTY_RECALL_TOOLTIP := "Recall — the party walks home"
const PARTY_RECALL_WIDTH := 24.0
## The per-row recall stays VISIBLE (parties have no other removal path) but rests dimmed, so it
## reads as available without competing with the row it sits on.
const PARTY_RECALL_REST_ALPHA := 0.45
const PARTY_RECALL_ALL_FORMAT := "Recall all parties (%d)"
const PARTY_RECALL_CONFIRM_FORMAT := "Recall all %d parties? They walk home carrying what they have."
const PARTY_RECALL_CONFIRM_OK := "Recall all"
const SEND_PARTY_NO_IDLE_REASON := "No idle workers to spare. Free some from Work."

## The compose sheet — MISSION FIRST: the footer launches straight into a mission, so the sheet is
## always already on one and the policy picker is unreachable except under Hunt.
const COMPOSE_MISSION_SCOUT := "scout"
const COMPOSE_MISSION_HUNT := "hunt"
const COMPOSE_MISSION_LABEL_SCOUT := "⚑ Scout"
const COMPOSE_MISSION_LABEL_HUNT := "🏹 Hunt"
const COMPOSE_TITLE_SCOUT := "Setup a scouting party…"
const COMPOSE_TITLE_HUNT := "Setup a hunting party…"
const COMPOSE_FIELD_PARTY := "Party"
const COMPOSE_FIELD_POLICY := "Policy"
## The QUARRY is the hunt form's FIRST question: the herd sets the useful party size, the per-policy
## take and the trip length, so every field below it is unanswerable until it is picked.
const COMPOSE_FIELD_QUARRY := "Quarry"
const COMPOSE_QUARRY_CHOOSE := "Choose…"
const COMPOSE_QUARRY_HINT := "Choose a quarry — the rest of the form follows from it."
const COMPOSE_QUARRY_TOOLTIP_FORMAT := "%s (%d, %d)\nClick to choose a different herd."
const COMPOSE_QUARRY_LABEL_FORMAT := "%s %s"
## The refusal when the player picks a herd the band can already work from home. The hunt_reach split
## is a rule the map does not spell out, so the refusal is where it gets taught — it names the herd,
## the distance, the reach that binds and the local alternative.
const QUARRY_WITHIN_REACH_FORMAT := "%s is %d tiles away — inside %s's hunt reach (%d). Hunt it from the herd itself instead of sending a party."
const COMPOSE_OF_IDLE_FORMAT := "of %d idle"
const COMPOSE_CANCEL_TOOLTIP := "Cancel"

## `cancel_order` scopes (the server grammar: `cancel_order <faction> [band] [all|work|roles]`).
## `work` clears Forage + Hunt only — standing roles, parties and an in-progress move survive.
## A policy picker rendered INSIDE a zone wraps to this many columns — four rungs abreast do not fit
## a 380px L/R dock, and a picker wider than its zone drags the whole zone column past its host.
const ZONE_POLICY_PICKER_COLUMNS := 2

const CANCEL_SCOPE_ALL := "all"
const CANCEL_SCOPE_WORK := "work"
const CANCEL_SCOPE_ROLES := "roles"
# The hunt party's carry-ceiling FULL badge (shown in the hunt panel when carried ≥ cap).
const HUNT_FULL_BADGE := "· FULL"
# The launch policy (Sustain/Surplus/Market/Eradicate) chosen for a hunting EXPEDITION, with a
# one-line behaviour hint so the choice is legible. Reuses `LABOR_HUNT_POLICIES` for the option set.
#
# An expedition's Hunting arm credits **FOOD ONLY** — no husbandry accrual, no trade goods (a known v1
# gap, tracked server-side). So these hints promise NEITHER, even though the resident-band versions of
# Sustain and Market do exactly those things (see `LOCAL_HUNT_POLICY_HINTS`). The asymmetry is real;
# blurring it would have the UI promise the player a payoff the sim never pays.
const SEND_HUNT_POLICY_HINTS := {
	# Sustain is the MAXIMUM SUSTAINABLE YIELD flow — the same per-turn skim a resident band's Sustain
	# hunt takes, so the herd stays healthy indefinitely. The trade-off is speed: MSY is a small flow,
	# so a party fills slowly, and on a small herd the trip may not be worth sending. The per-herd
	# turns-to-fill forecast (shown at the herd-targeting step) is the number that decides it. It does
	# NOT tame the herd — only a resident band's Sustain hunt builds husbandry.
	"sustain": "Sustain — takes only the herd's sustainable yield; it stays healthy forever, but the party fills slowly on a small herd.",
	"surplus": "Surplus — takes the herd's spare stock, so the party fills fast; the herd declines.",
	"market": "Market — grinds the herd down with repeated trips; the party still hauls home food, not trade goods.",
	"eradicate": "Eradicate — denial mission: hunts the herd toward extinction and delivers no food.",
}
# A resident BAND and a detached EXPEDITION are told apart by the sim, and the client reads a DIFFERENT
# herd field for each — never one for the other:
#   `hunt_policy_ceilings`  {policy → provisions/turn} — the BAND's renewable FLOW ceiling. With the
#       cohort's levers this makes the LOCAL hunt preview pure arithmetic (see `_hunt_take_rate`).
#   `hunt_trip_estimates`   {"<policy>:<workers>" → {turns_to_fill, delivers_food}} — the sim's
#       PRE-LAUNCH TRIP ESTIMATE, forward-simulated server-side. An expedition's trip length is NOT a
#       rate division: on Surplus/Market the ceiling is a *stock*, so the party strips the headroom in
#       a turn or two and then crawls at the herd's regrowth trickle. A re-derived `carryCap / rate`
#       closed form is wrong, and wrong by a lot — on a FULL Rabbit Warren under Surplus only a LONE
#       hunter fills at all (23 turns); a party of 4 never fills within the sim's horizon. So the
#       client does ZERO arithmetic here — it looks the answer up. Never re-derive it.
const HERD_BAND_CEILINGS_KEY := "hunt_policy_ceilings"
const HERD_TRIP_ESTIMATES_KEY := "hunt_trip_estimates"
# `hunt_trip_estimates` is keyed "<policy><sep><party_workers>" — the sim's key format, mirrored by
# `_hunt_estimate_key` so the single-cell lookup and the whole-row scan can never disagree on it.
const HUNT_ESTIMATE_KEY_SEPARATOR := ":"
# (The denial case — an Eradicate party hunts the herd toward extinction and carries NOTHING home —
# is NOT inferred from the policy string: the estimate itself carries `delivers_food = false`, so the
# sim, not the client, decides which policies are denial missions.)
# Pre-launch hunt-trip forecast (shown in the targeting banner while a hunt expedition is armed and
# the player hovers a herd, and live above the herd panel's Send button). It is a PURE TABLE LOOKUP
# into the sim-exported per-(policy, party-size) `hunt_trip_estimates` carried on the herd — each cell
# {policy, party_workers, turns_to_fill, delivers_food}, where `turns_to_fill == 0` means the party
# does NOT fill within the sim's `forecast_horizon_turns`. The client reads the cell and stops (see
# `_hunt_trip_forecast`); the only thing it computes is the display verdict:
#     viable = turns <= expedition_viability_warn_turns   (the band's own exported lever)
# THE CLIENT DOES ZERO ARITHMETIC FOR AN EXPEDITION, and must NEVER divide a carry cap by a take rate.
# The sim FORWARD-SIMULATES the trip — the herd's state moves under the party, its stock exhausts, and
# a horizon bounds the answer — so any client-side re-derivation drifts from the take the sim actually
# performs. That forward simulation is the only honest number (pinned by core_sim/tests/expedition_hunt.rs).
# This does NOT mean the client does no math anywhere: the LOCAL (resident band) per-turn yield preview
# IS legitimate arithmetic — `min(workers × hunt_per_worker_provisions, band_ceiling) × output_multiplier`
# over `hunt_policy_ceilings`, the BAND flow ceiling (`_hunt_take_rate` / `_local_hunt_preview_bbcode`,
# pinned by exported_snapshot_fields_reproduce_band_hunt_take). Band = flow arithmetic; expedition = lookup.
# A hunting expedition is a GREEDY RAID: it grabs the herd's standing surplus above the policy's floor
# in a burst and comes home. So the headline is the PAYLOAD — the whole animals the raid delivers over
# the turns it takes: "delivers ≈5 Wild Boar over ≈7 turns". `animals` is `HuntTripEstimate.animalsTaken`
# (the sim's forward-simulated answer), `turns` is `turnsToFill` — now "turns until the raid comes home",
# NOT "turns to fill the pack" (a big party leaves a partial pack once it strips the surplus).
const HUNT_FORECAST_DELIVERS_FORMAT := "delivers ≈%d %s over ≈%d turns"
# `turnsToFill == 0` no longer means "won't fill" — under the raid model it means the raid ran the whole
# forecast horizon still delivering (a slow breeder a big party can neither fill nor exhaust). The
# client has no horizon lever, so it words this "over many turns" rather than a bare number.
const HUNT_FORECAST_LONG_RAID_FORMAT := "delivers ≈%d %s over many turns"
# The FOOD the delivered animals are worth (`animals × HerdTelemetryState.foodPerAnimal`), appended so
# the party-size tradeoff reads BOTH ways: a bigger party takes more animals AND more food. Omitted when
# the herd carries no `food_per_animal` (older snapshot) — a live guard, no fake zero.
const HUNT_FORECAST_FOOD_FORMAT := " · ~%d food"
# A finite raid past the band's `expedition_viability_warn_turns` — it still delivers, just slowly. A
# real tradeoff (told, then trusted), so the line stays WARN-amber and the button stays enabled.
const HUNT_FORECAST_SLOW_SUFFIX := " — a slow raid"
# Travel is NOT in `turnsToFill` — that now counts HUNTING turns only (once the party is in reach). The
# round trip out to the herd and back is band-relative (the per-herd estimate table is band-agnostic, so
# it cannot carry it), so the client adds it: ceil(2 × wrap-aware hex_distance(band, herd) /
# band_move_tiles_per_turn), the SAME formula the server's launch feed uses. When travel > 0 the headline
# turns is the TOTAL and this breakdown spells the split out; when 0 the headline is just the hunting turns.
const HUNT_FORECAST_TRAVEL_BREAKDOWN := " (%d hunting + %d travel)"
# The long-raid line has no bounded hunting-turn count ("over many turns"), so travel rides as a trailing
# "(+T travel)" rather than a two-part split.
const HUNT_FORECAST_LONG_TRAVEL_SUFFIX := " (+%d travel)"
# The ONE non-viable case under the raid model: `animalsTaken == 0` — the herd is at/below the policy's
# floor, so there is no standing surplus to raid and the party would return empty. NOT a "won't fill"
# verdict (the raid always completes); the herd simply has nothing to give this policy right now.
const HUNT_FORECAST_NO_SURPLUS_FORMAT := "%s is too lean to raid — its surplus is spent"
# An Eradicate expedition is a DENIAL mission, not a failed raid: it delivers no food BY DESIGN (the sim
# says so via `delivers_food`, the client never infers it from the policy string).
const HUNT_FORECAST_DENIAL_FORMAT := "%s — denial mission: hunts the herd toward extinction, delivers no food"
const HUNT_FORECAST_WARN_GLYPH := "⚠ "
# Sentinel for "the snapshot doesn't carry the levers/ceiling this forecast needs" (older server).
# A real take rate / ceiling is always ≥ 0, so a negative reads unambiguously as absent → the caller
# renders NO forecast line rather than a misleading zero.
const HUNT_RATE_UNAVAILABLE := -1.0
# The herd panel's SECOND entry point into a hunting expedition: selecting a herd BEYOND the band's
# hunt_reach composes party + policy right in the panel and sends immediately — no targeting step, so
# the banner (and its forecast) never appears. Everything is known at compose time and the block
# re-renders on every stepper tick / policy click, so the SAME forecast renders LIVE above the button.
# When the trip is a trap, the button itself names the cost (amber "armed"); it is NEVER disabled and
# never gated behind a confirm — the player can always send. This is information, not a gate.
const SEND_HUNT_ANYWAY_TURNS_FORMAT := "Send Anyway (≈%d turns)"
# A LONG raid (`turnsToFill == 0`, ran the whole horizon still delivering) still lands animals — enabled,
# but the button names it a long haul rather than quoting a turn count the client can't bound.
const SEND_HUNT_LONG_RAID_BUTTON := "Send Anyway (long raid)"
# The ONE blocked case: `animalsTaken == 0`, the herd has no surplus above the policy's floor. A raid
# that returns empty is a mistake with no upside (unlike a slow-but-delivering one), so the button is
# DISABLED and says why + the way out. Party size can't fix it — surplus is a property of the HERD, not
# the party — so the reason names no alternative size (the old row-scan is meaningless under the raid).
const SEND_HUNT_NO_SURPLUS_BUTTON := "Herd too lean to raid"
const SEND_HUNT_NO_SURPLUS_REASON := "%s has no surplus above this policy's floor — the raid would return empty. Wait for the herd to rebuild, ease the policy, or hunt it locally."
# Eradicate's button states the deal rather than implying failure — the mission IS the point.
const SEND_HUNT_DENIAL_BUTTON := "Send (delivers no food)"
# Live per-turn yield preview for the LOCAL hunt branch. A resident hunt has no carry cap, so
# turns-to-fill is meaningless there; the number that decides a standing assignment is the food/turn
# it will produce — the sim's hunt take:
#     rate = min(workers × hunt_per_worker_provisions, ceiling_for(policy)) × output_multiplier
# The band applies its morale/discontent productivity modifier (`output_multiplier`) at payout; a
# detached expedition does not, which is why the two branches show different numbers from the same
# exported fields. (pinned sim-side by core_sim/tests/expedition_hunt.rs.)
const LOCAL_HUNT_YIELD_FORMAT := "≈ %s"
# The Sustain ceiling IS the herd's sustainable yield, so a take above it draws the herd down — flagged
# with the same ⚠ / WARN amber. This is the COMPOSE preview, which derives the flag from the steady
# forecast via `_is_overdraw` (there is no assignment yet, so no wire `overdraws` field); the CONFIRMED
# allocation rows instead read the sim-answered `overdraws` bool off the assignment.
const LOCAL_HUNT_OVERDRAW_SUFFIX := " — overdraws the herd"
# The FORAGE twin of the hunt overdraw suffix: a take above the patch's Sustain ceiling draws its
# biomass down. Forage is smooth food (no whole-animal rhythm), so the preview shows a bare rate + this.
const LOCAL_FORAGE_OVERDRAW_SUFFIX := " — overdraws the patch"
# CARRY-AWARE ANIMALS-FIRST preview. A hunt delivers WHOLE animals via a kill-credit bank, so an
# unquantized food/turn rate credits fractional-animal throughput the crew can never carry home (the sim
# itself quantizes to whole bodies). The line instead leads with the honest carry-aware delivered rate in
# ANIMALS: `≈<rate> <animal>/turn`, rate = delivered ÷ food_per_animal (`_hunt_delivered_and_waste`).
const HUNT_DELIVERED_FORMAT := "≈%s %s/turn"
# When a kill can't be fully carried (a big animal the crew is too small to haul) the surplus meat rots.
# A WARN-tinted suffix flags the fraction wasted — its OWN concern, rendered amber even on a green line.
const HUNT_WASTE_SUFFIX_FORMAT := " · ⚠ %d%% wasted"
# The delivered animals-per-turn rate is a long-run average of lumpy whole-animal delivery — you take
# WHOLE animals, so per-turn delivery varies. A STABLE, worker-independent disclaimer (always shown on an
# extractive hunt rung) naming the averaging span, computed from the SELECTED policy's ceiling by
# `_hunt_avg_window_turns` so it never blinks out as workers change (a faster policy averages over a
# different span, so the line reflects the composed action, not a Sustain-wide claim).
const HUNT_AVG_WINDOW_FORMAT := "This estimate is a long-run average over ~%d turns — you take whole animals, so per-turn delivery varies."
# The averaging window's upper clamp: near-integer animals/turn rates make the "extra animal" cycle span
# read absurdly long, so cap it at a plausible span.
const HUNT_WINDOW_MAX_TURNS := 12
# Animals-per-turn rate formatting: up to 2 decimals, trailing zeros/dot stripped (1.90→"1.9", 1.00→"1",
# 0.65→"0.65"). `String.num` already trims (unlike the padded food-rate formatter).
const HUNT_ANIMAL_RATE_DECIMALS := 2
# Tile-card PASTURE rows (the graze layer). The twin of `Forage biomass`, and the pair is the point:
# forage is what HUMANS can eat here (seeds, nuts, tubers — food-module tiles only), pasture is what
# ANIMALS can eat here (grass and browse — cellulose humans cannot digest, on nearly every land tile).
# Your best farm is usually not your best pasture. Rendered ONLY where the ground actually carries
# pasture (`graze_capacity > 0`): on a glacier the card prints nothing, never "0 / 0".
const PASTURE_KEY := "Pasture"
# Its own row key rather than the shared "Ecology" one — a forage tile would otherwise show two rows
# both called "Ecology" (the patch's and the pasture's) with no way to tell them apart. The LABEL and
# the TINT are still the shared `_ecology_phase_label` / `_ecology_value_hex` path, so a stressed
# pasture reads exactly like a stressed herd or a stressed patch.
const PASTURE_ECOLOGY_KEY := "Pasture ecology"
# Tile-card SIGHT row — the player must ALWAYS be able to tell "there is nothing here" apart from
# "I cannot see what is here". Herds/bands are LIVE state and are fog-gated out of `tile_info`
# (MapView._herds_on_tile), so on a remembered hex an empty Occupants list would otherwise read as
# "empty" when the truth is "unknown". These three rows name the hex's sight state in plain words,
# and `OCCUPANTS_UNKNOWN_*` replaces the roster with the honest statement.
# Sim-side the states are Active / Discovered / Unexplored.
# The FoW states MapView tags onto `tile_info.visibility_state` (mirrors `_visibility_state_at`).
# Empty string = FoW disabled → everything is in sight, and the Sight row is omitted entirely.
const VISIBILITY_ACTIVE := "active"
const VISIBILITY_DISCOVERED := "discovered"
const VISIBILITY_UNEXPLORED := "unexplored"
const TILE_SIGHT_KEY := "Sight"
const TILE_SIGHT_ACTIVE := "In sight"
const TILE_SIGHT_REMEMBERED := "Remembered — not in sight now"
# The chip FACE for the remembered state. A pill states a condition in a word; the full sentence
# above is a sentence — it was the widest element in the strip — so it rides the chip's tooltip.
const TILE_SIGHT_REMEMBERED_SHORT := "Remembered"
const TILE_SIGHT_UNEXPLORED := "Unexplored"
# Shown INSTEAD of the Occupants roster on a hex the player cannot currently see. Never render an
# empty roster there — an absent list is a claim of emptiness the client cannot back up.
const OCCUPANTS_UNKNOWN_TITLE := "out of sight"
const OCCUPANTS_UNKNOWN_REMEMBERED := "You remember the ground here, but not what's on it now — bands and herds move. Scout it to see."
const OCCUPANTS_UNKNOWN_UNEXPLORED := "Nobody has been here. Send a band to reveal what's on this ground."
# Your OWN party can stand on a hex it cannot see (a scouting expedition doesn't reveal fog — discovery
# is comm-range gated), so the roster CAN be non-empty on an unseen hex while still hiding everything
# that isn't ours. Listing only your own party without a word would quietly imply it's alone there.
const OCCUPANTS_UNSEEN_OTHERS_HINT := "Out of sight — you can't see anything here but your own."
# ---- Action-status vocabulary: row GLYPHS, tooltip WORDS ---------------------------------------
# A Current-actions / Active-expeditions row states its state with a GLYPH (`FoodIcons.STATUS_ICONS`,
# the one glyph registry, exactly like the policy icons) and moves the WORDS into the row tooltip.
# The rows were spelling everything out (`🌰 Forage (27, 26) [sustain] · pending`) — long, and the
# pending row is ALREADY amber, so "· pending" repeated what the tint said.
# Two orthogonal layers (see `FoodIcons.STATUS_ICONS`), kept deliberately separate:
#   • STATUS — what the action is doing: a confirmed local forage/hunt row has no sim phase, it is
#     simply `working`; an expedition's is the sim's `ExpeditionPhase`.
#   • `pending` — a state of the ORDER, not the action (composed locally, not yet acknowledged by the
#     sim, resolves on turn advance). It rides on ANY row, is a MODIFIER (never a phase member), and
#     takes the row's glyph slot + keeps the amber label tint that ties it to the pending map hex.
# EXCEPTION — `awaiting` KEEPS ITS WORDS. It is not a status but a DEMAND ON THE PLAYER: the party is
# parked at its objective burning provisions until you act. A status you already expect is fine to
# hide behind a hover; a call to action must never require one. So an awaiting row renders
# glyph + WARN-tinted words, while every other state is glyph-only.
# Separates a row's trailing glyphs from its label (and from each other): "🌰 Forage (27, 26)  ♻  ●".
const ROW_GLYPH_SEPARATOR := "  "
# Word forms for the two ORDER-level statuses. The expedition PHASE words are NOT duplicated here —
# `_status_label` reads them from `EXPEDITION_PHASE_LABELS`, their single source of truth.
const STATUS_LABELS := {
	FoodIcons.STATUS_PENDING: "Pending",
	FoodIcons.STATUS_WORKING: "Working",
}
# The one-line behaviour hint the tooltip appends after the status word ("" = the word says it all).
const STATUS_HINTS := {
	FoodIcons.STATUS_PENDING: "starts when you advance the turn",
	FoodIcons.STATUS_WORKING: "",
	EXPEDITION_PHASE_OUTBOUND: "heading to the target",
	EXPEDITION_PHASE_AWAITING: "parked at the objective — it needs an order",
	EXPEDITION_PHASE_HUNTING: "taking food from the herd",
	EXPEDITION_PHASE_DELIVERING: "bringing the haul home",
	EXPEDITION_PHASE_RETURNING: "heading home",
}
const STATUS_HINT_FORMAT := "%s — %s"
# The single player band, captured from the latest snapshot populations (there is exactly
# one player band in the current start). assign_labor / move_band / clear-all target it; the
# herd/tile assign controls also read its labor_assignments to show the current staffing.
var _player_band: Dictionary = {}
# Every player-faction band from the latest snapshot (in roster order; first == _player_band).
# The assign controls' band-picker dropdown lists these so an assignment explicitly names WHICH
# band supplies the workers. One entry today (multi-band split is deferred), but built for N.
# EXCLUDES expeditions (detached scout/hunt parties are cohorts in the same populations[] array) —
# they are never a labor actor band and must not be counted by the panel cycler.
var _player_bands: Array = []
# The player-faction expedition cohorts (is_expedition) captured each snapshot, split out of
# `_player_bands`. The Band/City panel's "Active expeditions" section lists the ones whose
# `home_band_entity` matches the shown band.
var _player_expeditions: Array = []

# ---- Band/City zone state (persists across renders, so a filter/tab/page survives a snapshot) ----
## Which sources the work board shows, how it orders them, and which page is on screen.
var _work_filter: StringName = WORK_FILTER_ALL
var _work_sort: StringName = WORK_SORT_YIELD
var _work_page: int = 0
## The source key open in the work inspector strip ("" = none), and whether its policy picker is out.
## One row at a time — the strip costs board rows, which `_work_board_capacity` subtracts.
var _work_open_key: String = ""
var _work_policy_open: bool = false
## The live work-zone column + its band, so `zones_resized` can RE-PAGE the board in place instead of
## re-rendering all three zones.
var _work_zone_host: VBoxContainer = null
var _work_zone_band: Dictionary = {}
## The band-zone height tier the current render was built for (see `_band_zone_tier`).
var _band_zone_tier: int = BAND_ZONE_TIER_TALL
## The parties compose sheet: open, and which mission has been picked ("" = none yet, which is what
## keeps the party size / policy / forecast fields hidden until the mission decides them).
var _party_compose_open: bool = false
var _party_compose_mission: String = ""
# The dockable Band/City command center (docs/plan_band_city_dock.md §3), injected by Main. When
# present, a selected player band's detail (summary + labor allocation) renders into IT rather than
# the Occupants card, and the panel persists across selection changes showing `_panel_band`.
var _band_city_panel: BandCityPanel = null
# The band currently shown in the panel — synced from map/roster selection and the cycler, and
# re-resolved against each snapshot so its steppers/idle stay live. Empty when no player bands.
var _panel_band: Dictionary = {}
# Map grid dimensions (width/height/horizontal-wrap), captured each snapshot from the `grid` key
# (Main forwards it via set_grid_dimensions). Feed the wrap-aware hex distance the herd-hunt
# affordance keys its LOCAL-hunt-vs-hunting-EXPEDITION decision off. Grid rides full snapshots only;
# it persists across deltas. Defaults to no-wrap until the first snapshot.
var _grid_width: int = 0
var _grid_height: int = 0
var _grid_wrap_horizontal: bool = false
# The authoritative snapshot turn (header tick), set from update_overlay each snapshot. Used
# to reconcile optimistic pending actions (a newer turn means the server has processed them).
var _current_turn: int = -1
# Optimistic pending labor actions per band entity (Early-Game Labor slice 3b UX): assign/move
# clicks show immediately in the panel AND on the map, then reconcile when a newer-turn snapshot
# confirms them (the snapshot is authoritative — this cleanly absorbs server-side clamping).
#   _pending_labor[entity] = {
#     "turn": <snapshot turn at issue>,
#     "assign": { key -> {kind, workers, x, y, herd_id, policy} },   # key via _pending_key
#     "move": {x, y},                                                # optional
#   }
var _pending_labor: Dictionary = {}
# Move-band targeting: the pending band-relocation tile pick. {} when inactive. Holds the
# band dict whose move is being targeted.
var _pending_move_band: Dictionary = {}
# Send-expedition targeting: the pending expedition-launch tile pick. {} when inactive. Holds the
# resident band being outfitted plus the chosen party size (mirrors `_pending_move_band`).
var _pending_send_expedition: Dictionary = {}
# Quarry-pick targeting: the pending HERD pick for the party compose sheet (the click resolves to a
# huntable herd on the clicked hex, not a tile). {} when inactive. It carries only the band — party
# size and policy are chosen in the sheet AFTER the quarry, which is what the pick is for.
var _pending_pick_quarry: Dictionary = {}
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = WORKER_STEP
# Compose state for the hunt-expedition launch policy (Sustain/Surplus/Market/Eradicate).
var _send_hunt_policy: String = DEFAULT_HUNT_POLICY
# The quarry the party compose sheet is aimed at (a world herd id), "" until one is picked. It is
# the FIRST question the hunt form asks, because the herd sets the useful party size, the per-policy
# take and the trip length — everything below it in the form.
var _send_party_quarry_id: String = ""
# One-shot: set when the player CLICKS a policy in the compose sheet so the next rebuild auto-fills
# the party to that policy's max-useful cap. The sheet's OWN autofill — `_hunt_assign_autofill` is
# the herd drawer's, and sharing it would let one surface's click refill the other's stepper.
var _send_party_autofill := false
# Compose state for the herd/tile "Assign" controls — the in-progress worker count (and, for
# hunts, the policy) the player is dialing before pressing Assign. Keyed to the source so a
# per-snapshot re-render preserves the count, but re-initializes from current staffing when
# the selected source changes.
var _forage_assign_key: String = ""
var _forage_assign_count: int = 0
# One-shot: set when the player CLICKS a forage policy so the next rebuild auto-fills the worker count
# to the policy's max-useful cap ("give me everything this patch sustains"). The forage twin of
# `_hunt_assign_autofill` — cleared as soon as consumed, never set by a −/+ tick.
var _forage_assign_autofill := false
var _forage_assign_policy: String = DEFAULT_HUNT_POLICY
var _hunt_assign_key: String = ""
var _hunt_assign_count: int = 0
var _hunt_assign_policy: String = DEFAULT_HUNT_POLICY
# One-shot: set when the player CLICKS a policy so the next rebuild auto-fills the worker count to the
# policy's max-useful cap ("give me everything this herd sustains" — zero waste + full rate). Cleared as
# soon as it's consumed; never set by a −/+ stepper tick, so manual counts are preserved.
var _hunt_assign_autofill := false
# Per-faction intensification knowledge from the latest snapshot: entity → {cultivation, herding},
# each 0..1. Gates the Cultivate/Corral picker options (a rung needs its track fully learned) and
# backs the top-bar meters; the previous value is what makes the one-shot unlock nudge possible.
var _intensification_knowledge: Dictionary = {}
# "<faction>:<track>" keys already announced to the command feed, so the nudge fires once.
var _knowledge_announced: Dictionary = {}
# The band-picker selection (actor band entity) for each assign control, persisted across the
# per-snapshot re-renders of the same source. Re-defaults to _resolve_assign_band() when the
# selected source changes; -1 means "fall back to the resolved band".
var _hunt_assign_band: int = -1
var _forage_assign_band: int = -1
var _targeting_banner: PanelContainer = null
var _targeting_banner_label: RichTextLabel = null
var _stockpile_totals: Dictionary = {}
var travel_tiles_per_turn: float = DEFAULT_TRAVEL_SPEED
var travel_preview_turn_cap: int = DEFAULT_TRAVEL_PREVIEW_LIMIT
var left_dock: PanelDock
var right_dock: PanelDock
# Edges reserved by docked panels (Inspector, Band/City panel). Each reserver
# registers a (edge, size) contribution keyed by a StringName id; the whole HUD
# insets by the summed per-edge totals.
var _reservations: Dictionary = {}
# ---- The Telling (docs/plan_the_telling.md) --------------------------------
# The player faction's pending forks + stance axes, cached from the snapshot. `_band_attention` is
# the band/expedition half of the orb registry, cached so a fork arriving on its own delta can
# rebuild the registry WITHOUT re-running the band producers (set_attention is a full replace, and
# _prev_band_sizes is consumed by the losing-population producer, so re-invoking update_band_alerts
# would silently eat that alert).
var _pending_forks: Array = []
var _stance_axes: Array = []
var _band_attention: Array = []
# Beats already auto-opened this session, so a fork the player dismissed does not re-open on
# every subsequent snapshot. Keyed by beat_id.
var _auto_opened_forks: Dictionary = {}
var _fork_panel: NarrativeForkPanel = null
# The floating compose sheet + which source it is composing. `_compose_kind` is one of the
# COMPOSE_KIND_* constants; `_compose_subject` is the source key (a "x,y" tile key or a herd id), so
# a per-snapshot refresh can tell "the same source, restated" from "a different source, gone".
var _compose_sheet: ComposeSheet = null
var _compose_kind: String = COMPOSE_KIND_NONE
var _compose_subject: String = ""
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0

func _ready() -> void:
    _legend = LegendController.new(terrain_legend_panel, terrain_legend_scroll, terrain_legend_list, terrain_legend_description)
    _command_feed = CommandFeedController.new(command_feed_panel, command_feed_scroll, command_feed_label, left_dock_scroll)
    # The telling GROWS TO FIT its current page, capped at `PAGE_MAX_HEIGHT` (docs/plan_the_telling_book_ux.md),
    # so it no longer needs a dock-scroll ceiling to fit against — a page is bounded (one turn's beats), and
    # the right dock's own scroll stacks it above Victory + Terrain Types with no bespoke height math.
    _telling = TellingPanel.new(telling_panel, telling_scroll, telling_label)
    _load_ui_balance_config()
    _connect_zoom_rail()
    _connect_turn_orb()
    _ensure_fork_panel()
    _setup_tooltip()
    _legend.refresh_rows()
    _refresh_campaign_label()
    _refresh_victory_status()
    _command_feed.render()
    _telling.render()
    _connect_selection_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    left_dock.add(stockpile_panel, 20)
    left_dock.add(command_feed_panel, 30)
    # The right dock is the narrative surface's home: the telling owns the top of it and, with both
    # reference cards hidden by default, effectively the whole column. Sharing the left dock left it
    # cramped under the selection cards + command feed.
    right_dock.add(telling_panel, 10)
    right_dock.add(victory_panel, 20)
    right_dock.add(terrain_legend_panel, 30)
    _load_hud_panel_prefs()
    if stockpile_panel != null:
        stockpile_panel.visible = false
    if stockpile_title != null:
        stockpile_title.text = "Stockpiles"
    _apply_hud_style()
    _ensure_targeting_banner()
    _setup_build_overlay()
    # The selection drawer's Food/Morale labels are click-to-expand breakdown disclosures.
    if occupant_detail != null:
        occupant_detail.meta_clicked.connect(_on_detail_meta_clicked.bind(occupant_detail))
    # Re-cap the drawer whenever its content changes SIZE, whoever changed it — a stepper tick, a
    # policy click, a per-snapshot rebuild. One hookup instead of a refit call sprinkled through
    # every early-return in the three compose builders. No feedback loop: the fit writes the
    # SCROLL's minimum, which is outside the body it measures.
    if subject_body != null:
        subject_body.minimum_size_changed.connect(_fit_subject_drawer)
    # A window resize changes the dock's height, hence the room the drawer may claim.
    get_viewport().size_changed.connect(_fit_subject_drawer)

## Apply the shared HudStyle console look to the selection panel: restyle its
## action buttons, tint the detail text, and bring the two plain PanelContainers
## (stockpile, victory) up to the same card chrome the PanelCards already use.
func _apply_hud_style() -> void:
    for detail in [tile_detail, occupant_detail]:
        if detail != null:
            detail.add_theme_color_override("default_color", HudStyle.INK_DIM)
            detail.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
            detail.add_theme_constant_override("table_h_separation", 16)
            detail.add_theme_constant_override("table_v_separation", 3)
    # The list ↔ drawer hairline: the palette owns the rule, the node owns its thickness.
    if subject_divider != null:
        subject_divider.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
        subject_divider.custom_minimum_size = Vector2(0.0, SUBJECT_DIVIDER_HEIGHT)
        subject_divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
    if stockpile_panel != null:
        stockpile_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    if victory_panel != null:
        victory_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())

## Floating targeting banner, pinned to the top-centre of the map. Shown only
## while a command is choosing its target; it names the command + what to click
## next and offers Cancel. This is the primary targeting feedback — it replaces
## the easy-to-miss "select a band…" line buried in the selection panel.
func _ensure_targeting_banner() -> void:
    if _targeting_banner != null:
        return
    var center := CenterContainer.new()
    center.name = "TargetingBannerCenter"
    center.anchor_left = 0.0
    center.anchor_right = 1.0
    center.anchor_top = 0.0
    center.anchor_bottom = 0.0
    center.offset_top = 12.0
    # Anchored to the top edge with zero anchored height; grow downward so the
    # container takes its child's (the banner's) height instead of a 0/negative
    # rect that could clip it.
    center.grow_vertical = Control.GROW_DIRECTION_END
    center.mouse_filter = Control.MOUSE_FILTER_IGNORE
    layout_root.add_child(center)

    var banner := PanelContainer.new()
    banner.name = "TargetingBanner"
    banner.add_theme_stylebox_override("panel", HudStyle.banner_stylebox())
    banner.visible = false
    center.add_child(banner)

    var hbox := HBoxContainer.new()
    hbox.add_theme_constant_override("separation", 12)
    banner.add_child(hbox)

    var reticle := Label.new()
    reticle.text = "⌖"  # ⌖ target reticle
    reticle.add_theme_color_override("font_color", HudStyle.SIGNAL)
    reticle.add_theme_font_size_override("font_size", 20)
    reticle.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    hbox.add_child(reticle)

    var label := RichTextLabel.new()
    label.name = "TargetingLabel"
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_OFF
    label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    hbox.add_child(label)

    var cancel := Button.new()
    cancel.text = "Cancel  (Esc)"
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(cancel_active_targeting)
    hbox.add_child(cancel)

    _targeting_banner = banner
    _targeting_banner_label = label

## Recompute targeting state from the pending flows, update the banner, and
## notify listeners (Main -> MapView). Call after any pending change.
func _refresh_targeting() -> void:
    _ensure_targeting_banner()
    var info := _current_targeting_info()
    if info.is_empty():
        _targeting_banner.visible = false
    else:
        _targeting_banner.visible = true
        _targeting_banner_label.text = _targeting_banner_bbcode(info)
    emit_signal("targeting_changed", info)

## True while any command-targeting flow is armed (move-band / send-expedition /
## send-hunt-expedition). The ESC pause menu (Main._unhandled_input) checks this so it
## yields ESC to MapView's targeting-cancel path instead of stealing it to open the menu.
func is_targeting_active() -> bool:
    return not _current_targeting_info().is_empty()

## The active targeting descriptor, or {} when nothing is targeting. A pending
## harvest/hunt needs a band; a pending scout needs a tile.
## The active targeting descriptor, or {} when nothing is targeting. Move-band is the
## one remaining targeting flow (the single-task Harvest/Hunt/Scout flows were retired
## with the labor-allocation model): it needs a destination tile.
func _current_targeting_info() -> Dictionary:
    if not _pending_move_band.is_empty():
        var pos: Array = Array(_pending_move_band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(_pending_move_band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(_pending_move_band.get("current_y", -1))
        return {
            "active": true,
            "command": "move",
            "need": "tile",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": String(_pending_move_band.get("id", "Band")),
        }
    if not _pending_send_expedition.is_empty():
        var band: Dictionary = _pending_send_expedition.get("band", {})
        var pos: Array = Array(band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
        return {
            "active": true,
            "command": "expedition",
            "need": "tile",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": "%s · %d" % [
                String(band.get("id", "Band")), int(_pending_send_expedition.get("party_workers", 0)),
            ],
        }
    if not _pending_pick_quarry.is_empty():
        var band: Dictionary = _pending_pick_quarry.get("band", {})
        var pos: Array = Array(band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
        # `need: "herd"` is what makes MapView glow the huntable herds. No party size in the label —
        # none is chosen yet; the sheet asks for it once the quarry is known.
        # `min_distance`: a valid target must lie STRICTLY farther than this from the origin — the
        # render-side half of `_is_expedition_quarry`, so the halo cannot offer a herd the pick will
        # refuse. Every other targeting mode omits the key and MapView defaults it to 0, which admits
        # everything and so changes nothing for move/scout-tile targeting.
        return {
            "active": true,
            "command": "quarry",
            "need": "herd",
            "origin_x": ox,
            "origin_y": oy,
            "min_distance": int(band.get("hunt_reach", 0)),
            "context_label": String(band.get("id", "Band")),
        }
    return {}

func _targeting_banner_bbcode(info: Dictionary) -> String:
    var cmd := String(info.get("command", "")).to_upper()
    var need := String(info.get("need", ""))
    var ctx := String(info.get("context_label", ""))
    var loc := ""
    if need == "band":
        loc = "  [color=#%s](%d, %d)[/color]" % [
            HudStyle.INK_DIM_HEX, int(info.get("origin_x", 0)), int(info.get("origin_y", 0)),
        ]
    var instruction := ""
    if need == "band":
        instruction = "click a band to send it here"
    elif cmd == "MOVE":
        instruction = "click a destination tile"
    elif cmd == "EXPEDITION":
        instruction = "click a target tile to scout"
    elif cmd == "QUARRY":
        instruction = "click on a herd to hunt"
    else:
        instruction = "click a tile to survey"
    return "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
        HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
    ]

## Render a `_hunt_trip_forecast` result as its one-line BBCode readout — the three states in their
## three colors (cyan viable / amber too-slow / red returns-empty), or "" when the forecast isn't
## available (a herd with no exported estimate → the caller shows no line at all). SHARED by both hunt-expedition entry
## points: the targeting banner (band-first flow) and the herd panel's live compose block (herd-first
## flow), so the two can never drift apart.
func _hunt_forecast_line_bbcode(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    # A denial mission (Eradicate) brings nothing home BY DESIGN — say what it does, amber, no payload.
    if bool(forecast.get("denial", false)):
        return "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_FORECAST_DENIAL_FORMAT % herd_name,
        ]
    # No surplus above the policy's floor → the raid returns empty. The ONE non-viable case (red).
    if bool(forecast.get("empty", false)):
        return "[color=#%s]%s%s[/color]" % [
            HudStyle.DANGER_HEX, HUNT_FORECAST_WARN_GLYPH,
            HUNT_FORECAST_NO_SURPLUS_FORMAT % herd_name,
        ]
    # A real raid: headline the delivered PAYLOAD (the animal count over turns + the food it LANDS), then
    # the waste. `food` is the sim's `delivered_food` (always set on a delivering forecast).
    var animals := int(forecast.get("animals", 0))
    var food: String = HUNT_FORECAST_FOOD_FORMAT % int(forecast["food"]) if forecast.has("food") else ""
    # The waste % rides BELOW the food as its own WARN-amber segment (even on a cyan line — a high-waste
    # partial is informative, not a block). Empty when the raid carried its full kill home.
    var waste := ""
    var waste_pct := float(forecast.get("waste_pct", 0.0))
    if waste_pct > 0.0:
        waste = "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_WASTE_SUFFIX_FORMAT % int(round(waste_pct * 100.0))]
    if bool(forecast.get("long_raid", false)):
        # Ran the whole horizon still delivering (no bounded turn count) — a slow but real haul (amber).
        var long_text: String = HUNT_FORECAST_LONG_RAID_FORMAT % [animals, herd_name]
        var long_travel := int(forecast.get("travel", 0))
        if long_travel > 0:
            long_text += HUNT_FORECAST_LONG_TRAVEL_SUFFIX % long_travel
        return "[color=#%s]%s%s%s[/color]%s" % [
            HudStyle.WARN_HEX, long_text, food, HUNT_FORECAST_SLOW_SUFFIX, waste,
        ]
    # `turns` is the TOTAL (hunting + round-trip travel); the breakdown spells the split out when there's
    # travel to show — a band-relative addition the band-agnostic estimate table can't carry.
    var turns := int(forecast.get("turns", 0))
    var text: String = HUNT_FORECAST_DELIVERS_FORMAT % [animals, herd_name, turns]
    var travel := int(forecast.get("travel", 0))
    if travel > 0:
        text += HUNT_FORECAST_TRAVEL_BREAKDOWN % [int(forecast.get("hunt_turns", 0)), travel]
    # Slow raid (past the band's warn threshold) — still a real delivery, just a long one: amber, told
    # then trusted. A brisk raid reads income-cyan.
    if bool(forecast.get("slow", false)):
        return "[color=#%s]%s%s%s%s[/color]%s" % [
            HudStyle.WARN_HEX, HUNT_FORECAST_WARN_GLYPH, text, food, HUNT_FORECAST_SLOW_SUFFIX, waste,
        ]
    return "[color=#%s]%s%s[/color]%s" % [HudStyle.SIGNAL_HEX, text, food, waste]

## The raid `workers` from `band` deliver hunting `herd` under `policy`. A PURE TABLE LOOKUP into the
## sim's forward-simulated `hunt_trip_estimates` (`HERD_TRIP_ESTIMATES_KEY`) — ZERO arithmetic: the sim
## grabs the herd's standing surplus above the policy floor in a burst and reports the whole animals it
## lands (`animals_taken`) and the turns until the party comes home (`turns_to_fill`, NOT "turns to fill
## the pack"). The ecology/MSY model is never reproduced here. (The LOCAL band hunt preview DOES compute
## — see `_hunt_take_rate` over the band ceiling `hunt_policy_ceilings`.) Returns {available, denial,
## empty, animals, turns, food, long_raid, slow}: `available` false = the snapshot carries no estimate
## for this (policy, party size) (older server → the caller shows no forecast at all).
func _hunt_trip_forecast(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if workers <= 0 or not (estimates_variant is Dictionary):
        return {"available": false}
    var key := _hunt_estimate_key(policy, workers)
    var estimates := estimates_variant as Dictionary
    if not estimates.has(key):
        return {"available": false}
    var estimate: Dictionary = estimates[key]
    # A denial mission (eradicate) delivers no food BY DESIGN — never a payload, never a failure. This
    # carve-out MUST come first: it takes animals (down to the 0 floor) but banks none as food.
    if not bool(estimate.get("delivers_food", false)):
        return {"available": true, "denial": true, "empty": false}
    # delivered_food == 0 = the herd is at/below the policy's floor: no standing surplus to raid, the
    # party returns empty. The ONE non-viable case (the raid always completes; the herd has nothing).
    # NOT `animals_taken == 0`: a party too small to carry a whole animal now KILLS one and hauls the
    # fraction its pack holds (mirroring the local hunt), so `animals_taken >= 1` whenever there's any
    # surplus — the delivered PAYLOAD (with waste) is the honest bind, not the whole-animal kill count.
    var delivered_food := float(estimate.get("delivered_food", 0.0))
    if delivered_food <= 0.0:
        return {"available": true, "denial": false, "empty": true}
    var animals := int(estimate.get("animals_taken", 0))
    # turns_to_fill == 0 = the raid ran the whole horizon still delivering (a long raid). A warn
    # threshold of 0 means the server sent none — report the raid, judge nothing. `turns_to_fill` now
    # counts HUNTING turns only; the band-relative round trip is added on top so the headline is honest.
    var hunt_turns := int(estimate.get("turns_to_fill", 0))
    var long_raid: bool = hunt_turns <= 0
    var travel := _round_trip_travel_turns(band, herd)
    var total := hunt_turns + travel
    var warn_turns := int(band.get("expedition_viability_warn_turns", 0))
    var slow: bool = not long_raid and warn_turns > 0 and total > warn_turns
    # Waste fraction: killed-but-not-carried food over total killed. A small party on big game raids one
    # animal and hauls only the pack's worth, wasting the rest — a high % here is informative, not a block.
    var wasted_food := float(estimate.get("wasted_food", 0.0))
    var killed := delivered_food + wasted_food
    var waste_pct := (wasted_food / killed) if killed > 0.0 else 0.0
    return {
        "available": true, "denial": false, "empty": false,
        "animals": animals, "turns": total, "hunt_turns": hunt_turns, "travel": travel,
        "long_raid": long_raid, "slow": slow,
        # The delivered PAYLOAD in food — what the party actually LANDS (a partial for a small party),
        # straight from the sim's forward-simulated raid, NOT animals × food_per_animal (which counts the
        # whole kill and overstates a partial). Guaranteed > 0 here (empty returned above otherwise).
        "food": int(round(delivered_food)), "waste_pct": waste_pct,
    }

## Round-trip TRAVEL turns for a raid party walking from `band` out to `herd` and back — the honest
## remainder of the trip length the band-agnostic `hunt_trip_estimates` table cannot carry (one row
## serves every band). Matches the sim launch feed EXACTLY: ceil(2 × wrap-aware hex_distance(band, herd)
## / band_move_tiles_per_turn), from the SELECTED band's tile + the exported move rate.
## Returns 0 — so the forecast degrades to hunting turns only, never a fabricated travel — when the move
## rate isn't on the band dict or a position is unknown. `band_move_tiles_per_turn` (a LaborConfig scalar
## echoed per-cohort) is now decoded in `native/src/lib.rs` and flowed onto the band marker, so this
## lights up on the live wire; it degrades gracefully if a future snapshot omits it.
func _round_trip_travel_turns(band: Dictionary, herd: Dictionary) -> int:
    var move_rate := float(band.get("band_move_tiles_per_turn", 0.0))
    if move_rate <= 0.0:
        return 0
    var band_tile := _band_tile(band)
    var one_way := _hex_distance_wrapped(
        band_tile.x, band_tile.y, int(herd.get("x", -1)), int(herd.get("y", -1)))
    if one_way < 0:
        return 0
    return int(ceil(float(2 * one_way) / move_rate))

## The per-turn provisions `workers` from `band` take off `herd` under `policy` — the sim's LOCAL/band
## hunt take before the output multiplier: `min(workers × hunt_per_worker_provisions, band_ceiling)`.
## Resident-band only: an EXPEDITION's trip is never a rate division (see `_hunt_trip_forecast`).
## Returns `HUNT_RATE_UNAVAILABLE` when the levers/ceiling are absent.
func _hunt_take_rate(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> float:
    var per_worker_rate := float(band.get("hunt_per_worker_provisions", 0.0))
    var ceiling := _hunt_policy_ceiling(herd, policy)
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return HUNT_RATE_UNAVAILABLE
    return maxf(minf(float(workers) * per_worker_rate, ceiling), 0.0)

## The sim-exported per-turn BAND take ceiling for `policy` on `herd` (`hunt_policy_ceilings` — the
## herd's renewable FLOW), or `HUNT_RATE_UNAVAILABLE` when the snapshot carries none. NEVER derived
## here — the ecology/MSY model that produces these numbers lives in the sim.
func _hunt_policy_ceiling(herd: Dictionary, policy: String) -> float:
    var ceilings_variant: Variant = herd.get(HERD_BAND_CEILINGS_KEY, {})
    if not (ceilings_variant is Dictionary) or not (ceilings_variant as Dictionary).has(policy):
        return HUNT_RATE_UNAVAILABLE
    return float((ceilings_variant as Dictionary)[policy])

## The averaging WINDOW (turns) for the whole-animal disclaimer — a STABLE, worker-independent property
## derived from the SELECTED policy's raw flow ceiling (NOT the crew's current delivered rate, which
## moves as workers change and made the old line blink out). Keyed on `policy` because a faster policy
## (Surplus/Market) delivers lumpy whole animals over a different span. `g` = animals/turn the policy's
## flow buys: slow/big game (`g < 1`) lands one animal every ~`1/g` turns; fast game (`g >= 1`) delivers
## the "extra" fractional animal every ~`1/frac` turns. Returns 0 when `food_per_animal` / the ceiling is
## unknown (caller then skips the line). NEVER scaled by `output_multiplier` — it's a pure herd property.
func _hunt_avg_window_turns(herd: Dictionary, policy: String) -> int:
    var fpa := float(herd.get("food_per_animal", 0.0))
    var ceiling := _hunt_policy_ceiling(herd, policy)
    if fpa <= 0.0 or ceiling <= 0.0:
        return 0
    var g: float = ceiling / fpa
    var x: int
    if g < 1.0:
        x = int(ceil(1.0 / g))
    else:
        var frac: float = g - floor(g)
        x = 1 if frac < 0.01 else int(ceil(1.0 / frac))
    return clampi(x, 1, HUNT_WINDOW_MAX_TURNS)

## The HONEST carry-aware delivery model for a local hunt: what a crew of `workers` from `band` actually
## lands off `herd` under `policy` per turn, and how much of the kill they can't carry (which rots). A
## hunt takes WHOLE animals via a kill-credit bank, so the crew's raw food throughput is quantized to the
## whole bodies it can haul — fractional carry capacity is idle (NOT waste), but a crew too small to carry
## even one whole animal loses the surplus meat. Returns `{available, delivered, waste, waste_pct}` (all
## food/turn; `waste_pct` 0..1) or `{available=false}` when a lever/ceiling is absent (caller degrades to
## the old food/turn line). NEVER re-derives the ecology model — `food_per_animal` and the flow ceiling
## are sim exports.
func _hunt_delivered_and_waste(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> Dictionary:
    var fpa := float(herd.get("food_per_animal", 0.0))
    var per_worker := float(band.get("hunt_per_worker_provisions", 0.0))
    var output := float(band.get("output_multiplier", OUTPUT_FULL))
    var ceiling := _hunt_policy_ceiling(herd, policy)
    if fpa <= 0.0 or per_worker <= 0.0 or ceiling < 0.0 or workers <= 0:
        return {"available": false}
    ceiling *= output
    var collection := float(workers) * per_worker * output   # crew's raw food throughput /turn
    var carryable := floorf(collection / fpa)                # whole animals /turn the crew can carry
    var delivered := 0.0
    var waste := 0.0
    if carryable >= 1.0:
        # Carry quantized to whole bodies; the flow ceiling still caps it. Leftover carry capacity is
        # idle, NOT waste (no animal was killed and dropped).
        delivered = minf(ceiling, carryable * fpa)
        waste = 0.0
    else:
        # Can't carry even one whole animal → the meat that can't be hauled rots.
        var kills_per_turn := minf(1.0, ceiling / fpa)
        delivered = collection * kills_per_turn
        waste = (fpa - collection) * kills_per_turn
    var killed_food := delivered + waste
    var waste_pct := (waste / killed_food) if killed_food > 0.0 else 0.0
    return {"available": true, "delivered": delivered, "waste": waste, "waste_pct": waste_pct}

## An animals-per-turn rate string: up to 2 decimals with trailing zeros AND a trailing dot stripped
## (1.90→"1.9", 1.00→"1", 0.65→"0.65", 0.15→"0.15"). `String.num` keeps a lone ".0", so format fixed and
## strip the tail ourselves (rstrip stops at the first non-matching char, so integer zeros survive).
func _format_animal_rate(value: float) -> String:
    var text := ("%." + str(HUNT_ANIMAL_RATE_DECIMALS) + "f") % value
    if "." in text:
        text = text.rstrip("0")
        if text.ends_with("."):
            text = text.rstrip(".")
    return text

## A hunt source is MANAGED (its crew are herders/keepers, not a hunt party) once the herd is penned,
## fully tamed (pastoral), or being penned under the composed Corral policy. `workersNeeded` on such a
## source scales with the HERD (max herders, haulers), so the crew label must read as herders.
func _is_managed_hunt_source(herd: Dictionary, policy: String) -> bool:
    return bool(herd.get("corralled", false)) \
        or float(herd.get("domestication", 0.0)) >= DOMESTICATION_COMPLETE \
        or policy == LABOR_POLICY_CORRAL

## Each hunt policy's button metric, keyed policy → a `{compact, full}` pair (compact for the one-line
## button face, full for the tooltip). The plant twin of this is `_forage_policy_takes`; both wear the
## same shape, only the metric differs:
##   EXTRACTIVE (Sustain/Surplus/Market/Eradicate) → the herd's worker-independent CAP for the policy
##       (`hunt_policy_ceilings`): a bare signed rate on the face, framed "up to X/turn" in the tooltip
##       — the ceiling it is, distinct from the crew's carry-aware delivered line below the picker. Read
##       straight off the sim; never re-derived.
##   INVESTMENT (Tame/Corral) → the PAYOFF the rung builds toward (`pastoral_yield` / `corral_yield`),
##       `→+Y` on the face / "builds toward Y/turn" in the tooltip. NOT the during-building dip: that dip
##       reads BELOW Sustain and is identical
##       for both rungs, so quoting it made taming/penning look strictly worse than hunting. Shown even
##       when the rung is gated/greyed — informative — with the gate-reason line explaining the lock; a
##       0/absent payoff leaves the button bare.
## Empty when the herd carries no ceilings (older snapshot / non-huntable).
func _hunt_policy_takes(herd: Dictionary) -> Dictionary:
    var takes := {}
    var ceilings_variant: Variant = herd.get(HERD_BAND_CEILINGS_KEY, {})
    if not (ceilings_variant is Dictionary):
        return takes
    for policy in (ceilings_variant as Dictionary):
        # The INVESTMENT rungs are skipped here — their during-build dip rides this list too, but they
        # wear the PAYOFF (the second loop), not the dip. Mirrors `_forage_policy_takes`.
        if String(policy) in INVESTMENT_POLICIES:
            continue
        var rate := float((ceilings_variant as Dictionary)[policy])
        if rate < 0.0:
            continue
        takes[String(policy)] = _extractive_take(rate)
    for policy in [LABOR_POLICY_TAME, LABOR_POLICY_CORRAL]:
        var forecast := _forecast_inputs(herd, SOURCE_KIND_HERD, BARE_FORECAST_PREFIX, policy)
        if not bool(forecast["known"]) or not bool(forecast["investment"]):
            continue
        var payoff := float(forecast["payoff"])
        if payoff > 0.0:
            takes[policy] = _payoff_take(payoff)
    return takes

## A `{compact, full}` metric pair for an EXTRACTIVE rung's per-turn cap — the bare signed rate on the
## button face, the "up to X/turn" wording in the tooltip. Shared by the hunt + forage takes helpers.
func _extractive_take(rate: float) -> Dictionary:
    var signed := _format_signed(rate)
    return {"compact": signed, "full": POLICY_CAP_FORMAT % signed}

## A `{compact, full}` metric pair for an INVESTMENT rung's PAYOFF — the arrow-led rate on the button
## face (`→+1.20`), the "builds toward X/turn" wording in the tooltip. Shared by hunt + forage.
func _payoff_take(payoff: float) -> Dictionary:
    var signed := _format_signed(payoff)
    return {"compact": POLICY_PAYOFF_COMPACT % signed, "full": POLICY_PAYOFF_FULL_FORMAT % signed}

## The LOCAL hunt's live per-turn yield preview, or "" when the snapshot lacks the levers/ceilings
## (graceful degrade — no line, panel otherwise unchanged). A resident band applies its
## `output_multiplier` (morale/discontent productivity) at payout, so the preview is the take rate
## scaled by it. Reads income-green when the take is within the herd's sustainable yield (the Sustain
## ceiling), WARN-amber with the shared ⚠ when it overdraws — the same flag the allocation rows carry.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    var sustain_ceiling := _hunt_policy_ceiling(herd, DEFAULT_HUNT_POLICY)
    if sustain_ceiling < 0.0:
        return ""
    var output := float(band.get("output_multiplier", OUTPUT_FULL))
    var sustainable := sustain_ceiling * output
    var dw := _hunt_delivered_and_waste(band, herd, policy, workers)
    if not bool(dw.get("available", false)):
        # Graceful degrade — `food_per_animal` (or a lever) is unknown, so fall back to the old smoothed
        # food/turn line unchanged rather than regress the readout.
        var rate := _hunt_take_rate(band, herd, policy, workers)
        if rate < 0.0:
            return ""
        var actual := rate * output
        var text: String = LOCAL_HUNT_YIELD_FORMAT % _format_yield(actual)
        if _is_overdraw(actual, sustainable):
            return "[color=#%s]%s %s%s[/color]" % [
                HudStyle.WARN_HEX, OVERHUNT_FLAG, text, LOCAL_HUNT_OVERDRAW_SUFFIX]
        return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, YIELD_TOOLTIP_RENEWABLE]
    # ANIMALS-FIRST: the crew's honest carry-aware delivered take, as a per-turn animal rate (one
    # consistent format — no fast/slow flip). `delivered` is already carry-quantized, so this credits no
    # throughput the crew can't haul home.
    var fpa := float(herd.get("food_per_animal", 0.0))
    var delivered := float(dw["delivered"])
    var animal_rate := delivered / fpa if fpa > 0.0 else 0.0
    var primary := HUNT_DELIVERED_FORMAT % [_format_animal_rate(animal_rate), _herd_display_name(herd)]
    # Overdraw and waste are DIFFERENT flags and may co-occur — render both. Overdraw = the delivered take
    # exceeds the herd's Sustain ceiling (Surplus/Market draw it down); waste = a kill the crew couldn't
    # carry. The Sustain reading stays green + "· renewable".
    var body := ""
    if _is_overdraw(delivered, sustainable):
        body = "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, OVERHUNT_FLAG, primary, LOCAL_HUNT_OVERDRAW_SUFFIX]
    else:
        body = "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, primary, YIELD_TOOLTIP_RENEWABLE]
    var waste_pct := float(dw["waste_pct"])
    if waste_pct > 0.0:
        # Waste is its OWN concern — always WARN-tinted, even when the main line is green.
        body += "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_WASTE_SUFFIX_FORMAT % int(round(waste_pct * 100.0))]
    return body

## The LOCAL forage patch's live per-turn yield preview — the plant twin of `_local_hunt_preview_bbcode`.
## Forage is SMOOTH food (no whole-animal rhythm — no lumpy carry, no waste), so the line is just the
## per-turn take + a sustainability verdict: income-green `+2.74 /turn · renewable` when the take is
## within the patch's Sustain ceiling, WARN-amber `⚠ … — overdraws the patch` when a Surplus/Market/
## Eradicate policy draws it down. Both scaled by the acting band's output multiplier, like the hunt
## line. "" (no line) when the forecast levers are unknown, so the panel degrades gracefully.
func _local_forage_preview_bbcode(band: Dictionary, tile_info: Dictionary, policy: String, workers: int) -> String:
    # The Sustain ceiling IS the patch's sustainable yield (its regrowth take), so a take above it draws
    # the patch down — mirrors how the hunt version derives `sustainable` from the Sustain ceiling.
    var sustain := _forecast_inputs(tile_info, SOURCE_KIND_FORAGE, FORAGE_FORECAST_PREFIX, DEFAULT_HUNT_POLICY)
    if not bool(sustain["known"]):
        return ""
    var forecast := _forecast_inputs(tile_info, SOURCE_KIND_FORAGE, FORAGE_FORECAST_PREFIX, policy)
    if not bool(forecast["known"]):
        return ""
    var output := float(band.get("output_multiplier", OUTPUT_FULL))
    var sustainable := float(sustain["ceiling"]) * output
    var actual := _expected_yield(forecast, workers, band)
    var text := _format_yield(actual)
    if _is_overdraw(actual, sustainable):
        return "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, OVERHUNT_FLAG, text, LOCAL_FORAGE_OVERDRAW_SUFFIX]
    return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, YIELD_TOOLTIP_RENEWABLE]

## Cancel the active targeting (banner Cancel / Esc / right-click all route here).
func cancel_active_targeting() -> void:
    _cancel_pending_move_band()
    _cancel_pending_send_expedition()
    _cancel_pending_pick_quarry()

## Bottom-CENTRE version overlay showing the client build and the streamed server build,
## so the running builds can be confirmed at a glance. It lives centre-bottom rather than
## lower-left because the minimap + zoom rail own the lower-left corner and hid it. Spans the
## full width with centred text (so it can never collide with the corner clusters) and is
## mouse-transparent so it never intercepts map clicks.
func _setup_build_overlay() -> void:
    _build_label = Label.new()
    _build_label.name = "BuildOverlay"
    _build_label.anchor_left = 0.0
    _build_label.anchor_right = 1.0
    _build_label.anchor_top = 1.0
    _build_label.anchor_bottom = 1.0
    _build_label.offset_left = 0.0
    _build_label.offset_top = -26.0
    _build_label.offset_right = 0.0
    _build_label.offset_bottom = -6.0
    _build_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    _build_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    _build_label.add_theme_color_override("font_color", Color(0.85, 0.9, 1.0, 0.65))
    add_child(_build_label)
    _refresh_build_overlay()

func _refresh_build_overlay() -> void:
    if _build_label != null:
        _build_label.text = "build  cli %s · srv %s" % [ClientBuild.current(CLIENT_BUILD), _server_build]

## Called from Main with the server build id from each snapshot header.
func update_build_info(server_build: String) -> void:
    _server_build = server_build if server_build != "" else "?"
    _refresh_build_overlay()

func set_localization_store(store) -> void:
    localization_store = store
    _refresh_campaign_label()

func update_campaign_label(label: Dictionary) -> void:
    campaign_label = label.duplicate(true) if label is Dictionary else {}
    _refresh_campaign_label()

func update_victory_state(state: Dictionary) -> void:
    print("[HUD] update_victory_state: ", state.keys())
    victory_state = state.duplicate(true) if state is Dictionary else {}
    _refresh_victory_status()

func update_overlay(turn: int, metrics: Dictionary) -> void:
    # Authoritative snapshot turn — drives optimistic-pending reconciliation (see
    # _reconcile_pending, called from update_band_alerts later in the same snapshot cycle).
    _current_turn = turn
    turn_label.text = "Turn %d" % turn
    if turn_orb != null:
        turn_orb.set_turn(turn)
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

## Show the player faction's Sedentarization pressure as a compact top-bar text meter.
## Hidden until the score is meaningful; tinted amber (soft) / cyan (hard) as it climbs.
func update_sedentarization(sedentarization_variant: Variant) -> void:
    if sedentarization_label == null:
        return
    var score := 0.0
    var stage := ""
    if sedentarization_variant is Array:
        for entry in sedentarization_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                score = float(entry.get("score", 0.0))
                stage = String(entry.get("stage", ""))
                break
    if score < 1.0:
        sedentarization_label.visible = false
        return
    sedentarization_label.visible = true
    var suffix := "" if stage == "" or stage == "none" else " · %s" % stage
    sedentarization_label.text = "Sedentarization  %s  %d/100%s" % [_meter_bar(score), int(round(score)), suffix]
    sedentarization_label.add_theme_color_override("font_color", _sedentarization_color(stage))

## Show the player faction's age structure (children / working / elders) and the dependency
## ratio — the core demographic tension. Hidden until the faction has population.
func update_demographics(demographics_variant: Variant) -> void:
    if demographics_label == null:
        return
    var children := 0
    var working := 0
    var elders := 0
    var found := false
    if demographics_variant is Array:
        for entry in demographics_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                children = int(entry.get("children", 0))
                working = int(entry.get("working", 0))
                elders = int(entry.get("elders", 0))
                found = true
                break
    var total := children + working + elders
    if not found or total <= 0:
        demographics_label.visible = false
        return
    demographics_label.visible = true
    # NO DEPENDENCY RATIO HERE. This strip is the FACTION total across every band, and dependents are
    # fed per BAND — a band with more mouths than hands is in trouble whatever the faction average
    # says, and a healthy faction average can hide it. So the ratio belongs to the Band panel's PEOPLE
    # block (where it is stated as a dependent COUNT with the ratio in its tooltip) and is out of
    # place as a faction figure. The strip states the composition and nothing else.
    demographics_label.text = "Pop %d  👶%d 🛠%d 🧓%d" % [total, children, working, elders]
    demographics_label.add_theme_color_override("font_color", HudStyle.INK_DIM)

## Show the player faction's discovered Wondrous Sites as a compact top-bar readout: the count
## (`◈ Discoveries N`) followed by a strip of one mark per distinct site KIND, so landmark vs
## settle_site reads at a glance. Hidden until at least one site is known.
##
## THE TWO NUMBERS MEAN DIFFERENT THINGS AND ARE BOTH RIGHT: `N` is `sites.size()`, the count of
## INSTANCES found (site identity is the tile `(x, y)`); the strip shows KINDS, so three peaks are
## `N = 3` behind one peak mark. Do not "reconcile" them to a unique count.
##
## KEYED ON `site_id`, NOT on the glyph — the same rule `SecondaryMarkerRenderer` and `WonderSprites`
## follow. `site_id` is the sim's stable catalog key (`core_sim/src/data/sites_config.json`), while
## `glyph` is presentation the server happens to also send: two distinct sites may share one emoji
## (the fixture's `sky_arch` reuses ⛰), and deduping on the glyph collapsed them into a single strip
## entry that then silently disagreed with the count beside it.
func update_discoveries(discovered_variant: Variant) -> void:
    if discoveries_row == null or discoveries_label == null or discoveries_strip == null:
        return
    var sites: Array = []
    if discovered_variant is Array:
        for entry in discovered_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                var faction_sites: Variant = entry.get("sites", [])
                if faction_sites is Array:
                    sites = faction_sites
                break
    if sites.is_empty():
        discoveries_row.visible = false
        return
    discoveries_row.visible = true
    discoveries_row.add_theme_constant_override("separation", DISCOVERIES_STRIP_SEPARATION)
    discoveries_label.text = "%s Discoveries %d" % [DISCOVERIES_GLYPH, sites.size()]
    discoveries_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
    _rebuild_discoveries_strip(sites)

## One mark per distinct site KIND, first-seen order preserved. Called per snapshot, so the previous
## children are freed rather than accumulated.
func _rebuild_discoveries_strip(sites: Array) -> void:
    for child in discoveries_strip.get_children():
        child.queue_free()
        discoveries_strip.remove_child(child)
    discoveries_strip.add_theme_constant_override("separation", DISCOVERIES_STRIP_SEPARATION)
    # Sprites are sized to the readout's own font so the strip keeps the text baseline it had when
    # every entry was an emoji — derived, never a hardcoded pixel size.
    var mark_size := discoveries_label.get_theme_font_size("font_size")
    var seen: Array[String] = []
    for site in sites:
        if not (site is Dictionary):
            continue
        var entry := site as Dictionary
        var site_id := String(entry.get("site_id", "")).strip_edges()
        # A catalog-pruned site with no id still deserves one entry, so fall back to its tile —
        # instance identity — rather than dropping it or collapsing every such site together.
        var key := site_id
        if key == "":
            key = "%d,%d" % [int(entry.get("x", 0)), int(entry.get("y", 0))]
        if seen.has(key):
            continue
        seen.append(key)
        # Presentation precedence: bundled art, else the server's emoji, else the fallback mark.
        var name := String(entry.get("display_name", "")).strip_edges()
        var texture := WonderSprites.for_site_id(site_id)
        if texture != null:
            discoveries_strip.add_child(_discoveries_sprite(texture, mark_size, name))
        else:
            discoveries_strip.add_child(_discoveries_glyph_label(entry, name))

## A strip entry backed by bundled art, boxed to the text's font size and drawn aspect-preserved so
## a non-square illustration still sits on the same baseline as its emoji neighbours.
func _discoveries_sprite(texture: Texture2D, mark_size: int, site_name: String) -> TextureRect:
    var rect := TextureRect.new()
    rect.texture = texture
    rect.custom_minimum_size = Vector2(mark_size, mark_size)
    rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
    rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
    rect.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    rect.tooltip_text = site_name
    return rect

## A strip entry drawn as text: the site's server-provided emoji, or the named fallback mark when the
## catalog row carries neither art nor a glyph.
func _discoveries_glyph_label(entry: Dictionary, site_name: String) -> Label:
    var label := Label.new()
    var glyph := String(entry.get("glyph", "")).strip_edges()
    label.text = glyph if glyph != "" else DISCOVERIES_UNKNOWN_GLYPH
    label.add_theme_color_override("font_color", HudStyle.SIGNAL)
    label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    _set_label_tooltip(label, site_name)
    return label

## THE FACTION HALF OF THE TWO-METER SPLIT (docs/plan_intensification_ladder.md §4.1) — the player
## faction's intensification-ladder knowledge as a compact top-bar block-glyph strip, mirroring the
## Sedentarization readout.
##
## This strip is deliberately the ONLY place a knowledge meter appears. The split it enforces:
##   • KNOWLEDGE — "what my PEOPLE can do at all". Faction-wide, permanent, earned by cumulative
##     practice. It lives HERE, in the top bar, prefixed and labelled as your people's craft.
##   • PER-SOURCE PROGRESS — "have I done it to THIS herd/patch yet". Local, decays if abandoned. It
##     lives in that source's own drawer row (Husbandry / Corral / Cultivation / Field).
## They are different KINDS of thing, so they never share a surface: no knowledge percentage is ever
## rendered into a herd's or a patch's stat grid, where it would read as one more number about that
## animal. The two are related in exactly one place — a gated verb's reason line under the policy
## picker (`_hunt_policy_gates` / `_forage_policy_gates`), which names the knowledge, its live
## percent, and the practice that fills it. That line is what teaches the ladder.
##
## All FOUR tracks render (slice 4 added Seed Selection + Penning), in `KNOWLEDGE_TRACK_LABELS` order
## — each web's ladder bottom-rung-first — so the strip reads as two ladders climbing rather than
## four unrelated numbers. A track is hidden until the faction begins learning it (the snapshot row is
## sparse, and an unstarted rung is noise); a completed track reads "✔ known" (SIGNAL) not a full bar.
func update_intensification(intensification_variant: Variant) -> void:
    _ingest_intensification(intensification_variant)
    if intensification_label == null:
        return
    var segments: Array[String] = []
    var all_known := true
    for track in KNOWLEDGE_TRACK_LABELS:
        var progress := _faction_knowledge(PLAYER_FACTION_ID, String(track))
        if progress <= 0.0:
            continue
        segments.append("%s %s" % [
            String(KNOWLEDGE_TRACK_LABELS[track]), _knowledge_meter_text(progress)])
        all_known = all_known and progress >= KNOWLEDGE_COMPLETE
    if segments.is_empty():
        intensification_label.visible = false
        return
    intensification_label.visible = true
    # The prefix is what makes this strip read as YOUR PEOPLE'S SKILL rather than as a stat of
    # whatever is currently selected — the one line of copy carrying the §4.1 distinction in the HUD.
    # Break the tracks into rows of KNOWLEDGE_STRIP_TRACKS_PER_LINE so a fourth (or future fifth) track
    # can NEVER run off the right edge: the strip lives in a content-sized top-bar block, so a single
    # long line just widens the block until it clips (the Penning playtest report) — autowrap can't
    # engage without a bounded width. Explicit rows bound each line regardless of window width. The
    # prefix rides the first row.
    var rows: Array[String] = []
    var line_segs: Array[String] = []
    for seg in segments:
        line_segs.append(seg)
        if line_segs.size() >= KNOWLEDGE_STRIP_TRACKS_PER_LINE:
            rows.append(INTENSIFICATION_SEGMENT_SEP.join(line_segs))
            line_segs = []
    if not line_segs.is_empty():
        rows.append(INTENSIFICATION_SEGMENT_SEP.join(line_segs))
    intensification_label.text = "%s%s" % [KNOWLEDGE_STRIP_PREFIX, "\n".join(rows)]
    # Cyan once every learned track is fully known; neutral while any is still in progress.
    intensification_label.add_theme_color_override(
        "font_color", HudStyle.SIGNAL if all_known else HudStyle.INK_DIM)

## Capture the per-faction intensification tracks off the snapshot AND announce the moment one
## COMPLETES — the transition (`< 1.0` last snapshot, `>= 1.0` now) is exactly when a new policy
## becomes usable, and nothing else in the HUD would tell the player. One-shot per faction+track
## (`_knowledge_announced`), so it never re-fires on subsequent snapshots; a track already complete
## on the first snapshot we see (fresh connect / rehydrated save) has no prior value and is NOT
## announced — a nudge about something learned long ago is noise.
func _ingest_intensification(intensification_variant: Variant) -> void:
    if not (intensification_variant is Array):
        return
    for entry in intensification_variant:
        if not (entry is Dictionary):
            continue
        var row := entry as Dictionary
        var faction := int(row.get("faction", -1))
        if faction < 0:
            continue
        var previous: Dictionary = _intensification_knowledge.get(faction, {})
        # Every track the ladder defines, off the one list — so adding a rung's knowledge is a
        # KNOWLEDGE_TRACK_LABELS entry plus a decoder field, never an edit here.
        var current := {}
        for track in KNOWLEDGE_TRACK_LABELS:
            current[track] = float(row.get(track, 0.0))
        for track in KNOWLEDGE_UNLOCK_NOTES:
            if not previous.has(track):
                continue
            if float(previous[track]) >= KNOWLEDGE_COMPLETE:
                continue
            if float(current[track]) < KNOWLEDGE_COMPLETE:
                continue
            _announce_knowledge_unlock(faction, String(track))
        _intensification_knowledge[faction] = current

## Post the one-shot "policy unlocked" nudge to the command feed. Player faction only — another
## faction's tech is not the player's to see, and every other intensification readout filters the
## same way; the announced set is still keyed per faction so the dedupe is correct for all of them.
func _announce_knowledge_unlock(faction: int, track: String) -> void:
    var key := "%d:%s" % [faction, track]
    if _knowledge_announced.has(key):
        return
    _knowledge_announced[key] = true
    if faction != PLAYER_FACTION_ID:
        return
    _note_command_feed(String(KNOWLEDGE_UNLOCK_LABELS[track]), String(KNOWLEDGE_UNLOCK_NOTES[track]))

## A faction's progress (0..1) on one intensification track; 0 when the faction has not begun it
## (the snapshot row is sparse) or no snapshot has arrived yet.
func _faction_knowledge(faction: int, track: String) -> float:
    var tracks: Dictionary = _intensification_knowledge.get(faction, {})
    return float(tracks.get(track, 0.0))

## One knowledge track's readout: a compact block-glyph bar + the live percent while in progress, a
## "✔ known" badge once complete. `progress` is 0..1.
##
## Deliberately TIGHTER than the Sedentarization meter beside it (which keeps the shared 10-cell
## `_meter_bar`): slice 4 took this strip from two tracks to FOUR, and at 10 cells + the word
## "learning" per track the line overflowed the top bar and clipped its last track off-screen —
## verified in the first cut of `two_meter_split.png`. The percent replaces the word rather than
## joining it: it is strictly more informative, and it is the SAME number the gate reason under a
## locked verb quotes, so the strip and the reason line visibly agree.
func _knowledge_meter_text(progress: float) -> String:
    # A bare ✔ rather than "✔ known": the strip's own prefix already reads "Your people know:", so the
    # word was saying it twice — and the two it cost per known track were enough to push the fourth
    # track's percent onto the frame edge.
    if progress >= KNOWLEDGE_COMPLETE:
        return KNOWLEDGE_KNOWN_BADGE
    return "%s %d%%" % [
        _meter_bar(progress * PROGRESS_PERCENT_SCALE, KNOWLEDGE_METER_CELLS),
        _progress_percent(progress)]

## Tint the dependency readout: amber when dependents outnumber workers, cyan when there is a
## healthy labor surplus, neutral otherwise.
## A block-glyph bar for a 0–100 score. `cells` defaults to the standard width, so the
## Sedentarization meter and every existing caller are unchanged; the knowledge strip passes a
## narrower one because it now carries FOUR tracks on one top-bar line.
func _meter_bar(score: float, cells: int = METER_BAR_CELLS) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * float(cells)))
    return "▰".repeat(filled) + "▱".repeat(cells - filled)

func _sedentarization_color(stage: String) -> Color:
    match stage:
        "hard":
            return HudStyle.SIGNAL
        "soft":
            return HudStyle.WARN
        _:
            return HudStyle.INK_DIM

func update_stockpiles(faction_inventory_variant: Variant) -> void:
    if stockpile_panel == null:
        return
    var faction_array: Array = faction_inventory_variant if faction_inventory_variant is Array else []
    var next_totals: Dictionary = {}
    for faction_entry in faction_array:
        if not (faction_entry is Dictionary):
            continue
        if int(faction_entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        var inventory_variant: Variant = faction_entry.get("inventory", [])
        if inventory_variant is Array:
            var inventory_entries: Array = inventory_variant
            for stock_entry in inventory_entries:
                if not (stock_entry is Dictionary):
                    continue
                var item_name := String(stock_entry.get("item", "")).strip_edges()
                if item_name == "":
                    continue
                next_totals[item_name] = int(stock_entry.get("quantity", 0))
        break
    var combined_keys: Array = []
    for key in _stockpile_totals.keys():
        if not combined_keys.has(key):
            combined_keys.append(key)
    for key in next_totals.keys():
        if not combined_keys.has(key):
            combined_keys.append(key)
    combined_keys.sort()
    var panel_entries: Array = []
    for key in combined_keys:
        var amount := int(next_totals.get(key, 0))
        var previous := int(_stockpile_totals.get(key, 0))
        if amount == 0 and previous == 0:
            continue
        var delta := float(amount - previous)
        panel_entries.append({
            "label": _format_stockpile_label(key),
            "amount": amount,
            "delta": delta,
        })
    _stockpile_totals = next_totals
    if stockpile_list == null or stockpile_panel == null:
        return
    for child in stockpile_list.get_children():
        child.queue_free()
    if panel_entries.is_empty():
        stockpile_panel.visible = false
        return
    stockpile_panel.visible = true
    for entry in panel_entries:
        stockpile_list.add_child(_build_stockpile_row(entry))

## Render the live map-zoom readout (e.g. "1.6×"). Driven by MapView.zoom_changed
## via Main, so it reflects the rail buttons, the wheel, and the Q/E keys alike.
func set_zoom_readout(zoom_factor: float) -> void:
    if zoom_level_label != null:
        zoom_level_label.text = "%.1f×" % zoom_factor

## Wire the bottom-left zoom rail: ＋/－ step the map zoom, ⊡ fits to view. Every
## button is styled through HudStyle (no raw default-theme buttons); the readout
## label reads as tabular cyan mono.
func _connect_zoom_rail() -> void:
    if nav_backing != null:
        nav_backing.add_theme_stylebox_override("panel", HudStyle.nav_backing_stylebox())
    HudStyle.apply_button(zoom_in_button2, "ghost")
    HudStyle.apply_button(zoom_out_button2, "ghost")
    HudStyle.apply_button(zoom_fit_button, "ghost")
    if zoom_level_label != null:
        zoom_level_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
    if zoom_in_button2 != null and not zoom_in_button2.is_connected("pressed", Callable(self, "_on_zoom_in_pressed")):
        zoom_in_button2.pressed.connect(_on_zoom_in_pressed)
    if zoom_out_button2 != null and not zoom_out_button2.is_connected("pressed", Callable(self, "_on_zoom_out_pressed")):
        zoom_out_button2.pressed.connect(_on_zoom_out_pressed)
    if zoom_fit_button != null and not zoom_fit_button.is_connected("pressed", Callable(self, "_on_zoom_fit_pressed")):
        zoom_fit_button.pressed.connect(_on_zoom_fit_pressed)

## Wire the turn orb: it re-emits the existing advance/jump signals, so the Main
## wiring (next_turn_requested / alert_focus_requested → MapView.focus_on_tile) is
## unchanged — the orb just replaces the old advance-turn button as their source.
func _connect_turn_orb() -> void:
    if turn_orb == null:
        return
    if not turn_orb.is_connected("focus_requested", Callable(self, "_on_turn_orb_focus")):
        turn_orb.focus_requested.connect(_on_turn_orb_focus)
    if not turn_orb.is_connected("advance_requested", Callable(self, "_on_turn_orb_advance")):
        turn_orb.advance_requested.connect(_on_turn_orb_advance)
    if not turn_orb.is_connected("panel_requested", Callable(self, "_on_turn_orb_panel_requested")):
        turn_orb.panel_requested.connect(_on_turn_orb_panel_requested)

# ---- The Telling: the fork decision surface --------------------------------

## Build the narrative-fork panel once. It is a child of the HUD CanvasLayer itself, NOT of
## `layout_root`: the panel is a modal overlay over the whole window, so it must not inset with
## the reserved docks the way the layout column does. Added last so it draws above every card.
func _ensure_fork_panel() -> void:
    if _fork_panel != null:
        return
    _fork_panel = NarrativeForkPanel.new()
    _fork_panel.name = "NarrativeForkPanel"
    _fork_panel.answer_selected.connect(_on_fork_answer_selected)
    add_child(_fork_panel)

## Cache the PLAYER faction's pending forks and rebuild the orb registry.
##
## The caller only invokes this when the snapshot actually CARRIES the key (deltas omit an
## unchanged field), so this is never reached with "absent means cleared" semantics.
func update_pending_forks(forks_variant: Variant) -> void:
    _pending_forks = _faction_rows(forks_variant, "forks")
    _push_attention()
    _maybe_auto_open_fork()

## Cache the PLAYER faction's stance axes — what the people's behaviour and their answers together
## say about who they are. Displayed by the fork panel; no other consumer yet.
func update_stance_axes(axes_variant: Variant) -> void:
    _stance_axes = _faction_rows(axes_variant, "axes")

## The player faction's narrator MEDIUM — oral saga → painted chronicle → written record.
##
## Follows the `sedentarization` ingest precedent exactly: a per-faction wire array, scanned for
## PLAYER_FACTION_ID. The caller only invokes this when the snapshot actually CARRIES the key
## (a delta omits an unchanged field), so absence means "unchanged" and never "cleared".
##
## Presentational only — it never selects different copy. Both narrative surfaces take it, so the
## panel's title/accent and the fork panel's header age together rather than drifting apart.
func update_voice_medium(medium_variant: Variant) -> void:
    if not (medium_variant is Array):
        return
    var medium_id := ""
    for entry_variant in (medium_variant as Array):
        if entry_variant is Dictionary and int((entry_variant as Dictionary).get("faction", -1)) == PLAYER_FACTION_ID:
            medium_id = String((entry_variant as Dictionary).get("medium_id", ""))
            break
    if _telling != null:
        _telling.set_voice_medium(medium_id)
    if _fork_panel != null:
        _fork_panel.set_voice_medium(medium_id)

## Pull the player faction's nested child array out of a per-faction wire array
## (`[{faction, <key>: [...]}, …]`), the shape `discovered_sites`/`sedentarization` also use.
func _faction_rows(variant: Variant, key: String) -> Array:
    if not (variant is Array):
        return []
    for entry_variant in (variant as Array):
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        if int(entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        var rows: Variant = entry.get(key, [])
        return rows if rows is Array else []
    return []

## The orb registry = the cached band/expedition producers + the fork producer, pushed as ONE
## replace. `TurnOrb.set_attention` replaces wholesale, so the fork producer must fold in here
## rather than call it separately — a second call would wipe every band row.
func _push_attention() -> void:
    if turn_orb == null:
        return
    var attention: Array = _band_attention.duplicate(true)
    attention.append_array(_pending_fork_attention())
    turn_orb.set_attention(attention)

## Producer 6 — a narrative fork awaiting an answer. One row per pending fork, `blocking` so the
## orb disables its `Advance ▸` footer (the client-side end-turn gate; the server never blocks).
func _pending_fork_attention() -> Array:
    var rows: Array = []
    var register := NarrativeForkPanel.load_voice_register()
    for fork_variant in _pending_forks:
        if not (fork_variant is Dictionary):
            continue
        var fork: Dictionary = fork_variant
        rows.append({
            "kind": ATTENTION_KIND_DECISION,
            "severity": ATTENTION_SEVERITY_CRITICAL,
            "blocking": true,
            "label": ATTENTION_DECISION_LABEL,
            "detail": _fork_row_detail(fork, register),
            "x": ATTENTION_NON_LOCATING, "y": ATTENTION_NON_LOCATING,
        })
    return rows

## The orb row's one-line taste of the question. The narration is a paragraph and orb rows CLIP,
## so it is truncated here on purpose — the full telling is the panel's job.
func _fork_row_detail(fork: Dictionary, register: String) -> String:
    var narration := NarrativeForkPanel.text_in_register(fork.get("narration", []), register)
    if narration.length() <= ATTENTION_DECISION_DETAIL_MAX_CHARS:
        return narration
    return narration.substr(0, ATTENTION_DECISION_DETAIL_MAX_CHARS).strip_edges() + ATTENTION_DECISION_DETAIL_ELLIPSIS

## Open the panel automatically the FIRST time a fork appears — an identity-defining moment should
## not wait behind a click. Tracked by beat_id so a dismissed fork does not re-open every snapshot.
func _maybe_auto_open_fork() -> void:
    var fork := _first_pending_fork()
    if fork.is_empty():
        return
    var beat_id := String(fork.get("beat_id", ""))
    if beat_id == "" or _auto_opened_forks.has(beat_id):
        return
    _auto_opened_forks[beat_id] = true
    _open_fork_panel()

func _first_pending_fork() -> Dictionary:
    for fork_variant in _pending_forks:
        if fork_variant is Dictionary:
            return fork_variant
    return {}

func _open_fork_panel() -> void:
    _ensure_fork_panel()
    var fork := _first_pending_fork()
    if fork.is_empty():
        return
    _fork_panel.show_fork(fork)

## A non-locating orb row was activated. The orb reports only the KIND; the Hud owns which panel
## a kind opens, so a future non-locating producer needs no orb change.
func _on_turn_orb_panel_requested(kind: String) -> void:
    if kind == ATTENTION_KIND_DECISION:
        _open_fork_panel()

## The player answered. Drop the fork from the local cache OPTIMISTICALLY (so the orb's gate lifts
## immediately) and let the next snapshot be authoritative.
func _on_fork_answer_selected(payload: Dictionary) -> void:
    var beat_id := String(payload.get("beat_id", ""))
    var choice_id := String(payload.get("choice_id", ""))
    if beat_id == "" or choice_id == "":
        return
    var remaining: Array = []
    for fork_variant in _pending_forks:
        if fork_variant is Dictionary and String((fork_variant as Dictionary).get("beat_id", "")) == beat_id:
            continue
        remaining.append(fork_variant)
    _pending_forks = remaining
    _push_attention()
    emit_signal("answer_fork_requested", {
        "faction": PLAYER_FACTION_ID,
        "beat_id": beat_id,
        "choice_id": choice_id,
    })

## Is a fork holding the turn? Read by the Inspector-path advance note (the dev toolbar and
## autoplay are deliberately NOT gated — see docs/plan_the_telling.md).
func has_pending_fork() -> bool:
    return not _pending_forks.is_empty()

## The dev toolbar / autoplay advanced past an unanswered fork. Not a gate — a RECEIPT: the
## server will expire the fork to its defer branch, which is a real narrative outcome, so a
## developer who skipped the question must be able to see that they did.
func note_unanswered_fork() -> void:
    _note_command_feed(UNANSWERED_FORK_LABEL, UNANSWERED_FORK_DETAIL)

## The labor-allocation UI (allocation panel, herd/tile assign controls) is built at
## runtime with its own per-widget signal connections, so there are no static selection
## buttons left to wire here. Kept as a hook for future static selection controls.
func _connect_selection_buttons() -> void:
    pass

func _on_zoom_out_pressed() -> void:
    emit_signal("map_zoom_step", -1)

func _on_zoom_in_pressed() -> void:
    emit_signal("map_zoom_step", 1)

func _on_zoom_fit_pressed() -> void:
    emit_signal("map_zoom_fit")

## An orb row's "Jump →". A row that locates an AWAITING EXPEDITION routes through the SAME path the
## Band panel's Active-expeditions row click uses (`_on_panel_expedition_selected`: recenter + pin the
## exact expedition so its drawer opens and the panel band isn't hijacked) rather than a second,
## weaker jump that would only recenter the hex and auto-select whatever occupant sits on it. Every
## other producer (band-located) keeps the plain recenter.
func _on_turn_orb_focus(x: int, y: int) -> void:
    var exp := _awaiting_expedition_at(x, y)
    if not exp.is_empty():
        _on_panel_expedition_selected(int(exp.get("entity", -1)), x, y)
        return
    # A starving-pen row jumps to the HERD, not just its hex: `_focus_labor_source` (the very path
    # the Band panel's Hunt row uses) recenters AND pins the herd, so the drawer that explains the
    # alert — the "⚠ Starving" Corral row + the Pen feed cost — is what actually opens.
    var pen_herd := _starving_pen_at(x, y)
    if pen_herd != "":
        _focus_labor_source(x, y, pen_herd)
        return
    emit_signal("alert_focus_requested", x, y)

func _on_turn_orb_advance() -> void:
    # Advancing the turn is "catch me up": reveal the newest telling so a player who moves on isn't left
    # parked on an old page (mid-turn beats only mark the book unread — see TellingPanel.reveal_newest).
    if _telling != null:
        _telling.reveal_newest()
    emit_signal("next_turn_requested", 1)

# ---- Early-Game Labor allocation (slice 3b) --------------------------------
# Source-centric worker allocation for the single player band. The allocation panel
# (band drawer), the herd "assign hunters" controls, and the tile "assign foragers"
# controls are all built at runtime here; each emits `assign_labor_requested` (Main
# formats the `assign_labor …` command). The Work zone's bulk unassign reuses
# `cancel_order_requested`, scoped `work`.

## Resolve the band that assignment/move/clear commands target. The selected band when
## it is a player band; otherwise the single player band captured from the snapshot (so
## herd/tile assign controls still target it while a herd/tile is selected). {} if none.
func _resolve_assign_band() -> Dictionary:
    if not _selected_unit.is_empty() and _is_player_unit(_selected_unit):
        return _selected_unit
    return _player_band

## The player bands the band-picker lists. Normally `_player_bands` (captured each snapshot);
## falls back to `[_player_band]` when only the single band was seeded (e.g. the ui_preview
## harness, or before the first alerts pass) so the dropdown is always populated.
func _current_player_bands() -> Array:
    if not _player_bands.is_empty():
        return _player_bands
    return [_player_band] if not _player_band.is_empty() else []

## Resolve a listed player band by its entity id; {} if it is no longer present.
func _player_band_by_entity(entity: int) -> Dictionary:
    for b in _current_player_bands():
        if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
            return b
    return {}

## Max workers a band can commit to ONE source: its idle workers plus any it already has on
## that source (the assign REPLACES that count, so re-editing an existing assignment isn't
## capped below its current staffing). Reduces to `idle_workers` for a fresh source.
func _assignable_hunt_workers(band: Dictionary, herd_id: String) -> int:
    return int(band.get("idle_workers", 0)) + _workers_for_hunt(band, herd_id)

func _assignable_forage_workers(band: Dictionary, x: int, y: int) -> int:
    return int(band.get("idle_workers", 0)) + _workers_for_forage(band, x, y)

## Map grid dimensions captured each snapshot (Main forwards the snapshot `grid` key). Width + wrap
## feed the wrap-aware hex distance the herd-hunt affordance keys its local-vs-expedition decision
## off. Grid rides full snapshots only; persists across deltas (fields default to the last value).
func set_grid_dimensions(grid: Variant) -> void:
    if not (grid is Dictionary):
        return
    var g: Dictionary = grid
    _grid_width = int(g.get("width", _grid_width))
    _grid_height = int(g.get("height", _grid_height))
    _grid_wrap_horizontal = bool(g.get("wrap_horizontal", _grid_wrap_horizontal))

## The world's herds captured each snapshot (Main forwards the snapshot `herds` key, the same array
## `MapView._rebuild_herd_markers` consumes). Herds MIGRATE every turn, so this — not a hunt
## assignment's launch-time `target_x/target_y` — is the authority on where a hunted herd IS.
func update_herds(herds_variant: Variant) -> void:
    if not (herds_variant is Array):
        return
    _world_herds = herds_variant

## The snapshot herd with this id, wherever it is on the map; {} when unknown.
## Mirrors `MapView._herd_by_id` (the hunted-herd ring's resolver).
func _find_world_herd(herd_id: String) -> Dictionary:
    if herd_id == "":
        return {}
    for herd in _world_herds:
        if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
            return herd
    return {}

## The world's food modules captured each snapshot (Main forwards MapView's ingested food sites —
## the same dicts in `MapView.food_site_lookup`, each stamped with `terrain_id`). Keyed by tile so a
## forage assignment's `target_x/target_y` resolves to the module the map draws there — that's how a
## Current-actions Forage row shows the SAME resource glyph as the map marker (`FoodIcons.for_site`,
## including the riverine_delta fish↔reeds split that reads the stamped `terrain_id`).
var _food_module_by_tile: Dictionary = {}

## Ingests MapView's terrain-stamped food sites (x/y/module/kind + terrain_id) into the per-tile map
## the Forage row reads, so its glyph matches the map marker (riverine split included).
func update_food_modules(modules_variant: Variant) -> void:
    if not (modules_variant is Array):
        return
    _food_module_by_tile.clear()
    for entry in modules_variant:
        if not (entry is Dictionary):
            continue
        var site: Dictionary = entry
        var sx := int(site.get("x", -1))
        var sy := int(site.get("y", -1))
        if sx >= 0 and sy >= 0:
            _food_module_by_tile[Vector2i(sx, sy)] = site

## The world's forage patches captured each snapshot (Main forwards the snapshot `forage_patches`
## array — the same dicts in `MapView.forage_patch_lookup`), keyed by tile. A Current-actions Forage
## row reads the patch here to forecast its max-useful worker cap (the compose control gets the same
## forecast off `tile_info`'s `patch_`-prefixed cross-ref; the RAW wire dict here carries the fields
## BARE — see `BARE_FORECAST_PREFIX`).
var _forage_patch_lookup: Dictionary = {}

## Ingests the snapshot forage patches into the per-tile lookup the Current-actions Forage row reads
## to cap its worker stepper at max-useful, mirroring MapView's `forage_patch_lookup` ingest.
func update_forage_patches(patches_variant: Variant) -> void:
    if not (patches_variant is Array):
        return
    _forage_patch_lookup.clear()
    for entry in patches_variant:
        if not (entry is Dictionary):
            continue
        var patch: Dictionary = entry
        var px := int(patch.get("x", -1))
        var py := int(patch.get("y", -1))
        if px >= 0 and py >= 0:
            _forage_patch_lookup[Vector2i(px, py)] = patch

## "<glyph> " for a resolved glyph, "" for none — so a Current-actions row degrades to bare text
## (no stray leading space) when the resource can't be resolved.
func _source_icon_prefix(icon: String) -> String:
    return "%s " % icon if icon != "" else ""

## The resource glyph for the food module on (x, y) — the same icon `MapView._draw_food_site` draws
## there. "" when the tile has no known module (undiscovered), so the row renders
## bare rather than with a misleading fallback sprig.
func _food_module_icon(x: int, y: int) -> String:
    var site: Variant = _food_module_by_tile.get(Vector2i(x, y), null)
    if not (site is Dictionary):
        return ""
    var module_key := String((site as Dictionary).get("module", ""))
    var is_hunt := String((site as Dictionary).get("kind", "")) == FOOD_SITE_KIND_GAME_TRAIL
    return FoodIcons.for_site(module_key, is_hunt, int((site as Dictionary).get("terrain_id", -1)))

## The band's current tile (col,row), reading the raw cohort `current_x/y` (snapshot entries) or the
## MapView marker's `pos` fallback; (-1,-1) when unknown.
func _band_tile(band: Dictionary) -> Vector2i:
    var cx := int(band.get("current_x", -1))
    var cy := int(band.get("current_y", -1))
    if cx >= 0 and cy >= 0:
        return Vector2i(cx, cy)
    var pos_variant: Variant = band.get("pos", [])
    if pos_variant is Array and (pos_variant as Array).size() == 2:
        return Vector2i(int((pos_variant as Array)[0]), int((pos_variant as Array)[1]))
    return Vector2i(-1, -1)

## The player's starting band tile (col,row) — the first player-faction band captured this snapshot
## into `_player_band` (via update_band_alerts). Returns (-1,-1) when there is no player band, so a
## caller (Main's startup-view centering) can defensively skip the focus. Reads the same `current_x/y`
## cohort fields `_band_tile` does.
func get_player_band_tile() -> Vector2i:
    if _player_band.is_empty():
        return Vector2i(-1, -1)
    return _band_tile(_player_band)

## Shortest signed column delta from→to honoring horizontal wrap (mirrors MapView._wrapped_col_delta),
## so a herd across the seam measures by its short wrapped distance, not the long way across the map.
## Mirrors the sim's `grid_utils::shortest_delta_x` exactly (magnitude only here, no live
## direction effect): keep the direct delta when within half the width, else shift by one width.
## The exact-half tie (`abs(d) == width/2`) resolves POSITIVE like the sim, NOT `round()`'s
## half-away-from-zero — kept consistent with MapView._wrapped_col_delta.
func _wrapped_col_delta(from_col: int, to_col: int) -> int:
    var d := to_col - from_col
    if _grid_wrap_horizontal and _grid_width > 0:
        # Integer half-width mirrors the sim's `w / 2` truncation.
        var half_width := _grid_width / 2
        if d > half_width:
            d -= _grid_width
        elif d < -half_width:
            d += _grid_width
    return d

## odd-r offset (col,row) → axial (mirrors MapView._offset_to_axial).
func _offset_to_axial(col: int, row: int) -> Vector2i:
    var q := col - ((row - (row & 1)) >> 1)
    return Vector2i(q, row)

## Wrap-aware true odd-r hex distance between two offset tiles (mirrors the sim's `hex_distance_wrapped`
## / MapView._hex_distance): bring the target into the source's column frame via _wrapped_col_delta,
## then odd-r offset→axial→cube distance. Returns -1 when either tile is unknown.
func _hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
    if a_col < 0 or a_row < 0 or b_col < 0 or b_row < 0:
        return -1
    var b_eff_col := a_col + _wrapped_col_delta(a_col, b_col)
    var a := _offset_to_axial(a_col, a_row)
    var b := _offset_to_axial(b_eff_col, b_row)
    var dq: int = a.x - b.x
    var dr: int = a.y - b.y
    return int((abs(dq) + abs(dr) + abs(dq + dr)) / 2)

## Max party the band can detach as a hunting expedition: min(idle_workers, max_expedition_party_size),
## falling back to idle when the cap is absent/0 (mirrors the compose sheet's `party_max`).
func _expedition_party_cap(band: Dictionary) -> int:
    var idle := int(band.get("idle_workers", 0))
    var cap := int(band.get("max_expedition_party_size", 0))
    return mini(idle, cap) if cap > 0 else idle

## A "Band: [▼]" dropdown row for the assign controls: lists every player band (positional
## "Band N" names, matching the roster) and selects `selected_band`; `on_pick` fires with the
## chosen band dict. The actor band is always explicit — shown even with one band (single-item
## dropdown). NOTE: lists ALL player bands; in-range filtering (Forage within work_range / Hunt
## within work_range + leash) is deferred to the multi-band slice (needs the hunt-leash reach in
## the snapshot, and can't be exercised until a 2nd band can exist).
func _build_band_picker(selected_band: Dictionary, on_pick: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var name_label := Label.new()
    name_label.text = BAND_PICKER_LABEL
    name_label.add_theme_color_override("font_color", HudStyle.INK)
    row.add_child(name_label)
    var picker := OptionButton.new()
    picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var bands := _current_player_bands()
    var selected_entity := int(selected_band.get("entity", -1))
    var selected_idx := 0
    for i in bands.size():
        var b: Dictionary = bands[i]
        picker.add_item(_band_display_name(b, i + 1))
        picker.set_item_metadata(i, int(b.get("entity", -1)))
        if int(b.get("entity", -1)) == selected_entity:
            selected_idx = i
    picker.select(selected_idx)
    picker.item_selected.connect(func(idx: int) -> void:
        on_pick.call(_player_band_by_entity(int(picker.get_item_metadata(idx)))))
    row.add_child(picker)
    return row

func _labor_assignments_of(band: Dictionary) -> Array:
    var v: Variant = band.get("labor_assignments", [])
    return v if v is Array else []

## Workers currently on a band-wide role (scout/warrior); 0 when unstaffed.
func _workers_for_role(band: Dictionary, kind: String) -> int:
    for entry in _labor_assignments_of(band):
        if entry is Dictionary and String((entry as Dictionary).get("kind", "")).to_lower() == kind:
            return int((entry as Dictionary).get("workers", 0))
    return 0

## The band's standing FORAGE assignment on (x,y) — `{}` when it works no such tile. The one lookup
## behind the worker count, the seeded policy and the drawer's standing summary, so the three can
## never read different assignments.
func _forage_assignment_of(band: Dictionary, x: int, y: int) -> Dictionary:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_FORAGE \
                and int(a.get("target_x", -1)) == x and int(a.get("target_y", -1)) == y:
            return a
    return {}

## The band's standing HUNT assignment on `herd_id` — `{}` when it hunts no such herd. The herd twin
## of `_forage_assignment_of`.
func _hunt_assignment_of(band: Dictionary, herd_id: String) -> Dictionary:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
            return a
    return {}

## Workers currently foraging a specific in-range tile; 0 when unstaffed.
func _workers_for_forage(band: Dictionary, x: int, y: int) -> int:
    return int(_forage_assignment_of(band, x, y).get("workers", 0))

## Workers currently hunting a specific herd; 0 when unstaffed.
func _workers_for_hunt(band: Dictionary, herd_id: String) -> int:
    return int(_hunt_assignment_of(band, herd_id).get("workers", 0))

## The take policy of the band's existing hunt on `herd_id`, else the default.
func _policy_for_hunt(band: Dictionary, herd_id: String) -> String:
    var policy := String(_hunt_assignment_of(band, herd_id).get("policy", "")).strip_edges().to_lower()
    # HUNT_POLICY_OPTIONS, not the extractive four: a herd already being Corralled must
    # re-seed the compose picker as Corral, or re-staffing it would silently drop the pen.
    return policy if policy in HUNT_POLICY_OPTIONS else DEFAULT_HUNT_POLICY

## The take policy of the band's existing forage on (x,y), else the default.
func _policy_for_forage(band: Dictionary, x: int, y: int) -> String:
    var policy := String(_forage_assignment_of(band, x, y).get("policy", "")).strip_edges().to_lower()
    # FORAGE_POLICY_OPTIONS, not the extractive four: a patch already being Cultivated must
    # re-seed the compose picker as Cultivate, or re-staffing it would silently drop the
    # investment back to Sustain (and the patch would go feral).
    return policy if policy in FORAGE_POLICY_OPTIONS else DEFAULT_HUNT_POLICY

## A friendlier label for a herd id — the roster/selected herd's label when known, else the
## snapshot-wide herd list (a hunted herd usually sits on a DIFFERENT hex than the one selected,
## so the roster alone left those rows reading the raw `game_deer_07` id).
func _herd_label_for_id(herd_id: String) -> String:
    var herd := _find_roster_herd(herd_id)
    if not herd.is_empty():
        return String(herd.get("species", herd.get("label", herd_id)))
    if String(_selected_herd.get("id", "")) == herd_id:
        return String(_selected_herd.get("species", _selected_herd.get("label", herd_id)))
    var world_herd := _find_world_herd(herd_id)
    if not world_herd.is_empty():
        return String(world_herd.get("species", world_herd.get("label", herd_id)))
    return herd_id

## Emit an assign_labor request for the given band, and record it as an OPTIMISTIC pending
## action so the panel + map reflect the change immediately (reconciled by the next
## newer-turn snapshot). Main formats the text command from the emitted payload.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String) -> void:
    var bits := int(band.get("entity", -1))
    if bits < 0:
        return
    var clamped: int = max(0, workers)
    emit_signal("assign_labor_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": bits,
        "kind": kind,
        "workers": clamped,
        "x": x,
        "y": y,
        "herd_id": herd_id,
        "policy": policy,
    })
    _record_pending_assign(bits, kind, clamped, x, y, herd_id, policy)
    _after_pending_change()

# ---- Optimistic pending labor (slice 3b UX) --------------------------------

## Stable key identifying a source/role within a band's assignment set.
func _pending_key(kind: String, x: int, y: int, herd_id: String) -> String:
    match kind:
        LABOR_KIND_FORAGE:
            return "forage:%d,%d" % [x, y]
        LABOR_KIND_HUNT:
            return "hunt:%s" % herd_id
        _:
            return kind  # scout / warrior — one band-wide role each

func _record_pending_assign(entity: int, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String) -> void:
    if entity < 0:
        return
    var entry: Dictionary = _pending_labor.get(entity, {})
    entry["turn"] = _current_turn
    var assigns: Dictionary = entry.get("assign", {})
    assigns[_pending_key(kind, x, y, herd_id)] = {
        "kind": kind, "workers": max(0, workers), "x": x, "y": y, "herd_id": herd_id, "policy": policy,
    }
    entry["assign"] = assigns
    _pending_labor[entity] = entry

func _record_pending_move(entity: int, x: int, y: int) -> void:
    if entity < 0:
        return
    var entry: Dictionary = _pending_labor.get(entity, {})
    entry["turn"] = _current_turn
    entry["move"] = {"x": x, "y": y}
    _pending_labor[entity] = entry

## Re-render the current selection (so pending shows in the Occupants/Tile cards) and push the
## pending map to MapView (so pending hexes show), after any optimistic change. Also re-render the
## Band/City panel keyed off `_panel_band` — a worker-stepper edit in the panel must show its
## optimistic pending even when the current selection is a foreign hex (never blank it).
func _after_pending_change() -> void:
    if not _selected_tile_info.is_empty() or not _selected_unit.is_empty() or not _selected_herd.is_empty():
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _rerender_panel_allocation()
    emit_signal("labor_pending_changed", _pending_labor)

## Drop pending entries the server has already processed: a snapshot with a turn NEWER than
## the entry's issue turn is authoritative confirmation (and reflects any clamping). Called
## each snapshot from update_band_alerts, after update_overlay has set _current_turn.
func _reconcile_pending() -> void:
    if _pending_labor.is_empty():
        return
    var changed := false
    for entity in _pending_labor.keys():
        var entry: Dictionary = _pending_labor[entity]
        if int(entry.get("turn", -1)) < _current_turn:
            _pending_labor.erase(entity)
            changed = true
    if changed:
        emit_signal("labor_pending_changed", _pending_labor)

func _pending_assigns_for(entity: int) -> Dictionary:
    var e: Variant = _pending_labor.get(entity, {})
    if not (e is Dictionary):
        return {}
    var a: Variant = (e as Dictionary).get("assign", {})
    return a if a is Dictionary else {}

## Confirmed labor assignments overlaid with this band's pending assigns, keyed by source/role.
## Each value: {kind, workers, x, y, herd_id, policy, pending: bool}.
func _effective_worker_map(band: Dictionary) -> Dictionary:
    var merged: Dictionary = {}
    for a in _labor_assignments_of(band):
        if not (a is Dictionary):
            continue
        var kind := String((a as Dictionary).get("kind", "")).strip_edges().to_lower()
        var key := _pending_key(kind, int(a.get("target_x", -1)), int(a.get("target_y", -1)), String(a.get("fauna_id", "")))
        merged[key] = {
            "kind": kind, "workers": int(a.get("workers", 0)),
            "x": int(a.get("target_x", -1)), "y": int(a.get("target_y", -1)),
            "herd_id": String(a.get("fauna_id", "")), "policy": String(a.get("policy", "")), "pending": false,
            # Per-source yields (food/turn) for the row headline/tooltip/overhunt flag. `has_yield`
            # gates the readout — a confirmed assignment carries them; a pending one (below) does not.
            "actual_yield": float(a.get("actual_yield", 0.0)),
            "sustainable_yield": float(a.get("sustainable_yield", 0.0)),
            "has_yield": a.has("actual_yield"),
            # Min workers that produced this turn's take — drives the overstaffing note.
            "workers_needed": int(a.get("workers_needed", 0)),
            # Provisions offered but not collected (under-crewed) — drives the muted "· N wasted" note.
            "wasted_yield": float(a.get("wasted_yield", 0.0)),
            # WHEN this source's food lands (index i = i+1 turns from now) — drives the row's arrival
            # tick strip. Empty = "not projected", which renders no strip (never a famine).
            "arrival_schedule": _as_schedule(a.get("arrival_schedule", null)),
        }
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    for key in pend:
        var pd: Dictionary = pend[key]
        merged[key] = {
            "kind": String(pd.get("kind", "")), "workers": int(pd.get("workers", 0)),
            "x": int(pd.get("x", -1)), "y": int(pd.get("y", -1)),
            "herd_id": String(pd.get("herd_id", "")), "policy": String(pd.get("policy", "")), "pending": true,
            # A pending (optimistic) assign has no confirmed yield yet — render no yield number.
            # Likewise no confirmed workers_needed, so 0 ⇒ "unknown" ⇒ no overstaffing note until
            # the next snapshot resolves what the source actually used.
            "actual_yield": 0.0, "sustainable_yield": 0.0, "has_yield": false,
            "workers_needed": 0,
            # Nor any projected arrivals — the schedule comes from the sim's forward run, so an
            # un-acknowledged edit shows no strip until the next snapshot projects it.
            "arrival_schedule": PackedFloat32Array(),
        }
    return merged

## Coerce a wire `arrival_schedule` to a PackedFloat32Array. The native decoder already hands over a
## packed array; a fixture (or an absent field) may hand over a plain Array or null, and this is the
## one place that difference is absorbed.
func _as_schedule(value: Variant) -> PackedFloat32Array:
    if value is PackedFloat32Array:
        return value
    var packed := PackedFloat32Array()
    if value is Array:
        for amount in (value as Array):
            packed.push_back(float(amount))
    return packed

## Effective worker count for one role/source, overlaying any pending value.
func _effective_role_workers(band: Dictionary, kind: String) -> Dictionary:
    var key := _pending_key(kind, -1, -1, "")
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    if pend.has(key):
        return {"workers": int((pend[key] as Dictionary).get("workers", 0)), "pending": true}
    return {"workers": _workers_for_role(band, kind), "pending": false}

func _effective_forage_workers(band: Dictionary, x: int, y: int) -> int:
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    var key := _pending_key(LABOR_KIND_FORAGE, x, y, "")
    if pend.has(key):
        return int((pend[key] as Dictionary).get("workers", 0))
    return _workers_for_forage(band, x, y)

func _effective_hunt_workers(band: Dictionary, herd_id: String) -> int:
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    var key := _pending_key(LABOR_KIND_HUNT, -1, -1, herd_id)
    if pend.has(key):
        return int((pend[key] as Dictionary).get("workers", 0))
    return _workers_for_hunt(band, herd_id)

## Optimistic idle = working-age minus the sum of effective worker counts.
func _effective_idle(band: Dictionary) -> int:
    var assigned := 0
    var merged := _effective_worker_map(band)
    for key in merged:
        assigned += int((merged[key] as Dictionary).get("workers", 0))
    return max(0, int(band.get("working_age", 0)) - assigned)

## A trailing glyph on a row ("  ♻" / "  ●"), separated from the label — "" for an unknown/absent
## glyph, so a row with no policy / no status renders bare rather than trailing whitespace.
func _row_glyph_suffix(glyph: String) -> String:
    return "" if glyph == "" else ROW_GLYPH_SEPARATOR + glyph

## The WORDS behind a status glyph. Order-level statuses come from `STATUS_LABELS`; an expedition
## PHASE reads from `EXPEDITION_PHASE_LABELS` (`_expedition_phase_label`), which stays the single
## source of truth for the phase words — they are never re-typed here.
func _status_label(status: String) -> String:
    var key := status.strip_edges().to_lower()
    if key == "":
        return ""
    if STATUS_LABELS.has(key):
        return String(STATUS_LABELS[key])
    return _expedition_phase_label(key)

## One tooltip line spelling a status glyph out: the word plus its behaviour hint ("Pending — starts
## when you advance the turn"); a status whose word says it all (`Working`) renders bare.
func _status_tooltip_line(status: String) -> String:
    var label := _status_label(status)
    if label == "":
        return ""
    var hint := String(STATUS_HINTS.get(status.strip_edges().to_lower(), ""))
    return label if hint == "" else STATUS_HINT_FORMAT % [label, hint]

## Append the status words to a row tooltip. The glyph on the row is terse by design, so the hover
## must carry what it encodes — composed WITH the tooltip the row already had (yield readout,
## overstaffing explanation, policy hint), never replacing it.
func _append_status_tooltip(tooltip: String, status: String) -> String:
    var status_line := _status_tooltip_line(status)
    if status_line == "":
        return tooltip
    return status_line if tooltip == "" else tooltip + TOOLTIP_LINE_SEPARATOR + status_line

## Join the non-empty parts of a row tooltip (yield readout · policy behaviour · …) into one block.
func _join_tooltip_lines(lines: Array) -> String:
    var parts: Array[String] = []
    for line in lines:
        var text := String(line)
        if text != "":
            parts.append(text)
    return TOOLTIP_LINE_SEPARATOR.join(parts)

## The behaviour hint for a source's take policy, so the row's policy GLYPH is spelled out on hover.
## Reuses the picker's existing hint strings (kind-specific: gathering a patch vs culling a herd) —
## the same sentence the player read when they chose the policy. A worked source row is ALWAYS a
## resident band's standing assignment, so the hunt side reads the LOCAL hints (never the expedition
## set, whose payoffs differ).
func _policy_hint(kind: String, policy: String) -> String:
    var key := policy.strip_edges().to_lower()
    if kind == LABOR_KIND_FORAGE:
        return String(FORAGE_POLICY_HINTS.get(key, ""))
    return String(LOCAL_HUNT_POLICY_HINTS.get(key, ""))

## A "<label>   − N +" worker-count row. `on_change` is called with the new count
## when either stepper is pressed. `plus_enabled` gates the + (e.g. no idle workers).
## `status` is the row's action status (`FoodIcons.STATUS_WORKING` for a confirmed forage/hunt
## source; "" for the band-wide Scout/Warrior roles, which report no per-action state), and
## `pending` marks an optimistic (not-yet-confirmed) ORDER, which overrides the status: the row
## renders the `◌` glyph instead of `●` and its label reads amber, tying it to the amber pending hex
## on the map. Either way the state is a GLYPH, never a word — `tooltip` carries the words (see the
## action-status vocabulary above); the status line is appended to it here so every caller composes
## it the same way.
## `on_focus_source` (optional) makes the LABEL a clickable inline link that jumps the map to the
## row's source — a Forage tile / a hunted herd's live tile. It is a separate child from the
## steppers, so the −/+ buttons keep working untouched and the count stays right-aligned. Band-wide
## roles (Scout/Warrior) have no tile, so they pass nothing and keep a plain Label.
## `status_line` (default "") is the OPT-IN to the two-line form used ONLY by the Forage/Hunt
## Current-actions rows: when non-empty the title (icon + action + location) + the −/+ stepper ride
## line 1, and the yield/policy text (`status_line`) + the status glyph + the ⚠/overstaff/wasted notes
## drop to an indented, smaller secondary line 2 that WRAPS rather than widening the panel. When "",
## every existing caller (Scout/Warrior, the compose steppers) renders the unchanged single-line HBox.
## `arrival_schedule` (default empty) is the source's projected per-turn deliveries. When it has a GAP
## (`ArrivalStrip.has_gap`) the two-line form gains a third, indented line: the arrival tick strip that
## shows WHEN the steady average actually lands. A continuous source (or an unprojected row) has no
## lumpiness to explain and gets no strip. Ignored by the single-line form.
func _build_worker_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, pending: bool = false, warn: bool = false, tooltip: String = "", note: String = "", on_focus_source: Callable = Callable(), status: String = "", muted_note: String = "", status_line: String = "", arrival_schedule: PackedFloat32Array = PackedFloat32Array()) -> Control:
    # Pending is a state of the ORDER, so it wins the glyph slot over whatever the action is doing.
    var status_key := FoodIcons.STATUS_PENDING if pending else status
    var row_tooltip := _append_status_tooltip(tooltip, status_key)
    # Pending tints the row's IDENTITY amber (the title — it ties to the amber pending hex on the map);
    # a settled row reads plain INK.
    var row_ink: Color = HudStyle.WARN if pending else HudStyle.INK
    if status_line != "":
        return _build_two_line_stepper(
            label_text, count, plus_enabled, on_change, warn, row_tooltip, note,
            on_focus_source, status_key, muted_note, status_line, row_ink, arrival_schedule)
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    if row_tooltip != "":
        row.tooltip_text = row_tooltip
    var row_text := label_text + _row_glyph_suffix(FoodIcons.for_status(status_key))
    row.add_child(_build_row_name_label(row_text, row_ink, row_tooltip, on_focus_source))
    # Overhunting flag: a WARN-tinted ⚠ sits directly after the label (before the stepper), so an
    # overdrawn herd row pops without recoloring the whole label. Forage never trips this.
    if warn:
        row.add_child(_build_row_note_label(OVERHUNT_FLAG, HudStyle.WARN, row_tooltip))
    # Overstaffing note ("· only 1 of 5 working"): WARN-tinted, sits after the label/⚠ so the wasted
    # labor reads at a glance without recoloring the whole row. Deliberately NOT the ⚠ flag — that
    # means "overdrawing" (ecological); this means "extra workers idle here" (see
    # `_source_yield_readout`). The tooltip carries the full explanation.
    if note != "":
        row.add_child(_build_row_note_label(note, HudStyle.WARN, row_tooltip))
    # Understaffing note ("· 1.7 wasted"): MUTED (INK_FAINT), the low-key mirror of the WARN overstaff
    # note — it says "the source offered more than the crew carried home" (add workers), a softer nudge
    # than the ecological ⚠. Fed by `wasted_yield`; tooltip carries the full explanation.
    if muted_note != "":
        row.add_child(_build_row_note_label(muted_note, HudStyle.INK_FAINT, row_tooltip))
    # A spacer (not name_label's expand) pushes the −/+ stepper to the right edge, keeping the
    # label + ⚠ adjacent at the left.
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(spacer)
    _add_stepper_controls(row, count, plus_enabled, on_change)
    return row

## The two-line form of a worker-stepper row (see `_build_worker_stepper`'s `status_line`): line 1 =
## the clickable title + spacer + −/+ stepper; line 2 = an indented, smaller secondary status carrying
## the yield/policy text, the status glyph, then the ⚠/overstaff/wasted notes — the SAME per-part
## colors the single-line path uses, just relocated below. Pending tints the TITLE amber (row 1's
## identity) and shows the ◌ glyph on row 2.
func _build_two_line_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, warn: bool, row_tooltip: String, note: String, on_focus_source: Callable, status_key: String, muted_note: String, status_line: String, row_ink: Color, arrival_schedule: PackedFloat32Array) -> VBoxContainer:
    var col := VBoxContainer.new()
    col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    col.add_theme_constant_override("separation", TWO_LINE_STEPPER_SEPARATION)
    # Line 1: title + spacer + stepper. The status glyph is NOT appended to the title here (it lives on
    # line 2); the title keeps its click-to-jump link (or a plain Label for band-wide roles).
    var title_row := HBoxContainer.new()
    title_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    title_row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    title_row.add_child(_build_row_name_label(label_text, row_ink, row_tooltip, on_focus_source))
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    title_row.add_child(spacer)
    _add_stepper_controls(title_row, count, plus_enabled, on_change)
    col.add_child(title_row)
    # Line 2: indented, smaller, wrapping status. A MarginContainer insets it past the icon; an
    # HFlowContainer wraps the parts to the next line rather than widening the panel (its min width is
    # the widest single part, small by construction).
    var status_margin := MarginContainer.new()
    status_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status_margin.add_theme_constant_override("margin_left", int(STATUS_LINE_INDENT))
    var status_flow := HFlowContainer.new()
    status_flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status_flow.add_theme_constant_override("h_separation", STATUS_LINE_SEPARATION)
    if row_tooltip != "":
        status_flow.tooltip_text = row_tooltip
    # The yield + policy glyph the caller composed (INK), then the status glyph (row_ink — WARN with the
    # ◌ when pending, tying it to the amber title), then ⚠ (WARN), the overstaff note (WARN), and the
    # wasted note (INK_FAINT).
    status_flow.add_child(_build_status_part(status_line, HudStyle.INK))
    var status_glyph := FoodIcons.for_status(status_key)
    if status_glyph != "":
        status_flow.add_child(_build_status_part(status_glyph, row_ink))
    if warn:
        status_flow.add_child(_build_status_part(OVERHUNT_FLAG, HudStyle.WARN))
    if note != "":
        status_flow.add_child(_build_status_part(note, HudStyle.WARN))
    if muted_note != "":
        status_flow.add_child(_build_status_part(muted_note, HudStyle.INK_FAINT))
    status_margin.add_child(status_flow)
    col.add_child(status_margin)
    # Line 3 (only when the deliveries are LUMPY): the arrival tick strip, indented onto the same
    # gutter as line 2 so it reads as part of the row's secondary information. It stays INSIDE this
    # row's container, so the panel's section-block layout and the wide/tall packing are untouched.
    if ArrivalStrip.has_gap(arrival_schedule):
        var strip_margin := MarginContainer.new()
        strip_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        strip_margin.add_theme_constant_override("margin_left", int(STATUS_LINE_INDENT))
        var strip := ArrivalStrip.new()
        strip.set_schedule(arrival_schedule, _current_turn)
        strip_margin.add_child(strip)
        col.add_child(strip_margin)
    return col

## The clickable title (or plain Label) shared by both stepper forms. `on_focus_source` (when valid)
## makes it an inline link that jumps the map to the source; a band-wide role passes nothing.
func _build_row_name_label(text: String, ink: Color, row_tooltip: String, on_focus_source: Callable) -> Control:
    if on_focus_source.is_valid():
        var link := Button.new()
        link.text = text
        link.alignment = HORIZONTAL_ALIGNMENT_LEFT
        HudStyle.apply_link_button(link, ink)
        link.tooltip_text = (row_tooltip + TOOLTIP_LINE_SEPARATOR if row_tooltip != "" else "") + SOURCE_ROW_FOCUS_HINT
        link.pressed.connect(func() -> void: on_focus_source.call())
        return link
    var plain := Label.new()
    plain.text = text
    plain.add_theme_color_override("font_color", ink)
    _set_label_tooltip(plain, row_tooltip)
    return plain

## A single-line note Label (⚠ / overstaff / wasted) for the one-line stepper form.
func _build_row_note_label(text: String, color: Color, row_tooltip: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    _set_label_tooltip(label, row_tooltip)
    return label

## A secondary status part (line 2 of the two-line form): rendered a touch smaller
## (`ALLOC_SECTION_FONT_SIZE`) than the title.
func _build_status_part(text: String, color: Color) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    label.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    return label

## The shared −/+ stepper controls (minus, centered count, plus) appended to a row's HBox, so the
## one-line and two-line forms compose the same stepper. `on_change` fires with the new count.
func _add_stepper_controls(row: HBoxContainer, count: int, plus_enabled: bool, on_change: Callable, compact: bool = false) -> void:
    var minus := Button.new()
    minus.text = "−"
    minus.custom_minimum_size = Vector2(WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = count <= 0
    minus.pressed.connect(func() -> void: on_change.call(count - WORKER_STEP))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(count)
    value.custom_minimum_size = Vector2(WORKER_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if count > 0 else HudStyle.INK_FAINT)
    row.add_child(value)
    var plus := Button.new()
    plus.text = "+"
    plus.custom_minimum_size = Vector2(WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = not plus_enabled
    plus.pressed.connect(func() -> void: on_change.call(count + WORKER_STEP))
    row.add_child(plus)
    if compact:
        for control in [minus, value, plus]:
            _compact_control(control, WORK_STEPPER_FONT_SIZE, WORK_STEPPER_PADDING_V)

## Squeeze a control into a zone's fixed-height chrome row: smaller type, and a button's stylebox
## chrome trimmed vertically. `HudStyle._button_stylebox` pads 9px top and bottom, which alone makes a
## plain Button ~40px tall — taller than `WORK_ROW_HEIGHT`, `ZONE_HEAD_HEIGHT`, `WORK_CHIPS_HEIGHT`
## and `WORK_PAGER_HEIGHT` put together. Every one of those consts is a height the board's capacity
## maths SUBTRACTS, so a control that renders taller pushes the page off the bottom of the zone.
func _compact_control(control: Control, font_size: int, padding_v: int) -> void:
    control.add_theme_font_size_override("font_size", font_size)
    if not (control is Button):
        return
    for state in ["normal", "hover", "pressed", "disabled", "focus"]:
        var box: StyleBox = control.get_theme_stylebox(state)
        if box == null:
            continue
        var squeezed: StyleBox = box.duplicate()
        squeezed.content_margin_top = padding_v
        squeezed.content_margin_bottom = padding_v
        control.add_theme_stylebox_override(state, squeezed)

## The band allocation panel: Working/Idle header, one −/+ row per staffed Forage/Hunt
## source, the always-present Scout + Warrior band-wide role rows, and Move / Clear-all.
## Each source/role row re-sends assign_labor with the new count (0 removes).
## A dim uppercase section header inside the allocation panel ("Current actions" / "Band roles").
func _alloc_section_label(text: String) -> Label:
    var label := Label.new()
    label.text = text.to_upper()
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    return label

## A dim wrapping hint line (role explanation / empty-state prompt).
func _alloc_hint_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    return label

## A signed, fixed-decimal food-rate string ("+0.31" / "-0.30"). Actual yields are ≥0, but the
## formatter is sign-aware so it also renders Net (which can go negative) and Consumption (shown
## as a negative cost).
func _format_signed(value: float) -> String:
    var sign_str := "+" if value >= 0.0 else "-"
    return sign_str + _format_magnitude(value)

## The bare magnitude of a food rate ("1.74"), for a readout that supplies its own sign in words
## ("− 1.74 feed"). One rounding rule for every food rate the HUD prints.
func _format_magnitude(value: float) -> String:
    return String.num(absf(value), YIELD_DECIMALS).pad_decimals(YIELD_DECIMALS)

## The same rate with the "/turn" suffix, for the per-source row headline ("+0.31 /turn").
func _format_yield(value: float) -> String:
    return _format_signed(value) + YIELD_PER_TURN_SUFFIX

## Resolve a worked source's row readout. Two INDEPENDENT signals ride the same row:
##   • overdraw (`warn` → the ⚠ flag) — ecological: the take exceeds the renewable ceiling.
##   • overstaffed (`note` → "· only N of M working") — labor: the source's take was capped below
##     what the assigned workers could produce, so the surplus workers idled HERE and should be
##     reassigned. True for ALL policies (every source has a ceiling), and orthogonal to overdraw —
##     a source can be overstaffed while perfectly sustainable, or overdrawn while fully used.
## Parts are empty when the source carries no confirmed data (pending assign), so
## the row degrades to bare rather than asserting a wrong state.
func _source_yield_readout(m: Dictionary, kind: String) -> Dictionary:
    var label_suffix := ""
    var warn := false
    var tooltip := ""
    # The honest per-turn rate the row headlines (and the caller derives the kill-rhythm from).
    var rate := 0.0
    if bool(m.get("has_yield", false)):
        var actual := float(m.get("actual_yield", 0.0))
        var sustainable := float(m.get("sustainable_yield", 0.0))
        # A source overdraws when its take draws the stock below what it sustains. This is the
        # sim-answered `overdraws` flag (policy-driven: `!managed && policy.overdraws()`), NOT the
        # client-derived `actual > sustainable` — which false-positives on a hunt's kill turn (cashing a
        # banked whole animal spikes `actual` above the steady `sustainable` even under Sustain). Forage
        # on Sustain reads clean; a Surplus/Market/Eradicate patch or an over-hunted herd trips ⚠.
        warn = bool(m.get("overdraws", false))
        var renewable := kind == LABOR_KIND_FORAGE and not warn
        tooltip = "Actual %s" % _format_yield(actual)
        if renewable:
            tooltip += YIELD_TOOLTIP_RENEWABLE
        else:
            tooltip += " · Sustainable %s" % _format_yield(sustainable)
            if warn:
                tooltip += YIELD_TOOLTIP_OVERDRAW
        # HEADLINE the row with the STEADY realized average, never the lumpy pulse. `realized_yield` is
        # the honest long-run average of this source's `actual_yield`, so BOTH hunt and forage read it:
        # forage's realized ≈ its old `actual` (no visible change), while hunt switches off the
        # `sustainable` ceiling to the true realized average — which is what makes the row (and the
        # Food-line income these rows sum into) steady. The pulse's overdraw is still carried by
        # the ⚠ flag + tooltip. Falls back to the old sustainable/actual split if `realized_yield` is
        # absent (older snapshot).
        if m.has("realized_yield"):
            rate = float(m["realized_yield"])
        else:
            rate = sustainable if kind == LABOR_KIND_HUNT else actual
        label_suffix = " %s" % _format_yield(rate)
    # Overstaffing: fewer workers were needed than are assigned, so the remainder produced nothing
    # here. `workers_needed == 0` means "unknown" (rehydrated) → no note.
    var note := ""
    var workers := int(m.get("workers", 0))
    var needed := int(m.get("workers_needed", 0))
    if needed > 0 and workers > needed:
        note = OVERSTAFF_NOTE_FORMAT % [needed, workers]
        tooltip = OVERSTAFF_TOOLTIP if tooltip == "" \
            else tooltip + TOOLTIP_LINE_SEPARATOR + OVERSTAFF_TOOLTIP
    # UNDERSTAFFING: `wasted_yield` is food the source offered that the crew could not collect — the
    # party is under-crewed for the kill. A muted note (the low-key mirror of the overstaff note); the
    # tooltip spells it out. Below FOOD_FLOW_MIN ⇒ hidden (0 on a rehydrated save).
    var muted_note := ""
    var wasted := float(m.get("wasted_yield", 0.0))
    if wasted >= FOOD_FLOW_MIN:
        muted_note = WASTED_NOTE_FORMAT % _format_magnitude(wasted)
        var wasted_tip := WASTED_TOOLTIP % _format_yield(wasted)
        tooltip = wasted_tip if tooltip == "" else tooltip + TOOLTIP_LINE_SEPARATOR + wasted_tip
    return {
        "label_suffix": label_suffix, "warn": warn, "note": note,
        "muted_note": muted_note, "tooltip": tooltip, "rate": rate,
    }

## PRE-COMMIT FORECAST (the compose-time counterpart to `_source_yield_readout`'s post-hoc note).
## Pull the source's per-worker yield + the take ceiling for `policy` — both food/turn at its
## CURRENT biomass, at output_multiplier 1.0. `src` is a herd dict (bare keys) or a tile_info (the
## patch's fields, `patch_`-prefixed); `known` is false for a dead-season source or an older
## snapshot that carries no forecast fields, in which case callers show no row and apply no cap.
## An INVESTMENT policy additionally carries `payoff` (the tended/corral yield the preparation buys)
## and `investment: true`, so `_forecast_yield_row` can state the deal instead of one number.
## `kind` is the caller-stated SOURCE_KIND_*; `prefix` only spells the scalar keys (the two are
## independent — a forage patch reaches here under either forage prefix).
func _forecast_inputs(src: Dictionary, kind: String, prefix: String, policy: String) -> Dictionary:
    var per_worker := float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0))
    # The DIP ceiling paid while the source is prepared. The two source kinds carry it differently, so
    # branch on the kind the CALLER STATED — the prefix cannot answer this (a herd and a raw wire
    # forage patch share the empty prefix), and neither can the dict's shape:
    #   HERD  → the `hunt_policy_ceilings` LIST is the herd's ONLY wire representation (the old
    #           per-policy `ceilingSustain`/… scalars are deprecated schema slots), so every herd rung
    #           — Sustain/Surplus/Market/Eradicate, Tame, Corral — resolves through it.
    #   FORAGE→ a patch has no such list; its per-policy scalars are its only representation.
    # `_hunt_policy_ceiling` returns HUNT_RATE_UNAVAILABLE (< 0) for a herd with no row, which falls
    # back to Sustain's row exactly as the old scalar lookup did, then clamps to 0. That 0 never
    # manufactures a row: `known` is decided by `per_worker` alone, so a herd with no forecast data
    # still reads "not known" and callers show no row and apply no cap.
    var ceiling := 0.0
    if kind == SOURCE_KIND_HERD:
        ceiling = _hunt_policy_ceiling(src, policy)
        if ceiling < 0.0:
            ceiling = _hunt_policy_ceiling(src, DEFAULT_HUNT_POLICY)
        ceiling = maxf(ceiling, 0.0)
    elif policy in FORECAST_CEILING_KEYS:
        ceiling = float(src.get(prefix + String(FORECAST_CEILING_KEYS[policy]), 0.0))
    else:
        ceiling = float(src.get(prefix + String(FORECAST_CEILING_KEYS[DEFAULT_HUNT_POLICY]), 0.0))
    # Keyed off `policy` (not a Sustain-fallback key) so `tame` — absent from FORECAST_CEILING_KEYS —
    # is still recognized as the investment rung it is. For every other rung `policy` IS its ceiling key,
    # so this is identical to the old `policy_key in …` test.
    var investment: bool = policy in FORECAST_PAYOFF_KEYS
    var payoff := 0.0
    if investment:
        payoff = float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[policy]), 0.0))
    # The rung's RUNNING COST (Corral only — the pen's feed). `feed_rung` says the payoff is a GROSS
    # figure that a per-turn cost is paid out of; `feed` is that cost, and is 0 — i.e. unknown, not
    # free — while the herd is still un-penned (see FORECAST_FEED_KEYS).
    var feed_rung: bool = policy in FORECAST_FEED_KEYS
    var feed := 0.0
    if feed_rung:
        feed = float(src.get(prefix + String(FORECAST_FEED_KEYS[policy]), 0.0))
    # WHOLE-ANIMAL HUNT: a take of whole animals via a kill-credit bank (`food_per_animal` = one animal's
    # yield in food; 0/absent for a forage patch). The peak-turn carry need is quantized to whole bodies
    # (see `_max_useful_workers`), so it must fire ONLY for an extractive hunt of a live herd — never a
    # forage patch (no food_per_animal), an investment rung (Tame/Corral collapse to 1), or a corralled
    # herd (managed `worker_tend` harvest, whose forecast already collapses every ceiling to per_worker).
    var food_per_animal := float(src.get(prefix + "food_per_animal", 0.0))
    var whole_animal: bool = food_per_animal > 0.0 and not investment \
        and not bool(src.get("corralled", false))
    return {
        "per_worker": per_worker,
        "ceiling": ceiling,
        "payoff": payoff,
        "investment": investment,
        "feed_rung": feed_rung,
        "feed": feed,
        "food_per_animal": food_per_animal,
        "whole_animal": whole_animal,
        "known": per_worker >= FORECAST_MIN_PER_WORKER,
    }

## Workers beyond this produce nothing at this source under the selected policy —
## ceil(ceiling / per_worker). MAX_USEFUL_UNBOUNDED when there's no forecast data. A tended patch /
## corralled herd reports every ceiling == per_worker, so this collapses to 1 (policy irrelevant).
func _max_useful_workers(forecast: Dictionary) -> int:
    if not bool(forecast.get("known", false)):
        return MAX_USEFUL_UNBOUNDED
    var per_worker := float(forecast["per_worker"])
    var ceiling := float(forecast["ceiling"])
    # WHOLE-ANIMAL HUNT: the cap is the carriers needed to HAUL the animals that drop on the worst turn,
    # not ceil(smoothed-rate / per_worker). An 80-biomass aurochs drops all at once; one hunter carrying
    # <per_worker> food wastes the rest, so the smoothed rate under-counts. Worst case the kill-credit
    # bank holds just under one body when the turn's rate lands, so floor(ceiling / food_per_animal) + 1
    # whole animals drop, each worth food_per_animal — carry that peak, not the average flow.
    var food_per_animal := float(forecast.get("food_per_animal", 0.0))
    if bool(forecast.get("whole_animal", false)) and food_per_animal > 0.0:
        var animals := floori(ceiling / food_per_animal) + HUNT_PEAK_DROP_BANK_BONUS
        var peak_drop_food := float(animals) * food_per_animal
        return ceili(peak_drop_food / per_worker)
    return int(ceilf(ceiling / per_worker))

## The take `workers` would ACTUALLY produce here: min(workers × per_worker, ceiling), scaled by the
## acting band's output multiplier (the sim exports the forecast at 1.0).
func _expected_yield(forecast: Dictionary, workers: int, band: Dictionary) -> float:
    var raw := minf(float(workers) * float(forecast.get("per_worker", 0.0)),
        float(forecast.get("ceiling", 0.0)))
    return raw * float(band.get("output_multiplier", OUTPUT_FULL))

## Cap the worker stepper at what the source can absorb: min(the band's assignable workers,
## max-useful). Returns `{cap, note}` — `note` is set ONLY when max-useful is the binding cap, so a
## dead `+` button is always explained rather than mysterious (the idle-worker cap explains itself).
func _forecast_worker_cap(forecast: Dictionary, assignable: int, useful_floor: int = 0) -> Dictionary:
    var useful := _max_useful_workers(forecast)
    # A managed herd's maintenance crew raises the usefulness ceiling above what the take/prepare side
    # reports: a Corral rung's prep forecast says "1 worker suffices to prepare", but a growing pen needs
    # `herders_needed` hands EVERY turn to hold its tameness. Fold that floor in (callers pass it via
    # `useful_floor`) so the player can always staff the herders the herd requires. An UNBOUNDED forecast
    # stays unbounded — the floor is a RAISE, never a new cap — and a wild herd passes 0, so it's a no-op.
    if useful != MAX_USEFUL_UNBOUNDED:
        useful = maxi(useful, useful_floor)
    if useful == MAX_USEFUL_UNBOUNDED or useful >= assignable:
        # Labor-bound below the usefulness ceiling: the `+` capped at idle workers, not at
        # usefulness — name the reason so the cap doesn't read as a silent bug. Exactly staffed
        # (useful == assignable) and no-forecast (UNBOUNDED) stay noteless.
        var labor_note := ""
        if useful != MAX_USEFUL_UNBOUNDED and useful > assignable:
            labor_note = LABOR_BOUND_NOTE_FORMAT % [assignable, useful]
        return {"cap": assignable, "note": labor_note}
    var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## The live INVESTMENT-rung forecast row on the assign controls — it states the DEAL: "Preparing:
## +0.09 /turn → then +1.20 /turn", so the up-front dip AND the payoff are visible BEFORE the player
## commits. Both halves scaled by the acting band's output multiplier. This row is INVESTMENT-only now:
## every extractive rung (hunt AND forage) renders its own bare-rate + verdict preview
## (`_local_hunt_preview_bbcode` / `_local_forage_preview_bbcode`) instead, so the old non-investment
## "Expected yield:" branch was unreachable and was removed. Callers gate on the investment rung.
##
## The Corral payoff is GROSS (the pen's feed is a separate debit on the keeper's larder), so its row
## never shows the payoff bare — it subtracts the herd's own exported `pen_upkeep` (which the sim now
## projects for an un-penned herd too, on the same biomass basis). The feed is NEVER folded away, and
## a **zero payoff is rendered, loudly** (see INVESTMENT_FORECAST_DEPLETED_NOTE) — a depleted herd
## below the escapement point pays nothing, and that is the row's most important reading.
func _forecast_yield_row(forecast: Dictionary, workers: int, band: Dictionary) -> Label:
    var row := Label.new()
    var expected := _format_yield(_expected_yield(forecast, workers, band))
    var hex := HudStyle.HEALTHY
    var output := float(band.get("output_multiplier", OUTPUT_FULL))
    var payoff := float(forecast.get("payoff", 0.0)) * output
    var feed := float(forecast.get("feed", 0.0)) * output
    var has_feed := bool(forecast.get("feed_rung", false)) and feed >= FOOD_FLOW_MIN
    if has_feed:
        row.text = INVESTMENT_FORECAST_FEED_FORMAT % [
            expected, _format_yield(payoff), _format_magnitude(feed)]
    else:
        row.text = INVESTMENT_FORECAST_FORMAT % [expected, _format_yield(payoff)]
    # A prepared source that pays NOTHING is a trap, and one that pays nothing while EATING every
    # turn is a net loss. Say so — amber, in words, without hiding the zeros that prove it.
    if has_feed and payoff < FOOD_FLOW_MIN:
        row.text += "\n%s" % INVESTMENT_FORECAST_DEPLETED_NOTE
        hex = HudStyle.WARN
    row.add_theme_color_override("font_color", hex)
    return row

## THE overdraw test: a take above the source's renewable-sustainable ceiling (by more than the
## epsilon) draws the source down. One definition, shared by the confirmed allocation rows
## (`_source_yield_readout`) and the local hunt's pre-assign yield preview.
func _is_overdraw(actual: float, sustainable: float) -> bool:
    return actual > sustainable + OVERHUNT_EPSILON

## Net per-turn food flow: income − what the PEOPLE eat − what the band's penned ANIMALS eat.
## Positive → the larder is growing. `pen_feed_upkeep` is the sim's own answer for the third term
## (`PopulationCohortState.penFeedUpkeep` — the food this band actually PAID for pen feed this turn,
## summed across every pen it keeps); the client must NOT re-derive it by summing the herds'
## `pen_upkeep`, and the identity `larder_delta == income − consumption − pen_feed` is pinned sim-side
## (`integration_tests/tests/pen_food_ledger.rs`). Omitting the term made this row LIE: a band with a
## Red Deer pen showed a surplus overstated by the ~1.74/turn its herd ate, then drained anyway.
func _band_net_food(band: Dictionary) -> float:
    return _band_food_income(band) \
        - float(band.get("food_consumption", 0.0)) \
        - _band_pen_feed(band)

## The STEADY total food income = Gathered + Hunted (Σ per-source realized average across the band's
## forage + hunt assignments). Summed from the SAME per-source realized values as the breakdown rows, so
## it equals Gathered + Hunted exactly — the honest long-run average of the lumpy per-turn take, so it
## does NOT swing. It feeds the headline net (`_band_net_food` = income − Eaten − Pen feed) and the
## `_food_is_concerning` gate. **Deliberately summed from the rows rather than read off a band-level
## wire field** — a separately-computed total could drift from the Gathered/Hunted rows it sits above,
## and this way the headline equals them by construction. (A cohort-level `foodIncomeAverage` existed
## for one commit and was retired as redundant; do not reintroduce it.)
func _band_food_income(band: Dictionary) -> float:
    return _sum_realized_yield(band, LABOR_KIND_FORAGE) \
        + _sum_realized_yield(band, LABOR_KIND_HUNT)

## What this band paid to feed its pens this turn (food/turn). 0 for a band that keeps no corral.
func _band_pen_feed(band: Dictionary) -> float:
    return float(band.get("pen_feed_upkeep", 0.0))

## True when the band carries a meaningful food flow (income, consumption, or pen feed above the
## floor) — so a decode miss reads as "no flow" (net readout + breakdown omitted,
## not zeroed).
##
## **The income term MUST be the same `_band_food_income` the headline sums, not the wire's lumpy
## `food_income`.** They diverged once and it hid the readout exactly when it was needed: a starving
## band has `food_consumption` 0 (an empty larder debits nothing) and a whole-animal hunt pays 0 on a
## wait turn, so a band with a perfectly good STEADY income failed all three tests and lost its net
## line and breakdown entirely. Gate on the same number you display.
func _band_has_food_flow(band: Dictionary) -> bool:
    return _band_food_income(band) >= FOOD_FLOW_MIN \
        or float(band.get("food_consumption", 0.0)) >= FOOD_FLOW_MIN \
        or _band_pen_feed(band) >= FOOD_FLOW_MIN

## Sum of per-source `realized_yield` (the STEADY per-source average, food/turn) across this band's
## labor assignments of one kind — the category total behind the Food breakdown (Gathered = forage,
## Hunted = hunt). Reads the steady average (not the lumpy `actual_yield`) so the rows don't swing AND
## sum to the steady headline income (`_band_food_income`); falls back to `actual_yield` if absent.
func _sum_realized_yield(band: Dictionary, kind: String) -> float:
    var total := 0.0
    for a in _labor_assignments_of(band):
        if a is Dictionary and String((a as Dictionary).get("kind", "")).strip_edges().to_lower() == kind:
            var d := a as Dictionary
            total += float(d["realized_yield"]) if d.has("realized_yield") else float(d.get("actual_yield", 0.0))
    return total

## Food is "concerning" when the larder is net-draining OR the runway is below the warn threshold —
## mirroring `_morale_is_concerning`'s below-warn / falling gate. It no longer auto-EXPANDS anything
## (a popover that pops itself open on a snapshot would be worse than the clipping it replaced); it
## marks the row's caret WARN, so a row with something worth reading still says so at a glance.
func _food_is_concerning(band: Dictionary) -> bool:
    var turns := float(band.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    return _band_net_food(band) < 0.0 \
        or (BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.warn_turns())

## Per-row-per-band disclosure key — also the `[url]` meta payload and the popover's identity.
func _breakdown_key(kind: String, band: Dictionary) -> String:
    return "%s:%d" % [kind, int(band.get("entity", -1))]

## Register a summary row (`row_label`, e.g. "Food"/"Morale") as a click-to-open disclosure: stash the
## rows its popover will show and record the caret state for `_format_detail_bbcode`. Returns whether
## the affordance is offered at all — a row with nothing to show gets no caret. Shared by both
## disclosure rows and by BOTH hosts (the panel's vitals label and the Occupants-card drawer), which
## is the point: one click behaviour, no `is_panel` fork.
func _register_disclosure(row_label: String, kind: String, band: Dictionary, lines: Array[String]) -> bool:
    if lines.is_empty():
        return false
    var key := _breakdown_key(kind, band)
    _breakdown_payloads[key] = lines
    var concerning := _food_is_concerning(band) if kind == BREAKDOWN_KIND_FOOD else _morale_is_concerning(band)
    _disclosure_state[row_label] = {"key": key, "open": _breakdown_popover_key == key, "concerning": concerning}
    # A live popover restates the numbers it was opened on, so a snapshot refreshes it in place.
    if _breakdown_popover_key == key:
        _refresh_breakdown_popover_text()
    return true

## The category breakdown sub-lines under Food, one indented row per present category, mirroring the
## morale breakdown: `    ▲ +0.48  Gathered` / `    ▲ +0.46  Hunted` / `    ▼ −0.68  Eaten (people)`
## / `    ▼ −1.74  🐄 Pen feed (animals)` (income ▲ green, debits ▼ amber via the shared
## indented-sub-line tint). Only categories above the floor — a band with no pen shows no feed row.
##
## THREE kinds of row, not two: the pen's feed is a debit on the same larder as the people's meals,
## but it is a DIFFERENT decision (shrink the herd vs starve the band), so it gets its own line.
func _food_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var gathered := _sum_realized_yield(band, LABOR_KIND_FORAGE)
    if gathered >= FOOD_FLOW_MIN:
        lines.append(_food_breakdown_row(gathered, FOOD_LABEL_GATHERED))
    var hunted := _sum_realized_yield(band, LABOR_KIND_HUNT)
    if hunted >= FOOD_FLOW_MIN:
        lines.append(_food_breakdown_row(hunted, FOOD_LABEL_HUNTED))
    var eaten := float(band.get("food_consumption", 0.0))
    if eaten >= FOOD_FLOW_MIN:
        lines.append(_food_breakdown_row(-eaten, FOOD_LABEL_EATEN))
    var pen_feed := _band_pen_feed(band)
    if pen_feed >= FOOD_FLOW_MIN:
        lines.append(_food_breakdown_row(-pen_feed, FOOD_LABEL_PEN_FEED))
    return lines

## One `    ▲ +0.48  Gathered`-style breakdown row (morale-indent + sign glyph → shared tint path).
func _food_breakdown_row(value: float, label: String) -> String:
    var glyph := MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else MORALE_CONTRIB_NEGATIVE_GLYPH
    return "%s%s %s  %s" % [MORALE_BREAKDOWN_INDENT, glyph, _format_signed(value), label]

## Meta dispatcher for the summary-row disclosures (Food/Morale): the `[url]` meta IS the disclosure
## key, so the handler needs no band lookup and no host flag — the SAME click behaviour wherever the
## row renders. `anchor` is the label that emitted the click, bound at wire time; it is what the
## popover positions under.
func _on_detail_meta_clicked(meta: Variant, anchor: Control) -> void:
    var payload := String(meta)
    if not payload.begins_with(BREAKDOWN_TOGGLE_META_PREFIX):
        return
    var key := payload.substr(BREAKDOWN_TOGGLE_META_PREFIX.length())
    if _breakdown_popover_key == key:
        _close_breakdown_popover()
        return
    _open_breakdown_popover(key, anchor)

## Open a disclosure's breakdown in the popover, anchored under the clicked row. The anchor rect is
## captured BEFORE the hosts re-render, because that render frees the very label we are anchoring to
## (the panel builds a fresh vitals label each time).
func _open_breakdown_popover(key: String, anchor: Control) -> void:
    var lines := _breakdown_lines_for(key)
    if lines.is_empty():
        return
    var anchor_rect := _breakdown_anchor_rect(anchor)
    _breakdown_popover_key = key
    _refresh_disclosure_hosts()
    var popover := _ensure_breakdown_popover()
    _refresh_breakdown_popover_text()
    popover.popup(anchor_rect)

## Dismiss the breakdown popover, if any. Idempotent — `popup_hide` runs the same teardown, so a
## click-away / Esc and an explicit close converge on one path.
func _close_breakdown_popover() -> void:
    if _breakdown_popover != null and _breakdown_popover.visible:
        _breakdown_popover.hide()
        return
    _on_breakdown_popover_hidden()

func _on_breakdown_popover_hidden() -> void:
    if _breakdown_popover_key == "":
        return
    _breakdown_popover_key = ""
    _refresh_disclosure_hosts()

## The rows a disclosure key's popover shows — stashed by `_register_disclosure`, never recomputed.
func _breakdown_lines_for(key: String) -> Array[String]:
    var stored: Variant = _breakdown_payloads.get(key, null)
    var lines: Array[String] = []
    if stored is Array:
        for line in (stored as Array):
            lines.append(String(line))
    return lines

## Where the popover sits: a zero-height rect at the bottom-left of the clicked row, in SCREEN space
## (what `Popup.popup` wants). `get_screen_transform` folds in the window position and the canvas
## stretch, both of which this HUD has.
func _breakdown_anchor_rect(anchor: Control) -> Rect2i:
    if anchor == null or not is_instance_valid(anchor):
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + BREAKDOWN_POPOVER_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

## The popover itself: a `PopupPanel`, so it is a WINDOW — it cannot change any zone's height, which
## is the whole reason the breakdown moved here. Styled through `HudStyle` like every other card.
func _ensure_breakdown_popover() -> PopupPanel:
    if _breakdown_popover != null and is_instance_valid(_breakdown_popover):
        return _breakdown_popover
    var popover := PopupPanel.new()
    popover.name = "BreakdownPopover"
    popover.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in ["left", "top", "right", "bottom"]:
        margin.add_theme_constant_override("margin_%s" % side, BREAKDOWN_POPOVER_PADDING)
    popover.add_child(margin)
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD
    label.custom_minimum_size = Vector2(BREAKDOWN_POPOVER_WIDTH, 0.0)
    margin.add_child(label)
    popover.popup_hide.connect(_on_breakdown_popover_hidden)
    add_child(popover)
    _breakdown_popover = popover
    _breakdown_popover_label = label
    return popover

## Restate the open popover from the current payload — the breakdown CONTENT is unchanged from the
## inline form: the same indented ▲/▼ rows through the same shared two-tone tint path.
func _refresh_breakdown_popover_text() -> void:
    if _breakdown_popover_label == null or not is_instance_valid(_breakdown_popover_label):
        return
    _breakdown_popover_label.text = _format_detail_bbcode(_breakdown_lines_for(_breakdown_popover_key))

## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Both
## hosts, unconditionally — that is the `is_panel` fork this change exists to remove.
func _refresh_disclosure_hosts() -> void:
    if _band_city_panel != null and not _panel_band.is_empty():
        _render_band_into_panel(_panel_band)
    _render_subject_drawer()

## The band's larder (provisions) as a float — the starting point of the food-outlook projection and
## the number the Food summary row prints (rounded there).
func _band_provisions(band: Dictionary) -> float:
    var stores_variant: Variant = band.get("stores", {})
    if stores_variant is Dictionary:
        return float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))
    return 0.0

## The band-wide merged arrival schedule: element-wise sum of every source's `arrival_schedule`, so
## slot i is ALL the food landing i+1 turns from now. Length = the longest schedule present (they are
## all `arrivals_horizon_turns` long in practice); empty when no source was projected, which is the
## signal to omit the Food-outlook block entirely rather than draw a flat starving line.
func _merged_arrival_schedule(band: Dictionary) -> PackedFloat32Array:
    var merged := PackedFloat32Array()
    for a in _labor_assignments_of(band):
        if not (a is Dictionary):
            continue
        var schedule := _as_schedule((a as Dictionary).get("arrival_schedule", null))
        if schedule.is_empty():
            continue
        if merged.size() < schedule.size():
            merged.resize(schedule.size())
        for i in range(schedule.size()):
            merged[i] += schedule[i]
    return merged

## "FOOD OUTLOOK" section block: the merged larder projection chart (`FoodOutlookChart`). Returns null
## — the block is omitted — for a non-player band, a band with no real food flow (same gate as the Food
## breakdown), or one whose sources carry no projected schedule. The block is its own section rather
## than a summary line because BBCode cannot host a drawn chart.
func _build_food_outlook_block(band: Dictionary, compact: bool = false) -> VBoxContainer:
    if not (_is_player_unit(band) and _band_has_food_flow(band)):
        return null
    var arrivals := _merged_arrival_schedule(band)
    if arrivals.is_empty():
        return null
    var block := _make_alloc_block()
    block.add_child(_alloc_section_label(ALLOC_HEADER_FOOD_OUTLOOK))
    var chart := FoodOutlookChart.new()
    # Drain = the people's meals plus the pens' feed, held flat across the horizon (see the chart's
    # header): the same two debits the Food breakdown itemizes, so the two readouts cannot disagree.
    chart.set_projection(
        _band_provisions(band), arrivals,
        float(band.get("food_consumption", 0.0)) + _band_pen_feed(band), _current_turn)
    # A short zone gets a COMPACT chart (same series, same empty marker, less height) rather than a
    # clipped full-height one — the zone's height is fixed, so the chart yields, not the layout.
    if compact:
        chart.custom_minimum_size = Vector2(chart.custom_minimum_size.x, FOOD_CHART_COMPACT_HEIGHT)
    block.add_child(chart)
    return block

## A fresh section-block VBox: the discrete, self-contained unit the Band/City panel arranges (a
## vertical stack when tall, a column-flow when wide). Rows are added into it exactly as they used to
## be added into the flat allocation container — only the parent node changes.
func _make_alloc_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", ALLOC_BLOCK_SEPARATION)
    return block

## Stack the three ZONE contents into `target` — the legacy flat host (the Occupants card's
## %AllocationPanel, used by the no-dock `ui_preview` harness). It renders exactly what the dock
## renders, through the SAME three builders; there is no second layout to maintain.
func _build_allocation_panel(band: Dictionary, target: VBoxContainer = null) -> void:
    var container: VBoxContainer = target if target != null else allocation_panel
    if container == null:
        return
    _clear_children(container)
    var is_player := not band.is_empty() and _is_player_unit(band)
    container.visible = is_player
    if not is_player:
        return
    container.add_child(_build_band_zone_content(band, false))
    container.add_child(_build_work_zone_content(band))
    container.add_child(_build_parties_zone_content(band))
    # The docked path offers Move from `_build_band_move_actions`; this host must offer it too, or a
    # selected player band has no way to be moved at all here (see `_make_band_move_actions`).
    container.add_child(_make_band_move_actions())

## Per-SOURCE `+`-gate for a Current-actions Forage/Hunt row: the compose controls cap the stepper at
## max-useful (`_forecast_worker_cap`), and a confirmed row must cap the same way — a source's `+` may
## add a worker only while the band has an idle worker AND this source is below its own max-useful
## ceiling, so a single source can't absorb workers past the point they help. An unknown forecast
## (MAX_USEFUL_UNBOUNDED — no wire data) falls back to the plain `idle > 0` gate. Returns
## `{can_add, note}`; `note` is set ONLY when max-useful (not idle) is what stopped the `+`, so the
## row tooltip explains a dead button rather than leaving it mysterious (the idle-exhausted gate
## explains itself). Scout/Warrior are band-wide roles with no ceiling — they keep the plain gate.
func _source_worker_cap_state(forecast: Dictionary, workers: int, idle: int) -> Dictionary:
    var useful := _max_useful_workers(forecast)
    if useful == MAX_USEFUL_UNBOUNDED or workers < useful:
        return {"can_add": idle > 0, "note": ""}
    # At/over this source's max-useful: the `+` is capped by the source, not by idle. Explain only
    # when idle workers remain (else the idle-exhausted gate already reads for itself).
    var note := ""
    if idle > 0:
        var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
        note = MAX_USEFUL_CAPPED_TOOLTIP % [useful, noun]
    return {"can_add": false, "note": note}

## ============================================================================
## Band/City panel ZONES (docs/band_panel_ux_proposal.html §02/§05)
## ----------------------------------------------------------------------------
## The panel hosts three named zones at a FIXED size (see BandCityPanel): `band`
## (who they are + what they do), `work` (the paged board of worked sources) and
## `parties`. Each builder below returns a bare VBox; `_wrap_zone` anchors it into
## the plain-Control zone host the panel hands out, and the legacy flat host
## (`_build_allocation_panel`, the no-dock ui_preview fallback) simply stacks the
## same three VBoxes — ONE set of builders, never a second layout.
##
## NOTHING here scrolls. Content that can outgrow its box is PAGED against
## `BandCityPanel.work_zone_size()`; a ScrollContainer would reintroduce exactly
## the content-dependent height the panel rework removed.
## ============================================================================

## A zone's content column: the VBox every zone builder fills.
func _make_zone_column() -> VBoxContainer:
    var col := VBoxContainer.new()
    col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    col.size_flags_vertical = Control.SIZE_EXPAND_FILL
    col.add_theme_constant_override("separation", ZONE_SECTION_SEPARATION)
    return col

## Wrap a zone column in the plain `Control` the panel parents into its fixed-size zone host (the host
## reports no minimum size, so the content must anchor itself — see BandCityPanel `_make_zone_host`).
func _wrap_zone(content: VBoxContainer) -> Control:
    var host := Control.new()
    host.mouse_filter = Control.MOUSE_FILTER_IGNORE
    host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    host.size_flags_vertical = Control.SIZE_EXPAND_FILL
    host.add_child(content)
    content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    return host

## Detach-then-free a container's children. `queue_free` alone leaves them parented for the rest of
## the frame, so a rebuild-in-place (the work zone's re-page) would briefly stack old rows under new.
func _clear_children(node: Node) -> void:
    for child in node.get_children():
        node.remove_child(child)
        child.queue_free()

## The interior box a zone's content may fill, in canvas px. The panel answers it from its FIXED
## geometry (`work_zone_size`), so it is a pure function of dock/collapse/window — never of content.
## The fallback keeps the no-dock ui_preview host laying out sensibly.
func _zone_box() -> Vector2:
    if _band_city_panel != null:
        var box: Vector2 = _band_city_panel.work_zone_size()
        if box.x > 0.0 and box.y > 0.0:
            return box
    return ZONE_FALLBACK_SIZE

## A zone section head: an uppercase title on the left, a dim readout on the right, and an optional
## trailing `⋯` menu button. The one head vocabulary all three zones use.
func _zone_head(title: String, readout: String, menu: MenuButton = null, readout_color: Color = HudStyle.INK_DIM, readout_tooltip: String = "") -> HBoxContainer:
    var head := HBoxContainer.new()
    head.custom_minimum_size = Vector2(0.0, ZONE_HEAD_HEIGHT)
    head.add_theme_constant_override("separation", ZONE_HEAD_SEPARATION)
    head.add_child(_alloc_section_label(title))
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    head.add_child(spacer)
    if readout != "":
        var right := Label.new()
        right.text = readout
        right.add_theme_font_size_override("font_size", ZONE_HEAD_FONT_SIZE)
        right.add_theme_color_override("font_color", readout_color)
        _set_label_tooltip(right, readout_tooltip)
        head.add_child(right)
    if menu != null:
        head.add_child(menu)
    return head

## The `⋯` section menu: a `MenuButton`, so its popup is a WINDOW and opening it cannot change any
## zone's layout height (the whole zone model depends on heights not moving). `entries` is an ordered
## array of `{label, disabled, on_pick}` dictionaries.
func _build_section_menu(entries: Array, tooltip: String) -> MenuButton:
    var button := MenuButton.new()
    button.text = SECTION_MENU_GLYPH
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(SECTION_MENU_WIDTH, 0.0)
    HudStyle.apply_button(button, "ghost")
    _compact_control(button, ZONE_HEAD_FONT_SIZE, ZONE_MENU_PADDING_V)
    var popup := button.get_popup()
    var picks: Array[Callable] = []
    for entry_variant in entries:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var index := picks.size()
        popup.add_item(String(entry.get("label", "")), index)
        popup.set_item_disabled(index, bool(entry.get("disabled", false)))
        var pick: Variant = entry.get("on_pick", null)
        picks.append(pick if pick is Callable else Callable())
    popup.id_pressed.connect(func(id: int) -> void:
        if id >= 0 and id < picks.size() and picks[id].is_valid():
            picks[id].call())
    return button

## Ask before a destructive bulk action. A `ConfirmationDialog` is a Window — like the section menu,
## it cannot disturb any zone's height. The body names what is SPARED, so "unassign all" never reads
## as "undo everything".
func _confirm_destructive(body: String, ok_text: String, on_confirm: Callable) -> void:
    var dialog := ConfirmationDialog.new()
    dialog.dialog_text = body
    dialog.ok_button_text = ok_text
    dialog.title = CONFIRM_DIALOG_TITLE
    dialog.confirmed.connect(func() -> void:
        on_confirm.call()
        dialog.queue_free())
    dialog.canceled.connect(func() -> void: dialog.queue_free())
    add_child(dialog)
    dialog.popup_centered()

# ---- shared stacked bar (People + Workforce) --------------------------------

## A proportional stacked bar. `segments` are `{key, count, color, tooltip}`; zero-count segments are
## dropped by the caller. Widths come from `size_flags_stretch_ratio`, so the bar fills its zone at
## any width without any measuring.
func _build_composition_bar(segments: Array) -> HBoxContainer:
    var bar := HBoxContainer.new()
    bar.custom_minimum_size = Vector2(0.0, COMPOSITION_BAR_HEIGHT)
    bar.add_theme_constant_override("separation", COMPOSITION_BAR_SEPARATION)
    for segment_variant in segments:
        var segment: Dictionary = segment_variant
        var cell := ColorRect.new()
        cell.color = segment.get("color", HudStyle.INK_FAINT)
        cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        cell.size_flags_stretch_ratio = maxf(float(segment.get("count", 0)), COMPOSITION_MIN_RATIO)
        cell.custom_minimum_size = Vector2(0.0, COMPOSITION_BAR_HEIGHT)
        cell.tooltip_text = String(segment.get("tooltip", ""))
        cell.mouse_filter = Control.MOUSE_FILTER_STOP
        bar.add_child(cell)
    return bar

## The key under a stacked bar: one `▪ <key> <count>` chip per segment. An `HFlowContainer` so a
## narrow zone wraps the key rather than widening (the zone has a fixed width to respect).
func _build_composition_key(segments: Array, trailing: Control = null) -> HFlowContainer:
    var key := HFlowContainer.new()
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    key.add_theme_constant_override("h_separation", COMPOSITION_KEY_SEPARATION)
    for segment_variant in segments:
        var segment: Dictionary = segment_variant
        var chip := HBoxContainer.new()
        chip.add_theme_constant_override("separation", COMPOSITION_SWATCH_SEPARATION)
        chip.tooltip_text = String(segment.get("tooltip", ""))
        var swatch := ColorRect.new()
        swatch.color = segment.get("color", HudStyle.INK_FAINT)
        swatch.custom_minimum_size = COMPOSITION_SWATCH_SIZE
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(swatch)
        var text := Label.new()
        text.text = "%s %d" % [String(segment.get("key", "")), int(segment.get("count", 0))]
        text.add_theme_font_size_override("font_size", COMPOSITION_KEY_FONT_SIZE)
        text.add_theme_color_override("font_color", HudStyle.INK_DIM)
        text.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(text)
        key.add_child(chip)
    if trailing != null:
        key.add_child(trailing)
    return key

# ---- zone `band` ------------------------------------------------------------

## Zone `band`: vitals · people · food outlook · workforce (+ the two role cards).
## `with_vitals` is false for the legacy flat host, whose Occupants card already renders the very
## same Food/Morale/Position rows in its own `%OccupantDetail` drawer above this.
func _build_band_zone_content(band: Dictionary, with_vitals: bool = true) -> VBoxContainer:
    var col := _make_zone_column()
    _band_zone_tier = _band_zone_tier_for(_zone_box().y)
    if with_vitals:
        col.add_child(_build_vitals_label(band))
    var people := _build_people_block(band)
    if people != null:
        col.add_child(people)
    if _band_zone_tier != BAND_ZONE_TIER_SHORT:
        var outlook := _build_food_outlook_block(band, _band_zone_tier == BAND_ZONE_TIER_COMPACT)
        if outlook != null:
            col.add_child(outlook)
    col.add_child(_build_workforce_block(band, _band_zone_tier == BAND_ZONE_TIER_SHORT))
    return col

## The vitals readout — the Food / Morale / Output rows with their click-to-expand disclosures. A
## FRESH RichTextLabel each render (it resets and re-sets the food/morale tint context), so its
## `meta_clicked` is wired here, bound `is_panel = true`.
func _build_vitals_label(band: Dictionary) -> RichTextLabel:
    var detail_label := RichTextLabel.new()
    detail_label.bbcode_enabled = true
    detail_label.fit_content = true
    detail_label.scroll_active = false
    detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD
    detail_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    detail_label.meta_clicked.connect(_on_detail_meta_clicked.bind(detail_label))
    detail_label.text = _format_detail_bbcode(_unit_summary_lines(band))
    return detail_label

## Round fractional age brackets to whole people SO THEY STILL SUM TO THE WHOLE BAND — the
## largest-remainder method: floor every part, then hand the leftover people out to the biggest
## fractions, biggest first. `round()` per part does NOT preserve the total (9.29 + 16.54 + 4.64 =
## 30.47 rounds to 9 + 17 + 5 = 31), and a Band panel that disagrees with the top bar about how many
## people are in the band reads as a bug in both.
func _apportion_people(parts: Array[float]) -> Array[int]:
    var whole: Array[int] = []
    var assigned := 0
    var total := 0.0
    for part in parts:
        var floored: int = maxi(int(floor(part)), 0)
        whole.append(floored)
        assigned += floored
        total += maxf(part, 0.0)
    var leftover := roundi(total) - assigned
    while leftover > 0:
        var best := -1
        var best_fraction := -1.0
        for i in range(parts.size()):
            var fraction: float = maxf(parts[i], 0.0) - float(whole[i])
            if fraction > best_fraction:
                best_fraction = fraction
                best = i
        if best < 0:
            break
        whole[best] += 1
        leftover -= 1
    return whole

## "PEOPLE" — who the band IS: a stacked children/working-age/elders bar plus its key and the
## dependency ratio. Returns null when the snapshot carries no age structure at all, so the block is
## OMITTED rather than rendered from a fabricated split.
## The palette is deliberately MUTED against the Workforce bar below: the two bars share a shape but
## answer different questions (composition vs allocation) and must not read as the same chart twice.
func _build_people_block(band: Dictionary) -> VBoxContainer:
    # The brackets arrive FRACTIONAL (Scalar) — a real band is 9.29 children, 16.54 workers, 4.64
    # elders — so they are APPORTIONED to whole people rather than rounded one at a time. Rounding
    # each independently is what made this panel read 9 + 17 + 5 = 31 beside a top bar reading 30:
    # the same band, counted twice, disagreeing by a person.
    var raw: Array[float] = [
        float(band.get("age_children", 0.0)),
        float(band.get("age_working", 0.0)),
        float(band.get("age_elders", 0.0)),
    ]
    # `age_working` is the age COHORT; `working_age` is the count of ASSIGNABLE workers (a different
    # quantity that happens to track it). Fall back to the latter when the cohort field is absent.
    if raw[1] <= 0.0:
        raw[1] = float(band.get("working_age", 0))
    var whole := _apportion_people(raw)
    var children: int = whole[0]
    var working: int = whole[1]
    var elders: int = whole[2]
    var total := children + working + elders
    if total <= 0:
        return null
    var segments: Array = []
    if children > 0:
        segments.append({"key": PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT, "tooltip": "%d %s" % [children, PEOPLE_LABEL_CHILDREN]})
    if working > 0:
        segments.append({"key": PEOPLE_GLYPH_WORKING, "count": working,
            "color": HudStyle.INK_DIM, "tooltip": "%d %s" % [working, PEOPLE_LABEL_WORKING]})
    if elders > 0:
        segments.append({"key": PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK, "tooltip": "%d %s" % [elders, PEOPLE_LABEL_ELDERS]})
    var block := _make_zone_block()
    block.add_child(_zone_head(ZONE_HEADER_PEOPLE, str(total)))
    block.add_child(_build_composition_bar(segments))
    block.add_child(_build_composition_key(segments, _build_dependency_chip(children, working, elders)))
    return block

## Give a `Label` a tooltip AND the hover it needs to show one. **`Label` defaults to
## `MOUSE_FILTER_IGNORE`**, so setting `tooltip_text` on one and walking away is a SILENT no-op — the
## text is stored, the mouse never reaches the control, nothing ever appears. Six labels across this
## HUD shipped tooltips that had never once been seen. Route every Label tooltip through here.
func _set_label_tooltip(label: Label, text: String) -> void:
    label.tooltip_text = text
    label.mouse_filter = Control.MOUSE_FILTER_STOP if text != "" else Control.MOUSE_FILTER_IGNORE

## Dependents per 100 working-age adults — the ratio itself, which only the tooltips render now.
func _dependency_per_hundred(dependents: int, working: int) -> int:
    if working <= 0:
        return 0
    return int(round(float(dependents) / float(working) * float(PEOPLE_DEPENDENCY_BASE)))

## What "dependents" MEANS, in the player's terms. The ratio is no longer shown anywhere — it only
## decides the WARN tint — so it stays out of the words too.
func _dependency_tooltip(dependents: int, working: int) -> String:
    var text: String = PEOPLE_DEPENDENCY_TOOLTIP % working
    if _dependency_per_hundred(dependents, working) > PEOPLE_DEPENDENCY_HEAVY:
        text += PEOPLE_DEPENDENCY_HEAVY_TOOLTIP
    return text

## The dependency ratio chip: dependents (children + elders) per 100 working-age adults, WARN-tinted
## once the band carries more mouths than hands. Null when there is no working-age cohort to divide by.
func _build_dependency_chip(children: int, working: int, elders: int) -> Control:
    if working <= 0:
        return null
    var dependents := children + elders
    var per_hundred := _dependency_per_hundred(dependents, working)
    var chip := Label.new()
    chip.text = PEOPLE_DEPENDENCY_FORMAT % dependents
    chip.add_theme_font_size_override("font_size", COMPOSITION_KEY_FONT_SIZE)
    chip.add_theme_color_override("font_color",
        HudStyle.WARN if per_hundred > PEOPLE_DEPENDENCY_HEAVY else HudStyle.INK_FAINT)
    _set_label_tooltip(chip, _dependency_tooltip(dependents, working))
    chip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    chip.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    return chip

## "WORKFORCE" — what the band DOES: a stacked Forage/Hunt/Roles/Parties/Idle bar, its key, and the
## two standing-role CARDS. Saturated against People's muted palette (see `_build_people_block`).
func _build_workforce_block(band: Dictionary, compact_cards: bool) -> VBoxContainer:
    var idle := _effective_idle(band)
    var forage_workers := 0
    var hunt_workers := 0
    var merged := _effective_worker_map(band)
    for key in merged:
        var m: Dictionary = merged[key]
        var workers := int(m.get("workers", 0))
        match String(m.get("kind", "")):
            LABOR_KIND_FORAGE: forage_workers += workers
            LABOR_KIND_HUNT: hunt_workers += workers
    var scout_eff := _effective_role_workers(band, LABOR_KIND_SCOUT)
    var warrior_eff := _effective_role_workers(band, LABOR_KIND_WARRIOR)
    var role_workers := int(scout_eff.get("workers", 0)) + int(warrior_eff.get("workers", 0))
    var party_workers := _band_party_workers(band)
    var segments: Array = []
    for spec in [
        [WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        [WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        [WORKFORCE_KEY_PARTIES, party_workers, HudStyle.WARN],
        [WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var block := _make_zone_block()
    block.add_child(_zone_head(ZONE_HEADER_WORKFORCE,
        WORKFORCE_IDLE_FORMAT % [idle, int(band.get("working_age", 0))],
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM))
    if not segments.is_empty():
        block.add_child(_build_composition_bar(segments))
        block.add_child(_build_composition_key(segments))
    # The two standing roles as CARDS, side by side — a bordered card reads as "a standing role", not
    # as one more worked source in a list (the complaint the card treatment fixes).
    var cards := HBoxContainer.new()
    cards.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    cards.add_theme_constant_override("separation", ROLE_CARD_SEPARATION)
    cards.add_child(_build_role_card(band, ROLE_NAME_SCOUT, SCOUT_ROLE_HINT, LABOR_KIND_SCOUT, scout_eff, idle, compact_cards))
    cards.add_child(_build_role_card(band, ROLE_NAME_WARRIOR, WARRIOR_ROLE_HINT, LABOR_KIND_WARRIOR, warrior_eff, idle, compact_cards))
    block.add_child(cards)
    return block

## One standing-role card: name · one-line hint · the SAME −/+ stepper (same `assign_labor` emit,
## same idle gating) the role rows used to carry.
func _build_role_card(band: Dictionary, role_name: String, hint: String, kind: String, effective: Dictionary, idle: int, compact: bool = false) -> PanelContainer:
    var workers := int(effective.get("workers", 0))
    var pending := bool(effective.get("pending", false))
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    card.add_theme_stylebox_override("panel", _role_card_stylebox())
    # In a short zone the hint moves to the card's tooltip — the words survive, the two lines do not.
    card.tooltip_text = hint
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", ROLE_CARD_SEPARATION)
    card.add_child(col)
    var title := Label.new()
    title.text = role_name
    title.add_theme_font_size_override("font_size", ROLE_CARD_NAME_FONT_SIZE)
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK)
    col.add_child(title)
    if not compact:
        var hint_label := _alloc_hint_label(hint)
        hint_label.custom_minimum_size = Vector2(0.0, ROLE_CARD_HINT_HEIGHT)
        col.add_child(hint_label)
    var stepper := HBoxContainer.new()
    stepper.alignment = BoxContainer.ALIGNMENT_CENTER
    stepper.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    _add_stepper_controls(stepper, workers, idle > 0,
        func(n: int) -> void: _emit_assign_labor(band, kind, n, -1, -1, "", ""))
    col.add_child(stepper)
    return card

func _role_card_stylebox() -> StyleBoxFlat:
    var sb := StyleBoxFlat.new()
    sb.bg_color = HudStyle.GROUND_2
    sb.set_border_width_all(1)
    sb.border_color = HudStyle.LINE
    sb.set_corner_radius_all(ROLE_CARD_CORNER_RADIUS)
    sb.content_margin_left = ROLE_CARD_PADDING
    sb.content_margin_right = ROLE_CARD_PADDING
    sb.content_margin_top = ROLE_CARD_PADDING
    sb.content_margin_bottom = ROLE_CARD_PADDING
    return sb

## A tight sub-block inside a zone (bar + key + cards belong together, closer than the zone's own
## section spacing).
func _make_zone_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", ZONE_BLOCK_SEPARATION)
    return block

# ---- zone `work` (the paged board) ------------------------------------------

## Zone `work`: header · filter chips · the paged board · pager · inspector strip. The column keeps a
## reference to itself so `zones_resized` can RE-PAGE in place rather than re-render the whole panel.
func _build_work_zone_content(band: Dictionary) -> VBoxContainer:
    var col := _make_zone_column()
    col.add_theme_constant_override("separation", ZONE_BLOCK_SEPARATION)
    _work_zone_host = col
    _work_zone_band = band
    _fill_work_zone(col, band)
    return col

## The panel's `zones_resized` handler. Re-paging the work board is the cheap common case, but the
## BAND zone yields by height tier too (chart / role-card hints), so a tier change needs the zones
## rebuilt rather than the board re-paged — otherwise a tall-shell band zone lands in a short box and
## is silently clipped by its host.
func _on_zones_resized() -> void:
    if _band_zone_tier != _band_zone_tier_for(_zone_box().y):
        _rerender_panel_allocation()
        return
    _repage_work_zone()

## Which content tier the band zone's height affords (see `BAND_ZONE_*_MIN_HEIGHT`).
func _band_zone_tier_for(zone_height: float) -> int:
    if zone_height >= BAND_ZONE_TALL_MIN_HEIGHT:
        return BAND_ZONE_TIER_TALL
    if zone_height >= BAND_ZONE_CHART_MIN_HEIGHT:
        return BAND_ZONE_TIER_COMPACT
    return BAND_ZONE_TIER_SHORT

## Re-page the live work board against the panel's new zone box. Only the board is rebuilt — the
## other two zones are untouched.
func _repage_work_zone() -> void:
    if _work_zone_host == null or not is_instance_valid(_work_zone_host) or _work_zone_band.is_empty():
        return
    _clear_children(_work_zone_host)
    _fill_work_zone(_work_zone_host, _work_zone_band)

func _fill_work_zone(col: VBoxContainer, band: Dictionary) -> void:
    var idle := _effective_idle(band)
    var models := _work_source_models(band, idle)
    var income := 0.0
    for m in models:
        income += float((m as Dictionary).get("rate", 0.0))
    col.add_child(_build_work_head(band, models, income))
    # BEFORE the chips are built, so the pressed chip is always one that actually renders.
    _reconcile_work_filter(models)
    col.add_child(_build_work_chips(models))
    var filtered := _filter_work_models(models)
    _sort_work_models(filtered)
    # Drop an inspector pinned to a source that has left the filtered set (unassigned, filtered out).
    var inspected := _find_work_model(filtered, _work_open_key)
    if inspected.is_empty():
        _work_open_key = ""
        _work_policy_open = false
    if filtered.is_empty():
        var hint := _alloc_hint_label(WORK_EMPTY_HINT)
        hint.size_flags_vertical = Control.SIZE_EXPAND_FILL
        col.add_child(hint)
        return
    var capacity := _work_board_capacity(filtered.size(), not inspected.is_empty())
    var page_size := int(capacity["page_size"])
    var pages := int(capacity["pages"])
    _work_page = clampi(_work_page, 0, maxi(pages - 1, 0))
    var start := _work_page * page_size
    col.add_child(_build_work_board(band, filtered.slice(start, start + page_size),
        int(capacity["cols"]), int(capacity["rows_per_col"])))
    if pages > 1:
        col.add_child(_build_work_pager(pages, start, mini(start + page_size, filtered.size()), filtered.size()))
    if not inspected.is_empty():
        col.add_child(_build_work_inspector(band, inspected))

## Board capacity, derived ENTIRELY from the fixed zone box:
##   cols        = zone width / WORK_COLUMN_MIN_WIDTH, clamped to 1..WORK_MAX_COLUMNS
##   rows_per_col = remaining height / WORK_ROW_HEIGHT, after the head, chips, inspector and (when it
##                  is actually needed) the pager — each of which reserves the very height it draws at.
## The pager is circular (it only exists when one page cannot hold everything, but it costs a row), so
## it is resolved in two passes: measure without it, and if that still needs more than one page, remeasure.
func _work_board_capacity(count: int, inspector_open: bool) -> Dictionary:
    var box := _zone_box()
    var cols := clampi(int(box.x / WORK_COLUMN_MIN_WIDTH), 1, WORK_MAX_COLUMNS)
    var inspector_h := 0.0
    if inspector_open:
        inspector_h = WORK_INSPECTOR_POLICY_HEIGHT if _work_policy_open else WORK_INSPECTOR_HEIGHT
    var chrome := ZONE_HEAD_HEIGHT + WORK_CHIPS_HEIGHT + inspector_h \
        + float(ZONE_BLOCK_SEPARATION) * WORK_ZONE_GAP_COUNT
    var rows := maxi(1, int((box.y - chrome) / WORK_ROW_HEIGHT))
    var pages := ceili(float(count) / float(maxi(cols * rows, 1)))
    if pages > 1:
        rows = maxi(1, int((box.y - chrome - WORK_PAGER_HEIGHT - float(ZONE_BLOCK_SEPARATION)) / WORK_ROW_HEIGHT))
        pages = ceili(float(count) / float(maxi(cols * rows, 1)))
    return {"cols": cols, "rows_per_col": rows, "page_size": cols * rows, "pages": maxi(pages, 1)}

## The board itself: `cols` column VBoxes filled COLUMN-MAJOR (top of column 1 to its bottom, then
## column 2), separated by a hairline rule. Fixed-height rows, no scroll — the page IS the limit.
func _build_work_board(band: Dictionary, page: Array, cols: int, rows_per_col: int) -> HBoxContainer:
    var board := HBoxContainer.new()
    board.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    board.size_flags_vertical = Control.SIZE_EXPAND_FILL
    board.add_theme_constant_override("separation", WORK_COLUMN_SEPARATION)
    for c in range(cols):
        if c > 0:
            var rule := ColorRect.new()
            rule.color = HudStyle.LINE_SOFT
            rule.custom_minimum_size = Vector2(WORK_COLUMN_RULE_WIDTH, 0.0)
            rule.size_flags_vertical = Control.SIZE_EXPAND_FILL
            rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
            board.add_child(rule)
        var column := VBoxContainer.new()
        column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.size_flags_vertical = Control.SIZE_FILL
        column.add_theme_constant_override("separation", 0)
        board.add_child(column)
        for r in range(rows_per_col):
            var index := c * rows_per_col + r
            if index >= page.size():
                break
            column.add_child(_build_work_row(band, page[index]))
    return board

## The zone's head row: WORK · n sources · the band's total rate · the `⋯` section menu.
func _build_work_head(band: Dictionary, models: Array, income: float) -> HBoxContainer:
    var menu := _build_section_menu([
        {"label": WORK_MENU_SORT_YIELD, "on_pick": func() -> void: _set_work_sort(WORK_SORT_YIELD)},
        {"label": WORK_MENU_SORT_NAME, "on_pick": func() -> void: _set_work_sort(WORK_SORT_NAME)},
        {"label": WORK_MENU_UNASSIGN_FORMAT % models.size(), "disabled": models.is_empty(),
            "on_pick": func() -> void: _on_work_unassign_all_pressed(band, models.size())},
    ], WORK_MENU_TOOLTIP)
    var head := _zone_head(ZONE_HEADER_WORK, WORK_SOURCES_FORMAT % models.size(), menu)
    # The total sits between the count and the menu, tinted like the Food line's net rate.
    var total := Label.new()
    total.text = _format_yield(income)
    total.add_theme_font_size_override("font_size", ZONE_HEAD_FONT_SIZE)
    total.add_theme_color_override("font_color", HudStyle.HEALTHY if income > 0.0 else HudStyle.INK_DIM)
    _set_label_tooltip(total, WORK_TOTAL_TOOLTIP)
    head.add_child(total)
    head.move_child(total, head.get_child_count() - 2)
    return head

## The filter chips ARE the summary: counts + per-kind rates, and pressing one filters the board.
## **A chip for an EMPTY set never renders** — a kind the band works none of is dead weight in a row
## that is otherwise live summary, and an always-present `⚠ 0` reads as an alarm. `All` always shows
## (it is the reset), so the row is never empty.
func _build_work_chips(models: Array) -> HFlowContainer:
    var chips := HFlowContainer.new()
    chips.custom_minimum_size = Vector2(0.0, WORK_CHIPS_HEIGHT)
    chips.add_theme_constant_override("h_separation", WORK_CHIP_SEPARATION)
    var forage: Array = models.filter(func(m): return String(m["kind"]) == LABOR_KIND_FORAGE)
    var hunt: Array = models.filter(func(m): return String(m["kind"]) == LABOR_KIND_HUNT)
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    chips.add_child(_build_work_chip(WORK_FILTER_ALL, WORK_CHIP_ALL_FORMAT % models.size(), false))
    if not forage.is_empty():
        chips.add_child(_build_work_chip(WORK_FILTER_FORAGE, WORK_CHIP_KIND_FORMAT % [
            FoodIcons.DEFAULT, forage.size(), _format_magnitude(_work_rate_sum(forage))], false))
    if not hunt.is_empty():
        chips.add_child(_build_work_chip(WORK_FILTER_HUNT, WORK_CHIP_KIND_FORMAT % [
            FoodIcons.HUNT, hunt.size(), _format_magnitude(_work_rate_sum(hunt))], false))
    if not attention.is_empty():
        chips.add_child(_build_work_chip(WORK_FILTER_ATTENTION,
            WORK_CHIP_ATTENTION_FORMAT % attention.size(), true))
    return chips

func _work_rate_sum(models: Array) -> float:
    var total := 0.0
    for m in models:
        total += float((m as Dictionary).get("rate", 0.0))
    return total

func _build_work_chip(filter: StringName, text: String, alert: bool) -> Button:
    var active := _work_filter == filter
    var chip := Button.new()
    chip.text = text
    chip.focus_mode = Control.FOCUS_NONE
    HudStyle.apply_button(chip, "primary" if active else "ghost")
    _compact_control(chip, WORK_CHIP_FONT_SIZE, WORK_CHIP_PADDING_V)
    if alert and not active:
        chip.add_theme_color_override("font_color", HudStyle.WARN)
    chip.tooltip_text = WORK_CHIP_TOOLTIP
    chip.pressed.connect(func() -> void: _set_work_filter(filter))
    return chip

## ONE-LINE source row: severity stripe · glyph · label (clipped) · rate · policy/⚠ marks · the
## existing −/+ stepper. Clicking anywhere but the stepper opens the row in the inspector strip.
func _build_work_row(band: Dictionary, model: Dictionary) -> PanelContainer:
    var open := String(model.get("key", "")) == _work_open_key
    var row := PanelContainer.new()
    row.custom_minimum_size = Vector2(0.0, WORK_ROW_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.mouse_filter = Control.MOUSE_FILTER_STOP
    row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    row.tooltip_text = String(model.get("tooltip", ""))
    row.add_theme_stylebox_override("panel", _work_row_stylebox(open))
    row.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
            _toggle_work_inspector(String(model.get("key", ""))))
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", WORK_ROW_SEPARATION)
    row.add_child(line)
    # Severity stripe: WARN when the source is overdrawing or overstaffed, SIGNAL while an edit is
    # still pending, transparent otherwise — so the eye finds trouble without reading a word.
    var stripe := ColorRect.new()
    stripe.custom_minimum_size = Vector2(WORK_ROW_STRIPE_WIDTH, 0.0)
    stripe.size_flags_vertical = Control.SIZE_EXPAND_FILL
    stripe.color = _work_row_stripe_color(model)
    stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(stripe)
    var icon := Label.new()
    icon.text = String(model.get("icon", ""))
    icon.custom_minimum_size = Vector2(WORK_ROW_ICON_WIDTH, 0.0)
    icon.add_theme_font_size_override("font_size", WORK_ROW_FONT_SIZE)
    icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(icon)
    var label := Label.new()
    label.text = String(model.get("label", ""))
    label.clip_text = true
    # A label too long even for the widened column ELLIPSISES rather than hard-cutting: `Hunt Woolly
    # Mamm…` reads as a truncation, `Forage (73, 20` reads as a wrong coordinate.
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("pending", false)) else HudStyle.INK)
    label.add_theme_font_size_override("font_size", WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    var rate := Label.new()
    rate.text = _format_signed(float(model.get("rate", 0.0))) if bool(model.get("has_yield", false)) else ""
    rate.custom_minimum_size = Vector2(WORK_ROW_RATE_WIDTH, 0.0)
    rate.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    rate.add_theme_color_override("font_color", HudStyle.INK_DIM)
    rate.add_theme_font_size_override("font_size", WORK_ROW_FONT_SIZE)
    rate.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(rate)
    var marks := Label.new()
    marks.text = String(model.get("marks", ""))
    marks.custom_minimum_size = Vector2(WORK_ROW_MARKS_WIDTH, 0.0)
    marks.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("warn", false)) else HudStyle.INK_DIM)
    marks.add_theme_font_size_override("font_size", WORK_ROW_FONT_SIZE)
    marks.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(marks)
    _add_stepper_controls(line, int(model.get("workers", 0)), bool(model.get("can_add", false)),
        func(n: int) -> void: _emit_work_assign(band, model, n), true)
    return row

func _work_row_stripe_color(model: Dictionary) -> Color:
    if bool(model.get("warn", false)) or String(model.get("note", "")) != "":
        return HudStyle.WARN
    if bool(model.get("pending", false)):
        return HudStyle.SIGNAL
    return Color(0.0, 0.0, 0.0, 0.0)

func _work_row_stylebox(open: bool) -> StyleBoxFlat:
    var sb := StyleBoxFlat.new()
    sb.bg_color = HudStyle.SIGNAL_WASH if open else Color(0.0, 0.0, 0.0, 0.0)
    sb.content_margin_left = WORK_ROW_PADDING_H
    sb.content_margin_right = WORK_ROW_PADDING_H
    sb.content_margin_top = WORK_ROW_PADDING_V
    sb.content_margin_bottom = WORK_ROW_PADDING_V
    return sb

## The pager, shown only when one page cannot hold the filtered set.
func _build_work_pager(pages: int, start: int, shown_end: int, total: int) -> HBoxContainer:
    var pager := HBoxContainer.new()
    pager.custom_minimum_size = Vector2(0.0, WORK_PAGER_HEIGHT)
    pager.add_theme_constant_override("separation", WORK_ROW_SEPARATION)
    var prev := Button.new()
    prev.text = PAGER_PREV_GLYPH
    prev.focus_mode = Control.FOCUS_NONE
    prev.disabled = _work_page <= 0
    prev.tooltip_text = PAGER_PREV_TOOLTIP
    HudStyle.apply_button(prev, "ghost")
    _compact_control(prev, WORK_CHIP_FONT_SIZE, WORK_PAGER_PADDING_V)
    prev.pressed.connect(func() -> void: _step_work_page(-1))
    pager.add_child(prev)
    var label := Label.new()
    label.text = PAGER_FORMAT % [_work_page + 1, pages]
    label.add_theme_font_size_override("font_size", WORK_CHIP_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    pager.add_child(label)
    var next := Button.new()
    next.text = PAGER_NEXT_GLYPH
    next.focus_mode = Control.FOCUS_NONE
    next.disabled = _work_page >= pages - 1
    next.tooltip_text = PAGER_NEXT_TOOLTIP
    HudStyle.apply_button(next, "ghost")
    _compact_control(next, WORK_CHIP_FONT_SIZE, WORK_PAGER_PADDING_V)
    next.pressed.connect(func() -> void: _step_work_page(1))
    pager.add_child(next)
    var range_label := Label.new()
    range_label.text = PAGER_RANGE_FORMAT % [start + 1, shown_end, total]
    range_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    range_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    range_label.add_theme_font_size_override("font_size", WORK_CHIP_FONT_SIZE)
    range_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    pager.add_child(range_label)
    return pager

## The inspector strip — the row's SECOND and THIRD lines, relocated to one place at the bottom of the
## zone so the board itself stays one line per source. Spells the yield/policy/status out in words,
## carries the warning lines and the arrival strip, and offers the three inline actions.
## `Unassign` lives HERE (not as a hover `✕` on the row) — a destructive control beside the `−`
## stepper would be a mis-click hazard; this is the labelled version.
func _build_work_inspector(band: Dictionary, model: Dictionary) -> PanelContainer:
    var strip := PanelContainer.new()
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.custom_minimum_size = Vector2(0.0,
        WORK_INSPECTOR_POLICY_HEIGHT if _work_policy_open else WORK_INSPECTOR_HEIGHT)
    strip.add_theme_stylebox_override("panel", _work_inspector_stylebox())
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", ZONE_BLOCK_SEPARATION)
    strip.add_child(col)
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", WORK_ROW_SEPARATION)
    var title := Label.new()
    title.text = "%s %s" % [String(model.get("icon", "")), String(model.get("label", ""))]
    title.add_theme_font_size_override("font_size", WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    _compact_control(close, WORK_ROW_FONT_SIZE, INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_work_inspector(String(model.get("key", ""))))
    head.add_child(close)
    col.add_child(head)
    col.add_child(_build_status_part(_work_inspector_sentence(model), HudStyle.INK_DIM))
    if bool(model.get("warn", false)):
        col.add_child(_build_status_part(WORK_INSPECT_OVERDRAW_LINE, HudStyle.WARN))
    if String(model.get("note", "")) != "":
        col.add_child(_build_status_part(String(model.get("note", "")), HudStyle.WARN))
    if String(model.get("muted_note", "")) != "":
        col.add_child(_build_status_part(String(model.get("muted_note", "")), HudStyle.INK_FAINT))
    var schedule: PackedFloat32Array = model.get("schedule", PackedFloat32Array())
    if ArrivalStrip.has_gap(schedule):
        var arrivals := ArrivalStrip.new()
        arrivals.set_schedule(schedule, _current_turn)
        col.add_child(arrivals)
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", COMPOSITION_KEY_SEPARATION)
    links.add_child(_build_inline_link(WORK_INSPECT_JUMP, HudStyle.INK, func() -> void:
        _focus_work_source(model)))
    links.add_child(_build_inline_link(WORK_INSPECT_POLICY, HudStyle.INK, func() -> void:
        _work_policy_open = not _work_policy_open
        _repage_work_zone()))
    links.add_child(_build_inline_link(WORK_INSPECT_UNASSIGN, HudStyle.DANGER, func() -> void:
        _work_open_key = ""
        _work_policy_open = false
        _emit_work_assign(band, model, 0)))
    col.add_child(links)
    if _work_policy_open:
        # The four EXTRACTIVE rungs only. The investment rungs (cultivate/sow/tame/corral) are ladder
        # COMMITMENTS made at the source's own compose control, where their knowledge gates and payoff
        # forecasts live; changing an existing assignment's take needs no gate.
        col.add_child(_build_policy_picker(func(policy: String) -> void:
            _work_policy_open = false
            _emit_work_assign(band, model, int(model.get("workers", 0)), policy),
            String(model.get("policy", "")), LABOR_HUNT_POLICIES, {}, {}, ZONE_POLICY_PICKER_COLUMNS))
    return strip

func _work_inspector_stylebox() -> StyleBoxFlat:
    var sb := StyleBoxFlat.new()
    sb.bg_color = HudStyle.GROUND_2
    sb.set_border_width_all(1)
    sb.border_color = HudStyle.LINE
    sb.set_corner_radius_all(ROLE_CARD_CORNER_RADIUS)
    sb.content_margin_left = ROLE_CARD_PADDING
    sb.content_margin_right = ROLE_CARD_PADDING
    sb.content_margin_top = ROLE_CARD_PADDING
    sb.content_margin_bottom = ROLE_CARD_PADDING
    return sb

## The inspector's one-sentence readout: rate · policy in WORDS · status · assigned workers.
func _work_inspector_sentence(model: Dictionary) -> String:
    var parts: Array[String] = []
    if bool(model.get("has_yield", false)):
        parts.append(_format_yield(float(model.get("rate", 0.0))))
    var policy := String(model.get("policy", ""))
    if policy != "":
        parts.append(policy.capitalize())
    parts.append(_status_label(FoodIcons.STATUS_PENDING if bool(model.get("pending", false)) \
        else FoodIcons.STATUS_WORKING))
    parts.append(WORK_INSPECT_ASSIGNED_FORMAT % int(model.get("workers", 0)))
    return WORK_INSPECT_SENTENCE_SEPARATOR.join(parts)

## An inline text link (the inspector's three actions / the parties footer reasons).
func _build_inline_link(text: String, ink: Color, on_press: Callable) -> Button:
    var link := Button.new()
    link.text = text
    link.focus_mode = Control.FOCUS_NONE
    link.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    HudStyle.apply_link_button(link, ink)
    link.pressed.connect(func() -> void: on_press.call())
    return link

# ---- work-zone models + state ----------------------------------------------

## One dict per worked source, carrying everything the row, the chips and the inspector need — built
## ONCE per render off `_effective_worker_map` (confirmed + optimistic pending), so the board, the
## chip counts and the totals can never disagree.
func _work_source_models(band: Dictionary, idle: int) -> Array:
    var models: Array = []
    var merged := _effective_worker_map(band)
    for key in merged:
        var m: Dictionary = merged[key]
        var kind := String(m.get("kind", "")).strip_edges().to_lower()
        var workers := int(m.get("workers", 0))
        var pending := bool(m.get("pending", false))
        if not (kind == LABOR_KIND_FORAGE or kind == LABOR_KIND_HUNT):
            continue
        if workers <= 0 and not pending:
            continue
        var yld := _source_yield_readout(m, kind)
        var x := int(m.get("x", -1))
        var y := int(m.get("y", -1))
        var herd_id := String(m.get("herd_id", ""))
        var policy := String(m.get("policy", "")).strip_edges().to_lower()
        var icon := ""
        var label := ""
        var cap := {}
        if kind == LABOR_KIND_FORAGE:
            if not (policy in FORAGE_POLICY_OPTIONS):
                policy = DEFAULT_HUNT_POLICY
            # The board draws the glyph in its OWN fixed column, so it takes the RAW icon — not
            # `_source_icon_prefix`, which welds it to the label with a trailing space for the
            # single-label row this replaced.
            icon = _food_module_icon(x, y)
            label = WORK_ROW_FORAGE_FORMAT % [x, y]
            cap = _source_worker_cap_state(_forecast_inputs(
                _forage_patch_lookup.get(Vector2i(x, y), {}), SOURCE_KIND_FORAGE,
                BARE_FORECAST_PREFIX, policy), workers, idle)
        else:
            if not (policy in HUNT_POLICY_OPTIONS):
                policy = _policy_for_hunt(band, herd_id)
            var herd_label := _herd_label_for_id(herd_id)
            icon = FoodIcons.for_herd(herd_label)
            label = WORK_ROW_HUNT_FORMAT % herd_label
            # Herds MIGRATE, so the cap reads the herd's LIVE dict from `_world_herds` rather than the
            # assignment's launch-time target.
            cap = _source_worker_cap_state(_forecast_inputs(
                _find_world_herd(herd_id), SOURCE_KIND_HERD,
                BARE_FORECAST_PREFIX, policy), workers, idle)
        var note := String(yld.get("note", ""))
        var marks := FoodIcons.for_policy(policy)
        if bool(yld.get("warn", false)):
            marks += " " + OVERHUNT_FLAG
        models.append({
            "key": String(key), "kind": kind, "icon": icon, "label": label,
            "rate": float(yld.get("rate", 0.0)), "has_yield": bool(m.get("has_yield", false)),
            "workers": workers, "pending": pending, "warn": bool(yld.get("warn", false)),
            "note": note, "muted_note": String(yld.get("muted_note", "")), "marks": marks,
            "policy": policy, "x": x, "y": y, "herd_id": herd_id,
            "can_add": bool(cap.get("can_add", idle > 0)),
            "schedule": _as_schedule(m.get("arrival_schedule", null)),
            "tooltip": _join_tooltip_lines([String(yld.get("tooltip", "")),
                _policy_hint(kind, policy), String(cap.get("note", "")), WORK_ROW_OPEN_HINT]),
            # A source wants attention when it overdraws, wastes workers, or is still unacknowledged.
            "attention": bool(yld.get("warn", false)) or note != "" or pending,
        })
    return models

## Reset a filter that now selects nothing back to `All`. A kind/attention chip is hidden once its set
## empties (the last herd is unassigned, the last ⚠ clears), so a standing filter would otherwise
## strand the player on an empty board with no chip left to press to get back out of it.
func _reconcile_work_filter(models: Array) -> void:
    if _work_filter == WORK_FILTER_ALL:
        return
    if _work_models_matching(_work_filter, models).is_empty():
        _work_filter = WORK_FILTER_ALL

func _filter_work_models(models: Array) -> Array:
    return _work_models_matching(_work_filter, models)

func _work_models_matching(filter: StringName, models: Array) -> Array:
    match filter:
        WORK_FILTER_FORAGE:
            return models.filter(func(m): return String(m["kind"]) == LABOR_KIND_FORAGE)
        WORK_FILTER_HUNT:
            return models.filter(func(m): return String(m["kind"]) == LABOR_KIND_HUNT)
        WORK_FILTER_ATTENTION:
            return models.filter(func(m): return bool(m["attention"]))
    return models.duplicate()

func _sort_work_models(models: Array) -> void:
    if _work_sort == WORK_SORT_NAME:
        models.sort_custom(func(a, b): return String(a["label"]).naturalnocasecmp_to(String(b["label"])) < 0)
    else:
        models.sort_custom(func(a, b): return float(a["rate"]) > float(b["rate"]))

func _find_work_model(models: Array, key: String) -> Dictionary:
    if key == "":
        return {}
    for m in models:
        if String((m as Dictionary).get("key", "")) == key:
            return m
    return {}

## Re-send this source's `assign_labor` at a new worker count (and optionally a new policy) — the
## same emit the old Current-actions stepper made.
func _emit_work_assign(band: Dictionary, model: Dictionary, workers: int, policy: String = "") -> void:
    var kind := String(model.get("kind", ""))
    _emit_assign_labor(band, kind, workers, int(model.get("x", -1)), int(model.get("y", -1)),
        String(model.get("herd_id", "")),
        policy if policy != "" else String(model.get("policy", "")))

## Jump the map to a worked source — a fixed forage tile, or a herd at its LIVE (migrated) tile.
func _focus_work_source(model: Dictionary) -> void:
    if String(model.get("kind", "")) == LABOR_KIND_HUNT:
        _focus_hunt_source(String(model.get("herd_id", "")), int(model.get("x", -1)), int(model.get("y", -1)))
    else:
        _focus_labor_source(int(model.get("x", -1)), int(model.get("y", -1)))

## One inspector row at a time — opening a second closes the first (and opening one costs the board
## rows, which is why `_work_board_capacity` subtracts the strip's height).
func _toggle_work_inspector(key: String) -> void:
    _work_open_key = "" if _work_open_key == key else key
    _work_policy_open = false
    _repage_work_zone()

func _set_work_filter(filter: StringName) -> void:
    if _work_filter == filter:
        return
    _work_filter = filter
    _work_page = 0
    _repage_work_zone()

func _set_work_sort(sort: StringName) -> void:
    if _work_sort == sort:
        return
    _work_sort = sort
    _work_page = 0
    _repage_work_zone()

func _step_work_page(delta: int) -> void:
    _work_page = maxi(_work_page + delta, 0)
    _repage_work_zone()

## The Work menu's destructive entry. Scoped `work`: Forage + Hunt only — standing roles, parties and
## an in-progress move are untouched, which is exactly what the confirm promises.
func _on_work_unassign_all_pressed(band: Dictionary, count: int) -> void:
    if band.is_empty() or count <= 0:
        return
    _confirm_destructive(WORK_UNASSIGN_CONFIRM_FORMAT % count, WORK_UNASSIGN_CONFIRM_OK,
        func() -> void: _emit_cancel_order(band, CANCEL_SCOPE_WORK))

## Clear labor for a band at `scope` (`all` / `work` / `roles`). Main formats the
## `cancel_order <faction> <band> <scope>` command.
func _emit_cancel_order(band: Dictionary, scope: String) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band, scope)

# ---- zone `parties` ---------------------------------------------------------

## Zone `parties`: head + `⋯` menu · one row per party in the field · the compose footer.
func _build_parties_zone_content(band: Dictionary) -> VBoxContainer:
    var col := _make_zone_column()
    col.add_theme_constant_override("separation", ZONE_BLOCK_SEPARATION)
    var parties := _band_parties(band)
    var menu := _build_section_menu([
        {"label": PARTY_RECALL_ALL_FORMAT % parties.size(), "disabled": parties.is_empty(),
            "on_pick": func() -> void: _on_recall_all_parties_pressed(parties)},
    ], PARTY_MENU_TOOLTIP)
    col.add_child(_zone_head(ZONE_HEADER_PARTIES,
        PARTIES_HEADER_FORMAT % [parties.size(), _band_party_workers(band)], menu))
    if parties.is_empty():
        col.add_child(_alloc_hint_label(PARTIES_EMPTY_HINT))
    else:
        for exp in parties:
            col.add_child(_build_party_row(exp))
    var spacer := Control.new()
    spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    col.add_child(spacer)
    col.add_child(_build_party_footer(band))
    return col

## The player expeditions this band detached (grouped by `home_band_entity`).
func _band_parties(band: Dictionary) -> Array:
    var band_entity := int(band.get("entity", -1))
    var rows: Array = []
    for exp_variant in _player_expeditions:
        if exp_variant is Dictionary and int((exp_variant as Dictionary).get("home_band_entity", 0)) == band_entity:
            rows.append(exp_variant)
    return rows

## Workers currently out with this band's parties — the Workforce bar's Parties segment.
func _band_party_workers(band: Dictionary) -> int:
    var total := 0
    for exp in _band_parties(band):
        total += int((exp as Dictionary).get("size", 0))
    return total

## One party row: mission glyph · subject · phase chip · an always-visible recall `✕` (dimmed at rest,
## bright on hover). Parties carry no stepper and no inspector, so the `✕` is their only removal path.
## Clicking the row BODY keeps the existing behaviour: focus + select the expedition on the map.
func _build_party_row(exp: Dictionary) -> HBoxContainer:
    var phase := _expedition_phase_key(exp)
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORK_ROW_SEPARATION)
    var body := Button.new()
    body.text = _panel_expedition_summary(exp)
    body.alignment = HORIZONTAL_ALIGNMENT_LEFT
    body.focus_mode = Control.FOCUS_NONE
    body.clip_text = true
    body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(body, "ghost")
    if phase == EXPEDITION_PHASE_AWAITING:
        body.add_theme_color_override("font_color", HudStyle.WARN)
    body.tooltip_text = _expedition_row_tooltip(exp, phase)
    var entity := int(exp.get("entity", -1))
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    body.pressed.connect(func() -> void: _on_panel_expedition_selected(entity, x, y))
    row.add_child(body)
    var recall := Button.new()
    recall.text = PARTY_RECALL_GLYPH
    recall.focus_mode = Control.FOCUS_NONE
    recall.tooltip_text = PARTY_RECALL_TOOLTIP
    recall.custom_minimum_size = Vector2(PARTY_RECALL_WIDTH, 0.0)
    HudStyle.apply_button(recall, "ghost")
    recall.modulate.a = PARTY_RECALL_REST_ALPHA
    recall.mouse_entered.connect(func() -> void: recall.modulate.a = 1.0)
    recall.mouse_exited.connect(func() -> void: recall.modulate.a = PARTY_RECALL_REST_ALPHA)
    recall.pressed.connect(func() -> void: _on_recall_expedition_pressed(exp))
    row.add_child(recall)
    return row

## Recall every party in one go — there is no bulk verb on the wire and parties are few, so this is
## one `recall_expedition` per party through the existing signal.
func _on_recall_all_parties_pressed(parties: Array) -> void:
    if parties.is_empty():
        return
    _confirm_destructive(PARTY_RECALL_CONFIRM_FORMAT % parties.size(), PARTY_RECALL_CONFIRM_OK,
        func() -> void:
            for exp in parties:
                _on_recall_expedition_pressed(exp))

## The parties footer: the two missions offered DIRECTLY (Scout / Hunt), each opening the compose
## sheet already on that mission, or the compose sheet in their place. With no idle workers the
## buttons stay VISIBLE and DISABLED with their reason — the section vanishing is what made
## expeditions look like they had been removed from the game.
func _build_party_footer(band: Dictionary) -> VBoxContainer:
    var idle := _effective_idle(band)
    var foot := _make_zone_block()
    if _party_compose_open and _party_compose_mission != "" and idle > 0:
        foot.add_child(_build_compose_sheet(band, idle))
        return foot
    var missions := HBoxContainer.new()
    missions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    missions.add_child(_build_mission_launch_button(COMPOSE_MISSION_SCOUT,
        COMPOSE_MISSION_LABEL_SCOUT, SEND_EXPEDITION_HINT, idle))
    missions.add_child(_build_mission_launch_button(COMPOSE_MISSION_HUNT,
        COMPOSE_MISSION_LABEL_HUNT, SEND_HUNT_EXPEDITION_HINT, idle))
    foot.add_child(missions)
    if idle <= 0:
        foot.add_child(_alloc_hint_label(SEND_PARTY_NO_IDLE_REASON))
    return foot

## One footer mission button: opens the compose sheet already committed to `mission`.
func _build_mission_launch_button(mission: String, label: String, hint: String,
        idle: int) -> Button:
    var btn := Button.new()
    btn.text = label
    btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(btn, "primary")
    btn.tooltip_text = hint
    btn.disabled = idle <= 0
    btn.pressed.connect(func() -> void:
        _party_compose_open = true
        _party_compose_mission = mission
        # A fresh compose act starts with no quarry — never a herd left over from a cancelled one.
        _send_party_quarry_id = ""
        _rerender_panel_allocation())
    return btn

## The compose sheet. The mission is already settled by the footer button that opened it, so the
## sheet titles itself by mission and the policy picker is unreachable except under Hunt (it used to
## sit above the scouting button and read as if it modified it). `✕` is the only way back.
func _build_compose_sheet(band: Dictionary, idle: int) -> VBoxContainer:
    var is_hunt := _party_compose_mission == COMPOSE_MISSION_HUNT
    var sheet := _make_zone_block()
    var head := HBoxContainer.new()
    var title := Label.new()
    title.text = COMPOSE_TITLE_HUNT if is_hunt else COMPOSE_TITLE_SCOUT
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var cancel := Button.new()
    cancel.text = INSPECTOR_CLOSE_GLYPH
    cancel.focus_mode = Control.FOCUS_NONE
    cancel.tooltip_text = COMPOSE_CANCEL_TOOLTIP
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(func() -> void:
        _close_party_compose())
    head.add_child(cancel)
    sheet.add_child(head)
    if is_hunt:
        _fill_hunt_compose_sheet(sheet, band, idle)
        return sheet
    # SCOUT — a single input. Its only question is party size, and nothing about a scouting party
    # depends on where it is going, so the destination is still picked on the map after the send.
    var party_max := _scout_party_max(band, idle)
    _send_expedition_count = clampi(_send_expedition_count, WORKER_STEP, party_max)
    sheet.add_child(_build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, WORKER_STEP, party_max)
            _rerender_panel_allocation()))
    sheet.add_child(_alloc_hint_label(COMPOSE_OF_IDLE_FORMAT % idle))
    sheet.add_child(_alloc_hint_label(SEND_EXPEDITION_HINT))
    var confirm := Button.new()
    confirm.text = SEND_EXPEDITION_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(confirm, "primary")
    confirm.tooltip_text = SEND_EXPEDITION_HINT
    confirm.pressed.connect(func() -> void:
        _close_party_compose()
        _on_send_expedition_pressed(band, _send_expedition_count))
    sheet.add_child(confirm)
    return sheet

## The HUNT form, in the order the decision is actually made: QUARRY → POLICY → PARTY → forecast →
## send. The quarry leads because it is what makes every field under it answerable — the per-policy
## metrics on the picker, the max-useful party cap, the trip forecast and the no-surplus verdict are
## all functions of the herd. Every one of those comes from the SAME helper the herd drawer's
## beyond-reach branch uses, so the two entry points cannot quote different numbers.
func _fill_hunt_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    # Re-resolve the quarry LIVE each render: a herd can be hunted out or leave the snapshot while the
    # sheet is open, and rendering a form against a stale id would forecast a herd that is gone. A herd
    # that MIGRATES into the band's hunt reach fails for the same reason — it is no longer a party's
    # job — so it falls back to the `Choose…` empty state rather than forecasting a raid the player
    # should not make.
    var herd := _find_world_herd(_send_party_quarry_id)
    if herd.is_empty() or not _is_expedition_quarry(band, herd):
        herd = {}
        _send_party_quarry_id = ""
    sheet.add_child(_build_quarry_row(band, herd))
    if _send_party_quarry_id == "":
        # Visible-and-disabled-with-its-reason, the same convention as the idle-0 footer: the send is
        # shown so the shape of the form is legible, and it says why it is not yet pressable.
        sheet.add_child(_alloc_hint_label(COMPOSE_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = SEND_HUNTING_EXPEDITION_BUTTON
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = COMPOSE_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    if not (_send_hunt_policy in LABOR_HUNT_POLICIES):
        _send_hunt_policy = DEFAULT_HUNT_POLICY
    sheet.add_child(_alloc_section_label(COMPOSE_FIELD_POLICY))
    # With a herd in hand the four rungs finally carry their ascending per-policy metric — the same
    # `_expedition_policy_takes` the herd drawer feeds its picker.
    sheet.add_child(_build_policy_picker(func(policy: String) -> void:
        _send_hunt_policy = policy
        # Auto-max on policy select, exactly as the herd drawer does: "give me everything this herd
        # sustains" — zero waste, full rate. Consumed on the next rebuild, never set by a −/+ tick.
        _send_party_autofill = true
        _rerender_panel_allocation(), _send_hunt_policy, LABOR_HUNT_POLICIES,
        {}, _expedition_policy_takes(band, herd), ZONE_POLICY_PICKER_COLUMNS))
    sheet.add_child(_alloc_hint_label(String(SEND_HUNT_POLICY_HINTS.get(_send_hunt_policy, ""))))
    # Party size, capped at the raid's max-useful plateau for THIS herd + policy (the herd drawer's
    # own cap), so extra hunters can no longer be sent to stand idle at the kill.
    var assignable := _scout_party_max(band, idle)
    var capped := _expedition_useful_cap(band, herd, _send_hunt_policy, assignable)
    var cap: int = maxi(int(capped["cap"]), WORKER_STEP)
    if _send_party_autofill:
        _send_expedition_count = cap
        _send_party_autofill = false
    _send_expedition_count = clampi(_send_expedition_count, WORKER_STEP, cap)
    sheet.add_child(_build_party_stepper_row(_send_expedition_count, cap,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, WORKER_STEP, cap)
            _rerender_panel_allocation()))
    sheet.add_child(_alloc_hint_label(COMPOSE_OF_IDLE_FORMAT % idle))
    var cap_note := String(capped["note"])
    if cap_note != "":
        sheet.add_child(_alloc_hint_label(cap_note))
    # LIVE raid forecast for the quarry + policy + party now dialed — the same trip lookup and the
    # same one-line renderer the herd drawer uses.
    var trip := _hunt_trip_forecast(band, herd, _send_hunt_policy, _send_expedition_count)
    var forecast_line := _hunt_forecast_line_bbcode(trip, _herd_display_name(herd))
    if forecast_line != "":
        sheet.add_child(_forecast_label(forecast_line))
    var no_surplus := _hunt_trip_no_surplus(trip)
    var reason := _hunt_no_surplus_reason(herd) if no_surplus else ""
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The button carries the verdict: slow/long/denial raids stay ENABLED and warn-styled, and only a
    # herd with no surplus disables. `_style_send_hunt_button` owns the text in every branch.
    _style_send_hunt_button(confirm, trip, reason)
    if no_surplus:
        sheet.add_child(_alloc_hint_label(reason))
    var quarry_id := _send_party_quarry_id
    confirm.pressed.connect(func() -> void:
        emit_signal("send_hunt_expedition_requested", {
            "faction": int(band.get("faction", PLAYER_FACTION_ID)),
            "band": int(band.get("entity", -1)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": _herd_display_name(herd),
            "policy": _send_hunt_policy,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## The Quarry row — the Party row's shape, with a button instead of a stepper. Unpicked it invites
## (`Choose…`, primary); picked it states the herd and stays available for a re-pick (ghost).
func _build_quarry_row(band: Dictionary, herd: Dictionary) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = COMPOSE_FIELD_QUARRY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    var pick := Button.new()
    pick.focus_mode = Control.FOCUS_NONE
    # EXPAND_FILL is load-bearing on the picked branch: `clip_text` drops the button's minimum width
    # to ~0, so beside an EXPAND_FILL key label it collapses to a sliver. Both branches take it so the
    # row does not resize as a quarry is chosen.
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    if herd.is_empty():
        pick.text = COMPOSE_QUARRY_CHOOSE
        pick.tooltip_text = SEND_HUNT_EXPEDITION_HINT
        HudStyle.apply_button(pick, "primary")
    else:
        var name_text := _herd_display_name(herd)
        pick.text = COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text]
        pick.clip_text = true
        pick.tooltip_text = COMPOSE_QUARRY_TOOLTIP_FORMAT % [
            name_text, int(herd.get("x", -1)), int(herd.get("y", -1)),
        ]
        HudStyle.apply_button(pick, "ghost")
    pick.pressed.connect(func() -> void: _on_pick_quarry_pressed(band))
    row.add_child(pick)
    return row

## The party stepper row, shared by both missions so they cannot drift apart in shape.
func _build_party_stepper_row(count: int, party_max: int, on_change: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = COMPOSE_FIELD_PARTY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    _add_stepper_controls(row, count, count < party_max, on_change)
    return row

## The party size the band can field at all: idle workers, capped by the server's party-size limit.
func _scout_party_max(band: Dictionary, idle: int) -> int:
    var cap := int(band.get("max_expedition_party_size", 0))
    return mini(idle, cap) if cap > 0 else idle

## Leave the compose sheet — every flag together, so `open` / `mission` / `quarry` can never disagree.
## Also disarms any in-flight quarry pick: the ✕ can be pressed while a docked-sheet quarry pick is
## armed (the pick leaves this sheet open, unlike the floating one), so closing must tear down the
## targeting banner + herd glow too, else they persist over no sheet and a later click still fills a
## closed sheet. The call no-ops when no pick is armed.
func _close_party_compose() -> void:
    _party_compose_open = false
    _party_compose_mission = ""
    _send_party_quarry_id = ""
    _cancel_pending_pick_quarry()
    _rerender_panel_allocation()

# ---- badges -----------------------------------------------------------------

## Push the narrow shell's tab badges: Work carries its attention count (hot) or its source count,
## Parties its size (hot while any party is awaiting orders). Band carries none — it is always there.
func _push_zone_badges(band: Dictionary) -> void:
    if _band_city_panel == null:
        return
    var models := _work_source_models(band, _effective_idle(band))
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    _band_city_panel.set_tab_badge(BandCityPanel.ZONE_BAND, "", false)
    _band_city_panel.set_tab_badge(BandCityPanel.ZONE_WORK,
        str(attention.size()) if not attention.is_empty() else str(models.size()),
        not attention.is_empty())
    var parties := _band_parties(band)
    var awaiting := false
    for exp in parties:
        if _expedition_phase_key(exp) == EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _band_city_panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

## The selected PLAYER band's one drawer action (§18): Move. Shares the allocation-panel host with
## `_build_expedition_panel` and `_build_allocation_panel` — all three branches are mutually
## exclusive on the selected occupant, so the fallback path's own Orders Move is never doubled.
##
## Wired straight to `_on_move_band_pressed`, which resolves through `_resolve_assign_band()` and so
## already targets the band selected in THIS list — the whole point on a hex carrying several.
## `Clear all` is deliberately NOT here: it returns every worker to idle, a heavier action that
## belongs beside the labor allocation it clears.
func _build_band_move_actions() -> void:
    if allocation_panel == null:
        return
    for child in allocation_panel.get_children():
        child.queue_free()
    allocation_panel.visible = true
    allocation_panel.add_child(_make_band_move_actions())

## The Move row itself, so the two hosts that offer it build the SAME control rather than two that
## can drift. **Both hosts must offer it**: the docked path adds it beside the panel pointer, and the
## NO-PANEL fallback appends it under the band content — the fallback used to inherit a Move from the
## allocation stack's Orders block, and when the Band panel rework deleted that block the fallback
## silently offered no way to move a band at all. `ui_preview`'s "exactly ONE Move button" assertion
## is what catches either half of that going wrong (none offered, or one offered twice).
func _make_band_move_actions() -> HBoxContainer:
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = MOVE_BAND_BUTTON_TEXT
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = MOVE_BAND_BUTTON_TOOLTIP
    move_btn.pressed.connect(_on_move_band_pressed)
    actions.add_child(move_btn)
    return actions

## The dedicated panel for a selected in-flight expedition (no labor in v1): an awaiting-orders
## callout (echoing the pulsing map ring) plus Move (retarget via move_band on the expedition
## entity) and Recall. Reuses the allocation-panel host; player expeditions only.
func _build_expedition_panel(expedition: Dictionary) -> void:
    if allocation_panel == null:
        return
    for child in allocation_panel.get_children():
        child.queue_free()
    var is_player := not expedition.is_empty() and _is_player_unit(expedition)
    allocation_panel.visible = is_player
    if not is_player:
        return
    var phase := String(expedition.get("expedition_phase", "")).strip_edges().to_lower()
    if phase == EXPEDITION_PHASE_AWAITING:
        var callout := _alloc_hint_label("Reached its objective — Recall it home, or Move it onward.")
        callout.add_theme_color_override("font_color", HudStyle.WARN)
        allocation_panel.add_child(callout)
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = "Send the expedition onward, then click a target tile."
    move_btn.pressed.connect(_on_move_band_pressed)
    actions.add_child(move_btn)
    # Already homeward-bound: the button reads its state ("Returning", disabled) rather than a
    # mysterious grayed-out "Recall". Otherwise it's an enabled "Recall" that folds the party home.
    var returning := phase == EXPEDITION_PHASE_RETURNING
    var recall_btn := Button.new()
    recall_btn.text = "Returning" if returning else "Recall"
    HudStyle.apply_button(recall_btn, "primary")
    recall_btn.tooltip_text = "Heading home — folds workers + provisions back on arrival." if returning \
        else "Order the expedition home (folds workers + provisions back on arrival)."
    recall_btn.disabled = returning
    recall_btn.pressed.connect(_on_recall_expedition_pressed.bind(expedition))
    actions.add_child(recall_btn)
    allocation_panel.add_child(actions)

## Recall the selected in-flight expedition (folds it home). Emits recall_expedition_requested;
## Main formats the `recall_expedition …` command.
func _on_recall_expedition_pressed(expedition: Dictionary) -> void:
    if expedition.is_empty():
        return
    emit_signal("recall_expedition_requested", {
        "faction": int(expedition.get("faction", PLAYER_FACTION_ID)),
        "expedition": int(expedition.get("entity", -1)),
    })

## The Extend-pen affordance on a selected PENNED herd (Grazing 2d-γ). While no ring is in flight
## (`pen_extend_progress == 0`) it offers an "Extend pen" button that issues `extend_pen <faction>
## <x> <y>` at the pen's anchor (a penned herd sits AT `corralled_at`, so the herd's own tile is the
## anchor). While a ring is being worked off (`pen_extend_progress > 0`) the button is replaced by a
## WARN-amber "Fencing N%" badge — the pen twin of the corral-build "Building N%" meter. The server
## rejects an extend at max radius / unowned / Herding-unknown with a feed message; the client does
## not pre-gate on those (max radius is not on the wire).
func _build_extend_pen_control(herd: Dictionary, target: VBoxContainer) -> void:
    var extend_progress := float(herd.get("pen_extend_progress", 0.0))
    if extend_progress > 0.0:
        var badge := Label.new()
        badge.text = PEN_FENCING_LABEL % int(round(extend_progress * PROGRESS_PERCENT_SCALE))
        badge.add_theme_color_override("font_color", HudStyle.WARN)
        target.add_child(badge)
        return
    var x := int(herd.get("x", -1))
    var y := int(herd.get("y", -1))
    if x < 0 or y < 0:
        return
    var extend_btn := Button.new()
    extend_btn.text = PEN_EXTEND_LABEL
    extend_btn.tooltip_text = PEN_EXTEND_TOOLTIP
    HudStyle.apply_button(extend_btn, "ghost")
    extend_btn.pressed.connect(_emit_extend_pen.bind(x, y))
    target.add_child(extend_btn)

## Emit the extend-pen request for the pen anchored at (x, y). Main formats `extend_pen <faction> <x> <y>`.
func _emit_extend_pen(x: int, y: int) -> void:
    emit_signal("extend_pen_requested", {
        "faction": PLAYER_FACTION_ID,
        "x": x,
        "y": y,
    })

## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _herd_compose_available(herd):
        return
    var resolved := _resolve_assign_band()
    var herd_id := String(herd.get("id", ""))
    # When the selected herd changes, default the actor band to the resolved band (and re-seed
    # the compose count/policy from its staffing); otherwise preserve the picked band + count
    # across per-snapshot re-renders of the same herd.
    var source_changed := _hunt_assign_key != herd_id
    if source_changed:
        _hunt_assign_key = herd_id
        _hunt_assign_band = int(resolved.get("entity", -1))
    # The actor is the band-picker selection; fall back to the resolved band if it has vanished.
    var band := _player_band_by_entity(_hunt_assign_band)
    if band.is_empty():
        band = resolved
        _hunt_assign_band = int(band.get("entity", -1))
    if source_changed:
        var staffed := _workers_for_hunt(band, herd_id)
        _hunt_assign_count = staffed if staffed > 0 else WORKER_STEP
        _hunt_assign_policy = _policy_for_hunt(band, herd_id)
    # Show the effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _effective_hunt_workers(band, herd_id)
    var pending := _pending_assigns_for(int(band.get("entity", -1))).has(_pending_key(LABOR_KIND_HUNT, -1, -1, herd_id))
    # The sheet's own header already names the verb ("ASSIGN HERDERS") and the herd, so this line
    # carries only what the header cannot: the standing staffing being edited.
    if current > 0 or pending:
        var title := Label.new()
        title.text = COMPOSE_NOW_STAFFED_FORMAT % [current, COMPOSE_PENDING_SUFFIX if pending else ""]
        title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
        target.add_child(title)
    # Which band supplies the hunters (above the worker/party stepper, so it reads "which band →
    # how many workers"). Switching bands re-runs the distance-aware branch below for that band.
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _hunt_assign_band = int(picked.get("entity", -1))
        _build_herd_assign_controls(herd, target)))
    # Distance-aware: a LOCAL hunt when the herd is within the SELECTED band's hunt_reach, a hunting
    # EXPEDITION when it's beyond. Distance is wrap-aware from the picked band's OWN tile — every part
    # of the decision (distance, reach, and the command's band target) keys off `band` explicitly, so
    # the right band drives it even with multiple bands (single-band playtest can't surface a mixup).
    var herd_x := int(herd.get("x", -1))
    var herd_y := int(herd.get("y", -1))
    var band_tile := _band_tile(band)
    var reach := int(band.get("hunt_reach", 0))
    var distance := _hex_distance_wrapped(band_tile.x, band_tile.y, herd_x, herd_y)
    # Beyond reach → expedition. Unknown distance (missing tiles) falls back to the local hunt.
    var is_expedition := distance >= 0 and distance > reach
    # Local hunt caps at the band's assignable hunt workers; an expedition caps at the party ceiling.
    var assignable := _expedition_party_cap(band) if is_expedition else _assignable_hunt_workers(band, herd_id)
    # Policy options: the Corral INVESTMENT rung is offered on a LOCAL hunt only — a detached party
    # follows the herd and hauls food home; it builds no pen. An expedition keeps the extractive four.
    var hunt_options: Array = LABOR_HUNT_POLICIES if is_expedition else HUNT_POLICY_OPTIONS
    # Grazing 2d-δ + the ladder's rung-2 verb: BOTH husbandry rungs are husbandry-ceiling affordances,
    # and the ceiling says how far up the ladder THIS SPECIES can climb ("wild" hunt-only / "pastoral"
    # tameable-but-never-pennable / "pen" the full ladder). An out-of-ceiling rung is HIDDEN OUTRIGHT,
    # never greyed: greying it would imply a reachable prerequisite, and no amount of knowledge or
    # work will ever let you pen an aurochs whose ceiling is "pastoral". Knowledge = "I know how";
    # ceiling = "this animal allows it" — decoupled (§4.2), so the gates above are orthogonal to this.
    #   • Corral needs a "pen" ceiling.
    #   • Tame needs anything ABOVE "wild" — and is pointless once the herd is fully tamed, so it
    #     retires from the picker at that point (its per-source meter is full; Corral is what's next).
    # `.filter` copies, so the HUNT_POLICY_OPTIONS const is untouched.
    if not is_expedition:
        var ceiling := _husbandry_ceiling(herd)
        if ceiling != HUSBANDRY_CEILING_PEN:
            hunt_options = hunt_options.filter(func(policy: String) -> bool: return policy != LABOR_POLICY_CORRAL)
        if ceiling == HUSBANDRY_CEILING_WILD \
                or float(herd.get("domestication", 0.0)) >= DOMESTICATION_COMPLETE:
            hunt_options = hunt_options.filter(func(policy: String) -> bool: return policy != LABOR_POLICY_TAME)
    var hunt_gates := {} if is_expedition else _hunt_policy_gates(herd)
    # A gated rung can never be the composed policy (the herd may still be taming under a standing
    # Corral selection), so re-validate every render — not just when the selected herd changes.
    if not (_hunt_assign_policy in hunt_options) \
            or not _gate_reasons(hunt_gates, _hunt_assign_policy).is_empty():
        _hunt_assign_policy = DEFAULT_HUNT_POLICY
    # Pre-commit forecast — LOCAL hunt only. An expedition travels for several turns and accumulates
    # toward a carry cap, so the herd's per-turn take ceiling is NOT the bound on its party size;
    # forecasting a per-turn yield for it would be a lie. On a local hunt the ceiling caps the
    # stepper (no over-assigning) and drives the live expected-yield row; both recompute here on
    # every stepper/policy change, since both re-render these controls.
    var forecast := _forecast_inputs(herd, SOURCE_KIND_HERD, BARE_FORECAST_PREFIX, _hunt_assign_policy)
    # ONE yield row per rung — each rung gets the row that actually informs ITS decision:
    #   INVESTMENT (Corral) → `_forecast_yield_row` states the DEAL ("Preparing: +0.23 → then +1.05"):
    #       what you give up, for how long, to get what. That IS the Corral decision, and the local
    #       preview below structurally cannot express it (a dip/payoff pair is not a single rate).
    #       Corral draws sustainably by design, so no overdraw verdict is lost by using this row.
    #   EXTRACTIVE (the four) → `_local_hunt_preview_bbcode` below, which carries the same per-turn
    #       number PLUS the sustainability verdict (`· renewable` / `⚠ overdraws the herd`).
    # Rendering both was the merge's mistake: the two paths are independently computed but agree
    # numerically (verified — the flat `per_worker_yield`/`ceiling_*` scalars and the
    # `hunt_policy_ceilings` list are two views of ONE sim hunt model, both yielding +0.54 on a Market
    # take), so the second row added no information and, worse, argued with the first — a HEALTHY-green
    # "Expected yield" sitting directly above a WARN-amber "⚠ overdraws the herd" for the same number.
    var forecast_active := not is_expedition and bool(forecast["known"]) \
        and bool(forecast["investment"])
    # The party stepper caps at the max-useful count on BOTH branches — a raid's haul (`animals_taken`)
    # PLATEAUS with party size once the herd's surplus binds, so extra hunters past the plateau raid no
    # more animals and should be flagged idle exactly as an over-staffed local hunt is (the silent-idle-
    # hunter gap this pass closes). The local branch caps at the source's max-useful ceiling.
    # A managed (corralling/pastoral) herd needs `herders_needed` hands every turn to hold its tameness,
    # but the take/prepare max-useful ignores that — a Corral rung's prep says "1 worker useful", pinning
    # the player at 1 even when a growing herd needs 2 herders (an unwinnable trap: the corral slips and is
    # lost). Fold the herding crew into the LOCAL-hunt cap's usefulness ceiling so the maintenance crew is
    # always staffable. `herders_needed == 0` on a wild herd, so max(take-useful, 0) is a no-op there. The
    # expedition party has no herding crew, so `_expedition_useful_cap` is left alone.
    var herd_floor := int(herd.get("herders_needed", 0))
    var capped := _expedition_useful_cap(band, herd, _hunt_assign_policy, assignable) if is_expedition \
        else _forecast_worker_cap(forecast, assignable, herd_floor)
    var cap := int(capped["cap"])
    # Auto-max on policy select — "give me everything this herd sustains": the max-useful for the policy
    # (clamped to idle below), which guarantees zero waste + the full rate. Only ever set by a policy
    # click (both branches), never by a −/+ tick, so manual counts survive the rebuild.
    if _hunt_assign_autofill:
        _hunt_assign_count = cap
        _hunt_assign_autofill = false
    _hunt_assign_count = clampi(_hunt_assign_count, 0, cap)
    # A managed herd's local crew are HERDERS/keepers (workersNeeded scales with the herd), not a hunt
    # party — so a pen needing several keepers doesn't read as a hunt-party bug (fix #6).
    var crew_label := HERD_CREW_LABEL if _is_managed_hunt_source(herd, _hunt_assign_policy) \
        else HUNT_CREW_LABEL
    target.add_child(_build_worker_stepper(
        "Party" if is_expedition else crew_label, _hunt_assign_count, _hunt_assign_count < cap,
        func(n: int) -> void:
            _hunt_assign_count = clampi(n, 0, cap)
            _build_herd_assign_controls(herd, target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(_alloc_hint_label(cap_note))
    # Ascending per-policy takes under BOTH pickers so all three (forage / local hunt / expedition) wear
    # the same "up to X/turn" button metric: each policy's MAX obtainable food/turn (Sustain < Surplus <
    # Market < Eradicate). Worker-independent on both branches (the expedition's is the max over party
    # sizes of delivered_food / trip_turns, so it never changes as the Party stepper steps).
    var policy_takes := _expedition_policy_takes(band, herd) if is_expedition \
        else _hunt_policy_takes(herd)
    target.add_child(_build_policy_picker(func(policy: String) -> void:
        _hunt_assign_policy = policy
        # Picking a policy auto-fills the crew to that policy's max-useful (consumed next rebuild).
        _hunt_assign_autofill = true
        _build_herd_assign_controls(herd, target), _hunt_assign_policy, hunt_options, hunt_gates, policy_takes))
    # The policy hint is rendered per BRANCH below, never here: a resident band and a detached party
    # earn DIFFERENT payoffs from the same policy word (the band tames the herd and trades the take;
    # an expedition's Hunting arm credits food only), so one shared hint line under the picker would
    # promise the expedition player a payoff the sim never pays.
    if forecast_active:
        target.add_child(
            _forecast_yield_row(forecast, _hunt_assign_count, band))
    if is_expedition:
        target.add_child(_alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
    var assign_btn := Button.new()
    if is_expedition:
        # LIVE turns-to-fill for the party + policy currently dialed. This block re-renders on every
        # stepper tick and policy click, so the forecast tracks the compose state instead of arriving
        # as a confirmation — and it comes from the SAME helpers the targeting banner uses, so the two
        # entry points can never quote different numbers.
        # `trip`, NOT `forecast`: the outer `forecast` is the LOCAL hunt's per-turn ceiling inputs
        # (client arithmetic over the BAND flow ceiling). This one is the sim's forward-simulated TRIP
        # estimate — a pure table lookup, zero client arithmetic. The two must never be confused.
        var trip := _hunt_trip_forecast(band, herd, _hunt_assign_policy, _hunt_assign_count)
        var forecast_line := _hunt_forecast_line_bbcode(trip, _herd_label_for_id(herd_id))
        if forecast_line != "":
            target.add_child(_forecast_label(forecast_line))
        # The no-surplus refusal — computed ONCE and used for both the button tooltip and the reason
        # line, and identical to what the targeting flow posts to the command feed.
        var no_surplus := _hunt_trip_no_surplus(trip)
        var reason := _hunt_no_surplus_reason(herd) if no_surplus else ""
        _style_send_hunt_button(assign_btn, trip, reason)
        # The reason is spelled out beside the button too — a disabled control's tooltip is easy to miss.
        if no_surplus:
            target.add_child(_alloc_hint_label(reason))
    else:
        # What this policy DOES for a resident band (the forecast line below carries the number; this
        # carries the consequence — above all what Sustain actually teaches, which is otherwise
        # invisible). Deliberately NOT the expedition hints: a party earns neither.
        target.add_child(_alloc_hint_label(
            String(LOCAL_HUNT_POLICY_HINTS.get(_hunt_assign_policy, ""))))
        # Averaging-window disclaimer — the delivered rate above is a long-run average of lumpy
        # whole-animal delivery (you take WHOLE animals, so per-turn delivery varies). ALWAYS shown on
        # an extractive rung (an investment rung shows a dip→payoff, not an animal cadence, so it's
        # skipped), as a STABLE herd-level statement: the span is keyed off the selected policy's flow
        # ceiling (`_hunt_avg_window_turns`), so it never moves as the Hunters count steps up and never
        # blinks out. Skipped only when the window is unknown (missing food_per_animal / ceiling).
        if not (_hunt_assign_policy in INVESTMENT_POLICIES):
            var window_turns := _hunt_avg_window_turns(herd, _hunt_assign_policy)
            if window_turns > 0:
                target.add_child(_alloc_hint_label(
                    HUNT_AVG_WINDOW_FORMAT % window_turns))
        # "Why isn't my Tame progressing?" — the ONE silent rule left on this rung, surfaced rather
        # than left to be guessed. See `_tame_stalled_hint`.
        var stalled := _tame_stalled_hint(herd)
        if stalled != "":
            var stalled_label := _alloc_hint_label(stalled)
            stalled_label.add_theme_color_override("font_color", HudStyle.WARN)
            target.add_child(stalled_label)
        # LIVE per-turn yield for the standing assignment being composed (no carry cap on a local
        # hunt, so turns-to-fill is meaningless — food/turn is the number that decides it).
        # EXTRACTIVE rungs ONLY — an INVESTMENT rung is answered by the dip→payoff row above
        # (`forecast_active`) or by Tame's row, and rendering both put two rows with the same number
        # on the panel. See the ONE-yield-row-per-rung note there. Tested against the named rung set,
        # NOT `forecast["investment"]` (which is really "has a payoff key" and so misses Tame).
        if not (_hunt_assign_policy in INVESTMENT_POLICIES):
            var yield_line := _local_hunt_preview_bbcode(
                band, herd, _hunt_assign_policy, _hunt_assign_count)
            if yield_line != "":
                target.add_child(_forecast_label(yield_line))
        assign_btn.text = ASSIGN_LOCAL_HUNT_BUTTON
        HudStyle.apply_button(assign_btn, "primary")
    if is_expedition:
        # A hunting expedition needs a positive party; a local hunt allows 0 (removes the assignment).
        # `_style_send_hunt_button` already disabled it when the raid returns empty (no surplus); a
        # positive party is the other precondition. (`or` — never clear a disable the style step set.)
        assign_btn.disabled = assign_btn.disabled or _hunt_assign_count <= 0
        assign_btn.pressed.connect(func() -> void:
            if _hunt_assign_count <= 0 or _hunt_trip_no_surplus(
                    _hunt_trip_forecast(band, herd, _hunt_assign_policy, _hunt_assign_count)):
                return
            emit_signal("send_hunt_expedition_requested", {
                "faction": int(band.get("faction", PLAYER_FACTION_ID)),
                "band": int(band.get("entity", -1)),
                "party_workers": _hunt_assign_count,
                "fauna_id": herd_id,
                "fauna_label": _herd_display_name(herd),
                "policy": _hunt_assign_policy if _hunt_assign_policy in LABOR_HUNT_POLICIES else DEFAULT_HUNT_POLICY,
            })
            # Committing is the end of the compose act — return to the read state (§15).
            close_compose_sheet())
    else:
        assign_btn.pressed.connect(func() -> void:
            _emit_assign_labor(band, LABOR_KIND_HUNT, _hunt_assign_count,
                herd_x, herd_y, herd_id, _hunt_assign_policy)
            close_compose_sheet())
    target.add_child(assign_btn)

## Style the hunt-expedition send button from the live forecast. Two treatments, and the line between
## them is the point:
##   DELIVERING (viable / slow / long / denial) — the raid lands something (animals, or the denial it
##     promises). "primary" for a brisk raid; "armed" amber for a slow/long raid (`Send Anyway (≈54
##     turns)` / `Send Anyway (long raid)`) or a denial (`Send (delivers no food)`) — ENABLED either
##     way: the player is told, then trusted.
##   NO SURPLUS (`animals_taken == 0`) — the raid returns empty, a mistake with no upside. DISABLED,
##     with the reason and the way out (party size can't fix it, so the reason names no alternative).
## No confirm dialogs either way.
func _style_send_hunt_button(button: Button, forecast: Dictionary, reason: String) -> void:
    # NO SURPLUS — the one blocked case. Disabled, and it says WHY plus what to do instead (the button is
    # the last thing the player looks at before clicking, so the reason belongs on it). Same words as the
    # panel line and the targeting refusal, from the one helper.
    if _hunt_trip_no_surplus(forecast):
        button.text = SEND_HUNT_NO_SURPLUS_BUTTON
        button.disabled = true
        button.tooltip_text = reason
        HudStyle.apply_button(button, "ghost")
        return
    if bool(forecast.get("denial", false)):
        # Eradicate: no food comes home, but that IS the mission — state the deal, don't cry failure.
        button.text = SEND_HUNT_DENIAL_BUTTON
        HudStyle.apply_button(button, "armed")
        return
    if bool(forecast.get("long_raid", false)):
        button.text = SEND_HUNT_LONG_RAID_BUTTON
        HudStyle.apply_button(button, "armed")
        return
    if bool(forecast.get("slow", false)):
        button.text = SEND_HUNT_ANYWAY_TURNS_FORMAT % int(forecast.get("turns", 0))
        HudStyle.apply_button(button, "armed")
        return
    # A brisk, delivering raid (or no forecast at all — older server): the plain primary send.
    button.text = SEND_HUNTING_EXPEDITION_BUTTON
    HudStyle.apply_button(button, "primary")

## The raid returns empty: the sim's estimate for THIS (policy, party size) says the herd has no surplus
## above the policy's floor (`animals_taken == 0`). The single definition of the blocked case — both
## entry points (panel button + targeting click) gate on it.
func _hunt_trip_no_surplus(forecast: Dictionary) -> bool:
    return bool(forecast.get("available", false)) and bool(forecast.get("empty", false))

## The `hunt_trip_estimates` key the sim exports a (policy, party size) estimate under. One definition —
## the lookup and the plateau scan must agree on the key format or the scan silently finds nothing.
func _hunt_estimate_key(policy: String, workers: int) -> String:
    return "%s%s%d" % [policy, HUNT_ESTIMATE_KEY_SEPARATOR, workers]

## The ONE sentence spoken about a no-surplus raid — shared verbatim by the herd panel (reason line +
## disabled-button tooltip) and the targeting-click command-feed refusal, so the two entry points can
## never disagree. Under the raid model party size cannot fix it (surplus is a property of the HERD, not
## the party), so — unlike the retired row scan — there is no alternative size to name.
func _hunt_no_surplus_reason(herd: Dictionary) -> String:
    return SEND_HUNT_NO_SURPLUS_REASON % _herd_display_name(herd)

## The max-useful party for a raid: `delivered_food` PLATEAUS with party size once the standing surplus
## (not the pack) binds, so beyond the plateau extra hunters raise the payload by nothing. Scan the current
## policy's row for the smallest size at which delivered food stops rising and cap there — the raid twin of
## `_forecast_worker_cap`, and it mirrors its `{cap, note}` shape + "max N useful" note so the expedition
## and local pickers explain a dead `+` the same way. Scans DELIVERED FOOD (not the whole-animal
## `animals_taken`, which sits at 1 across every small-party size on big game — its leading-zeros plateau
## fooled the old scan into capping at 1; with partials delivered food rises smoothly, so the cap tracks
## the true bind). A table SCAN, zero client arithmetic. Returns the full `assignable` (no note) when the
## row carries no data or never plateaus within the band's reach.
func _expedition_useful_cap(band: Dictionary, herd: Dictionary, policy: String, assignable: int) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return {"cap": assignable, "note": ""}
    var estimates := estimates_variant as Dictionary
    # Scan the herd's FULL exported absorption range — every party size the estimate table carries for
    # this policy, NOT the idle/party-limited cap — so `plateau` is the herd's true max-useful party
    # even when it exceeds what we can field right now. The returned cap still clamps to `assignable`
    # below, so this widens ONLY the explanatory note (it lets a labor-bound stepper name the ceiling
    # it's working toward, "N of M useful"), never the cap: within reach the loop breaks exactly as before.
    var scan_cap := 1
    for key in estimates:
        var parts := String(key).split(":")
        if parts.size() == 2 and String(parts[0]) == policy:
            scan_cap = maxi(scan_cap, int(parts[1]))
    var prev_delivered := -1.0
    var plateau := 0
    for workers in range(1, scan_cap + 1):
        var cell_variant: Variant = estimates.get(_hunt_estimate_key(policy, workers), null)
        if not (cell_variant is Dictionary):
            continue
        var delivered := float((cell_variant as Dictionary).get("delivered_food", 0.0))
        if delivered > prev_delivered:
            prev_delivered = delivered
            plateau = workers   # the payload is still rising — this size is useful
        else:
            break               # the payload stopped rising — the previous size is the plateau
    if plateau <= 0:
        return {"cap": assignable, "note": ""}
    var useful: int = mini(plateau, assignable)
    if useful >= assignable:
        # Labor-bound below the plateau: the party capped at what you can field, not at usefulness.
        # `assignable = min(idle, max_party_size)`, so distinguish which constraint binds — freeing
        # idle workers only helps when idle is the binder; if the party-size cap binds, say so.
        var labor_note := ""
        if plateau > assignable:
            var idle := int(band.get("idle_workers", 0))
            var max_party := int(band.get("max_expedition_party_size", 0))
            if max_party > 0 and idle >= max_party:
                labor_note = PARTY_SIZE_BOUND_NOTE_FORMAT % [assignable, plateau]
            else:
                labor_note = LABOR_BOUND_NOTE_FORMAT % [assignable, plateau]
        return {"cap": assignable, "note": labor_note}
    var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## Each extractive policy's MAX obtainable food/turn — the raid twin of the local hunt's per-policy cap,
## so all three pickers (forage / local hunt / expedition) wear the same "up to X/turn" button metric and
## the four read ASCENDING (Sustain < Surplus < Market < Eradicate; deeper floors free more surplus). The
## metric is WORKER-INDEPENDENT: the max over every party size of `delivered_food / trip_turns`, where
## `trip_turns = turns_to_fill + round-trip travel` (a far herd's best rate is correctly lower). A bigger
## party delivers more food in fewer turns, so the rate rises then plateaus — the max is the honest cap.
## Eradicate is a DENIAL rung (`delivers_food == false`, `delivered_food == 0`): it never qualifies, so it
## carries no rate and falls back to its name + skull glyph — its existing denial treatment. A table SCAN,
## zero client arithmetic. Empty when the herd carries no estimates (older snapshot / non-huntable).
func _expedition_policy_takes(band: Dictionary, herd: Dictionary) -> Dictionary:
    var takes := {}
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return takes
    var estimates := estimates_variant as Dictionary
    var travel := _round_trip_travel_turns(band, herd)
    for policy in LABOR_HUNT_POLICIES:
        var best_rate := -1.0
        for key in estimates:
            var parts := String(key).split(HUNT_ESTIMATE_KEY_SEPARATOR)
            if parts.size() != 2 or String(parts[0]) != String(policy):
                continue
            var cell: Dictionary = estimates[key]
            if not bool(cell.get("delivers_food", false)):
                continue
            var delivered := float(cell.get("delivered_food", 0.0))
            var trip_turns := int(cell.get("turns_to_fill", 0)) + travel
            if delivered <= 0.0 or trip_turns <= 0:
                continue
            best_rate = maxf(best_rate, delivered / float(trip_turns))
        if best_rate >= 0.0:
            takes[String(policy)] = _extractive_take(best_rate)
    return takes

## Each extractive policy's per-turn take on this forage patch — the policy ceiling from the shared
## `_forecast_inputs` (food/turn at output 1.0, like the hunt band ceiling), for the FORAGE picker's
## ascending per-policy readout. The plant twin of `_hunt_policy_takes`, so all three pickers wear the
## same "+X /turn" button metric. Empty entries (dead-season patch / older snapshot) are skipped.
func _forage_policy_takes(tile_info: Dictionary) -> Dictionary:
    var takes := {}
    for policy in LABOR_HUNT_POLICIES:
        var forecast := _forecast_inputs(tile_info, SOURCE_KIND_FORAGE, FORAGE_FORECAST_PREFIX, String(policy))
        if not bool(forecast["known"]):
            continue
        takes[String(policy)] = _extractive_take(float(forecast["ceiling"]))
    # The two forage INVESTMENT rungs wear the PAYOFF they build toward, not a per-turn take (the prep
    # dip is lower than Sustain and would make Cultivate look strictly worse than idling). A locked rung
    # may still show its payoff — informative ("this is what it'd give"), and the gate-reason line under
    # the picker already explains the lock. Absent/zero payoff → no entry, so the button stays bare.
    for policy in [LABOR_POLICY_CULTIVATE, LABOR_POLICY_SOW]:
        var forecast := _forecast_inputs(tile_info, SOURCE_KIND_FORAGE, FORAGE_FORECAST_PREFIX, policy)
        if not bool(forecast["known"]) or not bool(forecast["investment"]):
            continue
        var payoff := float(forecast["payoff"])
        if payoff > 0.0:
            takes[policy] = _payoff_take(payoff)
    return takes

## A one-line BBCode readout inside the assign controls (the live hunt-trip forecast / yield preview).
## Sized like the hint lines it sits among, but BBCode-capable so the forecast keeps its state colors.
func _forecast_label(bbcode: String) -> RichTextLabel:
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_font_size_override("normal_font_size", ALLOC_SECTION_FONT_SIZE)
    label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
    label.text = bbcode
    return label

## A 0..1 progress track (knowledge / domestication) as a whole percent. 0 is a MEANINGFUL reading in
## a gate reason — it tells the player they haven't started the track at all.
func _progress_percent(progress: float) -> int:
    return int(round(clampf(progress, 0.0, 1.0) * PROGRESS_PERCENT_SCALE))

## Unmet prerequisites for the FORAGE investment rungs (Cultivate = rung 2, Sow = rung 3), keyed
## policy → Array[String] of reasons (each already carrying its own remedy). Empty when every rung is
## available. Mirrors the sim's `assign_labor` validation.
##
## The two rungs gate on DIFFERENT things, which is the ladder made legible:
##   • Cultivate — Cultivation knowledge + a Thriving patch (you improve what is already there).
##   • Sow — Seed Selection knowledge + ground that will take seed. It needs NO prior patch and no
##     Thriving gate (seed travels, and sown ground starts at the reseed floor — i.e. Collapsing — so
##     a health gate would forbid the very case the rung exists for). What it needs instead is the
##     LAND: `patch_sow_site_refusal` is the sim's verdict on this ground, and it is the only gate
##     reason on either web that the player answers by MOVING rather than by working.
func _forage_policy_gates(tile_info: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(LABOR_POLICY_SUSTAIN)
    var gates := {}
    var cultivate_reasons: Array[String] = []
    var cultivation := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_CULTIVATION)
    if cultivation < KNOWLEDGE_COMPLETE:
        cultivate_reasons.append(GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT % [
            _progress_percent(cultivation), sustain_icon])
    var phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if phase != ECOLOGY_PHASE_THRIVING:
        var phase_label := phase.capitalize() if phase != "" else GATE_PHASE_UNKNOWN_LABEL
        cultivate_reasons.append(GATE_REASON_PATCH_THRIVING_FORMAT % phase_label)
    # A finished patch retires Cultivate outright: the build is DONE (Sustain harvests it, and Sow is the
    # next rung if unlocked). This SUPERSEDES the prep prerequisites — a tended patch's Thriving/knowledge
    # gates are moot — so it replaces the reason list rather than piling on.
    if bool(tile_info.get("is_cultivated", false)):
        cultivate_reasons.clear()
        cultivate_reasons.append(GATE_REASON_ALREADY_TENDED_FORMAT % sustain_icon)
    if not cultivate_reasons.is_empty():
        gates[LABOR_POLICY_CULTIVATE] = cultivate_reasons
    var sow_reasons: Array[String] = []
    var seed_selection := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_SEED_SELECTION)
    if seed_selection < KNOWLEDGE_COMPLETE:
        sow_reasons.append(GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT % [
            _progress_percent(seed_selection), sustain_icon])
    var refusal := _sow_site_refusal_reason(tile_info)
    if refusal != "":
        sow_reasons.append(refusal)
    # A finished Field retires Sow, same as a finished patch retires Cultivate.
    if bool(tile_info.get("patch_is_field", false)):
        sow_reasons.clear()
        sow_reasons.append(GATE_REASON_ALREADY_FIELD_FORMAT % sustain_icon)
    if not sow_reasons.is_empty():
        gates[LABOR_POLICY_SOW] = sow_reasons
    return gates

## WHY this ground will not take seed, in the manual's voice — "" when it will. Reads the sim's
## `patch_sow_site_refusal` verdict; the client never re-derives it (it has neither the per-biome
## capacity table nor the hydrology). An unknown key still refuses: the sim gates the command on the
## same seam, so offering the button anyway would only produce a failure the player cannot read.
func _sow_site_refusal_reason(tile_info: Dictionary) -> String:
    var key := String(tile_info.get("patch_sow_site_refusal", "")).strip_edges()
    if key == "":
        return ""
    return String(SOW_REFUSAL_REASONS.get(key, SOW_REFUSAL_FALLBACK))

## Unmet prerequisites for the HUNT investment rungs (Tame = rung 2, Corral = rung 3), keyed policy →
## Array[String] of reasons. The herd twin of `_forage_policy_gates`.
##
## The §4.3 GATE RESHUFFLE is what this function encodes: ONE knowledge per transition. **Herding
## gates Tame** (it no longer gates Corral, and taming is no longer ungated), and the **new Penning
## gates Corral**. Corral additionally needs THIS herd tamed — the per-source half of the split.
##
## Deliberately NOT gated: the herd being Thriving. Taming a herd whose phase swings under hunting
## would be un-actionable, so the sim just PAUSES the meter instead — see `_tame_stalled_hint`, which
## is how the player is told rather than left to guess.
##
## Known gap (pre-existing): no ownership check — the sim's tracks are per-faction, so a herd tamed by
## ANOTHER faction reads as available here while the sim rejects the assign.
func _hunt_policy_gates(herd: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(LABOR_POLICY_SUSTAIN)
    var gates := {}
    var domestication := float(herd.get("domestication", 0.0))
    var tame_reasons: Array[String] = []
    var herding := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_HERDING)
    if herding < KNOWLEDGE_COMPLETE:
        tame_reasons.append(GATE_REASON_HERDING_KNOWLEDGE_FORMAT % [
            _progress_percent(herding), sustain_icon])
    if not tame_reasons.is_empty():
        gates[LABOR_POLICY_TAME] = tame_reasons
    var corral_reasons: Array[String] = []
    var penning := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_PENNING)
    if penning < KNOWLEDGE_COMPLETE:
        corral_reasons.append(GATE_REASON_PENNING_KNOWLEDGE_FORMAT % [
            _progress_percent(penning), sustain_icon])
    if domestication < DOMESTICATION_COMPLETE:
        corral_reasons.append(GATE_REASON_HERD_DOMESTICATED_FORMAT % [
            _progress_percent(domestication), FoodIcons.for_policy(LABOR_POLICY_TAME)])
    if not corral_reasons.is_empty():
        gates[LABOR_POLICY_CORRAL] = corral_reasons
    return gates

## The one silent rule left on the Tame rung, said out loud. Taming accrues only while the herd is
## **Thriving**, but that is deliberately NOT a gate on selecting Tame (`_hunt_policy_gates`): a
## herd's phase swings as it is hunted, so refusing the verb would be un-actionable churn. The sim
## instead just PAUSES the meter — progress is neither lost nor switched — and resumes when the herd
## recovers.
##
## Saying nothing here would recreate the exact failure this whole arc exists to kill: a hidden rule
## the player can only learn by being told. So whenever Tame is the composed policy on a herd that
## is not Thriving, state the pause, name the cause (its live phase), and name the remedy — which is
## the opposite of "work harder" (ease off and let it recover), the same shape as the patch-ecology
## gate's advice. Returns "" when Tame is not selected or the herd is Thriving (nothing to explain).
func _tame_stalled_hint(herd: Dictionary) -> String:
    if _hunt_assign_policy != LABOR_POLICY_TAME:
        return ""
    var phase := String(herd.get("ecology_phase", "")).strip_edges().to_lower()
    if phase == "" or phase == ECOLOGY_PHASE_THRIVING:
        return ""
    return TAME_STALLED_HINT_FORMAT % phase.capitalize()

## The take-policy radio; `on_pick` fires with the chosen policy. The highlighted option is
## `selected` (defaults to the herd-assign compose policy so existing callers are unchanged; the
## send-hunt-expedition picker passes `_send_hunt_policy`). `options` is the option set for this
## source kind — the four extractive rungs by default, plus that kind's INVESTMENT rung on the
## forage/herd assign controls (FORAGE_POLICY_OPTIONS / HUNT_POLICY_OPTIONS).
##
## `gates` maps a policy → an Array[String] of its unmet-prerequisite reasons (empty / absent =
## available). A gated option is **shown, greyed, and explained** rather than hidden: it is disabled,
## its tooltip carries every reason (one per line), and the reasons render under the row — one
## compact line when there is a single reason, a "<policy> needs:" header + one bullet per reason
## when there are several (each reason now names its remedy, so two on one line would not fit). The
## player discovers the rung, what it costs to unlock, AND how to unlock it, BEFORE trying to use it.
func _build_policy_picker(
    on_pick: Callable,
    selected: String = "",
    options: Array = LABOR_HUNT_POLICIES,
    gates: Dictionary = {},
    takes: Dictionary = {},
    columns: int = 0) -> VBoxContainer:
    var current := selected if selected != "" else _hunt_assign_policy
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    # Wrap the rung buttons 3 per row (a GridContainer) so the six-rung pickers read as two rows of
    # three; a small picker (≤4 rungs, the expedition) stays a single row so it never strands a lone
    # sub-width button. Each button EXPAND_FILLs so the three in a row are equal width and fill the panel.
    var grid := GridContainer.new()
    # `columns > 0` overrides the width-driven default: a zone is a FIXED-width box, and a picker whose
    # buttons sum past it raises the zone content's minimum width, which pushes the whole zone column
    # out past its host (where it is clipped) — taking the section menu beside it off the edge.
    if columns > 0:
        grid.columns = columns
    else:
        grid.columns = maxi(1, options.size()) if options.size() <= POLICY_PICKER_MAX_SINGLE_ROW else POLICY_PICKER_COLUMNS
    grid.add_theme_constant_override("h_separation", WORKER_STEPPER_SEPARATION)
    grid.add_theme_constant_override("v_separation", WORKER_STEPPER_SEPARATION)
    for policy in options:
        var policy_key := String(policy)
        var icon := FoodIcons.for_policy(policy_key)
        var reasons := _gate_reasons(gates, policy_key)
        var btn := Button.new()
        # ONE-LINE FACE: the FoodIcons policy glyph (the same icon the map's yield labels append, so a
        # policy reads identically on the picker and on the worked tile/herd) + the compact per-policy
        # metric, NO name — so the rungs stay compact enough to wrap 3-per-row (see the grid above)
        # without overflow. The name + full metric live in the
        # tooltip. The metrics still read as ASCENDING (Sustain < Surplus < Market < Eradicate). A rung
        # with no metric (older snapshot / metric-less gated rung) falls back to the name so the face
        # is never a lone glyph.
        var take: Variant = takes.get(policy_key, null)
        var compact := String((take as Dictionary).get("compact", "")) if take is Dictionary else ""
        var full := String((take as Dictionary).get("full", "")) if take is Dictionary else ""
        var face := compact if compact != "" else policy_key.capitalize()
        btn.text = "%s%s" % [_source_icon_prefix(icon), face]
        # EXPAND_FILL so the buttons sharing a grid row are equal width and fill the panel content width.
        btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        HudStyle.apply_button(btn, "primary" if policy_key == current else "ghost")
        # Tooltip names the rung for EVERY button (the face no longer does), led by its full metric;
        # a gated button appends its gate reasons below, so a hover tells you what the rung is AND why
        # it is locked.
        var name_line := POLICY_TOOLTIP_NAME_FORMAT % [policy_key.capitalize(), full] \
            if full != "" else policy_key.capitalize()
        var tooltip_lines: Array[String] = [name_line]
        btn.disabled = not reasons.is_empty()
        if btn.disabled:
            tooltip_lines.append_array(reasons)
        else:
            btn.pressed.connect(func() -> void: on_pick.call(policy_key))
        btn.tooltip_text = GATE_REASON_TOOLTIP_SEPARATOR.join(tooltip_lines)
        grid.add_child(btn)
    block.add_child(grid)
    # Spell the unmet prerequisites out in the panel — a greyed button alone doesn't teach.
    for policy in options:
        var policy_key := String(policy)
        var reasons := _gate_reasons(gates, policy_key)
        if reasons.is_empty():
            continue
        var titled := "%s%s" % [
            _source_icon_prefix(FoodIcons.for_policy(policy_key)), policy_key.capitalize()]
        if reasons.size() == 1:
            block.add_child(_alloc_hint_label(GATE_REASON_LINE_FORMAT % [titled, reasons[0]]))
            continue
        block.add_child(_alloc_hint_label(GATE_REASON_HEADER_FORMAT % titled))
        for reason in reasons:
            block.add_child(_alloc_hint_label(GATE_REASON_BULLET_FORMAT % reason))
    return block

## The unmet-prerequisite reasons a `gates` dict holds for one policy — empty (available) for an
## absent key. The single reader of the gates contract, so callers never re-assert its shape.
func _gate_reasons(gates: Dictionary, policy: String) -> Array:
    var reasons: Variant = gates.get(policy, null)
    return reasons if reasons is Array else []

## The tile "Assign foragers" controls (compose a count, then Assign). Shown only for a
## tile with a food module while a player band exists to staff it — and only on a hex the player can
## actually SEE (a workable patch is live state, redacted from a remembered tile like its occupants;
## MapView already strips `food_module*` there, and this holds the line if anything ever feeds a
## non-redacted dict).
func _build_forage_assign_controls(tile_info: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _forage_compose_available(tile_info):
        return
    var resolved := _resolve_assign_band()
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var key := "%d,%d" % [x, y]
    # When the selected tile changes, default the actor band to the resolved band (and re-seed
    # the count from its staffing); otherwise preserve the picked band + count across the
    # per-snapshot re-renders of the same tile.
    var source_changed := _forage_assign_key != key
    if source_changed:
        _forage_assign_key = key
        _forage_assign_band = int(resolved.get("entity", -1))
    var band := _player_band_by_entity(_forage_assign_band)
    if band.is_empty():
        band = resolved
        _forage_assign_band = int(band.get("entity", -1))
    if source_changed:
        var staffed := _workers_for_forage(band, x, y)
        _forage_assign_count = staffed if staffed > 0 else WORKER_STEP
        _forage_assign_policy = _policy_for_forage(band, x, y)
    # Effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _effective_forage_workers(band, x, y)
    var pending := _pending_assigns_for(int(band.get("entity", -1))).has(_pending_key(LABOR_KIND_FORAGE, x, y, ""))
    # The sheet's own header already names the verb and the subject ("ASSIGN FORAGERS  Nut Grove"),
    # so this line carries only what the header cannot: the standing staffing being edited.
    if current > 0 or pending:
        var title := Label.new()
        title.text = COMPOSE_NOW_STAFFED_FORMAT % [current, COMPOSE_PENDING_SUFFIX if pending else ""]
        title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
        target.add_child(title)
    # Which band supplies the foragers (above the stepper). Switching re-runs the range check below
    # for that band.
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _forage_assign_band = int(picked.get("entity", -1))
        _build_forage_assign_controls(tile_info, target)))
    # Forage take policy (Sustain/Surplus/Market/Eradicate, default Sustain) — reuses the hunt policy
    # radio + option set (LABOR_HUNT_POLICIES) but shows forage-appropriate behaviour hints. Persisted
    # across re-renders like the hunt policy; re-seeded from current staffing when the tile changes.
    var forage_gates := _forage_policy_gates(tile_info)
    # A gated rung can never be the composed policy — the patch may have left Thriving under a
    # standing Cultivate selection, so re-validate every render, not just on a tile change.
    if not (_forage_assign_policy in FORAGE_POLICY_OPTIONS) \
            or not _gate_reasons(forage_gates, _forage_assign_policy).is_empty():
        _forage_assign_policy = DEFAULT_HUNT_POLICY
    # Ascending per-policy per-turn takes on the extractive buttons, so the forage picker wears the SAME
    # "+X /turn" button metric the local-hunt picker does (the investment rungs Cultivate/Sow carry none,
    # like Corral — their dip→payoff is stated by the forecast row below).
    var forage_takes := _forage_policy_takes(tile_info)
    target.add_child(_build_policy_picker(func(policy: String) -> void:
        _forage_assign_policy = policy
        # Picking a policy auto-fills the foragers to that policy's max-useful (consumed next rebuild).
        _forage_assign_autofill = true
        _build_forage_assign_controls(tile_info, target), _forage_assign_policy, FORAGE_POLICY_OPTIONS, forage_gates, forage_takes))
    target.add_child(_alloc_hint_label(String(FORAGE_POLICY_HINTS.get(_forage_assign_policy, ""))))
    # Pre-commit forecast: the patch's per-worker yield + the SELECTED policy's ceiling cap the
    # stepper at max-useful workers, so the player CAN'T over-assign while composing. Both the
    # stepper and the policy picker re-render these controls, so the cap and the expected-yield row
    # below recompute on every change (a Market/Eradicate ceiling is higher than Sustain's, so
    # switching policy moves the cap).
    var forecast := _forecast_inputs(tile_info, SOURCE_KIND_FORAGE, FORAGE_FORECAST_PREFIX, _forage_assign_policy)
    var capped := _forecast_worker_cap(forecast, _assignable_forage_workers(band, x, y))
    var cap := int(capped["cap"])
    # Auto-max on policy select — "give me everything this patch sustains": jump to the max-useful for
    # the policy (clamped to available below). Only ever set by a policy click, never by a −/+ tick.
    if _forage_assign_autofill:
        _forage_assign_count = cap
        _forage_assign_autofill = false
    _forage_assign_count = clampi(_forage_assign_count, 0, cap)
    target.add_child(_build_worker_stepper(
        FORAGE_CREW_LABEL, _forage_assign_count, _forage_assign_count < cap,
        func(n: int) -> void:
            _forage_assign_count = clampi(n, 0, cap)
            _build_forage_assign_controls(tile_info, target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(_alloc_hint_label(cap_note))
    # ONE yield row per rung, mirroring the local hunt: an INVESTMENT rung (Cultivate/Sow) keeps
    # `_forecast_yield_row`'s dip→payoff deal ("Preparing: +X → then +Y"), which a single rate can't
    # express; an EXTRACTIVE rung renders the bare-rate + verdict preview (`+2.74 /turn · renewable` /
    # `⚠ … — overdraws the patch`) at the same font as the hunt line — which also surfaces the overdraw
    # warning an Eradicate/Market forage used to render silently.
    if _forage_assign_policy in INVESTMENT_POLICIES:
        if bool(forecast["known"]):
            target.add_child(
                _forecast_yield_row(forecast, _forage_assign_count, band))
    else:
        var yield_line := _local_forage_preview_bbcode(
            band, tile_info, _forage_assign_policy, _forage_assign_count)
        if yield_line != "":
            target.add_child(_forecast_label(yield_line))
    # Range-aware: foraging is stationary gathering (there is NO forage-expedition alternative), so a
    # tile beyond the SELECTED band's work_range DISABLES the button + shows an out-of-range hint,
    # rather than a fallback. Distance is wrap-aware from the picked band's OWN tile — distance,
    # work_range, and the target band all key off `band` explicitly (never the faction's default band).
    var band_tile := _band_tile(band)
    var work_range := int(band.get("work_range", 0))
    var distance := _hex_distance_wrapped(band_tile.x, band_tile.y, x, y)
    var out_of_range := distance >= 0 and distance > work_range
    if out_of_range:
        target.add_child(_alloc_hint_label(
            "(%d,%d) is %d tiles away — beyond this band's forage range (%d)." % [x, y, distance, work_range]))
    var assign_btn := Button.new()
    assign_btn.text = FORAGE_ASSIGN_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range
    assign_btn.pressed.connect(func() -> void:
        _emit_assign_labor(band, LABOR_KIND_FORAGE, _forage_assign_count, x, y, "", _forage_assign_policy)
        close_compose_sheet())
    target.add_child(assign_btn)

# ---- THE COMPOSE SHEET: the drawer's read state + the floating write state --------------------
#
# docs/plan_tile_panel_layout.md §10-§15. The drawer keeps the detail rows, gains a one-line
# standing-assignment summary, and ends in `Assign … ▸`; the sheet (`ui/hud/ComposeSheet.gd`) hosts
# the compose block itself. NOTHING is re-derived here — the summary's rate comes from the same
# `_source_yield_readout` the Band panel's Current-actions rows use, and every gate, forecast and
# ceiling in the sheet comes from the same call it came from when the block lived in the drawer.

## Build the compose sheet once. Like the fork panel it is a child of the HUD CanvasLayer itself,
## NOT of `layout_root`: it floats over the whole window and must not inset with the reserved docks.
func _ensure_compose_sheet() -> void:
    if _compose_sheet != null:
        return
    _compose_sheet = ComposeSheet.new()
    _compose_sheet.name = "ComposeSheet"
    _compose_sheet.closed.connect(_on_compose_sheet_closed)
    add_child(_compose_sheet)

## Is a compose sheet open? `Main._unhandled_input` asks this FIRST on Esc — the sheet is the
## innermost surface, so it claims the key ahead of targeting-cancel and the pause menu (§15).
func is_compose_sheet_open() -> bool:
    return _compose_sheet != null and _compose_sheet.is_open()

## Close any open sheet and return to the read state. Idempotent, so every close reason (commit, ✕,
## catcher click, Esc, selection change, targeting) can call it unconditionally.
func close_compose_sheet() -> void:
    if _compose_sheet != null:
        _compose_sheet.close()

## The sheet reports itself closed (including when WE closed it) — drop the compose state so the two
## can never disagree, and restore the drawer's read state so its button un-presses.
func _on_compose_sheet_closed() -> void:
    _compose_kind = COMPOSE_KIND_NONE
    _compose_subject = ""
    _refresh_drawer_actions()

## The rect the sheet floats beside: the selection card, so the subject list + standing summary it
## is editing stay readable. A zero rect (card hidden) makes the sheet hug the viewport margin.
func _compose_anchor_rect() -> Rect2:
    if tile_panel == null or not tile_panel.visible:
        return Rect2()
    return tile_panel.get_global_rect()

## Can this LAND offer a forage compose at all? The gate the drawer's button and the sheet share, so
## the button can never open an empty sheet. (A workable patch is live state — redacted on a
## remembered hex like its occupants — and there must be a player band to staff it.)
func _forage_compose_available(tile_info: Dictionary) -> bool:
    return String(tile_info.get("food_module", "")).strip_edges() != "" \
        and not _resolve_assign_band().is_empty() \
        and not _tile_contents_unseen(tile_info)

## Can this HERD offer a hunt/herding compose? Huntable, with a player band to staff it. (A penned
## herd's Extend-pen action is NOT a compose — it stays in the drawer, see `_build_herd_drawer_actions`.)
func _herd_compose_available(herd: Dictionary) -> bool:
    return bool(herd.get("huntable", false)) and not _resolve_assign_band().is_empty()

## The stable key identifying a composed source, so a per-snapshot refresh can tell "the same
## source, restated" from "a different source" (§15: a snapshot must NOT close the sheet).
func _forage_source_key(tile_info: Dictionary) -> String:
    return "%d,%d" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]

## The crew noun the sheet's stepper uses for this herd — herders on a MANAGED (corralled/pastoral)
## herd, hunters on a wild one. Read by the drawer button too, so the two always agree.
func _herd_crew_noun(herd: Dictionary) -> String:
    return HERD_CREW_LABEL if _is_managed_hunt_source(herd, _hunt_assign_policy) else HUNT_CREW_LABEL

func _open_forage_compose(tile_info: Dictionary) -> void:
    if not _forage_compose_available(tile_info):
        return
    _ensure_compose_sheet()
    _compose_kind = COMPOSE_KIND_FORAGE
    _compose_subject = _forage_source_key(tile_info)
    var subject := String(tile_info.get("food_module_label", "")).strip_edges()
    if subject == "":
        subject = _format_food_module_label(String(tile_info.get("food_module", "")))
    var content := _compose_sheet.open(
        COMPOSE_SHEET_EYEBROW_FORMAT % FORAGE_CREW_LABEL.to_lower(),
        subject, _compose_subject, _compose_anchor_rect())
    _build_forage_assign_controls(tile_info, content)
    _refresh_drawer_actions()

func _open_herd_compose(herd: Dictionary) -> void:
    if not _herd_compose_available(herd):
        return
    _ensure_compose_sheet()
    _compose_kind = COMPOSE_KIND_HERD
    _compose_subject = String(herd.get("id", ""))
    var content := _compose_sheet.open(
        COMPOSE_SHEET_EYEBROW_FORMAT % _herd_crew_noun(herd).to_lower(),
        _herd_display_name(herd), _compose_subject, _compose_anchor_rect())
    _build_herd_assign_controls(herd, content)
    _refresh_drawer_actions()

## A snapshot arrived: re-render the OPEN sheet in place against the fresh subject. It must NOT
## close — `reapply_selection` runs every turn and closing would make the sheet unusable under
## autoplay (§15). It closes only when the subject it is composing is actually GONE (a different
## source is now selected, or the source stopped offering the compose at all).
func _refresh_compose_sheet() -> void:
    if not is_compose_sheet_open():
        return
    match _compose_kind:
        COMPOSE_KIND_FORAGE:
            if _forage_source_key(_selected_tile_info) != _compose_subject \
                    or not _forage_compose_available(_selected_tile_info):
                close_compose_sheet()
                return
            _build_forage_assign_controls(_selected_tile_info, _compose_sheet.content())
        COMPOSE_KIND_HERD:
            if String(_selected_herd.get("id", "")) != _compose_subject \
                    or not _herd_compose_available(_selected_herd):
                close_compose_sheet()
                return
            _build_herd_assign_controls(_selected_herd, _compose_sheet.content())
        _:
            close_compose_sheet()

## Re-render whichever subject's drawer actions are showing (the standing summary + the `Assign … ▸`
## button), so a turn's staffing change lands in the read state as well as in the open sheet.
func _refresh_drawer_actions() -> void:
    if not _selected_herd.is_empty():
        _build_herd_drawer_actions(_selected_herd)
    elif not _selected_tile_info.is_empty() and _selected_unit.is_empty():
        _build_forage_drawer_actions(_selected_tile_info)

## The LAND drawer's read state: the standing forage summary (when the player already works this
## patch) and the `Assign foragers ▸` button that opens the sheet. Fills `%ForageAssignControls`,
## which is why that node keeps its name and its place in the drawer — the compose block MOVED out
## of it, the node did not move.
func _build_forage_drawer_actions(tile_info: Dictionary) -> void:
    if forage_assign_controls == null:
        return
    for child in forage_assign_controls.get_children():
        child.queue_free()
    var available := _forage_compose_available(tile_info)
    forage_assign_controls.visible = available
    if not available:
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var standing := _standing_assignment(LABOR_KIND_FORAGE, x, y, "")
    if not standing.is_empty():
        forage_assign_controls.add_child(_build_standing_summary(
            standing, LABOR_KIND_FORAGE, FORAGE_CREW_LABEL.to_lower()))
    forage_assign_controls.add_child(_build_compose_open_button(
        FORAGE_CREW_LABEL, _forage_source_key(tile_info),
        func() -> void: _open_forage_compose(tile_info)))

## The HERD drawer's read state: the Extend-pen action (a one-click standing action on a built pen —
## NOT a compose, so it stays here rather than hiding behind a sheet), the standing hunt summary, and
## the `Assign hunters ▸` / `Assign herders ▸` button.
func _build_herd_drawer_actions(herd: Dictionary) -> void:
    if herd_assign_controls == null:
        return
    for child in herd_assign_controls.get_children():
        child.queue_free()
    var corralled := bool(herd.get("corralled", false))
    var available := _herd_compose_available(herd)
    # A penned herd always offers Extend-pen, even if it is no longer huntable — so the container
    # stays visible for a pen OR a composable herd.
    herd_assign_controls.visible = available or corralled
    if corralled:
        _build_extend_pen_control(herd, herd_assign_controls)
    if not available:
        return
    var herd_id := String(herd.get("id", ""))
    var noun := _herd_crew_noun(herd)
    var standing := _standing_assignment(LABOR_KIND_HUNT, -1, -1, herd_id)
    if not standing.is_empty():
        herd_assign_controls.add_child(_build_standing_summary(
            standing, LABOR_KIND_HUNT, noun.to_lower()))
    herd_assign_controls.add_child(_build_compose_open_button(
        noun, herd_id, func() -> void: _open_herd_compose(herd)))

## The `Assign … ▸` button. It lights "primary" (SIGNAL cyan — this HUD's LIVE state, as on the
## Sight chip and the selection accent) while ITS sheet is the open one, so the drawer shows which
## source is being composed rather than looking idle behind the sheet; "ghost" at rest. NOT "armed"
## — that is the destructive/warned treatment (DANGER border), and an open sheet is not a warning.
func _build_compose_open_button(noun: String, subject_key: String, on_press: Callable) -> Button:
    var button := Button.new()
    button.text = COMPOSE_OPEN_BUTTON_FORMAT % noun.to_lower()
    var composing := is_compose_sheet_open() and _compose_subject == subject_key
    HudStyle.apply_button(button, "primary" if composing else "ghost")
    button.pressed.connect(on_press)
    return button

## The player faction's standing assignment on a source, across every player band — `{}` when
## nobody works it. Scans `_player_bands` (the full player-faction list) and falls back to the
## single `_player_band` the one-band case (and the HUD-only preview harness) carries.
func _standing_assignment(kind: String, x: int, y: int, herd_id: String) -> Dictionary:
    var bands: Array = _player_bands if not _player_bands.is_empty() else [_player_band]
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        var found := _hunt_assignment_of(band, herd_id) if kind == LABOR_KIND_HUNT \
            else _forage_assignment_of(band, x, y)
        if not found.is_empty():
            return found
    return {}

## The drawer's one-line standing-assignment summary: `♻ 3 foragers · +2.74 /turn`, with the SAME
## warn/overdraw and overstaff/wasted flags the Band panel's Current-actions rows render, from the
## SAME `_source_yield_readout` call. The rate is never recomputed here.
func _build_standing_summary(assignment: Dictionary, kind: String, noun: String) -> Control:
    # `has_yield` is the ONE key `_source_yield_readout` reads that is not on the wire assignment —
    # it gates the rate on a CONFIRMED source (`_effective_worker_map` sets it false for a pending,
    # yield-less optimistic assign). Everything else — actual/sustainable/realized, `overdraws`,
    # `workers_needed`, `wasted_yield` — is read straight off the assignment the sim sent.
    var m := assignment.duplicate()
    m["has_yield"] = assignment.has("actual_yield")
    var readout := _source_yield_readout(m, kind)
    var tooltip := String(readout["tooltip"])
    var flow := HFlowContainer.new()
    flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    flow.add_theme_constant_override("h_separation", STATUS_LINE_SEPARATION)
    if tooltip != "":
        flow.tooltip_text = tooltip
    var text := STANDING_SUMMARY_FORMAT % [
        FoodIcons.for_policy(String(assignment.get("policy", ""))),
        int(assignment.get("workers", 0)),
        noun,
    ]
    var suffix := String(readout["label_suffix"])
    if suffix != "":
        text += STANDING_SUMMARY_SEPARATOR + suffix
    flow.add_child(_build_status_part(text.strip_edges(), HudStyle.INK))
    # ⚠ = ecological (the take outruns regrowth); the notes = labor (extra workers idle here / the
    # crew could not carry what the source offered). Same three parts, same three colours as a row.
    if bool(readout["warn"]):
        flow.add_child(_build_row_note_label(OVERHUNT_FLAG, HudStyle.WARN, tooltip))
    var note := String(readout["note"])
    if note != "":
        flow.add_child(_build_row_note_label(note, HudStyle.WARN, tooltip))
    var muted_note := String(readout["muted_note"])
    if muted_note != "":
        flow.add_child(_build_row_note_label(muted_note, HudStyle.INK_FAINT, tooltip))
    return flow

## Move-band: enter tile-targeting; the destination click emits move_band_requested.
func _on_move_band_pressed() -> void:
    # Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
    close_compose_sheet()
    var band := _resolve_assign_band()
    if band.is_empty():
        return
    _pending_move_band = band.duplicate(true)
    _refresh_targeting()

func _cancel_pending_move_band() -> void:
    if _pending_move_band.is_empty():
        return
    _pending_move_band = {}
    _refresh_targeting()

func _try_dispatch_pending_move_band(tile_info: Dictionary) -> void:
    if _pending_move_band.is_empty() or tile_info.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    var band := _pending_move_band
    var bits := int(band.get("entity", -1))
    emit_signal("move_band_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": bits,
        "x": x,
        "y": y,
    })
    _pending_move_band = {}
    _refresh_targeting()
    # Optimistic feedback: mark the destination pending until a newer-turn snapshot confirms.
    _record_pending_move(bits, x, y)
    _after_pending_change()

## Send-expedition: outfit `band` with `party_workers` and enter tile-targeting; the next tile
## click emits send_expedition_requested. Mirrors the move-band pending flow.
func _on_send_expedition_pressed(band: Dictionary, party_workers: int) -> void:
    # Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
    close_compose_sheet()
    if band.is_empty() or party_workers <= 0:
        return
    _pending_send_expedition = {"band": band.duplicate(true), "party_workers": party_workers}
    _refresh_targeting()

func _cancel_pending_send_expedition() -> void:
    if _pending_send_expedition.is_empty():
        return
    _pending_send_expedition = {}
    _refresh_targeting()

func _try_dispatch_pending_send_expedition(tile_info: Dictionary) -> void:
    if _pending_send_expedition.is_empty() or tile_info.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    var band: Dictionary = _pending_send_expedition.get("band", {})
    emit_signal("send_expedition_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": int(band.get("entity", -1)),
        "party_workers": int(_pending_send_expedition.get("party_workers", 0)),
        "x": x,
        "y": y,
    })
    _pending_send_expedition = {}
    _refresh_targeting()

## Quarry PICK: enter HERD-targeting so the next map click names the herd the compose sheet is aimed
## at. It dispatches NOTHING — the sheet stays open behind the targeting and fills its Quarry row in,
## then asks for policy and party size against that herd. (`_pending_send_hunt_expedition`, which used
## to mean "party is outfitted, now click a herd and send it", was repurposed into this.)
func _on_pick_quarry_pressed(band: Dictionary) -> void:
    # Targeting asks the player to click the map — the tile panel's FLOATING sheet over it is a trap
    # (§15). The DOCKED party sheet is not floating and deliberately stays open.
    close_compose_sheet()
    if band.is_empty():
        return
    _pending_pick_quarry = {"band": band.duplicate(true)}
    _refresh_targeting()

func _cancel_pending_pick_quarry() -> void:
    if _pending_pick_quarry.is_empty():
        return
    # Only the PICK is cancelled: a quarry chosen earlier stays chosen, so Esc during a re-pick
    # returns the player to the form they already had rather than emptying it.
    _pending_pick_quarry = {}
    _refresh_targeting()

func _try_pick_quarry(tile_info: Dictionary) -> void:
    if _pending_pick_quarry.is_empty() or tile_info.is_empty():
        return
    # Resolve the target from the clicked hex's herds (herd markers occupy the hex, so a click on a
    # herd lands here). Pick the first huntable herd on the tile; if none, keep targeting and nudge.
    var herd := _huntable_herd_on_tile(tile_info)
    var fauna_id := String(herd.get("id", "")).strip_edges()
    if fauna_id == "":
        _note_command_feed("Hunt expedition", "No huntable herd there — click on a herd.")
        return
    # A herd INSIDE the band's hunt reach is a local hunt, not a party's job. Refuse it here and stay
    # in targeting, exactly like the miss above — and say why, since the reach split is invisible on
    # the map. (MapView doesn't glow these, so this is the belt to that braces.)
    var band: Dictionary = _pending_pick_quarry.get("band", {})
    if not _is_expedition_quarry(band, herd):
        var band_tile := _band_tile(band)
        _note_command_feed("Hunt expedition", QUARRY_WITHIN_REACH_FORMAT % [
            _herd_display_name(herd),
            _hex_distance_wrapped(band_tile.x, band_tile.y,
                int(herd.get("x", -1)), int(herd.get("y", -1))),
            String(band.get("id", "this band")),
            int(band.get("hunt_reach", 0)),
        ])
        return
    # NO no-surplus check here: no policy is chosen yet, so that verdict is unknowable. It lives
    # entirely on the sheet's Send button, which has every input.
    _send_party_quarry_id = fauna_id
    # Fill the party to this herd's max-useful cap for the default policy, same one-shot a policy
    # click sets. Party size is meaningless until the quarry is known (the useful count is a property
    # of the HERD), so picking one is the first moment we CAN default it — "give me everyone this raid
    # can use". Consumed on the next render before the clamp; a −/+ tick still overrides freely.
    _send_party_autofill = true
    _pending_pick_quarry = {}
    _refresh_targeting()
    _rerender_panel_allocation()

## Is `herd` a valid quarry for a DETACHED party from `band`? A hunting party exists precisely for
## game the band cannot work from home, so the answer is the SAME split the herd drawer makes when it
## chooses between "Assign Local Hunt" and the expedition branch: strictly beyond the band's
## `hunt_reach`, wrap-aware, measured from the band's own tile. THE single definition — the pick, the
## sheet's re-validation and MapView's glow all route through it (the map must never promise a target
## the pick refuses). An unknown distance (missing tiles) is NOT a quarry, mirroring the drawer's
## fallback to the local hunt.
func _is_expedition_quarry(band: Dictionary, herd: Dictionary) -> bool:
    var band_tile := _band_tile(band)
    var distance := _hex_distance_wrapped(
        band_tile.x, band_tile.y, int(herd.get("x", -1)), int(herd.get("y", -1)))
    return distance >= 0 and distance > int(band.get("hunt_reach", 0))

## A herd's player-facing name (species → label → id). One definition, shared by the targeting banner's
## forecast line and the command-feed refusal, so a herd is never called two different things.
func _herd_display_name(herd: Dictionary) -> String:
    return String(herd.get("species", herd.get("label", herd.get("id", "This herd"))))

## The first huntable herd DICT on a hex's tile_info, or {} when there is none. The target click
## resolves its id from this; the hovered-herd forecast additionally needs the herd's exported
## `hunt_policy_ceilings`, so both read the same herd through one helper.
func _huntable_herd_on_tile(tile_info: Dictionary) -> Dictionary:
    var herds_variant: Variant = tile_info.get("herds", [])
    if not (herds_variant is Array):
        return {}
    for herd_variant in (herds_variant as Array):
        if herd_variant is Dictionary and bool((herd_variant as Dictionary).get("huntable", false)):
            var herd: Dictionary = herd_variant as Dictionary
            if String(herd.get("id", "")).strip_edges() != "":
                return herd
    return {}


## Map double-click convenience (Main forwards `MapView.herd_quick_hunt_requested`): assign
## ALL of the player band's currently-idle workers to hunt `herd_id` at the default Sustain
## policy. A no-op (with a command-feed note) when there's no player band or no idle workers,
## so the shortcut never silently does nothing.
func quick_assign_hunters(herd_id: String) -> void:
    if herd_id.strip_edges() == "":
        return
    var band := _resolve_assign_band()
    if band.is_empty():
        _note_command_feed("Quick-hunt", "No player band to assign.")
        return
    var idle := int(band.get("idle_workers", 0))
    if idle <= 0:
        _note_command_feed("Quick-hunt", "No idle workers to assign to %s." % herd_id)
        return
    _emit_assign_labor(band, LABOR_KIND_HUNT, idle,
        int(band.get("current_x", -1)), int(band.get("current_y", -1)), herd_id, DEFAULT_HUNT_POLICY)

func update_overlay_legend(legend: Dictionary) -> void:
    _legend.update(legend)
func get_upper_stack_height() -> float:
    var max_bottom := 0.0
    for label in [campaign_title_label, campaign_subtitle_label, turn_label, metrics_label, victory_status_label]:
        if label == null:
            continue
        var top: float = label.position.y
        var size: float = label.get_combined_minimum_size().y
        if size <= 0.0:
            size = label.size.y
        if size <= 0.0:
            size = 20.0
        max_bottom = max(max_bottom, top + size)
    if max_bottom <= 0.0:
        max_bottom = 24.0
    return max_bottom + STACK_ADDITIONAL_MARGIN

## Reserve a strip of one edge for a docked panel (keyed by reserver id). The
## panels keep their natural docks; the whole layout just lives in the smaller
## rectangle, matching the shrunk map area. `edge` is a Godot Side const
## (SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM); `size <= 0` releases the reserver.
func set_reserved_inset(id: StringName, edge: int, size: float) -> void:
    if size <= 0.0:
        _reservations.erase(id)
    else:
        _reservations[id] = {"edge": edge, "size": size}
    _recompute_insets()
    if layout_root != null:
        layout_root.offset_left = _inset_left
        layout_root.offset_top = _inset_top
        layout_root.offset_right = -_inset_right
        layout_root.offset_bottom = -_inset_bottom

## Sum the registered reservations into the four per-edge totals.
func _recompute_insets() -> void:
    _inset_left = 0.0
    _inset_right = 0.0
    _inset_top = 0.0
    _inset_bottom = 0.0
    for reservation in _reservations.values():
        var size: float = float(reservation["size"])
        match int(reservation["edge"]):
            SIDE_LEFT:
                _inset_left += size
            SIDE_TOP:
                _inset_top += size
            SIDE_RIGHT:
                _inset_right += size
            SIDE_BOTTOM:
                _inset_bottom += size
func _refresh_campaign_label() -> void:
    if campaign_title_label == null or campaign_subtitle_label == null:
        return
    var title_text := _resolve_localized_field("title")
    var subtitle_text := _resolve_localized_field("subtitle")
    var has_title := title_text.strip_edges() != ""
    var has_subtitle := subtitle_text.strip_edges() != ""
    campaign_title_label.visible = has_title
    campaign_subtitle_label.visible = has_subtitle
    campaign_title_label.text = title_text if has_title else ""
    campaign_subtitle_label.text = subtitle_text if has_subtitle else ""

## Clear the command FEED only — a full snapshot re-seeds it from the server's ring, so keeping
## stale receipts would double them up. The Telling panel is deliberately NOT reset here: its
## signature de-dup makes re-ingesting the ring harmless, and clearing would throw away every
## telling that has already scrolled past the server's 32-entry ring.
func reset_command_feed() -> void:
    _command_feed.reset()
func show_tile_selection(tile_info: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _selected_tile_info = tile_info.duplicate(true) if tile_info is Dictionary else {}
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_subject = SUBJECT_LAND
    _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(_selected_tile_info, {}, {})
    _try_dispatch_pending_move_band(_selected_tile_info)
    _try_dispatch_pending_send_expedition(_selected_tile_info)
    _try_pick_quarry(_selected_tile_info)

func notify_hex_selected(tile_info: Dictionary) -> void:
    if tile_info.is_empty():
        return
    _try_dispatch_pending_move_band(tile_info)
    _try_dispatch_pending_send_expedition(tile_info)
    _try_pick_quarry(tile_info)

func show_unit_selection(unit_data: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    var tile_info: Dictionary = {}
    var tile_variant: Variant = unit_data.get("tile_info", {})
    if tile_variant is Dictionary:
        tile_info = (tile_variant as Dictionary).duplicate(true)
    else:
        tile_info = _selected_tile_info
    _selected_tile_info = tile_info
    _selected_unit = unit_data.duplicate(true)
    _selected_herd.clear()
    _selected_subject = SUBJECT_UNIT
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, _selected_unit, {})

func show_herd_selection(herd_data: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    var tile_info: Dictionary = {}
    var tile_variant: Variant = herd_data.get("tile_info", {})
    if tile_variant is Dictionary and not (tile_variant as Dictionary).is_empty():
        tile_info = (tile_variant as Dictionary).duplicate(true)
    elif _herd_matches_selected_tile(herd_data):
        # Same hex as the currently-selected tile (a map click on a hex that has
        # both a gather module and a fauna group): surface Harvest alongside the
        # herd verbs. A herd picked from the inspector (no tile_info, unrelated tile
        # selected) falls through to herd-only so Harvest can't mis-target.
        tile_info = _selected_tile_info
    _selected_tile_info = tile_info
    _selected_herd = herd_data.duplicate(true)
    _selected_unit.clear()
    _selected_subject = SUBJECT_HERD
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, {}, _selected_herd)

## True when the currently-selected tile is the same hex the herd occupies, so it
## is safe to keep showing that tile's Harvest verb alongside the herd verbs.
func _herd_matches_selected_tile(herd_data: Dictionary) -> bool:
    if _selected_tile_info.is_empty():
        return false
    return int(_selected_tile_info.get("x", -1)) == int(herd_data.get("x", -2)) \
        and int(_selected_tile_info.get("y", -1)) == int(herd_data.get("y", -2))

## Coordinator: render both left-dock cards from the current selection state.
## The two cards are two scene nodes driven by one script — the Tile card is the
## place (terrain + Forage), the Occupants card is the selectable band/wildlife
## roster + a detail drawer for the chosen occupant. The `*_data` params mirror
## the members the show_*/pending flows already set; the members are authoritative.
## Re-render the selection panel for the still-selected occupant/tile using fresh
## snapshot data (called from Main after each snapshot via MapView.refresh_selection_payload).
## Unlike the show_* entry points this runs NO click-time side effects — no pending-scout
## dispatch, no forage/hunt/follow consumption — so refreshing every turn can't misfire a
## pending command. Keeps the panel live across turn advances instead of going stale until
## the user reselects the hex. "none" means the selected band/herd is gone → drop to its
## tile if we still have one, else hide the cards (without cancelling pending forage).
func reapply_selection(kind: String, data: Dictionary) -> void:
    match kind:
        "unit":
            _selected_unit = data.duplicate(true) if data is Dictionary else {}
            _selected_herd.clear()
            _selected_subject = SUBJECT_UNIT
            _adopt_tile_info_from(_selected_unit)
            _render_selection_panel(_selected_tile_info, _selected_unit, {})
        "herd":
            _selected_herd = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _selected_subject = SUBJECT_HERD
            _adopt_tile_info_from(_selected_herd)
            _render_selection_panel(_selected_tile_info, {}, _selected_herd)
        "tile":
            _selected_tile_info = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _selected_herd.clear()
            _selected_subject = SUBJECT_LAND
            _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
            _render_selection_panel(_selected_tile_info, {}, {})
        _:
            # Selected occupant vanished (e.g. the band expired). Drop to its last tile
            # if known, else hide the card. Intentionally does not touch pending state.
            _selected_unit.clear()
            _selected_herd.clear()
            _selected_subject = SUBJECT_LAND
            if _selected_tile_info.is_empty():
                _hide_selection_card()
            else:
                _render_selection_panel(_selected_tile_info, {}, {})

## Pull the fresh tile_info a refresh payload carries alongside the occupant, so the tile
## card + roster render against the same snapshot the occupant came from.
func _adopt_tile_info_from(occupant: Dictionary) -> void:
    var ti_variant: Variant = occupant.get("tile_info", {})
    if ti_variant is Dictionary and not (ti_variant as Dictionary).is_empty():
        _selected_tile_info = (ti_variant as Dictionary).duplicate(true)
    _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()

func _render_selection_panel(_tile_info: Dictionary, _unit_data: Dictionary, _herd_data: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    # Reset the band-food/morale/output tint context; `_unit_summary_lines` re-sets it if
    # a band is being rendered into the drawer.
    _selected_band_food_turns = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _assemble_roster(_selected_tile_info)
    _render_tile_card(_selected_tile_info)
    _resolve_auto_selected_subject()
    _rebuild_subject_list()
    _render_subject_drawer()

## Hide the whole selection card (no tile, no occupant). One place, so the drawer's three
## compose blocks can't be left visible behind a hidden card.
func _hide_selection_card() -> void:
    if tile_panel != null:
        tile_panel.visible = false
    _hide_drawer_blocks()

func _hide_drawer_blocks() -> void:
    if forage_assign_controls != null:
        forage_assign_controls.visible = false
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false

## Assemble the roster for the current hex from the tile's `units`/`herds`, then
## ensure the currently-selected occupant is represented even when the tile_info
## doesn't list it (an inspector-driven herd selection carries an empty tile_info).
func _assemble_roster(tile_info: Dictionary) -> void:
    _roster_units = []
    _roster_herds = []
    # Occupants are LIVE state, so on a hex the player cannot currently see they are redacted — MapView
    # fog-gates them out of `tile_info` at source, and this re-reads the SAME state flag it tagged (not
    # a second visibility test) so the roster stays honest no matter who feeds it.
    # THE ONE EXCEPTION: your OWN bands are always listed, even on an Unexplored hex. A scouting party
    # is deliberately excluded from fog reveal server-side, so it ROUTINELY stands on a tile it cannot
    # see — hiding it would delete your own expedition from the roster exactly while you're using it.
    # Mirrors `MapView._unit_hidden_by_fog`, which is the same rule for the map/click side.
    var unseen := _tile_contents_unseen(tile_info)
    var units_variant: Variant = tile_info.get("units", [])
    if units_variant is Array:
        for entry in units_variant:
            if entry is Dictionary and (not unseen or _is_player_unit(entry as Dictionary)):
                _roster_units.append(entry)
    # Wildlife is never ours — an unseen hex lists no herds at all.
    if not unseen:
        var herds_variant: Variant = tile_info.get("herds", [])
        if herds_variant is Array:
            for entry in herds_variant:
                if entry is Dictionary:
                    _roster_herds.append(entry)
    if not _selected_unit.is_empty() and _find_roster_unit(int(_selected_unit.get("entity", -1))).is_empty():
        _roster_units.append(_selected_unit)
    if not _selected_herd.is_empty() and _find_roster_herd(String(_selected_herd.get("id", ""))).is_empty():
        _roster_herds.append(_selected_herd)

## The card's chrome: the coordinates as the title, and the pinned chip strip. The terrain ROWS
## are no longer here — they are the land subject's drawer content, rendered by
## `_render_subject_drawer` when the land row is the lit one.
func _render_tile_card(tile_info: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    tile_panel.visible = true
    tile_panel.set_card_kind("Tile")
    var title_text := "—"
    if not tile_info.is_empty():
        title_text = "(%d, %d)" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]
    tile_panel.set_card_title(title_text)
    _build_tile_chips(tile_info)

## The sight state in plain words — the FULL form, which is the chip's tooltip. "" (FoW off) yields
## no chip at all.
func _tile_sight_value(visibility_state: String) -> String:
    match visibility_state:
        VISIBILITY_ACTIVE:
            return TILE_SIGHT_ACTIVE
        VISIBILITY_DISCOVERED:
            return TILE_SIGHT_REMEMBERED
        VISIBILITY_UNEXPLORED:
            return TILE_SIGHT_UNEXPLORED
        _:
            return ""

## The chip FACE: one or two words. Only the remembered state has a long form to shorten — the
## other two already ARE their short form, so they pass through and cannot drift out of step.
func _tile_sight_chip_value(value: String) -> String:
    return TILE_SIGHT_REMEMBERED_SHORT if value == TILE_SIGHT_REMEMBERED else value

## In-sight reads LIVE, both unseen states read remembered. The one test behind both the row's
## BBCode hex and the chip's Color, so the two forms cannot drift apart.
func _sight_is_live(value: String) -> bool:
    return value == TILE_SIGHT_ACTIVE

## Value tint for the Sight row: in-sight reads live (SIGNAL cyan — the HUD's "this is current"
## color), while both unseen states read dim (INK_DIM). The row states what you KNOW, not what is
## wrong, so it never borrows the WARN/DANGER palette.
func _sight_value_hex(value: String) -> String:
    return HudStyle.SIGNAL_HEX if _sight_is_live(value) else HudStyle.INK_DIM_HEX

func _sight_value_color(value: String) -> Color:
    return HudStyle.SIGNAL if _sight_is_live(value) else HudStyle.INK_DIM

# ---- The chip strip ---------------------------------------------------------
# Chips carry the tile's STANDING CONDITION — the one-word states you reason with while composing
# an action — pinned above the list so they never scroll away. Numbers stay as rows in the land
# drawer, where their subject is. Each chip is SKIPPED when its field is absent, exactly as the
# equivalent row is: a rehydrated tile must never show an invented rating.
func _build_tile_chips(tile_info: Dictionary) -> void:
    if tile_chips == null:
        return
    for child in tile_chips.get_children():
        child.queue_free()
    if tile_info.is_empty():
        tile_chips.visible = false
        return
    tile_chips.visible = true
    var visibility_state := String(tile_info.get("visibility_state", ""))
    var sight_value := _tile_sight_value(visibility_state)
    if sight_value != "":
        # Short face, full sentence on hover — same value behind both, so they cannot disagree.
        tile_chips.add_child(_make_chip(
            _tile_sight_chip_value(sight_value), _sight_value_color(sight_value), sight_value
        ))
    # Nothing else is knowable on ground nobody has stood on — not even its biome.
    if visibility_state == VISIBILITY_UNEXPLORED:
        return
    if tile_info.has("habitability"):
        var rating := TileHabitability.rating_for(float(tile_info["habitability"]))
        tile_chips.add_child(_make_chip(rating, TileHabitability.color_for(float(tile_info["habitability"]))))
    # Climate is INFORMATIONAL, so it wears neutral ink and never the warning palette; the cut
    # points are the SIM's, so until they are published there is no chip rather than a guess.
    if tile_info.has("temperature") and TileClimate.has_bands():
        tile_chips.add_child(_make_chip(TileClimate.band_for(float(tile_info["temperature"])), HudStyle.INK_DIM))
    var tags_text := String(tile_info.get("tags_text", "")).strip_edges()
    if tags_text != "" and tags_text.to_lower() != CHIP_TAGS_NONE:
        tile_chips.add_child(_make_chip(tags_text, HudStyle.INK_DIM))
    var site_name := String(tile_info.get("site_name", "")).strip_edges()
    if site_name != "":
        tile_chips.add_child(_make_chip(site_name, HudStyle.INK_DIM))

## One chip: a pill wearing the palette's chip chrome, tinted by the condition it states. An
## optional `tooltip` carries the long form of a condition whose face had to be short; a chip
## without one stays mouse-transparent, exactly as before.
func _make_chip(text: String, tint: Color, tooltip: String = "") -> PanelContainer:
    var chip := PanelContainer.new()
    chip.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(tint))
    chip.mouse_filter = Control.MOUSE_FILTER_IGNORE
    if tooltip != "" and tooltip != text:
        chip.tooltip_text = tooltip
        chip.mouse_filter = Control.MOUSE_FILTER_STOP
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", tint)
    label.add_theme_font_size_override("font_size", CHIP_FONT_SIZE)
    chip.add_child(label)
    return chip

## True when the hex's LIVE contents (occupants, workable sources) are unknowable right now — a
## remembered or a never-seen tile. MapView already redacts them from `tile_info` at source (it strips
## `herds`/`units`/`food_module*` and fog-gates `_herds_on_tile`); this re-reads the SAME state flag it
## tagged — not a second visibility test — so every consumer stays honest regardless of who feeds it.
## Terrain rows are exempt by design: geography is remembered knowledge, live contents are not.
func _tile_contents_unseen(tile_info: Dictionary) -> bool:
    var state := String(tile_info.get("visibility_state", ""))
    return state == VISIBILITY_DISCOVERED or state == VISIBILITY_UNEXPLORED

## The selected hex's coordinates, as the one key an explicit subject choice is remembered against.
func _selected_tile_coords() -> Vector2i:
    return Vector2i(int(_selected_tile_info.get("x", -1)), int(_selected_tile_info.get("y", -1)))

## Auto-select the subject whose drawer opens on a fresh tile click.
##
## THE RULE IS DELIBERATELY UNCHANGED, PLUS A LAND FALLBACK: first roster unit → else first herd →
## else the land. A hex with no occupants used to hide the Occupants card and leave the Tile card
## showing terrain, which IS "the land is selected" — so the fallback preserves today's behaviour
## rather than introducing a new default. Selecting the land emits `roster_occupant_selected("land",
## …)`, which moves no ring (the hex outline already marks the tile) but CLEARS MapView's occupant
## selection — see `_on_land_row_selected`.
func _resolve_auto_selected_subject() -> void:
    if not _selected_unit.is_empty() or not _selected_herd.is_empty():
        return
    # THE DEFAULT ONLY APPLIES WHERE THE PLAYER HAS NOT ALREADY CHOSEN. Both occupant dicts are
    # empty either because this is a fresh hex (auto-select) or because the player picked the LAND
    # row here (honour it) — and only the choice tile can tell the two apart. Without this, the
    # per-snapshot `reapply_selection("tile", …)` re-ran the default every turn and stole a
    # deliberately-chosen land selection back to the first band. A genuinely new hex has different
    # coords, so today's first-band → first-herd → land default is preserved exactly.
    if not _selected_tile_info.is_empty() and _subject_choice_tile == _selected_tile_coords():
        _selected_subject = SUBJECT_LAND
        return
    if not _roster_units.is_empty():
        _selected_unit = (_roster_units[0] as Dictionary).duplicate(true)
        _selected_subject = SUBJECT_UNIT
        emit_signal("roster_occupant_selected", "unit", int(_selected_unit.get("entity", -1)))
    elif not _roster_herds.is_empty():
        _selected_herd = (_roster_herds[0] as Dictionary).duplicate(true)
        _selected_subject = SUBJECT_HERD
        emit_signal("roster_occupant_selected", "herd", String(_selected_herd.get("id", "")))
    else:
        _selected_subject = SUBJECT_LAND

## The single drawer, filled by whichever subject row is lit. Exactly one of the three content
## paths is visible at a time — that is what bounds the card's height.
func _render_subject_drawer() -> void:
    if _selected_subject == SUBJECT_LAND:
        _render_land_drawer()
    else:
        _render_occupant_drawer()
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject. This is the SNAPSHOT path
    # (`reapply_selection` → here, every turn), and it must NOT close the sheet — closing would make
    # it unusable under autoplay (§15). A SELECTION change has already closed the sheet by the time it
    # reaches here, so this is a no-op there.
    _refresh_compose_sheet()
    _fit_subject_drawer()

## The LAND drawer: the terrain rows + the "Assign foragers" compose block (the land's only action).
## On a hex the player cannot see it also carries the unknown-contents statement — see below.
func _render_land_drawer() -> void:
    if tile_detail == null:
        return
    tile_detail.visible = true
    tile_detail.text = _format_detail_bbcode(_tile_terrain_lines(_selected_tile_info))
    _build_forage_drawer_actions(_selected_tile_info)
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false
    _render_unknown_contents_note()

## An EMPTY occupant list is a claim of emptiness the client cannot back up, so on a hex the player
## cannot see the list carries the land row and nothing else, and the drawer says so out loud. This
## is the whole point of the fog gate — silence would read as "nothing here".
##
## Skipped when the list DOES carry occupant rows: that only happens for your own party on an
## unseen hex, and `_rebuild_subject_list` already appends `OCCUPANTS_UNSEEN_OTHERS_HINT` there.
func _render_unknown_contents_note() -> void:
    if occupant_detail == null:
        return
    var unseen := _tile_contents_unseen(_selected_tile_info)
    var roster_empty := _roster_units.is_empty() and _roster_herds.is_empty()
    if not unseen or not roster_empty:
        occupant_detail.visible = false
        occupant_detail.text = ""
        return
    occupant_detail.visible = true
    var message := OCCUPANTS_UNKNOWN_UNEXPLORED \
        if String(_selected_tile_info.get("visibility_state", "")) == VISIBILITY_UNEXPLORED \
        else OCCUPANTS_UNKNOWN_REMEMBERED
    occupant_detail.text = _format_detail_bbcode([message])

## Cap the drawer against the room left in the dock beneath the card, so a crowded hex scrolls
## INSIDE the drawer rather than dragging the whole dock.
##
## WAITS A WHOLE FRAME, not just `call_deferred`, and that is load-bearing. The drawer's content
## height is a function of its WIDTH — the detail label wraps, and the card's width is itself set by
## whichever compose block is showing — so a measurement taken before the new subject has been laid
## out reports the PREVIOUS subject's wrapping. On a card that just got narrower that under-reports
## the height and the drawer caps short with a scrollbar over content that would have fit. A
## deferred call is flushed inside the same frame and is not enough; one `process_frame` is.
## Coalesced, so the render + the body's own `minimum_size_changed` collapse into one fit.
func _fit_subject_drawer() -> void:
    if subject_scroll == null or subject_body == null or _subject_fit_pending:
        return
    _subject_fit_pending = true
    await get_tree().process_frame
    _subject_fit_pending = false
    if subject_scroll == null or subject_body == null:
        return
    DockScrollFit.fit_height(
        subject_scroll,
        subject_body.get_combined_minimum_size().y,
        left_dock_scroll,
        SUBJECT_DRAWER_MIN_HEIGHT,
        SUBJECT_DRAWER_BOTTOM_MARGIN,
    )


## The LAND DRAWER's rows: only what a CHIP CANNOT CARRY.
##
## The pinned chip strip above the list already states this tile's standing condition — Sight,
## Habitability, Climate, Tags, Site — so printing those as rows here restated the strip verbatim,
## and `Biome` restated the land ROW's own label (the "no restated identity" rule,
## docs/plan_tile_panel_layout.md §8). The chips REPLACE those rows; what is left is the numbers and
## the stocks, whose subject is the land: Height · the rivers · Pasture · Forage · the patch's
## biomass/ecology · the two build meters — plus the FoW sentences, which are statements, not
## conditions, and have no chip.
##
## `_render_land_drawer` is the ONE caller (the map hover tooltip builds its own text in
## `show_tooltip`), so the trim is local to the drawer.
func _tile_terrain_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    # Fog of War: never-seen tiles reveal nothing; remembered (Discovered) tiles
    # show only their last-known terrain, not current contents. See MapView
    # _apply_visibility_to_info, which redacts the hidden fields before this runs.
    # The Sight CHIP states which of the three states this hex is in; the sentence says what that
    # costs you, which is the part a chip cannot carry.
    var visibility_state := String(tile_info.get("visibility_state", ""))
    if visibility_state == VISIBILITY_UNEXPLORED:
        lines.append("Not yet scouted — send a band to reveal this area.")
        return lines
    if tile_info.has("height_display"):
        lines.append("Height: %s" % String(tile_info["height_display"]))
    # Hex-edge rivers — which SIDES of this tile carry water (the sides a crossing cost will
    # apply to). Terrain-intrinsic permanent geography, so it renders before the discovered
    # early-return, like Pasture below. Guarded on the key so a rehydrated snapshot
    # degrades to no row instead of a wrong one; RiverEdges returns [] on a riverless tile, so it
    # never emits an empty "River:" label. Same formatter the map hover tooltip uses.
    if tile_info.has("river_edges"):
        lines.append_array(RiverEdges.summary_lines(int(tile_info["river_edges"])))
    # (A discovered Wondrous Site is a standing condition of the ground — it rides the chip strip.)
    # PASTURE — the animal-edible stock (see PASTURE_KEY). Surfaced BEFORE the discovered
    # early-return because, like the biome on the land row and the habitability chip, grass is a property of the
    # GROUND: you can read a steppe from a ridge, and a remembered tile already remembers its biome.
    # (What a remembered tile redacts is live CONTENTS — the bands and herds standing on it.) Only
    # when the ground carries pasture at all, so a glacier prints nothing rather than "0 / 0".
    var graze_capacity := float(tile_info.get("graze_capacity", 0.0))
    if graze_capacity > 0.0:
        lines.append("%s: %.0f / %.0f" % [
            PASTURE_KEY, float(tile_info.get("graze_biomass", 0.0)), graze_capacity
        ])
        var graze_phase := String(tile_info.get("graze_ecology_phase", "")).strip_edges().to_lower()
        if graze_phase != "":
            lines.append("%s: %s" % [PASTURE_ECOLOGY_KEY, _ecology_phase_label(graze_phase)])
    if visibility_state == VISIBILITY_DISCOVERED:
        lines.append("Last seen — information incomplete. Scout to update.")
        return lines
    var food_label := String(tile_info.get("food_module_label", "None")).strip_edges()
    if food_label == "":
        food_label = "None"
    var food_kind := String(tile_info.get("food_kind", "")).strip_edges()
    var food_line := "Forage: %s" % food_label
    if food_kind != "":
        food_line = "%s — %s" % [food_line, _format_food_kind_label(food_kind)]
    # NOTE: the module's `seasonal_weight` is deliberately NOT printed — it is an internal
    # yield coefficient, meaningless to the player (it still drives the sim's yield math).
    lines.append(food_line)
    # WHAT GROWS HERE — the named plants behind the Forage line above (flora roster F1). It reads
    # directly under the module because it says what that module's basket IS; the stock/ecology rows
    # below then say how much of it there is and how it is faring.
    var composition_text := _flora_composition_text(tile_info.get("patch_composition", []))
    if composition_text != "":
        lines.append("%s: %s" % [FLORA_COMPOSITION_ROW, composition_text])
    # Standing forage stock vs the patch's ceiling — the patch counterpart to a herd's "Biomass"
    # row, so a foraged patch reads like wild game does ("how much there is"). Foraging draws the
    # biomass down and it regrows logistically toward the capacity. Only rendered when the snapshot
    # carries a real patch (capacity > 0), so a plain food-module tile with no patch stays bare.
    var patch_capacity := float(tile_info.get("patch_carrying_capacity", 0.0))
    if patch_capacity > 0.0:
        lines.append("Forage biomass: %.0f / %.0f" % [float(tile_info.get("patch_biomass", 0.0)), patch_capacity])
    # Ecology phase of the patch — ALWAYS shown for any tile carrying a patch (not just a
    # cultivated one): the phase gates whether cultivation can accrue at all, so it is the
    # single most important condition on a forage tile. Same row name / label / tint as the
    # herd's Ecology row (`_ecology_phase_label` + `_ecology_value_hex`), so a stressed patch
    # and a stressed herd read identically.
    var patch_phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if patch_phase != "":
        lines.append("Ecology: %s" % _ecology_phase_label(patch_phase))
    # Forage-patch intensification ladder: while a patch is being tended it shows the
    # cultivation progress; once cultivated it reads as a "Tended Patch" (SIGNAL tint).
    # Mirrors the herd Husbandry row. Only when the snapshot carries the field so we
    # never invent a state on a patch that isn't being worked.
    if bool(tile_info.get("is_cultivated", false)):
        lines.append("Cultivation: %s" % _cultivation_label(1.0, true))
    elif tile_info.has("cultivation_progress"):
        var cultivation_progress := float(tile_info["cultivation_progress"])
        if cultivation_progress > 0.0:
            lines.append("Cultivation: %s" % _cultivation_label(cultivation_progress, false))
    # PLANT RUNG 3 — the Field, on its OWN row beside Cultivation. The patch carries TWO independent
    # build meters (a Field may stand on ground that was never tended: seed travels, so `Sow` needs no
    # prior patch), so they are two rows, never one merged "progress" number. This is the per-source
    # half of the two-meter split (§4.1) — the FACTION's Seed Selection knowledge is NOT shown here;
    # it lives in the top-bar knowledge strip, because it is a property of your people, not of this
    # ground. Both rows are the source's own, and both decay if the patch is abandoned.
    if bool(tile_info.get("patch_is_field", false)):
        lines.append("%s: %s" % [FIELD_ROW, _field_label(1.0, true)])
    elif tile_info.has("patch_field_progress"):
        var field_progress := float(tile_info["patch_field_progress"])
        if field_progress > 0.0:
            lines.append("%s: %s" % [FIELD_ROW, _field_label(field_progress, false)])
    return lines

# ---- The subject list ------------------------------------------------------

## Rebuild the subject rows: the LAND first (no group header — it is not one of a group), then a
## `Bands (N)` sub-group and a `Wildlife (N)` sub-group, each a dim uppercase header + one
## selectable row per occupant. The row matching the current selection is styled as selected.
func _rebuild_subject_list() -> void:
    if subject_list == null:
        return
    for child in subject_list.get_children():
        child.queue_free()
    if not _selected_tile_info.is_empty():
        subject_list.add_child(_build_land_row(_selected_tile_info))
    if not _roster_units.is_empty():
        subject_list.add_child(_roster_group_header("Bands", _roster_units.size()))
        for unit in _roster_units:
            subject_list.add_child(_build_band_row(unit))
    if not _roster_herds.is_empty():
        subject_list.add_child(_roster_group_header("Wildlife", _roster_herds.size()))
        for herd in _roster_herds:
            subject_list.add_child(_build_herd_row(herd))
    # Reached only when your OWN unit is on a hex you can't see (everything else was redacted): say so,
    # or the lone row would read as "and nothing else is here" — which we cannot know.
    if _tile_contents_unseen(_selected_tile_info) and not (_roster_units.is_empty() and _roster_herds.is_empty()):
        subject_list.add_child(_alloc_hint_label(OCCUPANTS_UNSEEN_OTHERS_HINT))

## The LAND row — the same shape as a band/herd row, because the land is the same KIND of thing:
## a subject on this hex you can put workers on.
##
## Label = the BIOME name (more informative than a generic "The land", and it leaves the card title
## as the coordinates). Glyph = the tile's food-module icon where it carries one — the SAME icon the
## map marker draws, so a source reads identically in the panel and on the map — else the neutral
## `◈`. Dot = the patch's ecology tier, the same vitality vocabulary as the band/herd dots.
func _build_land_row(tile_info: Dictionary) -> Button:
    var selected := _selected_subject == SUBJECT_LAND
    var patch_phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges()
    var dot_color := _ecology_tier_color(patch_phase) if patch_phase != "" else HudStyle.INK_FAINT
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    var glyph := LAND_ROW_GLYPH
    if module_key != "":
        glyph = FoodIcons.for_site(module_key, false, int(tile_info.get("terrain_id", -1)))
    var button := _make_roster_button(selected)
    var row := _make_roster_row(selected, dot_color)
    var terrain_label := String(tile_info.get("terrain_label", "Unknown"))
    row.add_child(_roster_name_label("%s %s" % [glyph, terrain_label], selected))
    var meta := _land_row_meta(tile_info)
    if meta != "":
        row.add_child(_roster_meta_label(meta))
    button.add_child(row)
    button.pressed.connect(_on_land_row_selected)
    return button

## The land row's meta, shortest true form: the foragers on this hex · else that the patch is
## unworked · else that there is nothing to gather here.
func _land_row_meta(tile_info: Dictionary) -> String:
    var workers := _forage_workers_on_tile(int(tile_info.get("x", -1)), int(tile_info.get("y", -1)))
    # Gated on the module KEY, never on its label: a tile with no module still ships the label
    # `"None"`, which would render as a source called "None" instead of the honest "No forage".
    if workers > 0 or String(tile_info.get("food_module", "")).strip_edges() != "":
        # An UNWORKED patch reads `0 🌾`, not its module label. The row already LEADS with that
        # module's own glyph, so the label restated it — and at dock width the row was the ONE
        # place the name truncated (`Savanna Gras…`) while the drawer's `Forage:` row and the
        # compose sheet's header both printed it whole. The zero form is parallel to the staffed
        # one, so "nobody is working this" reads at a glance instead of needing a comparison.
        return LAND_META_WORKERS_FORMAT % [workers, ACTIVITY_GLYPHS[LABOR_KIND_FORAGE]]
    return LAND_META_NO_FORAGE

## Foragers this faction has on (x, y), summed across every player band — the row states the hex's
## staffing, not one band's share of it.
func _forage_workers_on_tile(x: int, y: int) -> int:
    if x < 0 or y < 0:
        return 0
    var total := 0
    var bands: Array = _player_bands if not _player_bands.is_empty() else [_player_band]
    for band_variant in bands:
        if band_variant is Dictionary and not (band_variant as Dictionary).is_empty():
            total += _workers_for_forage(band_variant, x, y)
    return total

## The land row was clicked. It emits `roster_occupant_selected` with the THIRD kind, `"land"` (an
## additive kind on the existing `(kind, id)` contract — no id, so `LAND_SUBJECT_ID`), because
## MapView holds its OWN occupant selection: picking a band there clears the herd, and picking the
## land must clear both. There is still no map ring to move — the hex outline already marks the tile
## and `selected_tile` is untouched — but without this the next snapshot's
## `refresh_selection_payload` keeps answering `kind: "unit"` off the stale `selected_unit_id` and
## restores the band, which made the land unselectable on any occupied hex.
func _on_land_row_selected() -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _subject_choice_tile = _selected_tile_coords()
    _selected_unit = {}
    _selected_herd = {}
    _selected_subject = SUBJECT_LAND
    _selected_band_food_turns = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _rebuild_subject_list()
    _render_subject_drawer()
    emit_signal("roster_occupant_selected", SUBJECT_LAND, LAND_SUBJECT_ID)

func _roster_group_header(title: String, count: int) -> Label:
    var label := Label.new()
    label.text = "%s (%d)" % [title.to_upper(), count]
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ROSTER_HEADER_FONT_SIZE)
    return label

## One selectable band row. A Button (row click) hosts a mouse-transparent HBox
## laying out: a selection accent, a vitality dot (BandFoodStatus color for a
## player band, neutral for others), the name, the size, and an activity glyph.
func _build_band_row(unit: Dictionary) -> Button:
    var entity_id := int(unit.get("entity", -1))
    var is_player := _is_player_unit(unit)
    var selected := not _selected_unit.is_empty() and int(_selected_unit.get("entity", -1)) == entity_id
    # Neutral tint for a non-player band's vitality dot (we can't see their larder).
    var dot_color := HudStyle.INK_FAINT
    var glyph := ""
    if is_player:
        dot_color = BandFoodStatus.color_for_turns(float(unit.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS)))
        glyph = _activity_glyph(String(unit.get("activity", "")))
    var button := _make_roster_button(selected)
    var row := _make_roster_row(selected, dot_color)
    row.add_child(_roster_name_label(String(unit.get("id", "Band")), selected))
    row.add_child(_roster_meta_label(str(int(unit.get("size", 0)))))
    if glyph != "":
        row.add_child(_roster_glyph_label(glyph, String(unit.get("activity", "")) == BAND_ACTIVITY_IDLE))
    # Surface the data-driven settlement-stage label (e.g. "Nomadic band") on hover; omit when
    # the band has no resolved stage (pre-stage / missing snapshot).
    var stage_label := String(unit.get("settlement_stage_label", "")).strip_edges()
    if stage_label != "":
        button.tooltip_text = stage_label
    button.add_child(row)
    button.pressed.connect(_on_roster_row_selected.bind("unit", entity_id))
    return button

## One selectable wildlife row: an ecology-tier dot, the species glyph + name, and — as the meta —
## the hunters on it. Selecting it drives the drawer + the map ring to the herd.
func _build_herd_row(herd: Dictionary) -> Button:
    var herd_id := String(herd.get("id", ""))
    var selected := not _selected_herd.is_empty() and String(_selected_herd.get("id", "")) == herd_id
    var dot_color := _ecology_tier_color(String(herd.get("ecology_phase", "")))
    var button := _make_roster_button(selected)
    var row := _make_roster_row(selected, dot_color)
    var label := String(herd.get("label", herd.get("id", "Herd")))
    var glyph := FoodIcons.for_herd(label)
    var name_text := String(herd.get("species", label))
    row.add_child(_roster_name_label("%s %s" % [glyph, name_text], selected))
    # The fauna id (`game_fowl_27`) is a DATABASE KEY, not player-facing text: it is the handle the
    # code addresses this herd with (the `pressed` bind below, and every `assign_labor`/`tame`/
    # `send_hunt_expedition` command), so it stays as DATA and never as a rendered label. The row
    # shows the species and, as its meta, how many hunters are on it; the size class reads in the
    # drawer.
    var meta := _herd_row_meta(herd)
    if meta != "":
        row.add_child(_roster_meta_label(meta))
    button.tooltip_text = label
    button.add_child(row)
    button.pressed.connect(_on_roster_row_selected.bind("herd", herd_id))
    return button

## The herd row's meta — the deliberate twin of `_land_row_meta`'s rule: a workable source states
## its staffing, anything else states nothing. A huntable herd with nobody on it reads `0 🏹`,
## parallel to the staffed form (and to the land row's `0 🌾`), so "nobody is working this" reads at
## a glance. A NON-huntable herd is not a source at all, so it earns no meta — exactly as a
## module-less tile earns no worker meta.
func _herd_row_meta(herd: Dictionary) -> String:
    var herd_id := String(herd.get("id", "")).strip_edges()
    var workers := _hunt_workers_on_herd(herd_id)
    if workers > 0 or bool(herd.get("huntable", false)):
        return HERD_META_WORKERS_FORMAT % [workers, ACTIVITY_GLYPHS[LABOR_KIND_HUNT]]
    return ""

## Hunters this faction has on `herd_id`, summed across BOTH ways a herd can be worked: standing
## local hunts assigned by any player band, and detached hunting expeditions committed to it (in
## whatever phase — a party en route to a herd is hunting it). The row states the herd's TOTAL
## staffing, not one band's or one mechanism's share of it — the same rule
## `_forage_workers_on_tile` documents for a hex.
func _hunt_workers_on_herd(herd_id: String) -> int:
    if herd_id == "":
        return 0
    var total := 0
    var bands: Array = _player_bands if not _player_bands.is_empty() else [_player_band]
    for band_variant in bands:
        if band_variant is Dictionary and not (band_variant as Dictionary).is_empty():
            total += _workers_for_hunt(band_variant, herd_id)
    for exp_variant in _player_expeditions:
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if String(exp.get("expedition_mission", "")).strip_edges().to_lower() != EXPEDITION_MISSION_HUNT:
            continue
        if String(exp.get("expedition_target_herd", "")).strip_edges() != herd_id:
            continue
        total += int(exp.get("size", 0))
    return total

## A roster row's clickable Button shell: selected rows read as "primary", others
## as "ghost". Toggle_mode is off — selection is driven by a rebuild, not the
## button's own toggle state, so re-clicking the selected row can't un-highlight it.
func _make_roster_button(selected: bool) -> Button:
    var button := Button.new()
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(0, ROSTER_ROW_MIN_HEIGHT)
    HudStyle.apply_button(button, "primary" if selected else "ghost")
    return button

## The mouse-transparent HBox overlaying a roster button, anchored to fill it,
## carrying the left selection accent + the vitality/ecology dot.
func _make_roster_row(selected: bool, dot_color: Color) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.set_anchors_preset(Control.PRESET_FULL_RECT)
    row.offset_left = ROSTER_ROW_H_PADDING
    row.offset_right = -ROSTER_ROW_H_PADDING
    row.add_theme_constant_override("separation", ROSTER_ROW_SEPARATION)
    var accent := ColorRect.new()
    accent.custom_minimum_size = Vector2(ROSTER_ACCENT_WIDTH, 0)
    accent.color = HudStyle.SIGNAL if selected else Color(0, 0, 0, 0)
    accent.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(accent)
    var dot := ColorRect.new()
    dot.custom_minimum_size = Vector2(ROSTER_DOT_SIZE, ROSTER_DOT_SIZE)
    dot.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    dot.color = dot_color
    dot.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(dot)
    return row

## The row's IDENTITY — never elastic, never truncated. It takes its natural width and the meta
## beside it absorbs whatever is left (see `_roster_meta_label`).
func _roster_name_label(text: String, selected: bool) -> Label:
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK if selected else HudStyle.INK_DIM)
    return label

func _roster_meta_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    # The META is the row's ELASTIC, EXPENDABLE half: it claims the slack after the name (hence the
    # right alignment the rows have always read with) and, when the row runs out of width in a 320px
    # dock, it is the meta that gives — ellipsised, not hard-cut, and never the name, which is the
    # row's identity. Free for the short band/herd metas ("120", "1 🏹"); it is the land row's
    # long module label that would otherwise push past the card's edge, and that label also reads in
    # full in the drawer.
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    label.clip_text = true
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    return label

func _roster_glyph_label(glyph: String, dim: bool) -> Label:
    var label := Label.new()
    label.text = glyph
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT if dim else HudStyle.INK_DIM)
    return label

func _activity_glyph(activity: String) -> String:
    return String(ACTIVITY_GLYPHS.get(activity.strip_edges().to_lower(), ACTIVITY_GLYPHS[BAND_ACTIVITY_IDLE]))

## Shared green/amber/red tier for a herd's ecology phase, matching the band
## food dot so map/roster/drawer agree: thriving→green, stressed→amber,
## collapsing→red. Matched on the phase stems from `EcologyPhase::as_str`.
func _ecology_tier_color(phase: String) -> Color:
    var normalized := phase.strip_edges().to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER
    if normalized.contains("stress"):
        return HudStyle.WARN
    return HudStyle.HEALTHY

func _find_roster_unit(entity_id: int) -> Dictionary:
    for unit in _roster_units:
        if unit is Dictionary and int((unit as Dictionary).get("entity", -1)) == entity_id:
            return unit
    return {}

func _find_roster_herd(herd_id: String) -> Dictionary:
    if herd_id == "":
        return {}
    for herd in _roster_herds:
        if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
            return herd
    return {}

## A roster row was clicked: make it the selected occupant, refresh the cards, and
## notify the map so the selection ring follows.
func _on_roster_row_selected(kind: String, id: Variant) -> void:
    _select_roster_occupant(kind, id)
    emit_signal("roster_occupant_selected", kind, id)

func _select_roster_occupant(kind: String, id: Variant) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _subject_choice_tile = _selected_tile_coords()
    if kind == "unit":
        _selected_unit = _find_roster_unit(int(id)).duplicate(true)
        _selected_herd = {}
        _selected_subject = SUBJECT_UNIT
    else:
        _selected_herd = _find_roster_herd(String(id)).duplicate(true)
        _selected_unit = {}
        _selected_subject = SUBJECT_HERD
    _selected_band_food_turns = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _rebuild_subject_list()
    _render_subject_drawer()

## The detail drawer + action buttons for the currently-selected occupant. Shares the one drawer
## with the land, so it hides the land's content first — exactly one subject fills it.
func _render_occupant_drawer() -> void:
    if occupant_detail == null:
        return
    if tile_detail != null:
        tile_detail.visible = false
    if forage_assign_controls != null:
        forage_assign_controls.visible = false
    _selected_band_food_turns = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    var is_band := not _selected_unit.is_empty()
    var is_herd := not _selected_herd.is_empty()
    var is_expedition := is_band and bool(_selected_unit.get("is_expedition", false))
    var is_player_band := is_band and not is_expedition and _is_player_unit(_selected_unit)
    # A selected player band is the panel's subject: its detail + labor allocation render into the
    # dockable Band/City panel (docs/plan_band_city_dock.md §3), and the Occupants card shows NO
    # band detail (the roster still lists it). Falls back to the legacy in-card drawer only when no
    # panel is injected (e.g. the HUD-only ui_preview harness).
    if is_player_band and _band_city_panel != null:
        _render_band_into_panel(_selected_unit)
        # The drawer is now VISIBLE furniture rather than a hidden card, so an empty one reads as a
        # rendering fault. Point at where the band's detail actually went instead of leaving a gap.
        occupant_detail.visible = true
        occupant_detail.text = _format_detail_bbcode([BAND_PANEL_POINTER_TEXT])
        # The one order that stays HERE (§18): repositioning is a map action. Player resident bands
        # only — this branch is already player-band-gated, and a foreign band's orders aren't ours.
        _build_band_move_actions()
        if herd_assign_controls != null:
            herd_assign_controls.visible = false
        return
    # Herd / expedition / non-player band (or no-panel fallback) → the Occupants card drawer,
    # unchanged. Expedition → Recall/Move panel; player band (fallback) → allocation panel; herd →
    # assign-hunters controls. All mutually exclusive with the current selection.
    occupant_detail.visible = true
    var lines: Array[String] = []
    if not _selected_unit.is_empty():
        lines = _unit_summary_lines(_selected_unit)
    elif not _selected_herd.is_empty():
        lines = _herd_summary_lines(_selected_herd)
    occupant_detail.text = _format_detail_bbcode(lines)
    if is_expedition:
        _build_expedition_panel(_selected_unit)
    elif is_player_band:
        _build_allocation_panel(_selected_unit)
    elif allocation_panel != null:
        allocation_panel.visible = false
    if is_herd:
        _build_herd_drawer_actions(_selected_herd)
    elif herd_assign_controls != null:
        herd_assign_controls.visible = false

## Render a player band's detail + labor allocation into the dockable Band/City panel and
## populate its header/cycler. The single place the panel's subject is set — shared by roster/map
## selection (`_render_occupant_drawer`) and the per-snapshot refresh (`_refresh_panel_band`), so
## the panel is a persistent command center that survives selection changes.
func _render_band_into_panel(unit: Dictionary) -> void:
    if _band_city_panel == null or unit.is_empty():
        return
    # A quarry is chosen FOR a band (its travel time and useful party size are band-relative), so the
    # cycler swapping the panel subject must not carry one across.
    if int(unit.get("entity", -1)) != int(_panel_band.get("entity", -1)):
        _send_party_quarry_id = ""
    # DEEP-COPY the subject: `_panel_band` must NOT alias `_selected_unit` (the selection
    # path passes it in), because selecting a foreign tile calls `_selected_unit.clear()` —
    # which would empty a shared dict and blank the panel on its next stepper rebuild. The
    # zone closures below also capture this stable copy, so they keep targeting the panel band
    # regardless of the current selection.
    _panel_band = unit.duplicate(true)
    # The vitals RichTextLabel rebuilds the food/morale/output tint context from scratch each render.
    _selected_band_food_turns = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    # The three zone contents. Ownership passes to the panel, which frees the previous render's zones
    # and parents these into whichever shell (wide columns / narrow tabs) its width selected.
    _band_city_panel.set_zones(
        _wrap_zone(_build_band_zone_content(_panel_band)),
        _wrap_zone(_build_work_zone_content(_panel_band)),
        _wrap_zone(_build_parties_zone_content(_panel_band)))
    _push_zone_badges(_panel_band)
    # Header: settlement stage + name + stage label. The stage `id` is the panel's sprite key
    # (bundled art), the `icon` its emoji fallback for a stage with no art; both already flow
    # onto the marker/cohort dict. A missing stage falls back to a neutral glyph.
    var stage_id := String(_panel_band.get("settlement_stage_id", "")).strip_edges()
    var glyph := String(_panel_band.get("settlement_stage_icon", "")).strip_edges()
    var stage_label := String(_panel_band.get("settlement_stage_label", "")).strip_edges()
    var index := _index_of_player_band(int(_panel_band.get("entity", -1)))
    _band_city_panel.set_header(stage_id, glyph, _band_display_name(_panel_band, index + 1), stage_label)
    _band_city_panel.set_cycler(index, _player_bands.size())
    # `set_zones` above already flipped the panel to band-present; just make sure it is shown.
    _band_city_panel.set_shown(true)

## The expedition's sim phase key, normalized (the wire's `ExpeditionPhase` string).
func _expedition_phase_key(exp: Dictionary) -> String:
    return String(exp.get("expedition_phase", "")).strip_edges().to_lower()

## The phase as it renders ON the row: the glyph alone, except `awaiting`, which keeps its words
## (`▮▮ Awaiting orders`) — a demand on the player must read without a hover.
func _expedition_phase_suffix(phase: String) -> String:
    var suffix := _row_glyph_suffix(FoodIcons.for_status(phase))
    if phase == EXPEDITION_PHASE_AWAITING:
        return "%s %s" % [suffix, _expedition_phase_label(phase)]
    return suffix

## The row's hover text: everything the glyphs encode, in words — the mission, the hunt policy's
## behaviour hint, the phase + what it means, and the click affordance.
func _expedition_row_tooltip(exp: Dictionary, phase: String) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var policy_hint := ""
    if mission == EXPEDITION_MISSION_HUNT:
        var policy := String(exp.get("expedition_hunt_policy", "")).strip_edges().to_lower()
        policy_hint = String(SEND_HUNT_POLICY_HINTS.get(policy, ""))
    return _join_tooltip_lines([
        _expedition_mission_label(mission), policy_hint,
        _status_tooltip_line(phase), EXPEDITION_ROW_FOCUS_HINT])

## Compact one-line expedition summary: hunt → `🏹 <herd> · <Policy>  <phase glyph>`;
## scout → `⚑ → (x, y)  <phase glyph>`. Policy AND phase read as GLYPHS here exactly as they do on the
## Current-actions rows (one concept, one rendering, in both sections of the same panel); the words
## live in the tooltip. A scout has no policy → `for_policy` returns "" → `_row_glyph_suffix` emits
## nothing, so the row carries the phase glyph alone with no orphaned separator. Only `awaiting` keeps
## its words (`_expedition_phase_suffix`).
func _panel_expedition_summary(exp: Dictionary) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var phase_suffix := _expedition_phase_suffix(_expedition_phase_key(exp))
    var policy_suffix := _row_glyph_suffix(
        FoodIcons.for_policy(String(exp.get("expedition_hunt_policy", ""))))
    if mission == EXPEDITION_MISSION_HUNT:
        var herd := _herd_label_for_id(String(exp.get("expedition_target_herd", "")).strip_edges())
        return "%s %s%s%s" % [PANEL_EXPEDITION_HUNT_GLYPH, herd, policy_suffix, phase_suffix]
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    return "%s → (%d, %d)%s%s" % [
        PANEL_EXPEDITION_SCOUT_GLYPH, x, y, policy_suffix, phase_suffix]

## The expedition's OBJECTIVE in words — the herd it follows (hunt) or the tile it is parked on
## (scout) — the "where do I have to go / what is this about" half of an attention row's context.
func _expedition_objective(exp: Dictionary) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    if mission == EXPEDITION_MISSION_HUNT:
        return _herd_label_for_id(String(exp.get("expedition_target_herd", "")).strip_edges())
    return ATTENTION_TILE_FORMAT % [int(exp.get("current_x", -1)), int(exp.get("current_y", -1))]

## Turn-orb attention items for every expedition parked in `awaiting` (Producer 4). ONE ROW PER
## PARTY — each is its own decision with its own place to go (unlike idle workers, which are
## genuinely one aggregate per band) — capped at ATTENTION_AWAITING_MAX_ROWS, with the remainder
## folded into a single overflow row that jumps to the first party beyond the cap (so even the
## aggregate row is actionable rather than a dead "Open ▸" stub).
func _awaiting_orders_attention(expeditions: Array) -> Array:
    var awaiting: Array = []
    for exp_variant in expeditions:
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if _expedition_phase_key(exp) == EXPEDITION_PHASE_AWAITING:
            awaiting.append(exp)
    var items: Array = []
    for i in awaiting.size():
        var exp: Dictionary = awaiting[i]
        var x := int(exp.get("current_x", -1))
        var y := int(exp.get("current_y", -1))
        if i >= ATTENTION_AWAITING_MAX_ROWS:
            # Overflow: one aggregate row for the rest, locating to this (the first uncapped) party.
            items.append({
                "kind": ATTENTION_KIND_AWAITING_ORDERS,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": ATTENTION_AWAITING_OVERFLOW_LABEL_FORMAT % (awaiting.size() - i),
                "detail": ATTENTION_AWAITING_OVERFLOW_DETAIL,
                "x": x, "y": y,
            })
            break
        items.append({
            "kind": ATTENTION_KIND_AWAITING_ORDERS,
            "severity": ATTENTION_SEVERITY_WARN,
            # The demand headline reuses the phase words ("Awaiting orders"); the context line names
            # the mission + its objective, so the row is actionable without opening anything.
            "label": _expedition_phase_label(EXPEDITION_PHASE_AWAITING),
            "detail": ATTENTION_AWAITING_DETAIL_FORMAT % [
                _expedition_mission_label(String(exp.get("expedition_mission", ""))),
                _expedition_objective(exp)],
            "x": x, "y": y,
        })
    return items

## The awaiting expedition standing on (x, y), or {} — lets the orb's Jump reuse the panel's own
## expedition-focus path instead of a second, weaker one (see `_on_turn_orb_focus`).
## Turn-orb attention items for the STARVING PENS one band keeps (Producer 5). One row per pen — a
## pen is a distinct 25-turn investment with its own herd, its own tile and its own fed fraction, so
## (unlike idle workers) there is nothing meaningful to aggregate. Driven by `PenStatus`, the same
## test the herd drawer and the map badge ask, so the three surfaces cannot disagree.
##
## The pens are found through the band's OWN Corral labor assignments: the client has no owner field
## on a herd, so scanning `_world_herds` would happily alarm on a RIVAL's starving pen.
func _starving_pen_attention(band: Dictionary) -> Array:
    var items: Array = []
    for a_variant in _labor_assignments_of(band):
        if not (a_variant is Dictionary):
            continue
        var a: Dictionary = a_variant
        if String(a.get("kind", "")).to_lower() != LABOR_KIND_HUNT:
            continue
        if String(a.get("policy", "")).to_lower() != LABOR_POLICY_CORRAL:
            continue
        var herd_id := String(a.get("fauna_id", ""))
        var herd := _find_world_herd(herd_id)
        if herd.is_empty() or not PenStatus.herd_is_starving(herd):
            continue
        var fed := PenStatus.fed_fraction(herd)
        items.append({
            "kind": ATTENTION_KIND_STARVING_PEN,
            "severity": ATTENTION_SEVERITY_WARN,
            "label": ATTENTION_PEN_LABEL_FORMAT % _herd_label_for_id(herd_id),
            "detail": ATTENTION_PEN_DETAIL_FORMAT % int(round(fed * PROGRESS_PERCENT_SCALE)),
            # The HERD's live tile — a penned herd is pinned, but the jump must still land on the
            # animals (that is where the drawer with the fed fraction and the feed cost opens),
            # not on the keeper band.
            "x": int(herd.get("x", -1)), "y": int(herd.get("y", -1)),
        })
    return items

## The starving pen (if any) standing on `(x, y)`, for the orb's jump routing — the herd twin of
## `_awaiting_expedition_at`. Only pens the player's own bands keep, via the same producer path.
func _starving_pen_at(x: int, y: int) -> String:
    for band_variant in _player_bands:
        if not (band_variant is Dictionary):
            continue
        for a_variant in _labor_assignments_of(band_variant):
            if not (a_variant is Dictionary):
                continue
            var a: Dictionary = a_variant
            if String(a.get("kind", "")).to_lower() != LABOR_KIND_HUNT:
                continue
            if String(a.get("policy", "")).to_lower() != LABOR_POLICY_CORRAL:
                continue
            var herd_id := String(a.get("fauna_id", ""))
            var herd := _find_world_herd(herd_id)
            if herd.is_empty() or not PenStatus.herd_is_starving(herd):
                continue
            if int(herd.get("x", -1)) == x and int(herd.get("y", -1)) == y:
                return herd_id
    return ""

func _awaiting_expedition_at(x: int, y: int) -> Dictionary:
    for exp_variant in _player_expeditions:
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if _expedition_phase_key(exp) != EXPEDITION_PHASE_AWAITING:
            continue
        if int(exp.get("current_x", -1)) == x and int(exp.get("current_y", -1)) == y:
            return exp
    return {}

## Select an expedition (from the panel's Active-expeditions list) on the map: recenter + select
## its hex (rebuilds that hex's roster), then pin the exact expedition so the map ring moves and the
## Occupants card renders its expedition drawer. Mirrors `cycle_panel_band`'s routing. The Band/City
## panel itself stays on its band (expeditions detail in the Occupants card, per the existing split);
## a co-located band auto-select can't hijack it — we restore the panel band if it changed.
func _on_panel_expedition_selected(entity: int, x: int, y: int) -> void:
    var panel_band_keep: Dictionary = _panel_band.duplicate(true) if not _panel_band.is_empty() else {}
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _find_roster_unit(entity).is_empty():
        _select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    if not panel_band_keep.is_empty() and int(_panel_band.get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        _render_band_into_panel(panel_band_keep)

## A Current-actions row's label was clicked: show the source the band is working. Recenter + select
## its hex (`alert_focus_requested` → `MapView.focus_and_select_tile`) and, for a hunted herd, pin
## the herd itself (`roster_occupant_selected` → `MapView.select_occupant`) so its drawer opens on
## the herd rather than whatever occupant the hex auto-selects. This is exactly the routing the
## Active-expeditions rows and the turn-orb "Jump →" use — no new path. The Band/City panel stays on
## its band: focusing a hex that hosts another band would otherwise hijack the panel.
func _focus_labor_source(x: int, y: int, herd_id: String = "") -> void:
    if x < 0 or y < 0:
        return
    var panel_band_keep: Dictionary = _panel_band.duplicate(true) if not _panel_band.is_empty() else {}
    emit_signal("alert_focus_requested", x, y)
    # The focus above rebuilt the hex's roster, so the herd is resolvable now.
    if herd_id != "" and not _find_roster_herd(herd_id).is_empty():
        _select_roster_occupant("herd", herd_id)
        emit_signal("roster_occupant_selected", "herd", herd_id)
    if not panel_band_keep.is_empty() and int(_panel_band.get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        _render_band_into_panel(panel_band_keep)

## Show a hunted herd. Herds MIGRATE each turn, so the hunt assignment's `target_x/target_y` is a
## stale launch position: resolve the herd's LIVE tile from the snapshot herd list first, exactly as
## `MapView._draw_band_work_highlights` resolves the hunted-herd ring (`_herd_by_id`, falling back to
## the assignment target when the herd is unknown — e.g. it left the visible fauna set).
func _focus_hunt_source(herd_id: String, fallback_x: int, fallback_y: int) -> void:
    var herd := _find_world_herd(herd_id)
    var x := int(herd.get("x", fallback_x))
    var y := int(herd.get("y", fallback_y))
    _focus_labor_source(x, y, herd_id)

## Re-render the panel band into the panel container, keyed off `_panel_band` (never the current
## selection). The panel's own allocation rebuilds (optimistic pending, etc.) route through this so
## they stay pinned to the panel's subject even when a foreign hex is selected.
func _rerender_panel_allocation() -> void:
    if _band_city_panel == null or _panel_band.is_empty():
        return
    _render_band_into_panel(_panel_band)

## Keep the panel a live, persistent command center each snapshot: hide it when there are no
## player bands, else re-resolve the shown band against the fresh snapshot (so steppers/idle stay
## current) and re-render it. Called from update_band_alerts after _player_band(s) refresh.
func _refresh_panel_band() -> void:
    if _band_city_panel == null:
        return
    if _player_bands.is_empty():
        _panel_band = {}
        _band_city_panel.set_band_present(false)
        _band_city_panel.set_shown(false)
        return
    _render_band_into_panel(_resolve_panel_band())

## The band the panel should show: the same one across snapshots (re-fetched live by entity), or
## the first player band (the default actor) when the shown band is gone / unset.
func _resolve_panel_band() -> Dictionary:
    if not _panel_band.is_empty():
        var entity := int(_panel_band.get("entity", -1))
        for b in _player_bands:
            if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
                return b
    return _player_bands[0] if not _player_bands.is_empty() else {}

## Index of a band (by entity) within `_player_bands`, or -1 if absent.
func _index_of_player_band(entity: int) -> int:
    for i in range(_player_bands.size()):
        if int((_player_bands[i] as Dictionary).get("entity", -1)) == entity:
            return i
    return -1

## Injected by Main: the dockable Band/City panel the band drawer renders into.
## (The Food/Morale disclosure `meta_clicked` is wired per-render on the fresh summary RichTextLabel
## in `_render_band_into_panel`, since main's section-block model rebuilds that label each render.)
func set_band_city_panel(panel: BandCityPanel) -> void:
    _band_city_panel = panel
    # The panel re-reports its zone box on a shell flip / dock change / collapse / window resize.
    # Re-PAGE the work board on it — the other two zones are unaffected by a box change.
    if panel != null and not panel.zones_resized.is_connected(_on_zones_resized):
        panel.zones_resized.connect(_on_zones_resized)

## Walk to the next/prev player band (cycler ◀/▶). Routes through the SAME band-selection a roster
## click uses — recenter + select the band's hex (rebuilding that hex's roster), then pin the exact
## band — so the map ring, Tile card, roster, and this panel all land on the cycled band.
func cycle_panel_band(delta: int) -> void:
    if _band_city_panel == null or _player_bands.size() <= 1:
        return
    var idx := _index_of_player_band(int(_panel_band.get("entity", -1)))
    if idx < 0:
        idx = 0
    var n := _player_bands.size()
    var next_band: Dictionary = _player_bands[((idx + delta) % n + n) % n]
    _select_band_on_map(next_band)

## Jump to the panel band on the map (the header title is a "jump to my band" affordance): recenter
## + select its hex and move the ring, WITHOUT changing which band the panel shows (it's already
## `_panel_band`). No-op when there is no panel band.
func focus_panel_band() -> void:
    _select_band_on_map(_panel_band)

## Select a band's hex on the map — recenter + select the hex (rebuilding its roster) via
## `alert_focus_requested` (→ MapView.focus_and_select_tile) then pin the exact band so the map ring,
## Tile card, roster, and panel all agree. Shared by the cycler and the header "jump to band". A band
## with no live roster entry (no tile_info) is rendered directly into the panel instead.
func _select_band_on_map(band: Dictionary) -> void:
    if band.is_empty():
        return
    var entity := int(band.get("entity", -1))
    var x := int(band.get("current_x", -1))
    var y := int(band.get("current_y", -1))
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _find_roster_unit(entity).is_empty():
        _select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    else:
        _render_band_into_panel(band)

## Player-faction check for a roster/drawer band (mirrors MapView._is_player_unit).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", PLAYER_FACTION_ID)) == PLAYER_FACTION_ID

## The band summary rows. **No row here restates what its host's own header already shows.** Both
## hosts name the band above the detail — the Band/City dock in its panel header, the Occupants card
## in the band's roster row — and the roster row also carries the band's SIZE, so neither the
## `Unit: <name>` row nor the `Size: <n>` row survives.
## Nor does it state the population: the band zone's People + Workforce bars carry that, and the
## Occupants-card drawer has no worker breakdown to show for a band that isn't ours anyway.
func _unit_summary_lines(unit_data: Dictionary) -> Array[String]:
    if bool(unit_data.get("is_expedition", false)):
        return _expedition_summary_lines(unit_data)
    var lines: Array[String] = []
    # Disclosure carets + the tint context are rebuilt per render. Reset BOTH here, not inside
    # `_band_food_line` — a foreign band skips that call entirely (below), and a skipped Food row
    # must not inherit the previous render's caret or its food-turns tint.
    _disclosure_state = {}
    _food_flow_present = false
    _selected_band_food_turns = NAN
    # Food, like Morale below, is our OWN bands' business only. A rival's cohort carries no
    # `turns_of_food`/`stores` on the wire, so rendering the row for one printed a FABRICATED
    # `Food 0 (∞)` in healthy green — the UI claiming we'd counted a larder we cannot see. A foreign
    # band shows only what we can honestly observe from outside: where it is (Position) and roughly
    # how many (its roster row's size).
    if _is_player_unit(unit_data):
        lines.append(_band_food_line(unit_data))
        # Category-aggregated food breakdown under Food: a click-to-open disclosure. `_band_food_line`
        # set `_food_flow_present`; `_register_disclosure` stashes the rows for the popover and records
        # the row so `_format_detail_bbcode` draws the caret + clickable meta. The rows are NEVER
        # appended here — inline growth is what clipped the zone.
        if _food_flow_present:
            _register_disclosure(DETAIL_ROW_FOOD, BREAKDOWN_KIND_FOOD, unit_data,
                _food_breakdown_lines(unit_data))
    # Morale is our own bands' business only (a non-player band's morale isn't ours
    # to see); morale drives productivity + migration (a harsh tile erodes it until
    # people begin leaving), while deaths stay starvation/cold-driven.
    if _is_player_unit(unit_data):
        lines.append(_band_morale_line(unit_data))
        # Productivity ties visibly to morale: show the Output row when discontent is
        # dragging yield below full (near Morale, tinted by how low it is).
        var output_line := _band_output_line(unit_data)
        if output_line != "":
            lines.append(output_line)
        # Itemized morale breakdown: the SAME click-to-open disclosure as Food, in the same popover.
        # Only offered when there's actually a breakdown to show (a contribution above the epsilon, or
        # the concerning recovery line) — `_register_disclosure` declines an empty payload.
        _register_disclosure(DETAIL_ROW_MORALE, BREAKDOWN_KIND_MORALE, unit_data,
            _morale_breakdown_lines(unit_data))
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    # Per-source labor is now shown by the allocation panel (a real −/+ control set),
    # not as drawer text; the old single-task harvest/scout summaries are retired.
    var stockpile_variant: Variant = unit_data.get("accessible_stockpile", {})
    if stockpile_variant is Dictionary:
        var stockpile_lines := _accessible_stockpile_lines(stockpile_variant)
        if not stockpile_lines.is_empty():
            lines.append("")
            lines.append_array(stockpile_lines)
    return lines

## Drawer readout for a selected expedition (docs/plan_exploration_and_sites.md §2 / §2b):
## mission, humanized phase, party size, and carried food (from stores/turnsOfFood). A hunt
## expedition (§2b) also lists the target herd it follows. Expeditions have no labor in v1, so
## this replaces the band's labor/morale rows entirely.
## Like the band + herd drawers, it carries NO identity row: an expedition rides the same
## `_roster_units` path as a band, so its roster row (`_build_band_row`) already shows the very
## `id` the old `Unit:` line printed — nothing is lost with it (unlike the herd's fauna id, which
## had to move INTO the row). `Policy` / `Phase` deliberately keep their WORDS here: the compact
## Active-expeditions row is where the glyph vocabulary belongs; this block IS the disclosure.
func _expedition_summary_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var mission := String(unit_data.get("expedition_mission", ""))
    var is_hunt := mission == EXPEDITION_MISSION_HUNT
    lines.append("Mission: %s" % _expedition_mission_label(mission))
    if is_hunt:
        # The migratory herd it follows (species label from the fauna_id, falling back to the id).
        var herd_id := String(unit_data.get("expedition_target_herd", "")).strip_edges()
        if herd_id != "":
            lines.append("Target: %s" % _herd_label_for_id(herd_id))
        # The launched take policy (Sustain/Surplus/Market/Eradicate).
        var policy := String(unit_data.get("expedition_hunt_policy", "")).strip_edges()
        if policy != "":
            lines.append("Policy: %s" % policy.capitalize())
    var phase := String(unit_data.get("expedition_phase", "")).strip_edges()
    if phase != "":
        lines.append("Phase: %s" % _expedition_phase_label(phase))
    # NO `Party` row: it printed `unit_data["size"]` — the exact field the roster row already shows as
    # its size meta (`Hunters 1 … 5`), so it was the band `Size` restatement under another name.
    # Food it carries — larder-drawn provisions for a scout, the hunted haul for a hunt party —
    # turns from turnsOfFood. Reuse the food-turns tint context (`_selected_band_food_turns`, read
    # back in `_format_detail_bbcode`).
    var turns: float = float(unit_data.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    _selected_band_food_turns = turns
    var carried := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        if is_hunt:
            # The hunt party lives off its own kills; its store item key isn't fixed, so total it.
            for qty in (stores_variant as Dictionary).values():
                carried += int(round(float(qty)))
        else:
            carried = int(round(float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))))
    if is_hunt:
        # Carried X / cap + a FULL badge at the carry ceiling (the party heads home when full).
        var cap := int(round(float(unit_data.get("expedition_carry_cap", 0.0))))
        if cap > 0:
            var full_badge := "  %s" % HUNT_FULL_BADGE if carried >= cap else ""
            lines.append("Carried: %d / %d  (%s)%s" % [carried, cap, _food_turns_text(turns), full_badge])
        else:
            lines.append("Carried: %d  (%s)" % [carried, _food_turns_text(turns)])
    else:
        lines.append("Provisions: %d  (%s)" % [carried, _food_turns_text(turns)])
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    return lines

## Humanize an expedition mission id ("scout" → "Scouting expedition"); falls back to a
## capitalized token for an unknown/future mission (e.g. PR 2's "hunt").
func _expedition_mission_label(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if EXPEDITION_MISSION_LABELS.has(key):
        return EXPEDITION_MISSION_LABELS[key]
    return key.capitalize() if key != "" else "Expedition"

## Humanize an expedition phase id ("awaiting" → "Awaiting orders").
func _expedition_phase_label(phase: String) -> String:
    var key := phase.strip_edges().to_lower()
    if EXPEDITION_PHASE_LABELS.has(key):
        return EXPEDITION_PHASE_LABELS[key]
    return key.capitalize()

## Selection-panel band food row: "Food  <provisions>  (<turns>)" — provisions from
## the band's larder stores, turns from `turns_of_food` (∞ when not food-limited).
## Stashes the turns on `_selected_band_food_turns` so `_format_detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
func _band_food_line(unit_data: Dictionary) -> String:
    var turns: float = float(unit_data.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    _selected_band_food_turns = turns
    var provisions := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        provisions = int(round(float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))))
    var line := "Food: %d  (%s)" % [provisions, _food_turns_text(turns)]
    # For player bands with real flow, append the net per-turn rate (sign-tinted, inline) and mark
    # the Food label a clickable disclosure (`_food_flow_present`, read by `_format_detail_bbcode`).
    # An enemy band shows the bare larder line, exactly as before.
    _food_flow_present = false
    if _is_player_unit(unit_data) and _band_has_food_flow(unit_data):
        # The headline "/turn" is the STEADY net: income (Gathered + Hunted — the realized average,
        # so it no longer swings turn-to-turn) minus what the people (Eaten) and the pens (Pen feed)
        # draw off the larder. The breakdown below itemizes the income rows and the debits.
        var net := _band_net_food(unit_data)
        var net_hex := HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX
        line += " · [color=#%s]%s[/color]" % [net_hex, _format_yield(net)]
        _food_flow_present = true
    return line

## Selection-panel band morale row: "Morale: 41% ▼ — harsh terrain (Karst Cavern Mouth)".
## Morale, its per-turn trend, and the dominant cause come from the snapshot cohort dict
## (decoded in `native/src/lib.rs population_to_dict`). A falling trend appends the named
## cause; Terrain names the band's tile (the "it's the hex you're on" payload). A rehydrated
## save reports delta 0 / cause None for one turn, so the row degrades to a bare percentage.
## Stashes morale on `_selected_band_morale` so `_format_detail_bbcode` tints the value.
func _band_morale_line(unit_data: Dictionary) -> String:
    var morale: float = float(unit_data.get("morale", 1.0))
    _selected_band_morale = morale
    var text := "Morale: %d%%" % int(round(morale * 100.0))
    var delta: float = float(unit_data.get("morale_delta", 0.0))
    if delta <= -MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_FALLING_GLYPH
        # Name the cause only when morale is actually concerning — a healthy band
        # drifting slowly (nearly every tile bleeds a little today) shouldn't be
        # branded "harsh climate/terrain". Below the warn threshold, spell it out.
        if morale < BandFoodStatus.warn_morale():
            var cause := int(unit_data.get("morale_cause", MORALE_CAUSE_NONE))
            var cause_label := _morale_cause_label(cause)
            if cause_label != "":
                if cause == MORALE_CAUSE_TERRAIN:
                    var terrain_label := String(_selected_tile_info.get("terrain_label", "")).strip_edges()
                    if terrain_label != "":
                        cause_label = "%s (%s)" % [cause_label, terrain_label]
                text += " — %s" % cause_label
    elif delta >= MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_RISING_GLYPH
    return text

## Selection-panel band productivity row: "Output: 56%" — the modifier-stack result
## (snapshot `output_multiplier`, discontent being Phase 1's sole modifier). Only shown
## below full output; stashes the value on `_selected_band_output` so `_format_detail_bbcode`
## tints it by the output.{warn,critical} buckets (ink → amber → red).
func _band_output_line(unit_data: Dictionary) -> String:
    var output: float = float(unit_data.get("output_multiplier", OUTPUT_FULL))
    if output >= OUTPUT_FULL:
        return ""
    _selected_band_output = output
    return "Output: %d%%" % int(round(output * 100.0))

## True when the band's morale warrants surfacing the itemized breakdown + recovery
## guidance: below the warn threshold, or falling by more than the trend epsilon.
func _morale_is_concerning(unit_data: Dictionary) -> bool:
    var morale := float(unit_data.get("morale", 1.0))
    var delta := float(unit_data.get("morale_delta", 0.0))
    return morale < BandFoodStatus.warn_morale() or delta <= -MORALE_TREND_EPSILON

## Itemized morale breakdown: the four signed Layer-1 contributions (their sum IS morale_delta) as
## indented sub-lines, each above the breakdown epsilon rendered as `    ▲ +1.0%  settling`
## (`_format_detail_bbcode` tints by sign glyph). Now a click-to-expand disclosure (like Food): the
## contributions always compute so the row can be manually opened in the good state; the
## recovery-guidance line is appended ONLY when morale is concerning (don't tell a healthy band to
## "recover"). Returns [] when there is nothing to disclose (no contribution + not concerning).
func _morale_breakdown_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var terrain_label := String(_selected_tile_info.get("terrain_label", "")).strip_edges()
    var terrain_row_label := MORALE_CAUSE_LABEL_TERRAIN
    if terrain_label != "":
        terrain_row_label = "%s (%s)" % [MORALE_CAUSE_LABEL_TERRAIN, terrain_label]
    var unrest_value := float(unit_data.get("morale_unrest", 0.0))
    # (value, label) in the display order of the spec: settling, terrain, climate, unrest.
    var contributions := [
        [float(unit_data.get("morale_settling", 0.0)), MORALE_CONTRIB_LABEL_SETTLING],
        [float(unit_data.get("morale_terrain", 0.0)), terrain_row_label],
        [float(unit_data.get("morale_climate", 0.0)), MORALE_CAUSE_LABEL_COLD],
        [unrest_value, MORALE_CONTRIB_LABEL_CULTURE if unrest_value > 0.0 else MORALE_CAUSE_LABEL_UNREST],
    ]
    var epsilon := BandFoodStatus.morale_breakdown_epsilon()
    for entry in contributions:
        var value: float = entry[0]
        if absf(value) < epsilon:
            continue
        var glyph := MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else MORALE_CONTRIB_NEGATIVE_GLYPH
        var sign_str := "+" if value > 0.0 else "−"
        lines.append("%s%s %s%.1f%%  %s" % [
            MORALE_BREAKDOWN_INDENT, glyph, sign_str, absf(value) * 100.0, entry[1],
        ])
    # Recovery guidance is a "you have a problem" prompt — only when concerning.
    if _morale_is_concerning(unit_data):
        lines.append(RECOVERY_GUIDANCE_TEXT)
    return lines

## Plain-language label for a morale cause (0=None,1=Terrain,2=Cold,3=Unrest); "" for None
## or unknown. Shared by the drawer morale line and the losing-population alert reason.
func _morale_cause_label(cause: int) -> String:
    match cause:
        MORALE_CAUSE_TERRAIN:
            return MORALE_CAUSE_LABEL_TERRAIN
        MORALE_CAUSE_COLD:
            return MORALE_CAUSE_LABEL_COLD
        MORALE_CAUSE_UNREST:
            return MORALE_CAUSE_LABEL_UNREST
        _:
            return ""

## Human-readable food runway: the ∞ glyph when the source is not food-limited, otherwise a
## whole count of TURNS — spelled from the shared `FOOD_RUNWAY_UNIT`, which the Food-row tint guard
## in `_format_detail_bbcode` also keys on, so the two can never disagree about the unit. One helper
## feeds every surface that shows it (the band Food line, the expedition Carried/Provisions rows,
## and the turn-orb starving alert), so the unit is stated in exactly one place.
func _food_turns_text(runway: float) -> String:
    if not BandFoodStatus.is_limited(runway):
        return FOOD_UNLIMITED_GLYPH
    var turns := int(round(runway))
    if turns == 1:
        return "%d %s" % [turns, FOOD_RUNWAY_UNIT]
    return "%d %ss" % [turns, FOOD_RUNWAY_UNIT]

func _format_food_module_label(module_key: String) -> String:
    if module_key == "":
        return "Unknown"
    return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

func _format_stockpile_label(raw_value: String) -> String:
    var trimmed := raw_value.strip_edges()
    if trimmed == "":
        return "Stockpile"
    var tokens: PackedStringArray = trimmed.split("_", false)
    if tokens.is_empty():
        return trimmed.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return trimmed.capitalize()
    return " ".join(parts)

func _build_stockpile_row(entry: Dictionary) -> Control:
    var row := HBoxContainer.new()
    row.custom_minimum_size = Vector2(0, 24)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var label := Label.new()
    label.text = String(entry.get("label", "Stockpile"))
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(label)
    var amount_label := Label.new()
    amount_label.text = str(entry.get("amount", 0))
    amount_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    amount_label.custom_minimum_size = Vector2(60, 0)
    row.add_child(amount_label)
    var delta := float(entry.get("delta", 0.0))
    if not is_equal_approx(delta, 0.0):
        var delta_label := Label.new()
        delta_label.text = ("+%.0f" % delta) if delta > 0.0 else ("%.0f" % delta)
        delta_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        delta_label.custom_minimum_size = Vector2(60, 0)
        delta_label.modulate = Color(0.6, 0.9, 0.6) if delta > 0.0 else Color(0.95, 0.6, 0.5)
        row.add_child(delta_label)
    return row

func _accessible_stockpile_lines(stockpile: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var radius := int(stockpile.get("radius", 0))
    var entries_variant: Variant = stockpile.get("entries", [])
    var entries: Array = entries_variant if entries_variant is Array else []
    if entries.is_empty():
        return lines
    var formatted: Array[String] = []
    for entry in entries:
        if not (entry is Dictionary):
            continue
        var item := String(entry.get("item", ""))
        var qty := int(entry.get("quantity", 0))
        if item == "" and qty == 0:
            continue
        formatted.append("%d %s" % [qty, _format_stockpile_label(item)])
    if formatted.is_empty():
        return lines
    lines.append("Stockpile: radius %d" % radius)
    lines.append("Available: %s" % ", ".join(formatted))
    return lines

func _format_food_kind_label(kind_value: String) -> String:
    if kind_value == "":
        return ""
    var tokens: PackedStringArray = kind_value.split("_", false)
    if tokens.is_empty():
        return kind_value.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return kind_value.capitalize()
    return " ".join(parts)

func _herd_summary_lines(herd_data: Dictionary) -> Array[String]:
    # The split with the roster row above this drawer: the ROW carries identity (species glyph +
    # name) and STAFFING (`1 🏹`) — so no `Herd` / `Species` row here, which would be the same name a
    # second time. The SIZE class lives here because the row's one meta slot now belongs to the
    # staffing count, and the drawer is where the facts that don't fit the row live. Everything below
    # it is what the row can't show anyway: the herd's state.
    var lines: Array[String] = []
    var size_class := String(herd_data.get("size_class", "")).strip_edges()
    if size_class != "":
        lines.append("%s: %s" % [HERD_SIZE_ROW, HERD_SIZE_CLASS_FORMAT % size_class.capitalize()])
    # Biomass carries the herd's CURRENT head vs the K its range supports as a `current / max` pair
    # (`11636 / 11636`) — the convention the forage patch ("Forage biomass: 84 / 120") and the tile
    # card ("Pasture: 236 / 240") already use. K is derived each turn from the graze on the herd's
    # range; an overgrazed herd has `biomass > K`, so the pair honestly reads `current > max` (e.g.
    # `2100 / 1352`) — a FEATURE that makes the overshoot visible in the numbers (the ⚠ row below
    # spells out the consequence). The `~` the old standalone `Carrying cap` row carried is dropped:
    # a `current / max` pair already implies the max is the derived ceiling. Guard: a herd momentarily
    # on barren range derives K = 0, so `carrying_capacity <= 0` falls back to the bare `Biomass: X`
    # (never `X / 0`) and suppresses the overgrazing test below.
    var corralled := bool(herd_data.get("corralled", false))
    var carrying_capacity := float(herd_data.get("carrying_capacity", 0.0))
    var biomass: float = float(herd_data.get("biomass", 0.0))
    if biomass > 0.0:
        if carrying_capacity > 0.0:
            lines.append("Biomass: %d / %d" % [int(round(biomass)), int(round(carrying_capacity))])
        else:
            lines.append("Biomass: %.0f" % biomass)
    # The grazing range — WHY the herd is this size (the tiles it grazes / derives K over). A CORRALLED
    # herd doesn't roam-graze a range, so its Range row + overgrazing test are meaningless (its K is a
    # frozen pen-time value); the penned herd keeps the merged `Biomass: X / Y` pair, plainly.
    if not corralled:
        var range_radius := int(herd_data.get("graze_range_radius", 0))
        lines.append("%s: %s" % [HERD_RANGE_ROW, _graze_range_label(range_radius)])
    # Overgrazing: biomass exceeds what the range can sustainably feed (both numbers sim-provided — the
    # client compares, it does NOT re-derive the ecology). Suppressed for a corralled herd and when K is
    # unknown. The `X / Y` pair above already shows X > Y; this row states the consequence.
    if not corralled and carrying_capacity > 0.0 and biomass > carrying_capacity * (1.0 + OVERGRAZE_EPSILON):
        lines.append(OVERGRAZING_WARNING)
    var phase := String(herd_data.get("ecology_phase", "")).strip_edges().to_lower()
    if phase != "":
        lines.append("Ecology: %s" % _ecology_phase_label(phase))
    # Grazing 2d-δ — how far up the husbandry ladder THIS species can climb gates the whole section.
    # A WILD-ceiling herd shows NO husbandry track at all (just the hunt-only hint); a PASTORAL one
    # keeps the domestication track but can never be penned (hint where Corral would sit); a PEN one
    # (or empty/absent) shows the full ladder, exactly as before.
    var ceiling := _husbandry_ceiling(herd_data)
    if ceiling == HUSBANDRY_CEILING_WILD:
        lines.append(HUSBANDRY_WILD_HINT)
    else:
        var domestication := float(herd_data.get("domestication", 0.0))
        if domestication > 0.0:
            lines.append("Husbandry: %s" % _husbandry_label(domestication))
        # Staffing deficit — the fix for the silent "🐄 Domesticated but Penning stalled" playtest bug.
        # A managed herd needs `herders_needed` herders every turn to hold its tameness; understaffed,
        # its domestication decays, the herd slips back to WILD and stops earning Penning. Surface it
        # so the player has a signal to staff more herders. Shown only for a managed herd
        # (`herders_needed > 0`); `herded_fraction` defaults to FULLY_HERDED, so an unmanaged herd never
        # trips it. Fully staffed reads a calm "N / N"; under-herded an amber "A / N — under-herded".
        var herders_needed := int(herd_data.get("herders_needed", 0))
        if herders_needed > 0:
            var herded_fraction := float(herd_data.get("herded_fraction", FULLY_HERDED))
            var herders_assigned := int(round(herded_fraction * herders_needed))
            lines.append("%s: %s" % [HERDERS_ROW, _herders_label(herders_assigned, herders_needed, herded_fraction)])
            # Make the CONSEQUENCE explicit when the herd is slipping AND has real tameness to lose:
            # a muted one-liner naming why Penning has stalled and the single lever that fixes it.
            if herded_fraction < FULLY_HERDED and domestication > 0.0:
                lines.append(HERDERS_SLIPPING_FORMAT % herders_needed)
        # A corralled herd is penned by the band (intensification ladder). SIGNAL-tinted, mirroring the
        # Husbandry/Ecology row treatment. While the keepers are still BUILDING the pen (0 < progress < 1
        # under the Corral policy) the same row reports the meter — the animal twin of the tile card's
        # "Cultivation N%" row, so the investment the player committed to is visibly under way.
        # A PENNED herd is a managed population: it eats from its keeper's larder every turn, and an
        # underfed one is shrinking right now. That is the loudest thing the drawer can say about it, so
        # the Corral row itself flips to the starving state (DANGER-tinted via `_corral_value_hex`) and a
        # "Pen feed" row states the demand and how much of it the keeper actually paid.
        # The whole corral/pen readout is PEN-ceiling only — a pastoral herd can never be penned (the
        # server never builds one), so its Corral/pen rows are suppressed and a hint stands in their place.
        if ceiling == HUSBANDRY_CEILING_PEN:
            var corral_progress := float(herd_data.get("corral_progress", 0.0))
            var fed_fraction := PenStatus.fed_fraction(herd_data)
            if bool(herd_data.get("corralled", false)):
                lines.append("Corral: %s" % _corral_label(CORRAL_PROGRESS_COMPLETE, true, fed_fraction))
                # The pen is fenced LAND (Grazing 2d-γ): its footprint (radius + the SERVER's in-bounds
                # tile count, shown verbatim) and the feed SPLIT — how much of the herd's feed its own
                # grazed footprint covers vs what the keeper still hauls from the larder.
                var pen_radius := int(herd_data.get("pen_radius", 0))
                var footprint_tiles := int(herd_data.get("pen_footprint_tiles", 0))
                lines.append("%s: %s" % [PEN_FOOTPRINT_ROW, PEN_FOOTPRINT_FORMAT % [pen_radius, footprint_tiles]])
                var upkeep := float(herd_data.get("pen_upkeep", 0.0))
                var pasture_fraction := float(herd_data.get("pen_pasture_fraction", 0.0))
                lines.append("%s: %s" % [PEN_FEED_SPLIT_ROW, PEN_FEED_SPLIT_FORMAT \
                    % [int(round(pasture_fraction * PROGRESS_PERCENT_SCALE)), upkeep]])
                if upkeep >= FOOD_FLOW_MIN:
                    lines.append("%s: %s" % [PEN_FEED_ROW, _pen_feed_label(upkeep, fed_fraction)])
            elif corral_progress > 0.0:
                lines.append("Corral: %s" % _corral_label(corral_progress, false, PenStatus.FULLY_FED))
        elif ceiling == HUSBANDRY_CEILING_PASTORAL:
            lines.append(HUSBANDRY_PASTORAL_HINT)
    var x := int(herd_data.get("x", -1))
    var y := int(herd_data.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Position: (%d, %d)" % [x, y])
    var next_x := int(herd_data.get("next_x", -1))
    var next_y := int(herd_data.get("next_y", -1))
    if next_x >= 0 and next_y >= 0:
        lines.append("Next waypoint: (%d, %d)" % [next_x, next_y])
    return lines

## Player-facing label for a herd's ecology phase. Stressed/Collapsing carry a warning
## glyph; `_format_detail_bbcode` additionally tints the value (see `_ecology_value_hex`).
func _ecology_phase_label(phase: String) -> String:
    match phase:
        "collapsing":
            return "⚠ Collapsing"
        "stressed":
            return "⚠ Stressed"
        "thriving":
            return "Thriving"
        _:
            return phase.capitalize()

## BBCode hex for an "Ecology" value: red for a collapsing group, amber for stressed,
## normal ink otherwise. Matched on the lowercased phase stems ("collaps"/"stress" from
## `EcologyPhase::as_str`) so tinting survives glyph/capitalization tweaks to the label.
func _ecology_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER_HEX
    if normalized.contains("stress"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Tile-count label for a herd's grazing range from its hex radius — "the ground this herd grazes".
## The hex-disk count `1 + 3r(r+1)`: radius 0 → 1 tile (small game, its own hex), 1 → 7, 2 → 19. Same
## count the map ring draws, so the readout and the ring can never disagree. Singular for a lone tile.
func _graze_range_label(range_radius: int) -> String:
    var tiles := 1 + 3 * range_radius * (range_radius + 1)
    if tiles == 1:
        return "1 tile"
    return "%d tiles" % tiles

## The species' husbandry ceiling (Grazing 2d-δ) normalized to one of the three known values.
## Empty/absent/unrecognized ⇒ "pen" (the full ladder), so an un-tagged herd behaves as it did
## before the field existed. Read by the herd drawer + assign controls to gate husbandry affordances.
func _husbandry_ceiling(herd_data: Dictionary) -> String:
    var ceiling := String(herd_data.get("husbandry_ceiling", "")).strip_edges().to_lower()
    if ceiling == HUSBANDRY_CEILING_WILD or ceiling == HUSBANDRY_CEILING_PASTORAL:
        return ceiling
    return HUSBANDRY_CEILING_PEN

## Player-facing husbandry label from domestication progress (0.0–1.0). Fully tamed shows
## a livestock glyph; in-progress shows the percentage. `_format_detail_bbcode` tints a
## Domesticated value via `_husbandry_value_hex`.
func _husbandry_label(progress: float) -> String:
    if progress >= 1.0:
        return "🐄 Domesticated"
    return "Domesticating %d%%" % int(round(progress * 100.0))

## BBCode hex for a "Husbandry" value: signal (positive) for a domesticated herd, normal
## ink while it's still being tamed. Matched on the label produced by `_husbandry_label`.
func _husbandry_value_hex(value: String) -> String:
    if value.to_lower().contains("domesticated"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## The "Herders" row value: a calm "N / N" when fully staffed, an amber "A / N — under-herded" when
## the herd is decaying for lack of herders. Fully staffed uses [needed, needed] (assigned == needed
## at frac 1.0); under-herded uses the rounded assigned count. Tinted via `_herders_value_hex`.
func _herders_label(assigned: int, needed: int, herded_fraction: float) -> String:
    if herded_fraction >= FULLY_HERDED:
        return HERDERS_STAFFED_FORMAT % [needed, needed]
    return HERDERS_UNDER_FORMAT % [assigned, needed]

## BBCode hex for a "Herders" value: WARN (amber) while the herd is under-herded and its tameness is
## slipping, normal ink when fully staffed. Matched on the label from `_herders_label`, mirroring
## `_corral_value_hex` / the overgrazing warning's shared WARN tint.
func _herders_value_hex(value: String) -> String:
    if value.to_lower().contains("under-herded"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Player-facing cultivation label for a forage patch. A fully-tended patch shows a crop
## glyph; an in-progress patch shows the percentage. Mirrors `_husbandry_label`;
## `_format_detail_bbcode` tints a Tended value via `_cultivation_value_hex`.
func _cultivation_label(progress: float, cultivated: bool) -> String:
    if cultivated or progress >= 1.0:
        return "🌾 Tended Patch"
    # Lead with the build VERB, exactly as the herd's Husbandry row reads "Domesticating N%" — a bare
    # percentage buried in the tile card was easy to miss and broke parity with the animal side.
    return "%s %d%%" % [CULTIVATION_PREPARING_LABEL, int(round(progress * 100.0))]

## BBCode hex for a "Cultivation" value: signal (positive) for a tended patch, normal ink
## while it's still being cultivated. Matched on the label from `_cultivation_label`.
func _cultivation_value_hex(value: String) -> String:
    if value.to_lower().contains("tended"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## Player-facing label for the plant RUNG-3 meter — the patch twin of `_corral_label` and the rung
## above `_cultivation_label`. While the crop is going in it reads as a BUILD ("Sowing 40%"), using
## the same building-verb convention as the pen's "Building 40%" / the fence's "Fencing 60%"; once
## complete it is a **Field**, badged with its own glyph so it reads as a DIFFERENT THING from a
## 🌾 Tended Patch rather than as a bigger number — which is the whole point of rung 3.
func _field_label(progress: float, is_field: bool) -> String:
    if is_field or progress >= FIELD_PROGRESS_COMPLETE:
        return "%s %s" % [FoodIcons.for_policy(LABOR_POLICY_SOW), FIELD_BADGE_LABEL]
    return "%s %d%%" % [FIELD_SOWING_LABEL, _progress_percent(progress)]

## The tile's plant composition as one compact line — `Hazel 45% · Oak Mast 30% · Berry Scrub 25%`.
##
## The wire list is ALREADY sorted (share descending, then species key ascending) and its shares sum
## to 1, so this only formats: the order is the sim's and is never re-derived here.
##
## THE DISPLAYED PERCENTAGES ALWAYS SUM TO 100. Rounding each share independently can total 99 or 101
## — a decomposition that visibly fails to decompose — so the remainder is folded into the LARGEST
## share, i.e. the first entry, where a ±1 is proportionally smallest. Returns "" for a tile with no
## composition (a biome that carries no forage), so no empty row renders.
func _flora_composition_text(composition: Variant) -> String:
    if not (composition is Array):
        return ""
    var shares: Array = composition
    if shares.is_empty():
        return ""
    var names: Array[String] = []
    var percents: Array[int] = []
    var total := 0
    for entry_variant in shares:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var name := String(entry.get("display_name", "")).strip_edges()
        if name == "":
            continue
        var percent := int(round(float(entry.get("share", 0.0)) * FLORA_SHARE_PERCENT_TOTAL))
        names.append(name)
        percents.append(percent)
        total += percent
    if names.is_empty():
        return ""
    percents[0] += FLORA_SHARE_PERCENT_TOTAL - total
    var parts: Array[String] = []
    for i in names.size():
        parts.append(FLORA_SHARE_FORMAT % [names[i], percents[i]])
    return FLORA_SHARE_SEPARATOR.join(parts)

## BBCode hex for a "Field" value: signal (positive) for a completed Field, normal ink while the crop
## is still going in. Matched on the label from `_field_label`, mirroring `_cultivation_value_hex`.
func _field_value_hex(value: String) -> String:
    if value.to_lower().contains(FIELD_BADGE_LABEL.to_lower()):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## Player-facing corral label from pen-build progress (0.0–1.0) — the herd twin of
## `_cultivation_label`. A finished pen shows the livestock glyph; an in-progress one reads
## "Building N%", naming the work under way. A finished pen whose keeper did NOT pay this turn's
## feed reads the STARVING state instead of the penned badge — the herd is losing biomass every
## turn, which is the one fact the player must not be able to miss.
## `_format_detail_bbcode` tints via `_corral_value_hex`.
func _corral_label(progress: float, corralled: bool, fed_fraction: float) -> String:
    if corralled or progress >= CORRAL_PROGRESS_COMPLETE:
        if PenStatus.is_starving(fed_fraction):
            return PEN_STARVING_LABEL % int(round(fed_fraction * PROGRESS_PERCENT_SCALE))
        return "%s Corralled" % CORRAL_GLYPH
    return "%s %d%%" % [CORRAL_BUILDING_LABEL, int(round(progress * 100.0))]

## The "Pen feed" row's value: what this pen demands per turn, plus — when the keeper is short — how
## much of it was actually paid. Amber/red-tinted via `_pen_feed_value_hex`.
func _pen_feed_label(upkeep: float, fed_fraction: float) -> String:
    var demand := _format_yield(-upkeep)
    if PenStatus.is_starving(fed_fraction):
        return PEN_FEED_STARVING_FORMAT % [demand, int(round(fed_fraction * PROGRESS_PERCENT_SCALE))]
    return demand

## BBCode hex for a "Corral" value: DANGER for a starving pen (the herd is shrinking NOW), signal
## (positive) once penned and fed, normal ink while it's being built. Matched on the label from
## `_corral_label`, mirroring `_cultivation_value_hex`.
func _corral_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("starving"):
        return HudStyle.DANGER_HEX
    if normalized.contains("corralled"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## BBCode hex for the "Pen feed" value: DANGER while the pen goes unfed (the herd is shrinking),
## WARN otherwise — a paid pen is still a standing debit on the larder, never good news.
func _pen_feed_value_hex(value: String) -> String:
    if value.to_lower().contains("paid"):
        return HudStyle.DANGER_HEX
    return HudStyle.WARN_HEX

func _join_lines(lines: Array) -> String:
    var packed := PackedStringArray()
    for line in lines:
        packed.append(String(line))
    return "\n".join(packed)

## Render the selection detail lines as BBCode: consecutive "Key: value" rows
## become a 2-column table (dim key, bright value; Food value in amber) so the
## data aligns into columns, while sentences/section lines stay full-width and
## muted. Matches the mockup's Tile Banner body.
func _format_detail_bbcode(lines: Array) -> String:
    var out := ""
    var table_open := false
    for raw in lines:
        var line := String(raw)
        if line == "":
            if table_open:
                out += "[/table]"
                table_open = false
            out += "\n"
            continue
        # Itemized morale / food breakdown sub-lines render full-width, tinted by their sign
        # glyph (▲ positive = healthy, ▼ negative = amber) — kept two-tone, not a rainbow. The
        # `\n` after `[/table]` forces a block break: a RichTextLabel `[table]` is inline, so text
        # emitted right after it otherwise floats onto the table's top-right when there's room.
        if line.begins_with(MORALE_BREAKDOWN_INDENT):
            if table_open:
                out += "[/table]\n"
                table_open = false
            var row_hex := HudStyle.HEALTHY_HEX if line.contains(MORALE_CONTRIB_POSITIVE_GLYPH) else HudStyle.WARN_HEX
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        # The overgrazing warning is a full-width WARN sentence (biomass > K), tinted with the same
        # WARN_HEX the Ecology/Corral value rows use — not a parallel styling path, just the shared color.
        if line == OVERGRAZING_WARNING:
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.WARN_HEX, line]
            continue
        var kv := _split_detail_kv(line)
        if kv.is_empty():
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.INK_DIM_HEX, line]
        else:
            if not table_open:
                out += "[table=2]"
                table_open = true
            var value_hex := HudStyle.INK_HEX
            if String(kv[0]) == "Food" or String(kv[0]) == "Provisions" or String(kv[0]) == "Carried":
                # The band larder / expedition provisions / hunt-party carried-food row tints by the
                # larder-runway thresholds. It recognizes the row by the SHARED `FOOD_RUNWAY_UNIT`
                # the one renderer (`_food_turns_text`) spells the runway with — never a bare
                # literal, which is how this guard silently went dead when the unit changed — or by
                # the ∞ glyph for a band that is not food-limited.
                var food_value := String(kv[1])
                if not is_nan(_selected_band_food_turns) and (food_value.contains(FOOD_RUNWAY_UNIT) or food_value.contains(FOOD_UNLIMITED_GLYPH)):
                    value_hex = BandFoodStatus.hex_for_turns(_selected_band_food_turns)
            elif String(kv[0]) == "Morale":
                # The player band's morale row tints by the morale thresholds.
                if not is_nan(_selected_band_morale):
                    value_hex = BandFoodStatus.hex_for_morale(_selected_band_morale)
            elif String(kv[0]) == "Output":
                # The productivity row tints by the output buckets (ink → amber → red).
                if not is_nan(_selected_band_output):
                    value_hex = BandFoodStatus.hex_for_output(_selected_band_output)
            elif String(kv[0]) == "Forage":
                # The tile's gather module reads in the success/ETA amber.
                value_hex = HudStyle.WARN_HEX
            elif String(kv[0]) == "Habitability":
                # The tile's habitability rating tints by its bucket (green→red).
                value_hex = TileHabitability.hex_for_rating(String(kv[1]))
            elif String(kv[0]) == TILE_SIGHT_KEY:
                # The tile's sight state: live cyan when in sight, dim when only remembered/unknown.
                value_hex = _sight_value_hex(String(kv[1]))
            elif String(kv[0]) == "Ecology" or String(kv[0]) == PASTURE_ECOLOGY_KEY:
                # Shared by the herd drawer, the forage-patch tile card and the tile card's PASTURE
                # row — one phase tint (neutral/amber/red) for every ecology in the game. The pasture
                # row keeps its own KEY only so a forage tile doesn't print two rows named "Ecology";
                # the styling path is deliberately not forked.
                value_hex = _ecology_value_hex(String(kv[1]))
            elif String(kv[0]) == "Husbandry":
                value_hex = _husbandry_value_hex(String(kv[1]))
            elif String(kv[0]) == HERDERS_ROW:
                # A managed herd's staffing: amber when under-herded (tameness slipping), ink when full.
                value_hex = _herders_value_hex(String(kv[1]))
            elif String(kv[0]) == "Cultivation":
                value_hex = _cultivation_value_hex(String(kv[1]))
            elif String(kv[0]) == FIELD_ROW:
                # Plant rung 3 — the patch twin of the Corral row's tint (ink while building, signal
                # once complete). Same shape as Cultivation's; kept its own case because a Field is a
                # different rung with its own badge word, not a Tended Patch at a higher percentage.
                value_hex = _field_value_hex(String(kv[1]))
            elif String(kv[0]) == "Corral":
                value_hex = _corral_value_hex(String(kv[1]))
            elif String(kv[0]) == PEN_FEED_ROW:
                # The pen's running feed cost: amber as a standing debit, red when it goes unpaid.
                value_hex = _pen_feed_value_hex(String(kv[1]))
            # A disclosure row (Food/Morale) renders its key as a clickable `[url]` + ▸/▾ caret, which
            # opens its breakdown in the shared POPOVER via `meta_clicked` → `_on_detail_meta_clicked`
            # (never inline — see the BREAKDOWN_* consts). The caret is ▾ only while THIS row's
            # popover is up. A CONCERNING row wears the caret in WARN rather than SIGNAL: the
            # breakdown no longer opens itself, so the invitation to read it has to be visible.
            var key_cell := "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, kv[0]]
            if _disclosure_state.has(kv[0]):
                var st: Dictionary = _disclosure_state[kv[0]]
                var caret := BREAKDOWN_CARET_OPEN if bool(st.get("open", false)) else BREAKDOWN_CARET_CLOSED
                var caret_hex := HudStyle.WARN_HEX if bool(st.get("concerning", false)) else HudStyle.SIGNAL_HEX
                key_cell = "[url=%s%s][color=#%s]%s %s[/color][/url]" % [
                    BREAKDOWN_TOGGLE_META_PREFIX, String(st.get("key", "")),
                    caret_hex, kv[0], caret,
                ]
            out += "[cell]%s[/cell][cell][color=#%s]%s[/color][/cell]" % [
                key_cell, value_hex, kv[1],
            ]
    if table_open:
        out += "[/table]"
    return out

## Split a "Key: value" data line into [key, value]; returns [] for sentence
## lines (trailing period), long keys, or non-matching text so those stay
## full-width rather than becoming a lopsided table row.
func _split_detail_kv(line: String) -> Array:
    if line.ends_with("."):
        return []
    # The recovery-guidance line reads as a dim sentence, not a lopsided table row.
    if line.begins_with(RECOVERY_GUIDANCE_GLYPH):
        return []
    var idx := line.find(": ")
    if idx <= 0:
        return []
    var key := line.substr(0, idx)
    if key.length() > 16:
        return []
    var value := line.substr(idx + 2)
    if value.strip_edges() == "":
        return []
    return [key, value]

func clear_selection() -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_subject = SUBJECT_LAND
    _selected_food_module = ""
    _selected_food_is_hunt = false
    # Keep pending move-band so the user can still choose a destination after deselecting.
    if _selected_tile_info.is_empty():
        _hide_selection_card()
    else:
        _render_selection_panel(_selected_tile_info, {}, {})

func _travel_eta_hint(tile_info: Dictionary) -> String:
    var distance := int(tile_info.get("nearest_unit_distance", -1))
    if distance < 0:
        return ""
    var turns := _estimate_travel_turns(distance)
    if turns < 0:
        return ""
    var label := String(tile_info.get("nearest_unit_label", "")).strip_edges()
    if label == "":
        label = "Band"
    return "Nearest band %s is %d tiles away (~%d turns)." % [label, distance, turns]

func _travel_turns_for_tile(tile_info: Dictionary) -> int:
    var distance := int(tile_info.get("nearest_unit_distance", -1))
    return _estimate_travel_turns(distance)

func _estimate_travel_turns(distance: int) -> int:
    if distance < 0:
        return -1
    if travel_tiles_per_turn <= 0.0:
        return distance
    var turns := int(ceil(float(distance) / travel_tiles_per_turn))
    if travel_preview_turn_cap > 0:
        turns = min(turns, travel_preview_turn_cap)
    return turns

func _load_ui_balance_config() -> void:
    if not FileAccess.file_exists(UI_BALANCE_CONFIG_PATH):
        return
    var file := FileAccess.open(UI_BALANCE_CONFIG_PATH, FileAccess.READ)
    if file == null:
        return
    var text := file.get_as_text()
    file.close()
    var data: Variant = JSON.parse_string(text)
    if not (data is Dictionary):
        return
    var travel_dict_variant: Variant = data.get("travel", {})
    if travel_dict_variant is Dictionary:
        var travel_dict: Dictionary = travel_dict_variant
        var speed_value := float(travel_dict.get("tiles_per_turn", travel_tiles_per_turn))
        if speed_value > 0.0:
            travel_tiles_per_turn = speed_value
        var cap_value := int(travel_dict.get("max_preview_turns", travel_preview_turn_cap))
        if cap_value > 0:
            travel_preview_turn_cap = cap_value

## Fan one batch of command events out to BOTH surfaces. Each controller filters for the kinds it
## owns (the split's one definition is `TellingPanel.handles_kind`), so passing the whole array to
## both is correct and keeps each one's own retention + de-duplication.
##
## This is also the Telling panel's BACKFILL: a full snapshot carries the server's whole
## `commandEvents` ring, so a player opening the client mid-session sees recent history.
func ingest_command_events(events_variant: Variant) -> void:
    _command_feed.ingest_events(events_variant)
    _telling.ingest_events(events_variant)
func update_band_alerts(populations_variant: Variant) -> void:
    if not (populations_variant is Array):
        return
    var populations: Array = populations_variant
    var new_sizes: Dictionary = {}
    # Turn-orb attention registry: one loop over the player faction feeds three producers
    # per band (starving / losing_population / idle_workers). Pushed to the orb below (via
    # `_push_attention`, which folds in the snapshot-driven fork producer), which severity-sorts
    # (critical floats up). New band-derived producers append here.
    var attention: Array = []
    # Bands-only counter: increments for resident bands, NOT expeditions, so the "Band N"
    # attention labels match the band-picker (`_build_band_picker`, `i + 1`) and the panel
    # header (`_index_of_player_band` + 1) — all number positionally within `_player_bands`.
    var band_number := 0
    # Capture the player bands each snapshot; the labor-allocation UI targets them (assign/move/
    # clear) and reads their labor_assignments for the herd/tile assign controls. `player_band`
    # (first) stays the default actor; `player_bands` backs the assign controls' band-picker.
    var player_band: Dictionary = {}
    var player_bands: Array = []
    var player_expeditions: Array = []
    for entry_variant in populations:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        if int(entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        # Split expeditions out of the band roster: they are detached scout/hunt parties, never a
        # labor actor band, and must not be counted by the cycler, listed in the band-picker, or
        # given band-style attention labels. The attention producers key off the bands-only path
        # below, so an expedition never surfaces as "Band N starving/losing/idle".
        if bool(entry.get("is_expedition", false)):
            player_expeditions.append(entry)
            continue
        if player_band.is_empty():
            player_band = entry
        player_bands.append(entry)
        band_number += 1
        var entity := int(entry.get("entity", -1))
        var size := int(entry.get("size", 0))
        var turns := float(entry.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
        var morale := float(entry.get("morale", 1.0))
        var morale_cause := int(entry.get("morale_cause", MORALE_CAUSE_NONE))
        var last_emigrated := int(entry.get("last_emigrated", 0))
        var x := int(entry.get("current_x", -1))
        var y := int(entry.get("current_y", -1))
        var band_name := _band_display_name(entry, band_number)
        new_sizes[entity] = size
        # Producer 1 — starving: larder below the critical threshold (red/critical).
        if BandFoodStatus.is_critical(turns):
            attention.append({
                "kind": ATTENTION_KIND_STARVING,
                "severity": ATTENTION_SEVERITY_CRITICAL,
                "label": "%s starving" % band_name,
                "detail": _food_turns_text(turns),
                "x": x, "y": y,
            })
        # Producer 2 — losing population: shrank vs the previous snapshot (amber/warn).
        if _prev_band_sizes.has(entity) and size < int(_prev_band_sizes[entity]):
            attention.append({
                "kind": ATTENTION_KIND_LOSING_POPULATION,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": "%s losing population" % band_name,
                "detail": _decline_reason(turns, morale, morale_cause, last_emigrated),
                "x": x, "y": y,
            })
        # Producer 3 — idle labor: working-age workers unassigned (amber/warn). Supersedes
        # the old activity==idle alert (a worker count is more actionable than a state flag).
        var idle_workers := int(entry.get("idle_workers", 0))
        if idle_workers > 0:
            attention.append({
                "kind": ATTENTION_KIND_IDLE_WORKERS,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": "%d idle worker%s" % [idle_workers, "" if idle_workers == 1 else "s"],
                "detail": band_name,
                "x": x, "y": y,
            })
        # Producer 5 — a starving pen this band keeps (amber/warn; see ATTENTION_KIND_STARVING_PEN
        # for why it is not critical). Keyed off the band's OWN Corral assignments, never a scan of
        # every herd on the wire: that is what makes it the PLAYER's pen (a herd carries no owner
        # field client-side) and what lets the row name the keeper who has to fix it.
        attention.append_array(_starving_pen_attention(entry))
    # Producer 4 — awaiting orders: a detached party parked at its objective, burning provisions
    # until the player acts (amber/warn, same class as idle labor). Runs over the EXPEDITIONS split
    # out above, not the bands — an expedition is never "Band N", so it never enters the band loop.
    attention.append_array(_awaiting_orders_attention(player_expeditions))
    _prev_band_sizes = new_sizes
    _player_band = player_band
    _player_bands = player_bands
    _player_expeditions = player_expeditions
    # Cache the band/expedition half and push the whole registry (bands + the fork producer) as
    # ONE replace — set_attention is wholesale, so a separate call would wipe these rows.
    _band_attention = attention
    _push_attention()
    # This snapshot is authoritative: drop optimistic pending actions the server has now
    # processed (issued on an older turn), then let the panels render the confirmed state.
    _reconcile_pending()
    # Keep the dockable Band/City panel a persistent, live command center: shown whenever ≥1
    # player band exists, re-rendering the current _panel_band so its steppers/idle stay current.
    _refresh_panel_band()
    # Keep the on-screen allocation panel / assign controls live as the band's staffing
    # changes turn to turn (the coordinator re-renders occupant/tile cards separately, but
    # a herd/tile selection reads _player_band, which only just refreshed here).
    _refresh_drawer_actions()
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject — it must not close on a
    # snapshot, or it would be unusable under autoplay (§15). It closes only if its subject is gone.
    _refresh_compose_sheet()

## Why a band is shrinking: a food crisis (larder below critical) reads "starving" first;
## then, since morale no longer kills (discontent relocates people — see
## docs/plan_civ_wellbeing.md), a shrink with emigrants last turn reads "people leaving".
## Otherwise the dominant morale cause names it in plain language ("harsh terrain" /
## "harsh climate" / "unrest"). When no cause is attributed (morale steady/rising — e.g.
## a rehydrated save, or shrinkage from cold deaths / an aging cohort at healthy morale)
## only say "low morale" if morale is actually low, else leave it plain rather than
## asserting a false reason.
func _decline_reason(turns: float, morale: float, morale_cause: int, last_emigrated: int) -> String:
    if BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.critical_turns():
        return DECLINE_REASON_STARVING
    if last_emigrated > 0:
        return DECLINE_REASON_PEOPLE_LEAVING
    var cause_label := _morale_cause_label(morale_cause)
    if cause_label != "":
        return cause_label
    if morale < BandFoodStatus.warn_morale():
        return DECLINE_REASON_LOW_MORALE
    return ""

## Best-effort readable band name: a positional "Band N". (Cohorts carry no top-level
## band label in the snapshot yet — see the server-side follow-up.)
func _band_display_name(_entry: Dictionary, index: int) -> String:
    return "Band %d" % index

func _note_command_feed(label: String, detail: String) -> void:
    _command_feed.note(label, detail)
func _refresh_victory_status() -> void:
    # A data refresh never un-hides a card the player suppressed.
    _apply_victory_visibility()
    if victory_status_label == null:
        return
    if victory_state.is_empty():
        victory_status_label.visible = false
        victory_status_label.text = ""
        return
    victory_status_label.visible = true
    var lines: Array = ["[b]Victory[/b]"]
    var winner_variant: Variant = victory_state.get("winner", {})
    if winner_variant is Dictionary and not (winner_variant as Dictionary).is_empty():
        var winner_dict: Dictionary = winner_variant
        var label_text := String(winner_dict.get("label", winner_dict.get("mode", "Victory")))
        var tick := int(winner_dict.get("tick", 0))
        lines.append("[color=gold]Winner:[/color] %s · Tick %d" % [label_text, tick])
    else:
        lines.append("[color=gray]No victory declared.[/color]")
    var modes_variant: Variant = victory_state.get("modes", [])
    if modes_variant is Array:
        var sorted_modes: Array = _sorted_victory_modes(modes_variant as Array)
        var limit: int = min(sorted_modes.size(), 3)
        for idx in range(limit):
            var mode_dict: Dictionary = sorted_modes[idx]
            var label_text := String(mode_dict.get("label", mode_dict.get("id", "Mode")))
            if label_text.strip_edges() == "":
                label_text = _format_victory_label(String(mode_dict.get("id", mode_dict.get("kind", "Mode"))))
            var pct: float = clamp(float(mode_dict.get("progress_pct", 0.0)), 0.0, 1.0) * 100.0
            var achieved := bool(mode_dict.get("achieved", false))
            var prefix := "✔" if achieved else "•"
            lines.append("%s %s — %.1f%%" % [prefix, label_text, pct])
    victory_status_label.bbcode_enabled = true
    victory_status_label.text = String("\n".join(lines))

func _sorted_victory_modes(source: Array) -> Array:
    var entries: Array = []
    for entry in source:
        if entry is Dictionary:
            entries.append((entry as Dictionary).duplicate(true))
    entries.sort_custom(Callable(self, "_victory_mode_sort"))
    return entries

func _victory_mode_sort(a: Dictionary, b: Dictionary) -> bool:
    var pct_a := float(a.get("progress_pct", 0.0))
    var pct_b := float(b.get("progress_pct", 0.0))
    if is_equal_approx(pct_a, pct_b):
        var label_a := _format_victory_label(String(a.get("label", a.get("id", ""))))
        var label_b := _format_victory_label(String(b.get("label", b.get("id", ""))))
        return label_a < label_b
    return pct_a > pct_b

func _format_victory_label(raw: String) -> String:
    var trimmed := raw.strip_edges()
    if trimmed == "":
        return "Victory Mode"
    var sanitized := trimmed.replace("_", " ").replace("-", " ").replace(".", " ")
    var parts: Array = sanitized.split(" ", false)
    for i in range(parts.size()):
        parts[i] = String(parts[i]).capitalize()
    return String(" ".join(parts)).strip_edges()

func _resolve_localized_field(field: String) -> String:
    var text := String(campaign_label.get(field, ""))
    var loc_key_field := "%s_loc_key" % field
    var loc_key := String(campaign_label.get(loc_key_field, ""))
    if localization_store != null and loc_key != "":
        var localized: String = localization_store.resolve(loc_key, text)
        if localized.strip_edges() != "":
            return localized
    return text

func _on_legend_sort_pressed(field: String) -> void:
    _legend.on_sort_pressed(field)

func toggle_legend() -> void:
    _legend.toggle_suppressed()
    _refit_right_dock()
    _save_panel_pref(CONFIG_KEY_LEGEND_SUPPRESSED, _legend.legend_suppressed)

## Victory's counterpart to `toggle_legend` (bound to `V` in Main). Hides/shows the card through the
## dock so the stack reflows with no gap, and remembers the choice for next session.
func toggle_victory() -> void:
    _victory_suppressed = not _victory_suppressed
    _apply_victory_visibility()
    _save_panel_pref(CONFIG_KEY_VICTORY_SUPPRESSED, _victory_suppressed)

## The command feed's counterpart to `toggle_legend` / `toggle_victory` (bound to `R` in Main). The
## feed holds six read-only receipts and NO verbs, so hiding it absorbs nothing — it simply hands
## its dock height to the selection card, which is where the actions are. Hiding goes through the
## controller (not a bare `visible = false`) so the dock reflows with no gap AND the next command
## receipt can't re-show a card the player closed.
func toggle_command_feed() -> void:
    if _command_feed == null:
        return
    _command_feed.toggle_suppressed()
    _refit_left_dock()
    _save_panel_pref(CONFIG_KEY_COMMAND_FEED_SUPPRESSED, _command_feed.feed_suppressed)

func _apply_victory_visibility() -> void:
    if victory_panel == null:
        return
    var should_show := not _victory_suppressed
    if right_dock != null:
        right_dock.set_relevant(victory_panel, should_show)
    else:
        victory_panel.visible = should_show
    _refit_right_dock()

## The Telling panel grows to fit its own (bounded) page, so a sibling's visibility flip no longer
## changes its height — `refit()` just re-syncs the page geometry and re-fits the current page's height
## (it does NOT touch the inner scroll). Kept so this call stays valid and the right dock reflows the
## toggleable cards below it.
func _refit_right_dock() -> void:
    if _telling != null:
        _telling.refit()

## The left dock's twin, for the one event that moves BOTH of its growing cards at once: the `R`
## toggle. The drawer sizes itself against whatever the feed below it reserves, so on a toggle the
## two must settle in a fixed order or each measures the other mid-flight and their sum overspills
## the dock. Release the drawer's claim → let the feed re-fit into the freed column → then let the
## drawer take exactly the remainder. Ordinary selection changes need none of this: the feed is
## already settled and `_fit_subject_drawer` alone fits into what is left.
func _refit_left_dock() -> void:
    if subject_scroll != null:
        subject_scroll.custom_minimum_size.y = 0.0
    await get_tree().process_frame
    if _command_feed != null:
        _command_feed.refit()
    await get_tree().process_frame
    _fit_subject_drawer()

# ---- dock-card visibility persistence --------------------------------------

func _load_hud_panel_prefs() -> void:
    var cfg := ConfigFile.new()
    if cfg.load(NarrativeForkPanel.config_path()) == OK:
        if _legend != null:
            _legend.set_suppressed(bool(cfg.get_value(
                HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_LEGEND_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT)))
        _victory_suppressed = bool(cfg.get_value(
            HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_VICTORY_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT))
        if _command_feed != null:
            _command_feed.set_suppressed(bool(cfg.get_value(
                HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_COMMAND_FEED_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT)))
    else:
        # No prefs file yet (or unreadable): fall back to the hidden-by-default layout.
        if _legend != null:
            _legend.set_suppressed(PANEL_SUPPRESSED_BY_DEFAULT)
        if _command_feed != null:
            _command_feed.set_suppressed(PANEL_SUPPRESSED_BY_DEFAULT)
    _apply_victory_visibility()

## Persist ONE panel's preference — never the whole section.
##
## Writing both keys on either toggle is how a transient state becomes a stored preference: pressing
## `V` used to also write whatever the legend happened to be showing at that instant. That is fine
## while both values are genuine user choices, but it makes the file a snapshot of live UI state
## rather than of decisions, and anything that sets visibility WITHOUT intending to persist it (a
## preview harness, a future "peek" affordance) silently corrupts the other panel's preference. A
## toggle owns its own key and nothing else.
func _save_panel_pref(key: String, suppressed: bool) -> void:
    var cfg := ConfigFile.new()
    cfg.load(NarrativeForkPanel.config_path())   # preserve every other section/key; ignore load errors
    cfg.set_value(HUD_PANELS_CONFIG_SECTION, key, suppressed)
    cfg.save(NarrativeForkPanel.config_path())
func _setup_tooltip() -> void:
    tooltip_panel = PanelContainer.new()
    tooltip_panel.visible = false
    tooltip_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
    tooltip_panel.z_index = 100 # Ensure on top
    
    var style := StyleBoxFlat.new()
    style.bg_color = Color(0.1, 0.1, 0.1, 0.9)
    style.border_width_left = 1
    style.border_width_top = 1
    style.border_width_right = 1
    style.border_width_bottom = 1
    style.border_color = Color(0.4, 0.4, 0.4, 0.8)
    style.corner_radius_top_left = 4
    style.corner_radius_top_right = 4
    style.corner_radius_bottom_right = 4
    style.corner_radius_bottom_left = 4
    style.content_margin_left = 8
    style.content_margin_top = 4
    style.content_margin_right = 8
    style.content_margin_bottom = 4
    tooltip_panel.add_theme_stylebox_override("panel", style)
    
    tooltip_label = Label.new()
    tooltip_label.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
    tooltip_panel.add_child(tooltip_label)
    
    add_child(tooltip_panel)

func _process(_delta: float) -> void:
    _suppress_tooltip_over_ui()

## Hide the hex tooltip whenever the pointer is over an interactive HUD control
## (panel, button, minimap, inspector). The map cannot detect this itself: those
## controls are MOUSE_FILTER_STOP and consume the motion events, so the map never
## receives a "moved away" event to clear its tooltip and it would otherwise stay
## frozen on top of the panel.
func _suppress_tooltip_over_ui() -> void:
    if tooltip_panel == null or not tooltip_panel.visible:
        return
    var viewport := get_viewport()
    if viewport != null and viewport.gui_get_hovered_control() != null:
        tooltip_panel.visible = false

## MapView.tile_hovered lands here — the hex tooltip. The hovered hex is no longer recorded: its only
## reader was the targeting banner's pre-launch raid forecast, which moved INTO the compose sheet once
## the quarry is picked first (the sheet has the real party size and policy; a hover never did).
func show_tooltip(info: Dictionary) -> void:
    if tooltip_panel == null:
        return

    if info.is_empty():
        tooltip_panel.visible = false
        return

    # Never show over interactive HUD controls (see _suppress_tooltip_over_ui).
    var hover_viewport := get_viewport()
    if hover_viewport != null and hover_viewport.gui_get_hovered_control() != null:
        tooltip_panel.visible = false
        return

    var lines: PackedStringArray = []
    
    # Coordinates
    var x := int(info.get("x", -1))
    var y := int(info.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Hex: %d, %d" % [x, y])
        
    # Terrain
    var terrain := String(info.get("terrain_label", ""))
    if terrain != "":
        lines.append("Terrain: %s" % terrain)

    # Hex-edge rivers: which SIDES of the hovered hex carry water. Permanent geography, so it
    # reads on a remembered tile too — hence above the "(last seen)" note. Same RiverEdges
    # formatter as the Tile card; [] on a riverless tile, so no empty row.
    if info.has("river_edges"):
        for river_line in RiverEdges.summary_lines(int(info["river_edges"])):
            lines.append(river_line)

    # Remembered (Discovered) tiles: flag that contents are stale/incomplete.
    if String(info.get("visibility_state", "")) == "discovered":
        lines.append("(last seen — incomplete)")

    # Food
    var food := String(info.get("food_module_label", ""))
    if food != "" and food != "None":
        lines.append("Food: %s" % food)
        
    # Units
    var unit_count := int(info.get("unit_count", 0))
    if unit_count > 0:
        lines.append("Units: %d" % unit_count)
        
    # Herds
    var herd_count := int(info.get("herd_count", 0))
    if herd_count > 0:
        lines.append("Herds: %d" % herd_count)
        
    if lines.is_empty():
        tooltip_panel.visible = false
        return
        
    tooltip_label.text = "\n".join(lines)
    tooltip_panel.visible = true
    
    # Position near mouse
    var mouse_pos := get_viewport().get_mouse_position()
    var viewport_size := get_viewport().get_visible_rect().size
    var panel_size := tooltip_panel.get_combined_minimum_size()
    
    var pos := mouse_pos + Vector2(16, 16)
    
    # Keep within bounds
    if pos.x + panel_size.x > viewport_size.x:
        pos.x = mouse_pos.x - panel_size.x - 16
    if pos.y + panel_size.y > viewport_size.y:
        pos.y = mouse_pos.y - panel_size.y - 16
        
    tooltip_panel.position = pos

## Returns the minimap container for embedding the minimap panel.
## Returns null if container not found.
func get_minimap_container() -> Control:
    return minimap_container

