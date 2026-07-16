extends CanvasLayer
class_name HudLayer

## Map-zoom rail (bottom-left nav cluster). `map_zoom_step` carries +1 (in) / -1 (out);
## `map_zoom_fit` fits the map to the view. Main wires both to the single MapView zoom path.
signal map_zoom_step(direction: int)
signal map_zoom_fit
## Emitted when the player clears ALL of a band's labor assignments (the "Clear all"
## affordance); carries the band dict so Main can extract faction + entity bits for the
## repurposed `cancel_order` command (now a clear-all → fully idle).
signal cancel_order_requested(band: Dictionary)
## Early-Game Labor (docs/plan_early_game_labor.md, slice 3b): assign/unassign
## working-age workers to a source or band-wide role. Payload keys:
## { faction, band, kind ("forage"|"hunt"|"scout"|"warrior"), workers,
##   x, y (forage/hunt readout), herd_id, policy (hunt) }. Main formats the
## `assign_labor …` text command. workers==0 removes/zeroes the assignment.
signal assign_labor_requested(payload: Dictionary)
## Emitted after the player picks a destination tile for the selected band's move.
## Payload keys: { faction, band, x, y }. Main formats the `move_band …` command.
signal move_band_requested(payload: Dictionary)
## Scouting expedition (docs/plan_exploration_and_sites.md §2). Sent after the player outfits a
## party on a resident band (a party-size stepper) and clicks a target tile. Payload keys:
## { faction, band, party_workers, x, y }. Main formats the `send_expedition …` command.
signal send_expedition_requested(payload: Dictionary)
## Hunting expedition (docs/plan_exploration_and_sites.md §2b). Sent after the player outfits a party
## on a resident band and clicks a target herd. Payload keys: { faction, band, party_workers,
## fauna_id }. Main formats the `send_hunt_expedition …` command.
signal send_hunt_expedition_requested(payload: Dictionary)
## Emitted when the player recalls the selected in-flight expedition (folds it home). Payload
## keys: { faction, expedition }. Main formats the `recall_expedition …` command.
signal recall_expedition_requested(payload: Dictionary)
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

## Build identifier of THIS client (GDScript/native). **Bump on client-affecting
## changes.** Shown in the lower-left version overlay next to the server build (streamed
## in the snapshot header) so the running client+server builds can be confirmed at a
## glance. Format: `YYYY-MM-DD.N`.
const CLIENT_BUILD := "2026-07-10.3"
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var sedentarization_label: Label = %SedentarizationLabel
@onready var demographics_label: Label = %DemographicsLabel
@onready var discoveries_label: Label = %DiscoveriesLabel
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
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var tile_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/TilePanel as PanelCard
@onready var tile_detail: RichTextLabel = %TileDetail
@onready var occupants_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/OccupantsPanel as PanelCard
@onready var occupant_detail: RichTextLabel = %OccupantDetail
@onready var roster_list: VBoxContainer = %RosterList
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
# Separator between the Cultivation / Herding tracks in the top-bar intensification readout.
const INTENSIFICATION_SEGMENT_SEP := "  ·  "
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
const UI_BALANCE_CONFIG_PATH := "res://src/config/ui_balance.json"
const DEFAULT_TRAVEL_SPEED := 3.0
const DEFAULT_TRAVEL_PREVIEW_LIMIT := 12
# The legend card (rows + sort header + suppress state) is owned by _legend; the
# command feed card by _command_feed. Hud delegates to both.
var _legend: LegendController = null
var _command_feed: CommandFeedController = null
var localization_store = null
var campaign_label: Dictionary = {}
var victory_state: Dictionary = {}
# Previous per-band size (entity id -> size) so we can detect population loss
# across snapshots for the "losing population" alert.
var _prev_band_sizes: Dictionary = {}
var _selected_tile_info: Dictionary = {}
var _selected_unit: Dictionary = {}
var _selected_herd: Dictionary = {}
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
var _selected_band_food_days: float = NAN
# Set by `_band_food_line`: the current player band carries real food flow, so the Food row becomes a
# clickable disclosure (net rate on the line + a Gathered/Hunted/Eaten breakdown).
var _food_flow_present: bool = false
# Disclosure context for the Food + Morale summary rows, rebuilt each render in `_unit_summary_lines`:
# row-label → {kind, open}. `_format_detail_bbcode` reads it to render the caret + clickable meta.
var _disclosure_state: Dictionary = {}
# Per-row per-band expand override, keyed `"<kind>:<entity>"` → bool. Absent = follow the row's
# concerning default (food: net-negative / low runway; morale: below-warn / falling); a click stores one.
var _breakdown_expanded: Dictionary = {}
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
# The Sustain rung by name: the default compose policy AND the one policy that TEACHES — every
# intensification track (cultivation / herding knowledge, herd domestication) accrues only while a
# band works a Thriving source under Sustain, so the gate reasons below point back at it.
const LABOR_POLICY_SUSTAIN := "sustain"
const DEFAULT_HUNT_POLICY := LABOR_POLICY_SUSTAIN
# INVESTMENT rungs (Intensification): an up-front cost — the source pays only its `ceiling_cultivate`
# / `ceiling_corral` dip yield while the workers prepare it, then flips to the much higher tended /
# corral yield. Kind-specific, and the sim REJECTS the cross pairing: Cultivate is forage-only,
# Corral is hunt-only.
const LABOR_POLICY_CULTIVATE := "cultivate"
const LABOR_POLICY_CORRAL := "corral"
# The full picker option sets per source kind (extractive rungs + that kind's one investment rung).
const FORAGE_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "cultivate"]
const HUNT_POLICY_OPTIONS := ["sustain", "surplus", "market", "eradicate", "corral"]
# Forage take policies reuse the hunt picker, but carry forage-appropriate behaviour hints
# (gathering a plant patch's regrowth, not culling a herd).
const FORAGE_POLICY_HINTS := {
    "sustain": "Sustain — gather at the patch's regrowth; it stays healthy.",
    "surplus": "Surplus — gather more now; the patch declines.",
    "market": "Market — gather for trade goods; faster decline.",
    "eradicate": "Eradicate — strip the patch bare.",
    "cultivate": "Cultivate — prepare this patch: low yield while you work it, then a much higher tended yield. It must stay staffed or it goes feral.",
}
# GATES on the investment rungs. The option stays VISIBLE but disabled with its reasons, so the player
# learns the prerequisite BEFORE acting rather than never discovering the rung exists. Both gates
# mirror the sim's `assign_labor` validation (faction knowledge complete + the source ready).
#
# Each reason states WHAT'S MISSING + HOW FAR ALONG IT IS + THE ACTION THAT CLOSES IT — naming the
# prerequisite alone ("Herd must be domesticated") tells the player a door is locked without saying
# where the key is. All three tracks are taught by the SAME action: Sustain-work a THRIVING source
# (`core_sim/src/systems.rs` — cultivation/herding knowledge and per-herd domestication accrue only
# under Sustain on a Thriving patch/herd). The remedy therefore names the Sustain glyph, pulled from
# the shared `FoodIcons.POLICY_ICONS` map so it is literally the icon on the button beside it.
# Format args: %d = the live progress percent off the snapshot, %s = that Sustain glyph.
const GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT := "Cultivation knowledge %d%% — %s Sustain-forage a Thriving patch to learn it"
const GATE_REASON_HERDING_KNOWLEDGE_FORMAT := "Herding knowledge %d%% — %s Sustain-hunt a Thriving herd to learn it"
const GATE_REASON_HERD_DOMESTICATED_FORMAT := "Herd %d%% tamed — %s Sustain-hunt this Thriving herd to finish taming it"
# The patch-ecology gate is a STOCK condition, not a policy one, so its remedy is the opposite advice:
# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever. The
# patch only climbs back to Thriving when the take is LESS than the growth — fewer workers, or none.
# %s = the live `patch_ecology_phase`, capitalized.
const GATE_REASON_PATCH_THRIVING_FORMAT := "Patch is %s — ease workers off and let it regrow to Thriving"
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
# 0..1 progress tracks (knowledge, domestication) render as whole percents.
const PROGRESS_PERCENT_SCALE := 100.0
# A knowledge track (0..1) is usable only once fully learned; a domestication track likewise.
const KNOWLEDGE_COMPLETE := 1.0
const DOMESTICATION_COMPLETE := 1.0
# Herd drawer "Corral" row: the pen-build meter (0..1) reads "Building N%" until it completes, then
# the penned badge — the herd twin of the tile card's "Cultivation N%" → "🌾 Tended Patch" row.
const CORRAL_PROGRESS_COMPLETE := 1.0
const CORRAL_BUILDING_LABEL := "Building"
const CORRAL_GLYPH := "🐄"
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
# Herd drawer grazing range (Grazing Phase 2b-iii): the ground the herd grazes (tile count of its hex
# range, so it pairs with the map ring) — a SEPARATE fact from the biomass/cap pair, which the `Biomass`
# row now carries as a `current / max` pair (`11636 / 11636`). The `Range` key stays ≤ 16 chars so
# `_split_detail_kv` renders it as an aligned table row beside Biomass.
const HERD_RANGE_ROW := "Range"
# Overgrazing is a TRIVIAL honest comparison of two sim-provided numbers — biomass exceeds what the
# range can sustainably feed, so the herd is drawing the range down and will shrink. NOT a re-derivation
# of the ecology model (K and graze flow are the sim's). The epsilon keeps a herd sitting exactly at K
# from flickering the warning. WARN-tinted via `_format_detail_bbcode` (the Ecology/Corral rows' path).
const OVERGRAZE_EPSILON := 0.05
const OVERGRAZING_WARNING := "⚠ Overgrazing — range can't sustain this herd"
# The one ecology phase a patch can be cultivated from (matches `EcologyPhase::as_str`).
const ECOLOGY_PHASE_THRIVING := "thriving"
# The two intensification knowledge tracks (the `intensification_knowledge[]` row's field names),
# each gating one investment rung.
const KNOWLEDGE_TRACK_CULTIVATION := "cultivation"
const KNOWLEDGE_TRACK_HERDING := "herding"
# Command-feed nudge fired ONCE when a track completes: the rung it unlocks is a new verb the player
# has never seen, so learning the discovery has to say what it bought.
const KNOWLEDGE_UNLOCK_LABELS := {
    "cultivation": "Cultivation learned",
    "herding": "Herding learned",
}
const KNOWLEDGE_UNLOCK_NOTES := {
    "cultivation": "The Cultivate policy is now available on Thriving patches.",
    "herding": "The Corral policy is now available on domesticated herds.",
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
    "sustain": "Sustain — takes only the herd's renewable yield, so it stays healthy forever; on a thriving herd the hunt also tames it, building husbandry toward livestock that pays food every turn without being hunted down.",
    "surplus": "Surplus — more food now; the herd slowly declines. The fuller larder pushes the band toward settling.",
    "market": "Market — sells the take as trade goods rather than eating it; the herd declines fast. Trade has little effect yet.",
    "eradicate": "Eradicate — hunts the herd toward extinction. No food, no husbandry, no trade — denial only.",
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
# Allocation-panel section headers + role hints (make the panel read as a "current actions"
# report and make the standing Scout/Warrior roles discoverable — the −/+ steppers ARE how
# you staff a scout mission now; there is no targeted map action).
const ALLOC_SECTION_FONT_SIZE := 10
# Vertical gap between the rows within one allocation section block (Workers / Current actions /
# Band roles / Orders / Send expedition). Matches the pre-section-block flat-list spacing so the
# tall stack reads unchanged; the Band/City panel spaces the blocks THEMSELVES apart (tall) or flows
# them into columns (wide).
const ALLOC_BLOCK_SEPARATION := 6
const ALLOC_HEADER_ACTIONS := "Current actions"
const ALLOC_HEADER_ROLES := "Band roles"
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
# PRE-COMMIT YIELD FORECAST on the assign controls (%ForageAssignControls / %HerdAssignControls).
# The overstaffing note above is POST-HOC — it tells you a turn later that workers were wasted. The
# forecast is the same truth shown WHILE COMPOSING: the sim exports, with identical field names on
# both the forage patch and the herd, a `per_worker_yield` plus one take ceiling per policy — all
# food/turn at the source's CURRENT biomass and at output_multiplier 1.0:
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
    # The INVESTMENT rungs' ceiling is the DIP yield paid while the patch/pen is being prepared —
    # so the same expected(workers, policy) math shows the cost of the investment while composing.
    "cultivate": "ceiling_cultivate",
    "corral": "ceiling_corral",
}
# The PAYOFF the investment buys — the food/turn the source pays once prepared (one worker suffices).
# Only the investment rungs have one; an extractive rung's forecast is a single number.
const FORECAST_PAYOFF_KEYS := {
    "cultivate": "tended_yield",
    "corral": "corral_yield",
}
# The RUNNING COST the payoff is paid against. Only the pen has one: a corralled herd is a managed
# population that eats from the keeper's larder every turn (`pen_upkeep`), and `corral_yield` is the
# GROSS take with that feed NOT deducted — so advertising the payoff bare would promise a number the
# player never banks. A tended patch has no running cost, hence no entry.
const FORECAST_FEED_KEYS := {
    "corral": "pen_upkeep",
}
# The investment forecast states the DEAL, not a single yield: "Preparing: +0.09 /turn → then +1.20 /turn".
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
# A herd dict carries the forecast fields bare; `tile_info` carries the forage patch's under a
# `patch_` prefix (MapView._tile_info_at cross-refs them off `forage_patch_lookup`).
const HERD_FORECAST_PREFIX := ""
const FORAGE_FORECAST_PREFIX := "patch_"
# Below this a worker produces nothing here (a dead-season forage tile with no forecast fields).
# Dividing by it would blow max-useful up to infinity, so instead: no forecast row,
# and the stepper keeps its plain idle-worker cap.
const FORECAST_MIN_PER_WORKER := 0.0001
# Sentinel for "no forecast data" → the stepper is not forecast-capped.
const MAX_USEFUL_UNBOUNDED := -1
const FORECAST_LABEL_FORMAT := "Expected yield: %s"
# A tended patch / corralled herd collapses max-useful to exactly 1, so this note has to read
# "max 1 worker" — pluralize the noun rather than shipping "max 1 workers".
const MAX_USEFUL_NOTE_FORMAT := "max %d %s useful here — more would be idle"
const MAX_USEFUL_NOUN_ONE := "worker"
const MAX_USEFUL_NOUN_MANY := "workers"
# Band food flow lives on the Food summary line: `Food 15 (19 days) · −0.77 /turn` (net =
# food_income − food_consumption, sign-tinted), with a click-to-expand category breakdown
# (Gathered/Hunted/Eaten) underneath — mirroring the morale breakdown. `FOOD_FLOW_MIN` gates both
# the net readout and each breakdown category (below it → absent, not shown as a zero).
const FOOD_FLOW_MIN := 0.001
# Click-to-expand disclosure shared by the Food + Morale summary rows: a ▸/▾ caret on the row label,
# a clickable `[url]` meta = `<prefix><kind>` dispatched by `_on_detail_meta_clicked`, and a per-row
# per-band expand override in `_breakdown_expanded` (absent → the row's concerning default).
const BREAKDOWN_CARET_OPEN := "▾"
const BREAKDOWN_CARET_CLOSED := "▸"
const BREAKDOWN_TOGGLE_META_PREFIX := "breakdown:"
const BREAKDOWN_KIND_FOOD := "food"
const BREAKDOWN_KIND_MORALE := "morale"
# The detail-row labels the disclosure attaches to (must equal the `Key` in `_split_detail_kv`).
const DETAIL_ROW_FOOD := "Food"
const DETAIL_ROW_MORALE := "Morale"
# ---- Band/City panel identity grid ---------------------------------------------------------------
# The panel's own header already states the band's name + settlement stage, so the summary rows there
# drop the `Unit: <name>` row (a THIRD copy of the same name) and replace `Size: <n>` (population
# under another name) with the labor line — same numbers, one row, in the identity grid where they
# belong. The Occupants-card drawer (FOREIGN bands, and the no-panel ui_preview fallback) keeps
# Unit/Size: it has no panel header naming the band, and a foreign band exposes no worker breakdown.
# Hence `_unit_summary_lines(unit, in_panel)` rather than deleting the rows outright.
const DETAIL_ROW_POPULATION := "Population"
# The labor line's numbers. ONE format, two hosts: the panel's identity-grid row renders it without
# the leading label (the grid supplies that), the legacy in-card allocation block with it.
const WORKERS_VALUE_FORMAT := "%d · Workers %d (Idle %d)"
const WORKERS_HEADER_FORMAT := "%s %s" % [DETAIL_ROW_POPULATION, WORKERS_VALUE_FORMAT]
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
const PANEL_EXPEDITIONS_HEADER := "Active expeditions"
const PANEL_EXPEDITION_SCOUT_GLYPH := "⚑"
const PANEL_EXPEDITION_HUNT_GLYPH := "🏹"
const SEND_EXPEDITION_TITLE := "Send scouting expedition"
const SEND_EXPEDITION_HINT := "Detach a party to scout distant territory, then click a target tile."
const SEND_EXPEDITION_BUTTON := "Send scouting expedition…"
# Hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party that follows a
# migratory herd, accumulates food, and drops it at the band. Launched from a resident band by
# picking a herd (herd-target click, not a tile), and Recalled like a scout expedition.
const SEND_HUNT_EXPEDITION_TITLE := "Send hunting expedition"
const SEND_HUNT_EXPEDITION_HINT := "Detach a party to follow a migratory herd, then click on the herd."
const SEND_HUNT_EXPEDITION_BUTTON := "Send hunting expedition…"
# Distance-aware herd-hunt affordance (docs/plan_exploration_and_sites.md §2b): clicking a herd
# offers a LOCAL hunt when it's within the SELECTED band's hunt_reach, or a hunting EXPEDITION when
# it's beyond. One compose control (worker/party stepper + policy), two labels/commands keyed off the
# wrap-aware hex distance from the selected band's own tile.
const ASSIGN_LOCAL_HUNT_BUTTON := "Assign Local Hunt"
const SEND_HUNTING_EXPEDITION_BUTTON := "Send Hunting Expedition"
# Range-aware forage assign: foraging is stationary gathering (NO expedition fallback), so a tile
# beyond the selected band's `work_range` disables the button rather than offering an alternative.
const FORAGE_ASSIGN_BUTTON := "Forage"
# Generic section header for the outfit block (hosts both the scout + hunt send verbs).
const SEND_EXPEDITION_SECTION := "Send expedition"
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
const HUNT_FORECAST_TURNS_FORMAT := "%s · ≈%d turns to fill"
# The HAUL a filled pack delivers, appended to the turns line so the party-size tradeoff reads BOTH
# ways: a bigger party climbs the turns AND the food it brings home. The haul = party_workers ×
# expedition_per_worker_carry (blessed party×lever arithmetic, NOT the ecology lookup turns comes
# from). Shown ONLY when the pack fills (viable OR too-slow); a won't-fill / denial trip never reaches
# the cap, so quoting a haul there would be a lie. Absent/0 per-worker-carry → no suffix (live guard).
const HUNT_FORECAST_HAUL_FORMAT := " · ~%d food"
# Above the config's viability threshold the trip still launches (this is information, not a block) —
# but the herd's sustainable yield, not the hunters, is the binding constraint by a wide margin.
const HUNT_FORECAST_NOT_VIABLE_SUFFIX := " — too slow to be worth sending"
# The sim's forward simulation never filled this party's packs within its `forecast_horizon_turns`
# (`turns_to_fill == 0`) — a "won't fill" verdict, NOT "the herd is dead". A perfectly HEALTHY herd
# lands here whenever its yield is simply too slow for the packs this party carries: a thriving Rabbit
# Warren under Sustain fills NO party size at all. The party would still be out there at the horizon.
const HUNT_FORECAST_NEVER_FILLS_FORMAT := "%s can't fill a party this size — the packs would never fill"
# An Eradicate expedition is a DENIAL mission, not a failed hunt: it delivers no food BY DESIGN. Kept
# distinct from the won't-fill line above (red = the packs don't fill within the sim's horizon; amber =
# you are choosing to bring nothing home).
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
# A trip that CANNOT FILL is the one case that is blocked rather than warned. The distinction is the
# whole point: a SLOW trip (finite ETA past the warn threshold) is a real tradeoff and the player is
# told and then TRUSTED — `Send Anyway (≈54 turns)` stays enabled. A trip that cannot succeed has no
# upside at all, so offering the button would be offering a mistake. The button states the reason, and
# the reason SCANS THE SIM'S TABLE rather than dispensing generic advice (see `_hunt_impossible_reason`).
const SEND_HUNT_IMPOSSIBLE_BUTTON := "Can't fill this party's packs"
# BLOCKED, but the policy's row of the estimate table holds a party size that fills AND is VIABLE: name it
# (the LARGEST such — the most food per trip among the trips this UI considers worth making) and its ETA.
# Generic "send a smaller party" advice was often a lie: on Red Deer + Surplus, 1–5 workers fill in 5 turns
# while 8 never fills, so "smaller" and "bigger" are both wrong answers in general and only the row knows.
const SEND_HUNT_IMPOSSIBLE_ALTERNATIVE_REASON := "%s can't fill packs for a party of %d. A party of %d fills in %d turns."
# BLOCKED, and NOTHING on the row is viable — some size fills, but only past `viability_warn_turns` (Rabbit
# Warren + Surplus: a lone hunter, 23 turns). Name the best there is (the FASTEST — with no viable option
# left, time dominates haul), but word it as the marginal trip it is: recommending it in the same breath as
# a real fix would have the UI cheerfully suggesting a trip it flags "too slow to be worth sending".
const SEND_HUNT_IMPOSSIBLE_SLOW_REASON := "%s can't fill packs for a party of %d. A party of %d fills, but takes %d turns."
# BLOCKED with the WHOLE ROW zeroed — NO party size fills this herd under this policy (a Rabbit Warren on
# Sustain: its sustainable trickle never fills anyone's packs). Telling the player to try another size here
# would send them fruitlessly up and down the stepper, so say so plainly and point them somewhere real.
const SEND_HUNT_IMPOSSIBLE_NO_SIZE_REASON := "%s can't fill packs at any party size — hunt it locally instead."
# The party stepper is a REAL decision, not "more is better": a bigger party carries bigger packs, so
# stepping UP can turn a working trip impossible (Red Deer + Surplus fills at 5, never at 8). Surfaced as
# the stepper row's tooltip when the very next size up is impossible — zero added clutter on a panel that
# is otherwise fine, and it lands exactly where the trap is.
const SEND_HUNT_STEP_UP_IMPOSSIBLE_TOOLTIP := "A party of %d could not fill its packs on %s — bigger parties carry bigger packs than this herd can fill."
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
# with the same ⚠ / WARN amber (and the same `_is_overdraw` test) as the allocation rows.
const LOCAL_HUNT_OVERDRAW_SUFFIX := " — overdraws the herd"
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
# Send-hunt-expedition targeting: the pending hunt-launch HERD pick (the click resolves to a
# huntable herd on the clicked hex, not a tile). {} when inactive.
var _pending_send_hunt_expedition: Dictionary = {}
# The hex the pointer is currently over (MapView.tile_hovered -> show_tooltip), or {} when the
# pointer is off the map / over a HUD control. While a hunt expedition is armed, the targeting
# banner reads the hovered hex's herd out of this to show the pre-launch turns-to-fill forecast
# BEFORE the click commits (the herd is only known at the targeting step, never at outfit time).
var _hovered_tile_info: Dictionary = {}
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = WORKER_STEP
# Compose state for the hunt-expedition launch policy (Sustain/Surplus/Market/Eradicate).
var _send_hunt_policy: String = DEFAULT_HUNT_POLICY
# Compose state for the herd/tile "Assign" controls — the in-progress worker count (and, for
# hunts, the policy) the player is dialing before pressing Assign. Keyed to the source so a
# per-snapshot re-render preserves the count, but re-initializes from current staffing when
# the selected source changes.
var _forage_assign_key: String = ""
var _forage_assign_count: int = 0
var _forage_assign_policy: String = DEFAULT_HUNT_POLICY
var _hunt_assign_key: String = ""
var _hunt_assign_count: int = 0
var _hunt_assign_policy: String = DEFAULT_HUNT_POLICY
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
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0

func _ready() -> void:
    _legend = LegendController.new(terrain_legend_panel, terrain_legend_scroll, terrain_legend_list, terrain_legend_description)
    _command_feed = CommandFeedController.new(command_feed_panel, command_feed_scroll, command_feed_label, left_dock_scroll)
    _load_ui_balance_config()
    _connect_zoom_rail()
    _connect_turn_orb()
    _setup_tooltip()
    _legend.refresh_rows()
    _refresh_campaign_label()
    _refresh_victory_status()
    _command_feed.render()
    _connect_selection_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    left_dock.add(occupants_panel, 12)
    left_dock.add(stockpile_panel, 20)
    left_dock.add(command_feed_panel, 30)
    right_dock.add(victory_panel, 10)
    right_dock.add(terrain_legend_panel, 20)
    if stockpile_panel != null:
        stockpile_panel.visible = false
    if stockpile_title != null:
        stockpile_title.text = "Stockpiles"
    _apply_hud_style()
    _ensure_targeting_banner()
    _setup_build_overlay()
    # The Occupants-card drawer's Food/Morale labels are click-to-expand breakdown disclosures.
    if occupant_detail != null:
        occupant_detail.meta_clicked.connect(_on_detail_meta_clicked.bind(false))

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
    if not _pending_send_hunt_expedition.is_empty():
        var band: Dictionary = _pending_send_hunt_expedition.get("band", {})
        var pos: Array = Array(band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
        return {
            "active": true,
            "command": "hunt_expedition",
            "need": "herd",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": "%s · %d" % [
                String(band.get("id", "Band")), int(_pending_send_hunt_expedition.get("party_workers", 0)),
            ],
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
    elif cmd == "HUNT_EXPEDITION":
        instruction = "click on a herd to hunt"
    else:
        instruction = "click a tile to survey"
    var line := "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
        HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
    ]
    var forecast := _hunt_forecast_bbcode()
    if forecast != "":
        line += "\n" + forecast
    return line

## The pre-launch turns-to-fill line for the herd currently under the pointer, or "" when no hunt
## expedition is armed / the pointer isn't over a huntable herd / the snapshot predates the forecast
## fields. This is the real moment of decision: party size + policy are chosen at OUTFIT time, but the
## HERD — which sets the take ceiling, and therefore the trip length — is only known HERE, so the
## forecast cannot live in the outfit block. The click still commits: this is information, not a gate.
func _hunt_forecast_bbcode() -> String:
    if _pending_send_hunt_expedition.is_empty() or _hovered_tile_info.is_empty():
        return ""
    var herd := _huntable_herd_on_tile(_hovered_tile_info)
    if herd.is_empty():
        return ""
    var band: Dictionary = _pending_send_hunt_expedition.get("band", {})
    var workers := int(_pending_send_hunt_expedition.get("party_workers", 0))
    var policy := String(_pending_send_hunt_expedition.get("policy", DEFAULT_HUNT_POLICY))
    return _hunt_forecast_line_bbcode(
        _hunt_trip_forecast(band, herd, policy, workers), _herd_display_name(herd))

## Render a `_hunt_trip_forecast` result as its one-line BBCode readout — the three states in their
## three colors (cyan viable / amber too-slow / red returns-empty), or "" when the forecast isn't
## available (a herd with no exported estimate → the caller shows no line at all). SHARED by both hunt-expedition entry
## points: the targeting banner (band-first flow) and the herd panel's live compose block (herd-first
## flow), so the two can never drift apart.
func _hunt_forecast_line_bbcode(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    # Two different "no food comes home" stories, and they must not be confused: an Eradicate party
    # brings nothing home BY DESIGN (denial, amber), while any other policy the sim says won't fill
    # within its `forecast_horizon_turns` means the packs stay empty for the whole trip (red). The red
    # case is a "can't fill" verdict, NOT a collapsed herd — a thriving herd whose yield is simply too
    # slow for this party's packs (a full Rabbit Warren on Sustain) lands here too.
    if bool(forecast.get("denial", false)):
        return "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_FORECAST_DENIAL_FORMAT % herd_name,
        ]
    if not bool(forecast.get("fills", false)):
        return "[color=#%s]%s%s[/color]" % [
            HudStyle.DANGER_HEX, HUNT_FORECAST_WARN_GLYPH,
            HUNT_FORECAST_NEVER_FILLS_FORMAT % herd_name,
        ]
    var turns := int(forecast.get("turns", 0))
    var text: String = HUNT_FORECAST_TURNS_FORMAT % [herd_name, turns]
    # The pack fills here (viable OR too-slow), so the haul is a real delivery, not a lie — append it.
    # No `haul` key = the snapshot didn't carry the per-worker-carry lever → render turns only.
    var haul: String = HUNT_FORECAST_HAUL_FORMAT % int(forecast["haul"]) if forecast.has("haul") else ""
    if bool(forecast.get("viable", true)):
        return "[color=#%s]%s%s[/color]" % [HudStyle.SIGNAL_HEX, text, haul]
    return "[color=#%s]%s%s%s%s[/color]" % [
        HudStyle.WARN_HEX, HUNT_FORECAST_WARN_GLYPH, text, haul, HUNT_FORECAST_NOT_VIABLE_SUFFIX,
    ]

## Turns for `workers` from `band` to fill their carry cap hunting `herd` under `policy`. A PURE TABLE
## LOOKUP into the sim's forward-simulated `hunt_trip_estimates` (`HERD_TRIP_ESTIMATES_KEY`) — ZERO
## arithmetic, and NEVER a `carryCap / rate` division: the sim moves the herd's state under the party
## and bounds the trip by its `forecast_horizon_turns`, so only the sim's own number is honest (pinned
## by core_sim/tests/expedition_hunt.rs). The ecology/MSY model is never reproduced here. (The LOCAL
## band hunt preview DOES compute — see `_hunt_take_rate` over the band ceiling `hunt_policy_ceilings`.)
## Returns {available, fills, turns, viable, denial}: `available` false when the snapshot carries no
## estimate for this (policy, party size) — older server → the caller shows no forecast at all.
func _hunt_trip_forecast(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if workers <= 0 or not (estimates_variant is Dictionary):
        return {"available": false}
    var key := _hunt_estimate_key(policy, workers)
    var estimates := estimates_variant as Dictionary
    if not estimates.has(key):
        return {"available": false}
    var estimate: Dictionary = estimates[key]
    # A denial mission (eradicate) delivers no food BY DESIGN — never an ETA, never a failure, and NOT
    # "impossible": it does exactly what it says. This carve-out MUST come first, or gating on
    # "doesn't fill" would ban Eradicate outright (it never fills, by definition).
    if not bool(estimate.get("delivers_food", false)):
        return {"available": true, "fills": false, "denial": true, "impossible": false}
    # 0 turns = the sim's forward simulation never fills the party within its forecast horizon. A trip
    # that CANNOT SUCCEED is not a tradeoff, it's a mistake with no upside — so unlike a merely SLOW
    # trip (finite ETA past the warn threshold, which the player is told about and then trusted with),
    # this one is BLOCKED. Keyed off the sim's per-(policy, party-size) verdict, never off a species /
    # size_class / biomass proxy: a 1-worker pack may well fill off a herd that can't fill a 4-worker
    # pack, and an overhunted big-game herd near its collapse floor can't fill either.
    var turns := int(estimate.get("turns_to_fill", 0))
    if turns <= 0:
        return {"available": true, "fills": false, "denial": false, "impossible": true}
    # A warn threshold of 0 means the server sent none — report the turns, judge nothing.
    var warn_turns := int(band.get("expedition_viability_warn_turns", 0))
    var viable: bool = warn_turns <= 0 or turns <= warn_turns
    var result := {"available": true, "fills": true, "turns": turns, "viable": viable}
    # The haul a filled pack delivers: party_workers × the per-worker carry lever — blessed party×lever
    # arithmetic (the same kind as the band ceiling), NOT the ecology/turns-to-fill lookup. Only set
    # when the lever is present (> 0), so the renderer can guard on the key's absence rather than a fake
    # zero. Withheld from the denial / won't-fill branches above: those packs never reach the cap.
    var per_worker_carry := float(band.get("expedition_per_worker_carry", 0.0))
    if per_worker_carry > 0.0:
        result["haul"] = int(round(float(workers) * per_worker_carry))
    return result

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

## The LOCAL hunt's live per-turn yield preview, or "" when the snapshot lacks the levers/ceilings
## (graceful degrade — no line, panel otherwise unchanged). A resident band applies its
## `output_multiplier` (morale/discontent productivity) at payout, so the preview is the take rate
## scaled by it. Reads income-green when the take is within the herd's sustainable yield (the Sustain
## ceiling), WARN-amber with the shared ⚠ when it overdraws — the same flag the allocation rows carry.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    # The BAND ceiling (the herd's flow) — a resident hunt is capped by it, and the Sustain entry IS
    # the herd's sustainable yield. The expedition's stock-headroom ceilings never enter here.
    var rate := _hunt_take_rate(band, herd, policy, workers)
    var sustain_ceiling := _hunt_policy_ceiling(herd, DEFAULT_HUNT_POLICY)
    if rate < 0.0 or sustain_ceiling < 0.0:
        return ""
    var output := float(band.get("output_multiplier", OUTPUT_FULL))
    var actual := rate * output
    var sustainable := sustain_ceiling * output
    var text: String = LOCAL_HUNT_YIELD_FORMAT % _format_yield(actual)
    if _is_overdraw(actual, sustainable):
        return "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, OVERHUNT_FLAG, text, LOCAL_HUNT_OVERDRAW_SUFFIX,
        ]
    return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, YIELD_TOOLTIP_RENEWABLE]

## Cancel the active targeting (banner Cancel / Esc / right-click all route here).
func cancel_active_targeting() -> void:
    _cancel_pending_move_band()
    _cancel_pending_send_expedition()
    _cancel_pending_send_hunt_expedition()

## Lower-left version overlay showing the client build and the streamed server build,
## so the running builds can be confirmed at a glance. Mouse-transparent so it never
## intercepts map clicks.
func _setup_build_overlay() -> void:
    _build_label = Label.new()
    _build_label.name = "BuildOverlay"
    _build_label.anchor_left = 0.0
    _build_label.anchor_right = 0.0
    _build_label.anchor_top = 1.0
    _build_label.anchor_bottom = 1.0
    _build_label.offset_left = 8.0
    _build_label.offset_top = -26.0
    _build_label.offset_right = 480.0
    _build_label.offset_bottom = -6.0
    _build_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    _build_label.add_theme_color_override("font_color", Color(0.85, 0.9, 1.0, 0.65))
    add_child(_build_label)
    _refresh_build_overlay()

func _refresh_build_overlay() -> void:
    if _build_label != null:
        _build_label.text = "build  cli %s · srv %s" % [CLIENT_BUILD, _server_build]

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
    # Dependency ratio = dependents (children + elders) per 100 working-age.
    var dependency := 0
    if working > 0:
        dependency = int(round(float(children + elders) / float(working) * 100.0))
    else:
        dependency = 999
    demographics_label.text = "Pop %d  👶%d 🛠%d 🧓%d  dep %d/100" % [total, children, working, elders, dependency]
    # A high dependency ratio (more mouths than hands) is the warning state.
    demographics_label.add_theme_color_override("font_color", _dependency_color(working, dependency))

## Show the player faction's count of discovered Wondrous Sites as a compact top-bar readout
## (`◈ Discoveries N  ⛰ ⛲`, appending the distinct site glyphs so landmark vs settle_site reads
## at a glance). Hidden until at least one site is known.
func update_discoveries(discovered_variant: Variant) -> void:
    if discoveries_label == null:
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
        discoveries_label.visible = false
        return
    discoveries_label.visible = true
    var glyphs: Array[String] = []
    for site in sites:
        if not (site is Dictionary):
            continue
        var glyph := String((site as Dictionary).get("glyph", "")).strip_edges()
        if glyph != "" and not glyphs.has(glyph):
            glyphs.append(glyph)
    var suffix := ""
    if not glyphs.is_empty():
        suffix = "  %s" % " ".join(glyphs)
    discoveries_label.text = "%s Discoveries %d%s" % [DISCOVERIES_GLYPH, sites.size(), suffix]
    discoveries_label.add_theme_color_override("font_color", HudStyle.SIGNAL)

## Show the player faction's intensification-ladder knowledge (Cultivation / Herding) as a
## compact top-bar block-glyph meter, mirroring the Sedentarization readout. Each track is
## hidden until the faction begins learning it (the snapshot row is sparse); a completed
## track reads "✔ known" (SIGNAL) instead of the bar.
func update_intensification(intensification_variant: Variant) -> void:
    _ingest_intensification(intensification_variant)
    if intensification_label == null:
        return
    var cultivation := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_CULTIVATION)
    var herding := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_HERDING)
    var segments: Array[String] = []
    var all_known := true
    if cultivation > 0.0:
        segments.append("Cultivation %s" % _knowledge_meter_text(cultivation))
        all_known = all_known and cultivation >= 1.0
    if herding > 0.0:
        segments.append("Herding %s" % _knowledge_meter_text(herding))
        all_known = all_known and herding >= 1.0
    if segments.is_empty():
        intensification_label.visible = false
        return
    intensification_label.visible = true
    intensification_label.text = INTENSIFICATION_SEGMENT_SEP.join(segments)
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
        var current := {
            KNOWLEDGE_TRACK_CULTIVATION: float(row.get(KNOWLEDGE_TRACK_CULTIVATION, 0.0)),
            KNOWLEDGE_TRACK_HERDING: float(row.get(KNOWLEDGE_TRACK_HERDING, 0.0)),
        }
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

## One knowledge track's readout: the block-glyph bar + "learning" while in progress,
## a "✔ known" badge once complete. `progress` is 0..1.
func _knowledge_meter_text(progress: float) -> String:
    if progress >= 1.0:
        return "✔ known"
    return "%s learning" % _meter_bar(progress * 100.0)

## Tint the dependency readout: amber when dependents outnumber workers, cyan when there is a
## healthy labor surplus, neutral otherwise.
func _dependency_color(working: int, dependency: int) -> Color:
    if working <= 0 or dependency >= 100:
        return HudStyle.WARN
    if dependency <= 60:
        return HudStyle.SIGNAL
    return HudStyle.INK_DIM

## A 10-cell block-glyph bar for a 0–100 score.
func _meter_bar(score: float) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * 10.0))
    return "▰".repeat(filled) + "▱".repeat(10 - filled)

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
    emit_signal("next_turn_requested", 1)

# ---- Early-Game Labor allocation (slice 3b) --------------------------------
# Source-centric worker allocation for the single player band. The allocation panel
# (band drawer), the herd "assign hunters" controls, and the tile "assign foragers"
# controls are all built at runtime here; each emits `assign_labor_requested` (Main
# formats the `assign_labor …` command). Clear-all reuses `cancel_order_requested`.

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
## falling back to idle when the cap is absent/0 (mirrors _build_send_expedition_section' party_max).
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

## Workers currently foraging a specific in-range tile; 0 when unstaffed.
func _workers_for_forage(band: Dictionary, x: int, y: int) -> int:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_FORAGE \
                and int(a.get("target_x", -1)) == x and int(a.get("target_y", -1)) == y:
            return int(a.get("workers", 0))
    return 0

## Workers currently hunting a specific herd; 0 when unstaffed.
func _workers_for_hunt(band: Dictionary, herd_id: String) -> int:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
            return int(a.get("workers", 0))
    return 0

## The take policy of the band's existing hunt on `herd_id`, else the default.
func _policy_for_hunt(band: Dictionary, herd_id: String) -> String:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
            var policy := String(a.get("policy", "")).strip_edges().to_lower()
            # HUNT_POLICY_OPTIONS, not the extractive four: a herd already being Corralled must
            # re-seed the compose picker as Corral, or re-staffing it would silently drop the pen.
            if policy in HUNT_POLICY_OPTIONS:
                return policy
    return DEFAULT_HUNT_POLICY

## The take policy of the band's existing forage on (x,y), else the default.
func _policy_for_forage(band: Dictionary, x: int, y: int) -> String:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_FORAGE \
                and int(a.get("target_x", -1)) == x and int(a.get("target_y", -1)) == y:
            var policy := String(a.get("policy", "")).strip_edges().to_lower()
            # FORAGE_POLICY_OPTIONS, not the extractive four: a patch already being Cultivated must
            # re-seed the compose picker as Cultivate, or re-staffing it would silently drop the
            # investment back to Sustain (and the patch would go feral).
            if policy in FORAGE_POLICY_OPTIONS:
                return policy
    return DEFAULT_HUNT_POLICY

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
        }
    return merged

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
func _build_worker_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, pending: bool = false, warn: bool = false, tooltip: String = "", note: String = "", on_focus_source: Callable = Callable(), status: String = "") -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    # Pending is a state of the ORDER, so it wins the glyph slot over whatever the action is doing.
    var status_key := FoodIcons.STATUS_PENDING if pending else status
    var row_tooltip := _append_status_tooltip(tooltip, status_key)
    if row_tooltip != "":
        row.tooltip_text = row_tooltip
    var row_text := label_text + _row_glyph_suffix(FoodIcons.for_status(status_key))
    var row_ink: Color = HudStyle.WARN if pending else HudStyle.INK
    var name_label: Control
    if on_focus_source.is_valid():
        var link := Button.new()
        link.text = row_text
        link.alignment = HORIZONTAL_ALIGNMENT_LEFT
        HudStyle.apply_link_button(link, row_ink)
        link.tooltip_text = (row_tooltip + TOOLTIP_LINE_SEPARATOR if row_tooltip != "" else "") + SOURCE_ROW_FOCUS_HINT
        link.pressed.connect(func() -> void: on_focus_source.call())
        name_label = link
    else:
        var plain := Label.new()
        plain.text = row_text
        plain.add_theme_color_override("font_color", row_ink)
        if row_tooltip != "":
            plain.tooltip_text = row_tooltip
        name_label = plain
    row.add_child(name_label)
    # Overhunting flag: a WARN-tinted ⚠ sits directly after the label (before the stepper), so an
    # overdrawn herd row pops without recoloring the whole label. Forage never trips this.
    if warn:
        var warn_label := Label.new()
        warn_label.text = OVERHUNT_FLAG
        warn_label.add_theme_color_override("font_color", HudStyle.WARN)
        if row_tooltip != "":
            warn_label.tooltip_text = row_tooltip
        row.add_child(warn_label)
    # Overstaffing note ("· only 1 of 5 working"): WARN-tinted, sits after the label/⚠ so the wasted
    # labor reads at a glance without recoloring the whole row. Deliberately NOT the ⚠ flag — that
    # means "overdrawing" (ecological); this means "extra workers idle here" (see
    # `_source_yield_readout`). The tooltip carries the full explanation.
    if note != "":
        var note_label := Label.new()
        note_label.text = note
        note_label.add_theme_color_override("font_color", HudStyle.WARN)
        if row_tooltip != "":
            note_label.tooltip_text = row_tooltip
        row.add_child(note_label)
    # A spacer (not name_label's expand) pushes the −/+ stepper to the right edge, keeping the
    # label + ⚠ adjacent at the left.
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(spacer)
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
    return row

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
    if bool(m.get("has_yield", false)):
        var actual := float(m.get("actual_yield", 0.0))
        var sustainable := float(m.get("sustainable_yield", 0.0))
        # A source overdraws when its actual take exceeds its renewable-sustainable ceiling. Forage on
        # Sustain gathers at the patch's regrowth (actual == sustainable → never trips, reads
        # "· renewable"); a Surplus/Market/Eradicate forage patch OR an over-hunted herd pushes actual
        # above sustainable → the ⚠ flag. ONE definition of the test (`_is_overdraw`), shared with the
        # local-hunt yield preview so the row and the preview can never disagree.
        warn = _is_overdraw(actual, sustainable)
        var renewable := kind == LABOR_KIND_FORAGE and not warn
        tooltip = "Actual %s" % _format_yield(actual)
        if renewable:
            tooltip += YIELD_TOOLTIP_RENEWABLE
        else:
            tooltip += " · Sustainable %s" % _format_yield(sustainable)
            if warn:
                tooltip += YIELD_TOOLTIP_OVERDRAW
        label_suffix = " %s" % _format_yield(actual)
    # Overstaffing: fewer workers were needed than are assigned, so the remainder produced nothing
    # here. `workers_needed == 0` means "unknown" (rehydrated) → no note.
    var note := ""
    var workers := int(m.get("workers", 0))
    var needed := int(m.get("workers_needed", 0))
    if needed > 0 and workers > needed:
        note = OVERSTAFF_NOTE_FORMAT % [needed, workers]
        tooltip = OVERSTAFF_TOOLTIP if tooltip == "" \
            else tooltip + TOOLTIP_LINE_SEPARATOR + OVERSTAFF_TOOLTIP
    return {"label_suffix": label_suffix, "warn": warn, "note": note, "tooltip": tooltip}

## PRE-COMMIT FORECAST (the compose-time counterpart to `_source_yield_readout`'s post-hoc note).
## Pull the source's per-worker yield + the take ceiling for `policy` — both food/turn at its
## CURRENT biomass, at output_multiplier 1.0. `src` is a herd dict (bare keys) or a tile_info (the
## patch's fields, `patch_`-prefixed); `known` is false for a dead-season source or an older
## snapshot that carries no forecast fields, in which case callers show no row and apply no cap.
## An INVESTMENT policy additionally carries `payoff` (the tended/corral yield the preparation buys)
## and `investment: true`, so `_forecast_yield_row` can state the deal instead of one number.
func _forecast_inputs(src: Dictionary, prefix: String, policy: String) -> Dictionary:
    var per_worker := float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0))
    var policy_key: String = policy if policy in FORECAST_CEILING_KEYS else DEFAULT_HUNT_POLICY
    var ceiling := float(src.get(prefix + String(FORECAST_CEILING_KEYS[policy_key]), 0.0))
    var investment: bool = policy_key in FORECAST_PAYOFF_KEYS
    var payoff := 0.0
    if investment:
        payoff = float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[policy_key]), 0.0))
    # The rung's RUNNING COST (Corral only — the pen's feed). `feed_rung` says the payoff is a GROSS
    # figure that a per-turn cost is paid out of; `feed` is that cost, and is 0 — i.e. unknown, not
    # free — while the herd is still un-penned (see FORECAST_FEED_KEYS).
    var feed_rung: bool = policy_key in FORECAST_FEED_KEYS
    var feed := 0.0
    if feed_rung:
        feed = float(src.get(prefix + String(FORECAST_FEED_KEYS[policy_key]), 0.0))
    return {
        "per_worker": per_worker,
        "ceiling": ceiling,
        "payoff": payoff,
        "investment": investment,
        "feed_rung": feed_rung,
        "feed": feed,
        "known": per_worker >= FORECAST_MIN_PER_WORKER,
    }

## Workers beyond this produce nothing at this source under the selected policy —
## ceil(ceiling / per_worker). MAX_USEFUL_UNBOUNDED when there's no forecast data. A tended patch /
## corralled herd reports every ceiling == per_worker, so this collapses to 1 (policy irrelevant).
func _max_useful_workers(forecast: Dictionary) -> int:
    if not bool(forecast.get("known", false)):
        return MAX_USEFUL_UNBOUNDED
    return int(ceilf(float(forecast["ceiling"]) / float(forecast["per_worker"])))

## The take `workers` would ACTUALLY produce here: min(workers × per_worker, ceiling), scaled by the
## acting band's output multiplier (the sim exports the forecast at 1.0).
func _expected_yield(forecast: Dictionary, workers: int, band: Dictionary) -> float:
    var raw := minf(float(workers) * float(forecast.get("per_worker", 0.0)),
        float(forecast.get("ceiling", 0.0)))
    return raw * float(band.get("output_multiplier", OUTPUT_FULL))

## Cap the worker stepper at what the source can absorb: min(the band's assignable workers,
## max-useful). Returns `{cap, note}` — `note` is set ONLY when max-useful is the binding cap, so a
## dead `+` button is always explained rather than mysterious (the idle-worker cap explains itself).
func _forecast_worker_cap(forecast: Dictionary, assignable: int) -> Dictionary:
    var useful := _max_useful_workers(forecast)
    if useful == MAX_USEFUL_UNBOUNDED or useful >= assignable:
        return {"cap": assignable, "note": ""}
    var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## The live "Expected yield: +0.48 /turn" row on the assign controls. Food income → HEALTHY green,
## matching the map's per-source yield annotations and the Food line's income tint. Under an
## INVESTMENT policy (Cultivate/Corral) it states the deal instead — "Preparing: +0.09 /turn → then
## +1.20 /turn" — so the up-front cost AND the payoff are visible BEFORE the player commits. Both
## halves are scaled by the acting band's output multiplier, exactly as the plain forecast is.
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
    if bool(forecast.get("investment", false)):
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
    else:
        row.text = FORECAST_LABEL_FORMAT % expected
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
    return float(band.get("food_income", 0.0)) \
        - float(band.get("food_consumption", 0.0)) \
        - _band_pen_feed(band)

## What this band paid to feed its pens this turn (food/turn). 0 for a band that keeps no corral.
func _band_pen_feed(band: Dictionary) -> float:
    return float(band.get("pen_feed_upkeep", 0.0))

## True when the band carries a meaningful food flow (income, consumption, or pen feed above the
## floor) — so a decode miss reads as "no flow" (net readout + breakdown omitted,
## not zeroed).
func _band_has_food_flow(band: Dictionary) -> bool:
    return float(band.get("food_income", 0.0)) >= FOOD_FLOW_MIN \
        or float(band.get("food_consumption", 0.0)) >= FOOD_FLOW_MIN \
        or _band_pen_feed(band) >= FOOD_FLOW_MIN

## Sum of per-source `actual_yield` (food/turn) across this band's labor assignments of one kind —
## the category total behind the Food breakdown (Gathered = forage, Hunted = hunt).
func _sum_actual_yield(band: Dictionary, kind: String) -> float:
    var total := 0.0
    for a in _labor_assignments_of(band):
        if a is Dictionary and String((a as Dictionary).get("kind", "")).strip_edges().to_lower() == kind:
            total += float((a as Dictionary).get("actual_yield", 0.0))
    return total

## Food is "concerning" (breakdown auto-shown) when the larder is net-draining OR the runway is
## below the warn threshold — mirroring `_morale_is_concerning`'s below-warn / falling gate.
func _food_is_concerning(band: Dictionary) -> bool:
    var days := float(band.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
    return _band_net_food(band) < 0.0 \
        or (BandFoodStatus.is_limited(days) and days < BandFoodStatus.warn_days())

## Per-row-per-band expand-override key.
func _breakdown_key(kind: String, band: Dictionary) -> String:
    return "%s:%d" % [kind, int(band.get("entity", -1))]

## Effective expand state of a band's Food/Morale breakdown: the user's per-band override if set,
## else that row's concerning default (auto-open when concerning, closed when healthy). Shared by
## both disclosure rows — the only per-kind bit is which "concerning" gate to consult.
func _breakdown_open_for(kind: String, band: Dictionary) -> bool:
    var key := _breakdown_key(kind, band)
    if _breakdown_expanded.has(key):
        return bool(_breakdown_expanded[key])
    return _food_is_concerning(band) if kind == BREAKDOWN_KIND_FOOD else _morale_is_concerning(band)

## Register a summary row (`row_label`, e.g. "Food"/"Morale") as a click-to-expand disclosure so
## `_format_detail_bbcode` renders its caret + clickable meta; returns whether it's currently open
## (so the caller appends the breakdown sub-lines). Shared by both disclosure rows.
func _register_disclosure(row_label: String, kind: String, band: Dictionary) -> bool:
    var open := _breakdown_open_for(kind, band)
    _disclosure_state[row_label] = {"kind": kind, "open": open}
    return open

## The category breakdown sub-lines under Food, one indented row per present category, mirroring the
## morale breakdown: `    ▲ +0.48  Gathered` / `    ▲ +0.46  Hunted` / `    ▼ −0.68  Eaten (people)`
## / `    ▼ −1.74  🐄 Pen feed (animals)` (income ▲ green, debits ▼ amber via the shared
## indented-sub-line tint). Only categories above the floor — a band with no pen shows no feed row.
##
## THREE kinds of row, not two: the pen's feed is a debit on the same larder as the people's meals,
## but it is a DIFFERENT decision (shrink the herd vs starve the band), so it gets its own line.
func _food_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var gathered := _sum_actual_yield(band, LABOR_KIND_FORAGE)
    if gathered >= FOOD_FLOW_MIN:
        lines.append(_food_breakdown_row(gathered, FOOD_LABEL_GATHERED))
    var hunted := _sum_actual_yield(band, LABOR_KIND_HUNT)
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

## Meta dispatcher for the summary-row disclosures (Food/Morale): parses the clicked row kind from
## the `[url]` meta and toggles that row's per-band expand override, then re-renders. `is_panel`
## routes the re-render to the dockable Band/City panel vs the Occupants-card drawer.
func _on_detail_meta_clicked(meta: Variant, is_panel: bool) -> void:
    var payload := String(meta)
    if not payload.begins_with(BREAKDOWN_TOGGLE_META_PREFIX):
        return
    var kind := payload.substr(BREAKDOWN_TOGGLE_META_PREFIX.length())
    var band: Dictionary = _panel_band if is_panel else _selected_unit
    if band.is_empty():
        return
    _breakdown_expanded[_breakdown_key(kind, band)] = not _breakdown_open_for(kind, band)
    if is_panel:
        _render_band_into_panel(_panel_band)
    else:
        _render_occupant_drawer()

## A fresh section-block VBox: the discrete, self-contained unit the Band/City panel arranges (a
## vertical stack when tall, a column-flow when wide). Rows are added into it exactly as they used to
## be added into the flat allocation container — only the parent node changes.
func _make_alloc_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", ALLOC_BLOCK_SEPARATION)
    return block

## Build the labor-allocation UI into `target` (the legacy flat host — the Occupants card's
## %AllocationPanel, used by the no-panel `ui_preview` harness). `target` defaults to %AllocationPanel.
## Delegates the rows to `_build_allocation_sections` and appends the section blocks into the flat
## host, so the wiring lives in ONE place (the section builder), shared with the Band/City panel.
func _build_allocation_panel(band: Dictionary, target: VBoxContainer = null) -> void:
    var container: VBoxContainer = target if target != null else allocation_panel
    if container == null:
        return
    for child in container.get_children():
        child.queue_free()
    var is_player := not band.is_empty() and _is_player_unit(band)
    container.visible = is_player
    if not is_player:
        return
    # The legacy flat host rebuilds itself in place on a local-state edit (party size / send policy).
    var rebuild := func() -> void: _build_allocation_panel(band, container)
    for block in _build_allocation_sections(band, rebuild):
        container.add_child(block)

## Build the band's labor allocation as an ordered list of discrete **section blocks** (Workers /
## Current actions / Band roles / Orders / Send expedition), returned for the caller to host — the
## Band/City panel arranges them into a tall stack or a wide column-flow; the legacy flat host just
## stacks them. The per-row wiring (worker steppers, `_emit_assign_labor` closures, Move/Clear
## handlers, the expedition outfit) is byte-for-byte the pre-refactor logic — only each row's parent
## changed from one flat container to its section block. `rebuild` is called by the local-state
## controls (party stepper / send-policy picker) to re-render the host; labor edits re-render
## themselves through `_after_pending_change`.
## `with_population_header` hosts the `Population N · Workers W (Idle I)` line as the allocation
## stack's first block. The Band/City panel passes **false**: that line is the band's identity, not an
## allocation section, and riding along with the blocks it rendered wherever CURRENT ACTIONS did —
## stranded between Active expeditions and Current actions. There it lives in the summary's identity
## grid instead (`_unit_summary_lines(unit, in_panel = true)`), off the SAME `_effective_idle`. The
## legacy flat in-card host (no dock injected — the HUD-only ui_preview fallback) still shows it here,
## since that path renders no identity grid of its own.
func _build_allocation_sections(band: Dictionary, rebuild: Callable, with_population_header: bool = true) -> Array:
    var blocks: Array = []
    # Idle counts OPTIMISTICALLY (confirmed idle overlaid with any pending changes) so the
    # math reflects a just-issued assignment immediately.
    var idle := _effective_idle(band)
    var can_add := idle > 0
    # Workers block — clarified header: population (all people) vs the working-age labor split, so
    # nobody expects "Idle" to equal the 30 people — only the ~16 workers labor (children/elders eat
    # but don't work). E.g. "Population 30 · Workers 16 (Idle 16)".
    if with_population_header:
        var workers_block := _make_alloc_block()
        var header := Label.new()
        header.text = WORKERS_HEADER_FORMAT % [
            int(band.get("size", 0)), int(band.get("working_age", 0)), idle]
        header.add_theme_color_override("font_color", HudStyle.SIGNAL if can_add else HudStyle.INK_DIM)
        header.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        header.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        workers_block.add_child(header)
        blocks.append(workers_block)
    # "Current actions" block — the report of what each group is doing (confirmed + optimistic).
    var actions_block := _make_alloc_block()
    actions_block.add_child(_alloc_section_label(ALLOC_HEADER_ACTIONS))
    var merged := _effective_worker_map(band)
    var has_source := false
    for key in merged:
        var m: Dictionary = merged[key]
        var kind := String(m.get("kind", "")).strip_edges().to_lower()
        var workers := int(m.get("workers", 0))
        var pending := bool(m.get("pending", false))
        # Per-source yield readout: the actual take headlines the row, sustainable lives in the
        # tooltip, and a WARN ⚠ flags overhunting. Pending rows have no confirmed yield → no number.
        var yld := _source_yield_readout(m, kind)
        # Show a source row when it's staffed, or while its removal/change is still pending.
        if kind == LABOR_KIND_FORAGE and (workers > 0 or pending):
            has_source = true
            var fx := int(m.get("x", -1))
            var fy := int(m.get("y", -1))
            # Policy is now populated for forage assignments (sim writes the string field). The row
            # carries it as the shared POLICY GLYPH (`FoodIcons.for_policy` — the same icon on the
            # picker button and the map's yield label), not the old "[sustain]" word; the tooltip
            # spells it out. An assignment whose policy is unset falls back to no glyph. Re-staffing
            # via the stepper preserves the policy (default Sustain when absent).
            var fpolicy := String(m.get("policy", "")).strip_edges().to_lower()
            var forage_policy_glyph := _row_glyph_suffix(FoodIcons.for_policy(fpolicy)) \
                if fpolicy in FORAGE_POLICY_OPTIONS else ""
            var forage_emit_policy := fpolicy if fpolicy in FORAGE_POLICY_OPTIONS else DEFAULT_HUNT_POLICY
            # Lead with the resource glyph the map draws on that tile (FoodIcons — one source of
            # truth), so a source reads identically in the panel and on the map. Unknown module → "".
            var forage_icon := _source_icon_prefix(_food_module_icon(fx, fy))
            actions_block.add_child(_build_worker_stepper(
                "%sForage (%d, %d)%s%s" % [forage_icon, fx, fy, yld.label_suffix, forage_policy_glyph],
                workers, can_add,
                func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_FORAGE, n, fx, fy, "", forage_emit_policy),
                pending, yld.warn,
                _join_tooltip_lines([yld.tooltip, _policy_hint(kind, fpolicy)]), yld.note,
                # A forage patch is a fixed tile: the assignment's own target IS its live location.
                func() -> void: _focus_labor_source(fx, fy),
                # A confirmed local forage row has no sim phase — it is simply working.
                FoodIcons.STATUS_WORKING))
        elif kind == LABOR_KIND_HUNT and (workers > 0 or pending):
            has_source = true
            var herd_id := String(m.get("herd_id", ""))
            var hx := int(m.get("x", -1))
            var hy := int(m.get("y", -1))
            var policy := String(m.get("policy", ""))
            if not (policy in HUNT_POLICY_OPTIONS):
                policy = _policy_for_hunt(band, herd_id)
            # Same species glyph the map's herd marker uses (FoodIcons.for_herd, keyed off the herd
            # label/species) — the panel row and the map marker read as the same animal.
            var herd_label := _herd_label_for_id(herd_id)
            var hunt_icon := _source_icon_prefix(FoodIcons.for_herd(herd_label))
            actions_block.add_child(_build_worker_stepper(
                "%sHunt %s%s%s" % [
                    hunt_icon, herd_label, yld.label_suffix,
                    _row_glyph_suffix(FoodIcons.for_policy(policy))],
                workers, can_add,
                func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_HUNT, n, hx, hy, herd_id, policy),
                pending, yld.warn,
                _join_tooltip_lines([yld.tooltip, _policy_hint(kind, policy)]), yld.note,
                # Herds MIGRATE, so resolve the herd's live tile at CLICK time (hx/hy is only the
                # assignment's launch-time target, kept as the fallback for an unknown herd).
                func() -> void: _focus_hunt_source(herd_id, hx, hy),
                # A confirmed local hunt row has no sim phase — it is simply working.
                FoodIcons.STATUS_WORKING))
    if not has_source:
        actions_block.add_child(_alloc_hint_label(ALLOC_NO_SOURCES_HINT))
    blocks.append(actions_block)
    # "Band roles" block — Scout + Warrior are standing band-wide roles: always shown (even at 0
    # workers), each with a one-line hint so the −/+ steppers read as "this is how you staff this role".
    var roles_block := _make_alloc_block()
    roles_block.add_child(_alloc_section_label(ALLOC_HEADER_ROLES))
    var scout_eff := _effective_role_workers(band, LABOR_KIND_SCOUT)
    roles_block.add_child(_build_worker_stepper(
        "Scout", int(scout_eff.get("workers", 0)), can_add,
        func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_SCOUT, n, -1, -1, "", ""),
        bool(scout_eff.get("pending", false))))
    roles_block.add_child(_alloc_hint_label(SCOUT_ROLE_HINT))
    var warrior_eff := _effective_role_workers(band, LABOR_KIND_WARRIOR)
    roles_block.add_child(_build_worker_stepper(
        "Warrior", int(warrior_eff.get("workers", 0)), can_add,
        func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_WARRIOR, n, -1, -1, "", ""),
        bool(warrior_eff.get("pending", false))))
    roles_block.add_child(_alloc_hint_label(WARRIOR_ROLE_HINT))
    blocks.append(roles_block)
    # "Orders" block — Move / Clear all.
    var orders_block := _make_alloc_block()
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "primary")
    move_btn.tooltip_text = "Relocate the band, then click a destination tile."
    move_btn.pressed.connect(_on_move_band_pressed)
    actions.add_child(move_btn)
    var clear_btn := Button.new()
    clear_btn.text = "Clear all"
    HudStyle.apply_button(clear_btn, "ghost")
    clear_btn.tooltip_text = "Return every worker to idle (clears all assignments)."
    # Nothing to clear when no source is staffed AND every worker is already idle (roles included).
    clear_btn.disabled = not has_source and idle >= int(band.get("working_age", 0))
    clear_btn.pressed.connect(_on_clear_all_pressed.bind(band))
    actions.add_child(clear_btn)
    orders_block.add_child(actions)
    blocks.append(orders_block)
    # "Send expedition" block — detach a party from this band's idle workers (omitted when idle == 0).
    var send_block := _build_send_expedition_section(band, idle, rebuild)
    if send_block != null:
        blocks.append(send_block)
    return blocks

## Outfit affordance section block (docs/plan_exploration_and_sites.md §2): a party-size stepper
## (1..party_max, party_max = min(idle_workers, max_expedition_party_size)) + a "Send scouting
## expedition" button that enters tile-targeting, plus the hunt policy picker + "Send hunting
## expedition". Returns null when the band has no idle workers to spare. `rebuild` re-renders the
## host when the local party size / send policy changes (those don't go through `_emit_assign_labor`).
## The server still rejects a genuinely over-cap request with a feed message as a backstop.
func _build_send_expedition_section(band: Dictionary, idle: int, rebuild: Callable) -> VBoxContainer:
    if idle <= 0:
        return null
    var block := _make_alloc_block()
    block.add_child(_alloc_section_label(SEND_EXPEDITION_SECTION))
    # The party max is the smaller of the band's idle workers and the server's hard party-size cap
    # (from the expedition config). Guard defensively: a missing/0 cap (older server, or the field
    # absent) falls back to idle so the stepper is never clamped to 0.
    var cap := int(band.get("max_expedition_party_size", 0))
    var party_max: int = mini(idle, cap) if cap > 0 else idle
    # Clamp the persisted party size into 1..party_max (both can shrink between renders).
    _send_expedition_count = clampi(_send_expedition_count, WORKER_STEP, party_max)
    block.add_child(_build_worker_stepper(
        "Party", _send_expedition_count, _send_expedition_count < party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, WORKER_STEP, party_max)
            rebuild.call()))
    # Both expedition verbs share the one party stepper above (they detach the same workers); the
    # scout targets a tile, the hunt targets a herd.
    var send_btn := Button.new()
    send_btn.text = SEND_EXPEDITION_BUTTON
    HudStyle.apply_button(send_btn, "primary")
    send_btn.tooltip_text = SEND_EXPEDITION_HINT
    send_btn.pressed.connect(func() -> void: _on_send_expedition_pressed(band, _send_expedition_count))
    block.add_child(send_btn)
    # Hunt verb: a policy radio (Sustain/Surplus/Market/Eradicate, default Sustain) + a one-line
    # behaviour hint for the picked policy, then the launch button. The policy is the trailing arg.
    if not (_send_hunt_policy in LABOR_HUNT_POLICIES):
        _send_hunt_policy = DEFAULT_HUNT_POLICY
    block.add_child(_build_policy_picker(func(policy: String) -> void:
        _send_hunt_policy = policy
        rebuild.call(), _send_hunt_policy))
    block.add_child(_alloc_hint_label(String(SEND_HUNT_POLICY_HINTS.get(_send_hunt_policy, ""))))
    var hunt_btn := Button.new()
    hunt_btn.text = SEND_HUNT_EXPEDITION_BUTTON
    HudStyle.apply_button(hunt_btn, "primary")
    hunt_btn.tooltip_text = SEND_HUNT_EXPEDITION_HINT
    hunt_btn.pressed.connect(func() -> void: _on_send_hunt_expedition_pressed(band, _send_expedition_count, _send_hunt_policy))
    block.add_child(hunt_btn)
    return block

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

## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary) -> void:
    if herd_assign_controls == null:
        return
    for child in herd_assign_controls.get_children():
        child.queue_free()
    var resolved := _resolve_assign_band()
    var can_assign := bool(herd.get("huntable", false)) and not resolved.is_empty()
    herd_assign_controls.visible = can_assign
    if not can_assign:
        return
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
    var title := Label.new()
    title.text = "Assign hunters" + ("  (now %d%s)" % [current, " · pending" if pending else ""] if current > 0 or pending else "")
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
    herd_assign_controls.add_child(title)
    # Which band supplies the hunters (above the worker/party stepper, so it reads "which band →
    # how many workers"). Switching bands re-runs the distance-aware branch below for that band.
    herd_assign_controls.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _hunt_assign_band = int(picked.get("entity", -1))
        _build_herd_assign_controls(herd)))
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
    var forecast := _forecast_inputs(herd, HERD_FORECAST_PREFIX, _hunt_assign_policy)
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
    var capped := {"cap": assignable, "note": ""} if is_expedition \
        else _forecast_worker_cap(forecast, assignable)
    var cap := int(capped["cap"])
    _hunt_assign_count = clampi(_hunt_assign_count, 0, cap)
    # On an expedition the party stepper is a real decision (a bigger party carries bigger packs, so
    # stepping UP can turn a working trip impossible) — warn on the row itself when the next size up is.
    var stepper_tooltip := _hunt_step_up_tooltip(band, herd, _hunt_assign_policy, _hunt_assign_count) \
        if is_expedition else ""
    herd_assign_controls.add_child(_build_worker_stepper(
        "Party" if is_expedition else "Hunters", _hunt_assign_count, _hunt_assign_count < cap,
        func(n: int) -> void:
            _hunt_assign_count = clampi(n, 0, cap)
            _build_herd_assign_controls(herd),
        false, false, stepper_tooltip))
    var cap_note := String(capped["note"])
    if cap_note != "":
        herd_assign_controls.add_child(_alloc_hint_label(cap_note))
    herd_assign_controls.add_child(_build_policy_picker(func(policy: String) -> void:
        _hunt_assign_policy = policy
        _build_herd_assign_controls(herd), _hunt_assign_policy, hunt_options, hunt_gates))
    # The policy hint is rendered per BRANCH below, never here: a resident band and a detached party
    # earn DIFFERENT payoffs from the same policy word (the band tames the herd and trades the take;
    # an expedition's Hunting arm credits food only), so one shared hint line under the picker would
    # promise the expedition player a payoff the sim never pays.
    if forecast_active:
        herd_assign_controls.add_child(
            _forecast_yield_row(forecast, _hunt_assign_count, band))
    if is_expedition:
        herd_assign_controls.add_child(_alloc_hint_label(
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
            herd_assign_controls.add_child(_forecast_label(forecast_line))
        # The row-scanned refusal — computed ONCE and used for both the button tooltip and the reason
        # line, and identical to what the targeting flow posts to the command feed.
        var impossible := _hunt_trip_impossible(trip)
        var reason := _hunt_impossible_reason(
            band, herd, _hunt_assign_policy, _hunt_assign_count) if impossible else ""
        _style_send_hunt_button(assign_btn, trip, reason)
        # The reason is spelled out beside the button too — a disabled control's tooltip is easy to
        # miss, and the named alternative (a party size that DOES fill) is the actionable half.
        if impossible:
            herd_assign_controls.add_child(_alloc_hint_label(reason))
    else:
        # What this policy DOES for a resident band (the forecast line below carries the number; this
        # carries the consequence — above all Sustain's husbandry-toward-livestock payoff, which is
        # otherwise invisible). Deliberately NOT the expedition hints: a party earns neither.
        herd_assign_controls.add_child(_alloc_hint_label(
            String(LOCAL_HUNT_POLICY_HINTS.get(_hunt_assign_policy, ""))))
        # LIVE per-turn yield for the standing assignment being composed (no carry cap on a local
        # hunt, so turns-to-fill is meaningless — food/turn is the number that decides it).
        # EXTRACTIVE rungs ONLY — the investment rung (Corral) is answered by the dip→payoff row above
        # (`forecast_active`), and rendering both put two rows with the same number on the panel. See
        # the ONE-yield-row-per-rung note there.
        if not bool(forecast["investment"]):
            var yield_line := _local_hunt_preview_bbcode(
                band, herd, _hunt_assign_policy, _hunt_assign_count)
            if yield_line != "":
                herd_assign_controls.add_child(_forecast_label(yield_line))
        assign_btn.text = ASSIGN_LOCAL_HUNT_BUTTON
        HudStyle.apply_button(assign_btn, "primary")
    if is_expedition:
        # A hunting expedition needs a positive party; a local hunt allows 0 (removes the assignment).
        # `_style_send_hunt_button` already disabled it when the trip is impossible; a positive party
        # is the other precondition. (`or` — never clear a disable the style step set.)
        assign_btn.disabled = assign_btn.disabled or _hunt_assign_count <= 0
        assign_btn.pressed.connect(func() -> void:
            if _hunt_assign_count <= 0 or _hunt_trip_impossible(
                    _hunt_trip_forecast(band, herd, _hunt_assign_policy, _hunt_assign_count)):
                return
            emit_signal("send_hunt_expedition_requested", {
                "faction": int(band.get("faction", PLAYER_FACTION_ID)),
                "band": int(band.get("entity", -1)),
                "party_workers": _hunt_assign_count,
                "fauna_id": herd_id,
                "policy": _hunt_assign_policy if _hunt_assign_policy in LABOR_HUNT_POLICIES else DEFAULT_HUNT_POLICY,
            }))
    else:
        assign_btn.pressed.connect(func() -> void:
            _emit_assign_labor(band, LABOR_KIND_HUNT, _hunt_assign_count,
                herd_x, herd_y, herd_id, _hunt_assign_policy))
    herd_assign_controls.add_child(assign_btn)

## Style the hunt-expedition send button from the live forecast. Two treatments, and the line between
## them is the point:
##   SLOW (finite ETA past the warn threshold) — a real tradeoff. "armed" amber + a label that NAMES
##     the cost (`Send Anyway (≈54 turns)`), but ENABLED: the player is told, then trusted. Likewise
##     a denial mission (`Send (delivers no food)`) — it does exactly what it says.
##   IMPOSSIBLE (cannot fill within the horizon) — not a tradeoff, a mistake with no upside. DISABLED,
##     with the reason and the way out. Offering this button would be offering the player a mistake.
## No confirm dialogs either way.
func _style_send_hunt_button(button: Button, forecast: Dictionary, reason: String) -> void:
    var fills := bool(forecast.get("fills", false))
    # IMPOSSIBLE — the one blocked case. Disabled, and it says WHY plus what to do instead (the button
    # is the last thing the player looks at before clicking, so the reason belongs on it). The reason is
    # the caller's row-scanned sentence, so button tooltip and panel line are the SAME words.
    if _hunt_trip_impossible(forecast):
        button.text = SEND_HUNT_IMPOSSIBLE_BUTTON
        button.disabled = true
        button.tooltip_text = reason
        HudStyle.apply_button(button, "ghost")
        return
    var warned := bool(forecast.get("available", false)) \
        and (not fills or not bool(forecast.get("viable", true)))
    if not warned:
        button.text = SEND_HUNTING_EXPEDITION_BUTTON
        HudStyle.apply_button(button, "primary")
        return
    if bool(forecast.get("denial", false)):
        # Eradicate: no food comes home, but that IS the mission — state the deal, don't cry failure.
        # Explicitly NOT blocked: it never fills by design, and blocking it would ban denial outright.
        button.text = SEND_HUNT_DENIAL_BUTTON
    else:
        # The only warned non-denial case left is a trip that fills, just slowly. A warned trip
        # that does NOT fill is the impossible case, and that already returned disabled above.
        button.text = SEND_HUNT_ANYWAY_TURNS_FORMAT % int(forecast.get("turns", 0))
    HudStyle.apply_button(button, "armed")

## The trip provably cannot fill the party's packs: the sim's estimate for THIS (policy, party size)
## says it delivers food but never fills within the forecast horizon. The single definition of the
## blocked case — both entry points (panel button + targeting click) gate on it.
func _hunt_trip_impossible(forecast: Dictionary) -> bool:
    return bool(forecast.get("available", false)) and bool(forecast.get("impossible", false))

## The `hunt_trip_estimates` key the sim exports a (policy, party size) estimate under. One definition —
## the lookup and the row scan must agree on the key format or the scan silently finds nothing.
func _hunt_estimate_key(policy: String, workers: int) -> String:
    return "%s%s%d" % [policy, HUNT_ESTIMATE_KEY_SEPARATOR, workers]

## Scan the CURRENT policy's ROW of the sim's per-(policy, party-size) estimate table and pick the party
## size to RECOMMEND when the dialed-in one can't fill. Returns `{workers, turns, viable}` — or `{}` when
## the whole row is zeros (NO size fills at all). A table SCAN, not arithmetic: the client's zero-math
## property for expeditions is preserved exactly (see `_hunt_trip_forecast`).
##
## The objective is the LARGEST party that fills AND IS VIABLE (`turns <= expedition_viability_warn_turns`,
## the band's own exported lever). Largest-that-*fills* alone is WRONG — on Red Deer + Surplus it names 7
## (49 turns), a trip this very UI flags "too slow to be worth sending" the moment you dial it: we'd be
## recommending an option we elsewhere warn against. The party of 5 fills the same packs in 5 turns. Among
## trips the UI considers worth making, the biggest party hauls the most food — that is the only coherent
## recommendation. It is NOT "one smaller" either: the row is not monotonic (Surplus fills at 1–5 in 5
## turns, 6 in 23, 7 in 49, never at 8), so only the row itself knows the answer.
##
## Fallback when NOTHING on the row is viable (Rabbit + Surplus: only a lone hunter fills, in 23 turns):
## still name the best available, but pick the FASTEST-filling size, not the largest — with no viable
## option left, time dominates haul — and the caller words it as the marginal trip it is (`viable: false`).
##
## Capped at what this band could actually field (`_expedition_party_cap`): naming a party of 7 to a band
## with 4 idle workers would be advice it cannot take.
## Denial rows (`delivers_food == false` — Eradicate) never "fill" BY DESIGN and are skipped: an Eradicate
## trip is not impossible, so this scan must never speak for it.
func _recommended_party(band: Dictionary, herd: Dictionary, policy: String) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return {}
    var cap := _expedition_party_cap(band)
    # A warn threshold of 0 means the server sent none — judge nothing, so every filling size counts as
    # viable (mirrors `_hunt_trip_forecast`, which reports the turns and withholds the verdict).
    var warn_turns := int(band.get("expedition_viability_warn_turns", 0))
    var best_viable := {}
    var fastest := {}
    for workers in range(1, cap + 1):
        var entry_variant: Variant = (estimates_variant as Dictionary).get(
            _hunt_estimate_key(policy, workers), null)
        if not (entry_variant is Dictionary):
            continue
        var entry := entry_variant as Dictionary
        if not bool(entry.get("delivers_food", false)):
            continue
        var turns := int(entry.get("turns_to_fill", 0))
        if turns <= 0:
            continue
        if warn_turns <= 0 or turns <= warn_turns:
            # Keep scanning: we want the LARGEST viable size, so the last one that qualifies wins.
            best_viable = {"workers": workers, "turns": turns, "viable": true}
        # Strictly faster only, so a tie keeps the SMALLEST fast size... but a tie on turns means the
        # bigger party hauls more in the same time, so prefer the larger: `<=`.
        if fastest.is_empty() or turns <= int(fastest.get("turns", 0)):
            fastest = {"workers": workers, "turns": turns, "viable": false}
    return best_viable if not best_viable.is_empty() else fastest

## The ONE sentence spoken about a blocked trip — shared verbatim by the herd panel (reason line +
## disabled-button tooltip) and the targeting-click command-feed refusal, so the two entry points can
## never disagree. Says something TRUE and SPECIFIC by scanning the policy's row (above) instead of
## dispensing generic "try a smaller party" advice, which is wrong whenever the row is all zeros.
func _hunt_impossible_reason(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    var herd_name := _herd_display_name(herd)
    var alternative := _recommended_party(band, herd, policy)
    if alternative.is_empty():
        return SEND_HUNT_IMPOSSIBLE_NO_SIZE_REASON % herd_name
    var alt_workers := int(alternative.get("workers", 0))
    var alt_turns := int(alternative.get("turns", 0))
    # Nothing on the row is viable: name the best there is, but don't dress a trip the UI would flag
    # "too slow to be worth sending" up as a fix.
    if not bool(alternative.get("viable", false)):
        return SEND_HUNT_IMPOSSIBLE_SLOW_REASON % [herd_name, workers, alt_workers, alt_turns]
    return SEND_HUNT_IMPOSSIBLE_ALTERNATIVE_REASON % [herd_name, workers, alt_workers, alt_turns]

## Tooltip for the hunt-expedition PARTY stepper when the very next size up is impossible — the stepper is
## a real decision (a bigger party can break a working trip), and this is where that bites. "" when
## stepping up is fine (or impossible to do), so the tooltip only exists when it has something to say.
func _hunt_step_up_tooltip(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    var next_workers := workers + WORKER_STEP
    if workers <= 0 or next_workers > _expedition_party_cap(band):
        return ""
    var next_forecast := _hunt_trip_forecast(band, herd, policy, next_workers)
    if not _hunt_trip_impossible(next_forecast):
        return ""
    return SEND_HUNT_STEP_UP_IMPOSSIBLE_TOOLTIP % [next_workers, _herd_display_name(herd)]

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

## Unmet prerequisites for the FORAGE investment rung (Cultivate), keyed policy → Array[String] of
## reasons (each already carrying its own remedy). Empty when every rung is available. Mirrors the
## sim's `assign_labor` validation: the faction must have fully learned Cultivation, and only a
## Thriving patch can be prepared.
func _forage_policy_gates(tile_info: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(LABOR_POLICY_SUSTAIN)
    var reasons: Array[String] = []
    var cultivation := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_CULTIVATION)
    if cultivation < KNOWLEDGE_COMPLETE:
        reasons.append(GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT % [
            _progress_percent(cultivation), sustain_icon])
    var phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if phase != ECOLOGY_PHASE_THRIVING:
        var phase_label := phase.capitalize() if phase != "" else GATE_PHASE_UNKNOWN_LABEL
        reasons.append(GATE_REASON_PATCH_THRIVING_FORMAT % phase_label)
    if reasons.is_empty():
        return {}
    return {LABOR_POLICY_CULTIVATE: reasons}

## Unmet prerequisites for the HUNT investment rung (Corral), keyed policy → Array[String] of reasons.
## The herd twin of `_forage_policy_gates`: the faction must have fully learned Herding, and the herd
## must be fully domesticated before a pen can be built for it.
func _hunt_policy_gates(herd: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(LABOR_POLICY_SUSTAIN)
    var reasons: Array[String] = []
    var herding := _faction_knowledge(PLAYER_FACTION_ID, KNOWLEDGE_TRACK_HERDING)
    if herding < KNOWLEDGE_COMPLETE:
        reasons.append(GATE_REASON_HERDING_KNOWLEDGE_FORMAT % [
            _progress_percent(herding), sustain_icon])
    var domestication := float(herd.get("domestication", 0.0))
    if domestication < DOMESTICATION_COMPLETE:
        reasons.append(GATE_REASON_HERD_DOMESTICATED_FORMAT % [
            _progress_percent(domestication), sustain_icon])
    if reasons.is_empty():
        return {}
    return {LABOR_POLICY_CORRAL: reasons}

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
    gates: Dictionary = {}) -> VBoxContainer:
    var current := selected if selected != "" else _hunt_assign_policy
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    for policy in options:
        var policy_key := String(policy)
        var icon := FoodIcons.for_policy(policy_key)
        var reasons := _gate_reasons(gates, policy_key)
        var btn := Button.new()
        # Glyph + name, from the shared FoodIcons policy map — the same icon the map's yield labels
        # append, so a policy reads identically on the picker and on the worked tile/herd.
        btn.text = "%s%s" % [_source_icon_prefix(icon), policy_key.capitalize()]
        HudStyle.apply_button(btn, "primary" if policy_key == current else "ghost")
        btn.disabled = not reasons.is_empty()
        if btn.disabled:
            btn.tooltip_text = GATE_REASON_TOOLTIP_SEPARATOR.join(reasons)
        else:
            btn.pressed.connect(func() -> void: on_pick.call(policy_key))
        row.add_child(btn)
    block.add_child(row)
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
func _build_forage_assign_controls(tile_info: Dictionary) -> void:
    if forage_assign_controls == null:
        return
    for child in forage_assign_controls.get_children():
        child.queue_free()
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    var resolved := _resolve_assign_band()
    var can_assign := module_key != "" and not resolved.is_empty() and not _tile_contents_unseen(tile_info)
    forage_assign_controls.visible = can_assign
    if not can_assign:
        return
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
    var label := String(tile_info.get("food_module_label", module_key)).strip_edges()
    if label == "":
        label = module_key.capitalize()
    var title := Label.new()
    title.text = "Assign foragers — %s" % label + ("  (now %d%s)" % [current, " · pending" if pending else ""] if current > 0 or pending else "")
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
    forage_assign_controls.add_child(title)
    # Which band supplies the foragers (above the stepper). Switching re-runs the range check below
    # for that band.
    forage_assign_controls.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _forage_assign_band = int(picked.get("entity", -1))
        _build_forage_assign_controls(tile_info)))
    # Forage take policy (Sustain/Surplus/Market/Eradicate, default Sustain) — reuses the hunt policy
    # radio + option set (LABOR_HUNT_POLICIES) but shows forage-appropriate behaviour hints. Persisted
    # across re-renders like the hunt policy; re-seeded from current staffing when the tile changes.
    var forage_gates := _forage_policy_gates(tile_info)
    # A gated rung can never be the composed policy — the patch may have left Thriving under a
    # standing Cultivate selection, so re-validate every render, not just on a tile change.
    if not (_forage_assign_policy in FORAGE_POLICY_OPTIONS) \
            or not _gate_reasons(forage_gates, _forage_assign_policy).is_empty():
        _forage_assign_policy = DEFAULT_HUNT_POLICY
    forage_assign_controls.add_child(_build_policy_picker(func(policy: String) -> void:
        _forage_assign_policy = policy
        _build_forage_assign_controls(tile_info), _forage_assign_policy, FORAGE_POLICY_OPTIONS, forage_gates))
    forage_assign_controls.add_child(_alloc_hint_label(String(FORAGE_POLICY_HINTS.get(_forage_assign_policy, ""))))
    # Pre-commit forecast: the patch's per-worker yield + the SELECTED policy's ceiling cap the
    # stepper at max-useful workers, so the player CAN'T over-assign while composing. Both the
    # stepper and the policy picker re-render these controls, so the cap and the expected-yield row
    # below recompute on every change (a Market/Eradicate ceiling is higher than Sustain's, so
    # switching policy moves the cap).
    var forecast := _forecast_inputs(tile_info, FORAGE_FORECAST_PREFIX, _forage_assign_policy)
    var capped := _forecast_worker_cap(forecast, _assignable_forage_workers(band, x, y))
    var cap := int(capped["cap"])
    _forage_assign_count = clampi(_forage_assign_count, 0, cap)
    forage_assign_controls.add_child(_build_worker_stepper(
        "Foragers", _forage_assign_count, _forage_assign_count < cap,
        func(n: int) -> void:
            _forage_assign_count = clampi(n, 0, cap)
            _build_forage_assign_controls(tile_info)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        forage_assign_controls.add_child(_alloc_hint_label(cap_note))
    if bool(forecast["known"]):
        forage_assign_controls.add_child(
            _forecast_yield_row(forecast, _forage_assign_count, band))
    # Range-aware: foraging is stationary gathering (there is NO forage-expedition alternative), so a
    # tile beyond the SELECTED band's work_range DISABLES the button + shows an out-of-range hint,
    # rather than a fallback. Distance is wrap-aware from the picked band's OWN tile — distance,
    # work_range, and the target band all key off `band` explicitly (never the faction's default band).
    var band_tile := _band_tile(band)
    var work_range := int(band.get("work_range", 0))
    var distance := _hex_distance_wrapped(band_tile.x, band_tile.y, x, y)
    var out_of_range := distance >= 0 and distance > work_range
    if out_of_range:
        forage_assign_controls.add_child(_alloc_hint_label(
            "(%d,%d) is %d tiles away — beyond this band's forage range (%d)." % [x, y, distance, work_range]))
    var assign_btn := Button.new()
    assign_btn.text = FORAGE_ASSIGN_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range
    assign_btn.pressed.connect(func() -> void:
        _emit_assign_labor(band, LABOR_KIND_FORAGE, _forage_assign_count, x, y, "", _forage_assign_policy))
    forage_assign_controls.add_child(assign_btn)

## Move-band: enter tile-targeting; the destination click emits move_band_requested.
func _on_move_band_pressed() -> void:
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

## Send-hunt-expedition (docs/plan_exploration_and_sites.md §2b): outfit `band` with `party_workers`
## and enter HERD-targeting; the next click resolves to a huntable herd on the clicked hex and emits
## send_hunt_expedition_requested. Mirrors the scout send flow, but targets a herd not a tile.
func _on_send_hunt_expedition_pressed(band: Dictionary, party_workers: int, policy: String) -> void:
    if band.is_empty() or party_workers <= 0:
        return
    var chosen := policy if policy in LABOR_HUNT_POLICIES else DEFAULT_HUNT_POLICY
    _pending_send_hunt_expedition = {
        "band": band.duplicate(true), "party_workers": party_workers, "policy": chosen,
    }
    _refresh_targeting()

func _cancel_pending_send_hunt_expedition() -> void:
    if _pending_send_hunt_expedition.is_empty():
        return
    _pending_send_hunt_expedition = {}
    _refresh_targeting()

func _try_dispatch_pending_send_hunt_expedition(tile_info: Dictionary) -> void:
    if _pending_send_hunt_expedition.is_empty() or tile_info.is_empty():
        return
    # Resolve the target from the clicked hex's herds (herd markers occupy the hex, so a click on a
    # herd lands here). Pick the first huntable herd on the tile; if none, keep targeting and nudge.
    var herd := _huntable_herd_on_tile(tile_info)
    var fauna_id := String(herd.get("id", "")).strip_edges()
    if fauna_id == "":
        _note_command_feed("Hunt expedition", "No huntable herd there — click on a herd.")
        return
    var band: Dictionary = _pending_send_hunt_expedition.get("band", {})
    var workers := int(_pending_send_hunt_expedition.get("party_workers", 0))
    var policy := String(_pending_send_hunt_expedition.get("policy", DEFAULT_HUNT_POLICY))
    # The SAME block as the panel button, at the other entry point: a trip that provably can't fill is
    # not dispatchable here either. Stay in targeting and SAY WHY (never swallow the click silently),
    # exactly like the "no huntable herd here" nudge above — the player can pick another herd or cancel.
    # The refusal is the SAME row-scanned sentence the panel shows, from the one helper — the two entry
    # points cannot drift into contradicting each other about which party sizes work.
    if _hunt_trip_impossible(_hunt_trip_forecast(band, herd, policy, workers)):
        _note_command_feed("Hunt expedition",
            _hunt_impossible_reason(band, herd, policy, workers))
        return
    emit_signal("send_hunt_expedition_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": int(band.get("entity", -1)),
        "party_workers": int(_pending_send_hunt_expedition.get("party_workers", 0)),
        "fauna_id": fauna_id,
        "policy": String(_pending_send_hunt_expedition.get("policy", DEFAULT_HUNT_POLICY)),
    })
    _pending_send_hunt_expedition = {}
    _refresh_targeting()

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

## Clear-all: return every worker to idle (repurposed cancel_order).
func _on_clear_all_pressed(band: Dictionary) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band)

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

func reset_command_feed() -> void:
    _command_feed.reset()
func show_tile_selection(tile_info: Dictionary) -> void:
    _selected_tile_info = tile_info.duplicate(true) if tile_info is Dictionary else {}
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(_selected_tile_info, {}, {})
    _try_dispatch_pending_move_band(_selected_tile_info)
    _try_dispatch_pending_send_expedition(_selected_tile_info)
    _try_dispatch_pending_send_hunt_expedition(_selected_tile_info)

func notify_hex_selected(tile_info: Dictionary) -> void:
    if tile_info.is_empty():
        return
    _try_dispatch_pending_move_band(tile_info)
    _try_dispatch_pending_send_expedition(tile_info)
    _try_dispatch_pending_send_hunt_expedition(tile_info)

func show_unit_selection(unit_data: Dictionary) -> void:
    var tile_info: Dictionary = {}
    var tile_variant: Variant = unit_data.get("tile_info", {})
    if tile_variant is Dictionary:
        tile_info = (tile_variant as Dictionary).duplicate(true)
    else:
        tile_info = _selected_tile_info
    _selected_tile_info = tile_info
    _selected_unit = unit_data.duplicate(true)
    _selected_herd.clear()
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, _selected_unit, {})

func show_herd_selection(herd_data: Dictionary) -> void:
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
            _adopt_tile_info_from(_selected_unit)
            _render_selection_panel(_selected_tile_info, _selected_unit, {})
        "herd":
            _selected_herd = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _adopt_tile_info_from(_selected_herd)
            _render_selection_panel(_selected_tile_info, {}, _selected_herd)
        "tile":
            _selected_tile_info = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _selected_herd.clear()
            _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
            _render_selection_panel(_selected_tile_info, {}, {})
        _:
            # Selected occupant vanished (e.g. the band expired). Drop to its last tile
            # if known, else hide both cards. Intentionally does not touch pending state.
            _selected_unit.clear()
            _selected_herd.clear()
            if _selected_tile_info.is_empty():
                if tile_panel != null:
                    tile_panel.visible = false
                if forage_assign_controls != null:
                    forage_assign_controls.visible = false
                _set_occupants_relevant(false)
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
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _assemble_roster(_selected_tile_info)
    _render_tile_card(_selected_tile_info)
    _render_occupants_card()

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

## The Tile card: the place. Terrain rows + the "Assign foragers" controls (its only
## action). Kind stays "Tile" even when an occupant is selected.
func _render_tile_card(tile_info: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    tile_panel.visible = true
    tile_panel.set_card_kind("Tile")
    var title_text := "—"
    if not tile_info.is_empty():
        title_text = "(%d, %d)" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]
    tile_panel.set_card_title(title_text)
    tile_detail.text = _format_detail_bbcode(_tile_terrain_lines(tile_info))
    _build_forage_assign_controls(tile_info)

## The tile's `Sight: …` row — which of the three FoW states this hex is in, in plain words.
## "" (FoW off) yields no row.
func _tile_sight_line(visibility_state: String) -> String:
    var value := ""
    match visibility_state:
        VISIBILITY_ACTIVE:
            value = TILE_SIGHT_ACTIVE
        VISIBILITY_DISCOVERED:
            value = TILE_SIGHT_REMEMBERED
        VISIBILITY_UNEXPLORED:
            value = TILE_SIGHT_UNEXPLORED
        _:
            return ""
    return "%s: %s" % [TILE_SIGHT_KEY, value]

## Value tint for the Sight row: in-sight reads live (SIGNAL cyan — the HUD's "this is current"
## color), while both unseen states read dim (INK_DIM). The row states what you KNOW, not what is
## wrong, so it never borrows the WARN/DANGER palette.
func _sight_value_hex(value: String) -> String:
    return HudStyle.SIGNAL_HEX if value == TILE_SIGHT_ACTIVE else HudStyle.INK_DIM_HEX

## True when the hex's LIVE contents (occupants, workable sources) are unknowable right now — a
## remembered or a never-seen tile. MapView already redacts them from `tile_info` at source (it strips
## `herds`/`units`/`food_module*` and fog-gates `_herds_on_tile`); this re-reads the SAME state flag it
## tagged — not a second visibility test — so every consumer stays honest regardless of who feeds it.
## Terrain rows are exempt by design: geography is remembered knowledge, live contents are not.
func _tile_contents_unseen(tile_info: Dictionary) -> bool:
    var state := String(tile_info.get("visibility_state", ""))
    return state == VISIBILITY_DISCOVERED or state == VISIBILITY_UNEXPLORED

## The Occupants card: a selectable roster of bands + wildlife on the hex, plus a
## detail drawer for the selected occupant. Hidden (dock reflows) on an empty hex that the player
## can actually SEE; on an unseen hex it stays up and states that the contents are unknown.
func _render_occupants_card() -> void:
    if occupants_panel == null:
        return
    if _roster_units.is_empty() and _roster_herds.is_empty():
        if _tile_contents_unseen(_selected_tile_info):
            _render_occupants_unknown()
            return
        _set_occupants_relevant(false)
        if allocation_panel != null:
            allocation_panel.visible = false
        if herd_assign_controls != null:
            herd_assign_controls.visible = false
        return
    _set_occupants_relevant(true)
    occupants_panel.set_card_kind("Occupants")
    occupants_panel.set_card_title("on this hex")
    # Auto-select the first occupant on a fresh tile click (nothing selected yet),
    # driving the drawer + the map ring through the same signal a click would.
    if _selected_unit.is_empty() and _selected_herd.is_empty():
        if not _roster_units.is_empty():
            _selected_unit = (_roster_units[0] as Dictionary).duplicate(true)
            emit_signal("roster_occupant_selected", "unit", int(_selected_unit.get("entity", -1)))
        else:
            _selected_herd = (_roster_herds[0] as Dictionary).duplicate(true)
            emit_signal("roster_occupant_selected", "herd", String(_selected_herd.get("id", "")))
    _rebuild_roster()
    _render_occupant_drawer()

## The Occupants card on a hex the player CANNOT see: the roster is emptied and the drawer states
## that the hex's live contents are unknown. This is the whole point of the fog gate — an absent
## roster would silently claim "nothing here", which is a different (and unearned) statement.
func _render_occupants_unknown() -> void:
    _set_occupants_relevant(true)
    occupants_panel.set_card_kind("Occupants")
    occupants_panel.set_card_title(OCCUPANTS_UNKNOWN_TITLE)
    if roster_list != null:
        for child in roster_list.get_children():
            child.queue_free()
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false
    if occupant_detail != null:
        var message := OCCUPANTS_UNKNOWN_UNEXPLORED \
            if String(_selected_tile_info.get("visibility_state", "")) == VISIBILITY_UNEXPLORED \
            else OCCUPANTS_UNKNOWN_REMEMBERED
        occupant_detail.text = _format_detail_bbcode([message])

func _set_occupants_relevant(relevant: bool) -> void:
    if left_dock != null:
        left_dock.set_relevant(occupants_panel, relevant)
    elif occupants_panel != null:
        occupants_panel.visible = relevant

## Terrain-only tile readout: FoW redaction, Biome/Height/Tags, and the tile's
## gather module relabeled `Forage:` (occupant/harvester/scout listings moved to
## the roster + drawer). Keeps the forage-pending hint here (Forage is a tile action).
func _tile_terrain_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    # Fog of War: never-seen tiles reveal nothing; remembered (Discovered) tiles
    # show only their last-known terrain, not current contents. See MapView
    # _apply_visibility_to_info, which redacts the hidden fields before this runs.
    var visibility_state := String(tile_info.get("visibility_state", ""))
    if visibility_state == VISIBILITY_UNEXPLORED:
        lines.append(_tile_sight_line(visibility_state))
        lines.append("Not yet scouted — send a band to reveal this area.")
        return lines
    # The Sight row leads the card: it frames everything under it as either live truth (In sight) or
    # remembered knowledge (Remembered), so the terrain rows are never mistaken for current contents.
    lines.append(_tile_sight_line(visibility_state))
    var terrain_label := String(tile_info.get("terrain_label", "Unknown"))
    lines.append("Biome: %s" % terrain_label)
    if tile_info.has("height_display"):
        lines.append("Height: %s" % String(tile_info["height_display"]))
    var tags_text := String(tile_info.get("tags_text", "none"))
    lines.append("Tags: %s" % tags_text)
    # Habitability is terrain-intrinsic (band-independent), so it's fine on a remembered
    # tile — surface it before the discovered early-return. Only when the snapshot carries
    # the field (a rehydrated tile may lack it) so we never invent a rating.
    if tile_info.has("habitability"):
        var drain := float(tile_info["habitability"])
        lines.append("Habitability: %s" % TileHabitability.rating_for(drain))
    # Climate is the tile's latitude+elevation temperature band (informational, not a
    # warning). Terrain-intrinsic, so fine on a remembered tile; only when the snapshot
    # carries the field (a rehydrated tile may lack it) so we never invent a band.
    if tile_info.has("temperature"):
        var temperature := float(tile_info["temperature"])
        lines.append("Climate: %s" % TileClimate.band_for(temperature))
    # Hex-edge rivers — which SIDES of this tile carry water (the sides a crossing cost will
    # apply to). Terrain-intrinsic permanent geography, so it renders before the discovered
    # early-return, like Habitability/Climate. Guarded on the key so a rehydrated snapshot
    # degrades to no row instead of a wrong one; RiverEdges returns [] on a riverless tile, so it
    # never emits an empty "River:" label. Same formatter the map hover tooltip uses.
    if tile_info.has("river_edges"):
        lines.append_array(RiverEdges.summary_lines(int(tile_info["river_edges"])))
    # A discovered Wondrous Site is known knowledge — fine on a remembered tile — so surface
    # it before the discovered early-return. Only when the field is present.
    var site_name := String(tile_info.get("site_name", "")).strip_edges()
    if site_name != "":
        lines.append("Site: %s" % site_name)
    # PASTURE — the animal-edible stock (see PASTURE_KEY). Surfaced BEFORE the discovered
    # early-return because, like the biome and the habitability above it, grass is a property of the
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
    return lines

# ---- Occupants roster ------------------------------------------------------

## Rebuild the roster rows: a `Bands (N)` sub-group and a `Wildlife (N)` sub-group,
## each a dim uppercase header + one selectable row per occupant. The row matching
## the current selection is styled as selected.
func _rebuild_roster() -> void:
    if roster_list == null:
        return
    for child in roster_list.get_children():
        child.queue_free()
    if not _roster_units.is_empty():
        roster_list.add_child(_roster_group_header("Bands", _roster_units.size()))
        for unit in _roster_units:
            roster_list.add_child(_build_band_row(unit))
    if not _roster_herds.is_empty():
        roster_list.add_child(_roster_group_header("Wildlife", _roster_herds.size()))
        for herd in _roster_herds:
            roster_list.add_child(_build_herd_row(herd))
    # Reached only when your OWN unit is on a hex you can't see (everything else was redacted): say so,
    # or the lone row would read as "and nothing else is here" — which we cannot know.
    if _tile_contents_unseen(_selected_tile_info):
        roster_list.add_child(_alloc_hint_label(OCCUPANTS_UNSEEN_OTHERS_HINT))

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
        dot_color = BandFoodStatus.color_for_days(float(unit.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS)))
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

## One selectable wildlife row: an ecology-tier dot, the species glyph + name, and
## the size-class label. Selecting it drives the drawer + the map ring to the herd.
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
    # The fauna id as a DIM meta suffix (the roster's existing muted-ink convention, same label the
    # size class uses). It appears nowhere else in the UI and it is the handle the command feed
    # names, so it must survive the `Herd: Red Deer (game_deer_07)` row's removal — as secondary
    # text beside the name, not as a detail row restating it.
    if herd_id != "":
        row.add_child(_roster_meta_label(herd_id))
    var size_class := String(herd.get("size_class", "")).strip_edges()
    if size_class != "":
        row.add_child(_roster_meta_label("%s game" % size_class.capitalize()))
    button.tooltip_text = label
    button.add_child(row)
    button.pressed.connect(_on_roster_row_selected.bind("herd", herd_id))
    return button

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

func _roster_name_label(text: String, selected: bool) -> Label:
    var label := Label.new()
    label.text = text
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK if selected else HudStyle.INK_DIM)
    return label

func _roster_meta_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
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
    if kind == "unit":
        _selected_unit = _find_roster_unit(int(id)).duplicate(true)
        _selected_herd = {}
    else:
        _selected_herd = _find_roster_herd(String(id)).duplicate(true)
        _selected_unit = {}
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _rebuild_roster()
    _render_occupant_drawer()

## The detail drawer + action buttons for the currently-selected occupant.
func _render_occupant_drawer() -> void:
    if occupant_detail == null:
        return
    _selected_band_food_days = NAN
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
        occupant_detail.text = ""
        occupant_detail.visible = false
        if allocation_panel != null:
            allocation_panel.visible = false
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
        _build_herd_assign_controls(_selected_herd)
    elif herd_assign_controls != null:
        herd_assign_controls.visible = false

## Render a player band's detail + labor allocation into the dockable Band/City panel and
## populate its header/cycler. The single place the panel's subject is set — shared by roster/map
## selection (`_render_occupant_drawer`) and the per-snapshot refresh (`_refresh_panel_band`), so
## the panel is a persistent command center that survives selection changes.
func _render_band_into_panel(unit: Dictionary) -> void:
    if _band_city_panel == null or unit.is_empty():
        return
    # DEEP-COPY the subject: `_panel_band` must NOT alias `_selected_unit` (the selection
    # path passes it in), because selecting a foreign tile calls `_selected_unit.clear()` —
    # which would empty a shared dict and blank the panel on its next stepper rebuild. The
    # allocation closures below also capture this stable copy, so they keep targeting the
    # panel band regardless of the current selection.
    _panel_band = unit.duplicate(true)
    # Assemble the ordered section blocks Hud hands the panel to arrange (tall stack vs wide
    # column-flow). Ownership passes to the panel, which frees the previous render's blocks. Order
    # per docs/plan_band_panel_wide_flow.md: Summary, Active expeditions, then the allocation sections
    # (Workers / Current actions / Band roles / Orders / Send expedition).
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    var blocks: Array = []
    # Summary block — a fresh RichTextLabel per render (resets + re-sets the food/morale/output tint
    # context, then tints via bbcode). Wrapped in a section block so it columns like the rest.
    var summary_block := _make_alloc_block()
    var detail_label := RichTextLabel.new()
    detail_label.bbcode_enabled = true
    detail_label.fit_content = true
    detail_label.scroll_active = false
    detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD
    detail_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The panel's Food/Morale labels are the same click-to-expand disclosures as the Occupants drawer.
    # This RichTextLabel is rebuilt each render, so wire its `meta_clicked` here (bound is_panel = true).
    detail_label.meta_clicked.connect(_on_detail_meta_clicked.bind(true))
    # `in_panel`: the panel header already names the band + stage, so the summary drops the Unit row
    # and folds the population/workers line into the identity grid (see `_unit_summary_lines`).
    detail_label.text = _format_detail_bbcode(_unit_summary_lines(_panel_band, true))
    summary_block.add_child(detail_label)
    blocks.append(summary_block)
    # "Active expeditions" block the panel band has detached (grouped by home_band_entity); omitted
    # when the band has none.
    var exp_block := _build_panel_expeditions_block(_panel_band)
    if exp_block != null:
        blocks.append(exp_block)
    # Allocation section blocks (closures capture the stable `_panel_band`; the party/policy controls
    # re-render the panel via `_rerender_panel_allocation`, which re-runs this whole assembly).
    var rebuild := func() -> void: _rerender_panel_allocation()
    # No population header block here — the panel's identity grid carries that line (above), so it
    # can't strand itself between Active expeditions and Current actions.
    blocks.append_array(_build_allocation_sections(_panel_band, rebuild, false))
    _band_city_panel.set_band_sections(blocks)
    # Header: settlement stage glyph + name + stage label (glyph/label already flow onto the
    # marker/cohort dict; fall back to a neutral glyph when the stage is absent).
    var glyph := String(_panel_band.get("settlement_stage_icon", "")).strip_edges()
    var stage_label := String(_panel_band.get("settlement_stage_label", "")).strip_edges()
    var index := _index_of_player_band(int(_panel_band.get("entity", -1)))
    _band_city_panel.set_header(glyph, _band_display_name(_panel_band, index + 1), stage_label)
    _band_city_panel.set_cycler(index, _player_bands.size())
    # `set_band_sections` above already flipped the panel to band-present (non-empty block list);
    # just make sure it's shown.
    _band_city_panel.set_shown(true)

## Build the panel band's "Active expeditions" section block — the player expeditions whose
## `home_band_entity` matches the shown band. Its own block (separate from the allocation blocks, so
## a stepper rebuild can't clear it). Returns null when the band has detached none.
func _build_panel_expeditions_block(band: Dictionary) -> VBoxContainer:
    var band_entity := int(band.get("entity", -1))
    var rows: Array = []
    for exp_variant in _player_expeditions:
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if int(exp.get("home_band_entity", 0)) == band_entity:
            rows.append(exp)
    if rows.is_empty():
        return null
    var block := _make_alloc_block()
    block.add_child(_alloc_section_label(PANEL_EXPEDITIONS_HEADER))
    for exp in rows:
        block.add_child(_build_panel_expedition_row(exp))
    return block

## One clickable "Active expeditions" row: mission glyph + compact summary + the phase GLYPH. Click
## routes the map selection to the expedition (its detail then shows in the Occupants card's
## expedition drawer), via the same signal path a roster click uses.
## An `awaiting` row is the ONE state that keeps its words and reads WARN-amber: the party is parked
## at its objective burning provisions until the player acts, and a call to action must not hide
## behind a hover (see the action-status vocabulary).
func _build_panel_expedition_row(exp: Dictionary) -> Button:
    var phase := _expedition_phase_key(exp)
    var btn := Button.new()
    btn.text = _panel_expedition_summary(exp)
    btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
    btn.focus_mode = Control.FOCUS_NONE
    btn.clip_text = true
    btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(btn, "ghost")
    if phase == EXPEDITION_PHASE_AWAITING:
        btn.add_theme_color_override("font_color", HudStyle.WARN)
    btn.tooltip_text = _expedition_row_tooltip(exp, phase)
    var entity := int(exp.get("entity", -1))
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    btn.pressed.connect(func() -> void: _on_panel_expedition_selected(entity, x, y))
    return btn

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
## `in_panel` = rendered into the dock, which is the only host with a labor readout to give: there the
## population becomes the **Population** row carrying the labor line (`29 · Workers 14 (Idle 12)`) —
## the same reading the allocation block used to strand between Active expeditions and Current
## actions. The Occupants-card drawer (foreign bands / the no-panel fallback) has no worker breakdown
## to show for a band that isn't ours, so it states no population at all; the roster row has it.
func _unit_summary_lines(unit_data: Dictionary, in_panel: bool = false) -> Array[String]:
    if bool(unit_data.get("is_expedition", false)):
        return _expedition_summary_lines(unit_data)
    var lines: Array[String] = []
    # Disclosure carets + the tint context are rebuilt per render. Reset BOTH here, not inside
    # `_band_food_line` — a foreign band skips that call entirely (below), and a skipped Food row
    # must not inherit the previous render's caret or its food-days tint.
    _disclosure_state = {}
    _food_flow_present = false
    _selected_band_food_days = NAN
    if in_panel:
        # Idle counts OPTIMISTICALLY via the SAME `_effective_idle` the `+` stepper gates on — the
        # calculation is moved here, never forked.
        lines.append("%s: %s" % [DETAIL_ROW_POPULATION, WORKERS_VALUE_FORMAT % [
            int(unit_data.get("size", 0)), int(unit_data.get("working_age", 0)),
            _effective_idle(unit_data)]])
    # Food, like Morale below, is our OWN bands' business only. A rival's cohort carries no
    # `days_of_food`/`stores` on the wire, so rendering the row for one printed a FABRICATED
    # `Food 0 (∞)` in healthy green — the UI claiming we'd counted a larder we cannot see. A foreign
    # band shows only what we can honestly observe from outside: where it is (Position) and roughly
    # how many (its roster row's size).
    if _is_player_unit(unit_data):
        lines.append(_band_food_line(unit_data))
        # Category-aggregated food breakdown under Food: a click-to-expand disclosure (auto-shown
        # when concerning). `_band_food_line` set `_food_flow_present`; `_register_disclosure`
        # records the row so `_format_detail_bbcode` draws the caret + clickable meta.
        if _food_flow_present and _register_disclosure(DETAIL_ROW_FOOD, BREAKDOWN_KIND_FOOD, unit_data):
            lines.append_array(_food_breakdown_lines(unit_data))
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
        # Itemized morale breakdown: the SAME click-to-expand disclosure as Food (auto-shown when
        # concerning). Only offered when there's actually a breakdown to show (a contribution above
        # the epsilon, or the concerning recovery line).
        var morale_breakdown := _morale_breakdown_lines(unit_data)
        if not morale_breakdown.is_empty() \
                and _register_disclosure(DETAIL_ROW_MORALE, BREAKDOWN_KIND_MORALE, unit_data):
            lines.append_array(morale_breakdown)
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
## mission, humanized phase, party size, and carried food (from stores/daysOfFood). A hunt
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
    # days from daysOfFood. Reuse the food-days tint context (`_selected_band_food_days`, read
    # back in `_format_detail_bbcode`).
    var days: float = float(unit_data.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
    _selected_band_food_days = days
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
            lines.append("Carried: %d / %d  (%s)%s" % [carried, cap, _food_days_text(days), full_badge])
        else:
            lines.append("Carried: %d  (%s)" % [carried, _food_days_text(days)])
    else:
        lines.append("Provisions: %d  (%s)" % [carried, _food_days_text(days)])
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

## Selection-panel band food row: "Food  <provisions>  (<days>)" — provisions from
## the band's larder stores, days from `days_of_food` (∞ when not food-limited).
## Stashes the days on `_selected_band_food_days` so `_format_detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
func _band_food_line(unit_data: Dictionary) -> String:
    var days: float = float(unit_data.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
    _selected_band_food_days = days
    var provisions := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        provisions = int(round(float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))))
    var line := "Food: %d  (%s)" % [provisions, _food_days_text(days)]
    # For player bands with real flow, append the net per-turn rate (sign-tinted, inline) and mark
    # the Food label a clickable disclosure (`_food_flow_present`, read by `_format_detail_bbcode`).
    # An enemy band shows the bare larder line, exactly as before.
    _food_flow_present = false
    if _is_player_unit(unit_data) and _band_has_food_flow(unit_data):
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

## Human-readable days-of-food: the ∞ glyph when the band is not food-limited,
## otherwise a whole-day count.
func _food_days_text(days: float) -> String:
    if not BandFoodStatus.is_limited(days):
        return FOOD_UNLIMITED_GLYPH
    return "%d days" % int(round(days))

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
    # NO identity rows. The herd's own roster row above this drawer already shows the species glyph +
    # name, the `<size> game` class, AND (as a dim meta) the fauna id — so `Herd` / `Species` / `Size`
    # were the same three facts a second time (the name three times, counting the `Herd` row's
    # "Red Deer (game_deer_07)"). What follows is only what the header CAN'T show: the herd's state.
    var lines: Array[String] = []
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
    var domestication := float(herd_data.get("domestication", 0.0))
    if domestication > 0.0:
        lines.append("Husbandry: %s" % _husbandry_label(domestication))
    # A corralled herd is penned by the band (intensification ladder). SIGNAL-tinted, mirroring the
    # Husbandry/Ecology row treatment. While the keepers are still BUILDING the pen (0 < progress < 1
    # under the Corral policy) the same row reports the meter — the animal twin of the tile card's
    # "Cultivation N%" row, so the investment the player committed to is visibly under way.
    # A PENNED herd is a managed population: it eats from its keeper's larder every turn, and an
    # underfed one is shrinking right now. That is the loudest thing the drawer can say about it, so
    # the Corral row itself flips to the starving state (DANGER-tinted via `_corral_value_hex`) and a
    # "Pen feed" row states the demand and how much of it the keeper actually paid.
    var corral_progress := float(herd_data.get("corral_progress", 0.0))
    var fed_fraction := PenStatus.fed_fraction(herd_data)
    if bool(herd_data.get("corralled", false)):
        lines.append("Corral: %s" % _corral_label(CORRAL_PROGRESS_COMPLETE, true, fed_fraction))
        var upkeep := float(herd_data.get("pen_upkeep", 0.0))
        if upkeep >= FOOD_FLOW_MIN:
            lines.append("%s: %s" % [PEN_FEED_ROW, _pen_feed_label(upkeep, fed_fraction)])
    elif corral_progress > 0.0:
        lines.append("Corral: %s" % _corral_label(corral_progress, false, PenStatus.FULLY_FED))
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

## Player-facing cultivation label for a forage patch. A fully-tended patch shows a crop
## glyph; an in-progress patch shows the percentage. Mirrors `_husbandry_label`;
## `_format_detail_bbcode` tints a Tended value via `_cultivation_value_hex`.
func _cultivation_label(progress: float, cultivated: bool) -> String:
    if cultivated or progress >= 1.0:
        return "🌾 Tended Patch"
    return "%d%%" % int(round(progress * 100.0))

## BBCode hex for a "Cultivation" value: signal (positive) for a tended patch, normal ink
## while it's still being cultivated. Matched on the label from `_cultivation_label`.
func _cultivation_value_hex(value: String) -> String:
    if value.to_lower().contains("tended"):
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
                # food-days thresholds; its value carries a day count or the ∞ glyph.
                var food_value := String(kv[1])
                if not is_nan(_selected_band_food_days) and (food_value.contains("day") or food_value.contains(FOOD_UNLIMITED_GLYPH)):
                    value_hex = BandFoodStatus.hex_for_days(_selected_band_food_days)
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
            elif String(kv[0]) == "Cultivation":
                value_hex = _cultivation_value_hex(String(kv[1]))
            elif String(kv[0]) == "Corral":
                value_hex = _corral_value_hex(String(kv[1]))
            elif String(kv[0]) == PEN_FEED_ROW:
                # The pen's running feed cost: amber as a standing debit, red when it goes unpaid.
                value_hex = _pen_feed_value_hex(String(kv[1]))
            # A disclosure row (Food/Morale) renders its key as a clickable cyan `[url]` + ▸/▾ caret,
            # toggling its breakdown sub-lines via `meta_clicked` → `_on_detail_meta_clicked`. Which
            # rows are disclosures (and their open-state) is set in `_unit_summary_lines`.
            var key_cell := "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, kv[0]]
            if _disclosure_state.has(kv[0]):
                var st: Dictionary = _disclosure_state[kv[0]]
                var caret := BREAKDOWN_CARET_OPEN if bool(st.get("open", false)) else BREAKDOWN_CARET_CLOSED
                key_cell = "[url=%s%s][color=#%s]%s %s[/color][/url]" % [
                    BREAKDOWN_TOGGLE_META_PREFIX, String(st.get("kind", "")),
                    HudStyle.SIGNAL_HEX, kv[0], caret,
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
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = ""
    _selected_food_is_hunt = false
    # Keep pending move-band so the user can still choose a destination after deselecting.
    if _selected_tile_info.is_empty():
        if tile_panel != null:
            tile_panel.visible = false
        if forage_assign_controls != null:
            forage_assign_controls.visible = false
        _set_occupants_relevant(false)
    else:
        _render_selection_panel(_selected_tile_info, {}, {})
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false

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

func ingest_command_events(events_variant: Variant) -> void:
    _command_feed.ingest_events(events_variant)
func update_band_alerts(populations_variant: Variant) -> void:
    if not (populations_variant is Array):
        return
    var populations: Array = populations_variant
    var new_sizes: Dictionary = {}
    # Turn-orb attention registry: one loop over the player faction feeds three producers
    # per band (starving / losing_population / idle_workers). Pushed to the orb below, which
    # severity-sorts (critical floats up). New producers (wars/decisions/…) append here later.
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
        var days := float(entry.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
        var morale := float(entry.get("morale", 1.0))
        var morale_cause := int(entry.get("morale_cause", MORALE_CAUSE_NONE))
        var last_emigrated := int(entry.get("last_emigrated", 0))
        var x := int(entry.get("current_x", -1))
        var y := int(entry.get("current_y", -1))
        var band_name := _band_display_name(entry, band_number)
        new_sizes[entity] = size
        # Producer 1 — starving: larder below the critical threshold (red/critical).
        if BandFoodStatus.is_critical(days):
            attention.append({
                "kind": ATTENTION_KIND_STARVING,
                "severity": ATTENTION_SEVERITY_CRITICAL,
                "label": "%s starving" % band_name,
                "detail": _food_days_text(days),
                "x": x, "y": y,
            })
        # Producer 2 — losing population: shrank vs the previous snapshot (amber/warn).
        if _prev_band_sizes.has(entity) and size < int(_prev_band_sizes[entity]):
            attention.append({
                "kind": ATTENTION_KIND_LOSING_POPULATION,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": "%s losing population" % band_name,
                "detail": _decline_reason(days, morale, morale_cause, last_emigrated),
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
    if turn_orb != null:
        turn_orb.set_attention(attention)
    # This snapshot is authoritative: drop optimistic pending actions the server has now
    # processed (issued on an older turn), then let the panels render the confirmed state.
    _reconcile_pending()
    # Keep the dockable Band/City panel a persistent, live command center: shown whenever ≥1
    # player band exists, re-rendering the current _panel_band so its steppers/idle stay current.
    _refresh_panel_band()
    # Keep the on-screen allocation panel / assign controls live as the band's staffing
    # changes turn to turn (the coordinator re-renders occupant/tile cards separately, but
    # a herd/tile selection reads _player_band, which only just refreshed here).
    if not _selected_herd.is_empty():
        _build_herd_assign_controls(_selected_herd)
    elif not _selected_tile_info.is_empty() and _selected_unit.is_empty():
        _build_forage_assign_controls(_selected_tile_info)

## Why a band is shrinking: a food crisis (larder below critical) reads "starving" first;
## then, since morale no longer kills (discontent relocates people — see
## docs/plan_civ_wellbeing.md), a shrink with emigrants last turn reads "people leaving".
## Otherwise the dominant morale cause names it in plain language ("harsh terrain" /
## "harsh climate" / "unrest"). When no cause is attributed (morale steady/rising — e.g.
## a rehydrated save, or shrinkage from cold deaths / an aging cohort at healthy morale)
## only say "low morale" if morale is actually low, else leave it plain rather than
## asserting a false reason.
func _decline_reason(days: float, morale: float, morale_cause: int, last_emigrated: int) -> String:
    if BandFoodStatus.is_limited(days) and days < BandFoodStatus.critical_days():
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
## frozen on top of the panel. The hovered-hex record (which drives the targeting
## banner's hunt forecast) is dropped for the same reason — the pointer is no
## longer over a herd, so a stale forecast must not linger in the banner.
func _suppress_tooltip_over_ui() -> void:
    var over_ui_viewport := get_viewport()
    var over_ui := over_ui_viewport != null and over_ui_viewport.gui_get_hovered_control() != null
    if over_ui and not _hovered_tile_info.is_empty():
        _hovered_tile_info = {}
        _refresh_targeting()
    if tooltip_panel == null or not tooltip_panel.visible:
        return
    var viewport := get_viewport()
    if viewport != null and viewport.gui_get_hovered_control() != null:
        tooltip_panel.visible = false

## Record the hex under the pointer and, only while a hunt expedition is armed (the one flow that
## reads it), re-render the targeting banner so its forecast tracks the hovered herd. Gated so an
## ordinary hover doesn't re-emit targeting_changed on every hex the pointer crosses.
func _set_hovered_tile_info(info: Dictionary) -> void:
    _hovered_tile_info = info if info is Dictionary else {}
    if not _pending_send_hunt_expedition.is_empty():
        _refresh_targeting()

## MapView.tile_hovered lands here. Besides the hex tooltip it records the hovered hex, which is how
## the targeting banner knows WHICH herd the player is considering while a hunt expedition is armed
## (the pre-launch turns-to-fill forecast) — see _hunt_forecast_bbcode.
func show_tooltip(info: Dictionary) -> void:
    _set_hovered_tile_info(info)
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

