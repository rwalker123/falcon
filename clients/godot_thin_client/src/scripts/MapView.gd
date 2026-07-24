extends Node2D
class_name MapView

const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

signal hex_selected(col: int, row: int, terrain_id: int)
signal tile_selected(info: Dictionary)
signal overlay_legend_changed(legend: Dictionary)
signal unit_selected(unit: Dictionary)
signal herd_selected(herd: Dictionary)
## Double-click on a herd (Early-Game Labor slice 3b): a convenience that assigns the
## player band's idle workers to hunt this herd (Main → Hud.quick_assign_hunters). The
## old shift+double-click "scout" shortcut was retired with the single-task scout command.
signal herd_quick_hunt_requested(herd_id: String)
signal tile_hovered(info: Dictionary)
signal selection_cleared()
signal next_turn_requested(steps: int)
signal targeting_cancel_requested()
## Emitted whenever the map zoom factor changes (rail button, wheel, Q/E, or fit).
## The HUD renders the live zoom readout from it. `_apply_zoom` emits only on a
## real change (it early-returns on a no-op); `_fit_map_to_view` also emits after
## resetting zoom + pan, so a fit re-syncs the readout even when already at 1.0×.
signal zoom_changed(zoom_factor: float)

const LOGISTICS_COLOR := Color(0.15, 0.45, 1.0, 1.0)
const SENTIMENT_COLOR := Color(1.0, 0.35, 0.25, 1.0)
const CORRUPTION_COLOR := Color(0.92, 0.58, 0.18, 1.0)
const FOG_COLOR := Color(0.6, 0.78, 0.95, 1.0)
const CULTURE_COLOR := Color(0.72, 0.36, 0.88, 1.0)
const MILITARY_COLOR := Color(0.36, 0.7, 0.43, 1.0)
const CRISIS_COLOR := Color(0.92, 0.24, 0.46, 1.0)
const ELEVATION_LOW_COLOR := Color(0.16, 0.32, 0.78, 1.0)
const ELEVATION_MID_COLOR := Color(0.97, 0.82, 0.32, 1.0)
const ELEVATION_HIGH_COLOR := Color(0.78, 0.14, 0.18, 1.0)
# --- PASTURE (graze) overlay -------------------------------------------------------------------
# The channel paints the LAND'S GRAZE CAPACITY — "how good a pasture is this ground?" — because that
# is the question the layer exists to answer (is prairie really pasture; is forest really poor?), and
# because it is a property of the biome, not a transient. The *fill* (standing biomass ÷ capacity) is
# a different question ("how eaten-down is it?"), reported as a map-wide figure in the legend and
# per-tile on the tile card; it becomes worth its own ramp only once herds actually eat graze.
#
# ZERO PASTURE IS NOT LOW PASTURE. A desert at 8/8 (full, but marginal) and a glacier that carries no
# pasture at all are completely different facts, and a single ramp bottoming out at black renders both
# as "dark". So a zero-capacity tile leaves the ramp entirely and is painted a flat barren tone —
# water in a drowned slate (it is not ground), dead land in a bare rock-violet — while ANY positive
# capacity starts at PASTURE_POOR_COLOR, a visibly-on-the-ramp straw.
const PASTURE_OVERLAY_KEY := "pasture"
const PASTURE_POOR_COLOR := Color(0.85, 0.78, 0.42, 1.0)    # marginal grazing — dry straw
const PASTURE_RICH_COLOR := Color(0.13, 0.62, 0.24, 1.0)    # the reference pasture — deep grass green
const PASTURE_DEAD_COLOR := Color(0.34, 0.30, 0.38, 1.0)    # land that carries NO pasture (glacier/lava/rock)
const PASTURE_WATER_COLOR := Color(0.10, 0.16, 0.28, 1.0)   # water — no pasture, and not ground at all
# The Water terrain tag (bit 0 of TileState.terrain_tags — see TERRAIN_TAG_KEYS). Server truth, unlike
# the render-side `blend_class`, so it is what separates "sea" from "dead ground" in the overlay.
const PASTURE_WATER_TAG := 1 << 0
# --- FORAGE (human food) overlay ---------------------------------------------------------------
# The human-edible twin of the pasture channel. It paints the human-food CAPACITY (potential) of
# each tile's biome — "what human food could this land yield?" (seeds, nuts, tubers, fruit, fish) —
# from `TileState.forageCapacity`, cached in `tile_forage`, exactly as pasture reads `tile_graze`.
# Like pasture it is a POTENTIAL on every tile, not "where a gathering site stands".
#
# WHERE IT DIVERGES FROM PASTURE: water is NOT uniformly barren. Coastal shelves carry real FISHING
# potential and sit ON the capacity ramp, while deep ocean stays dark — so the coasts light up on
# the forage map where they are dead on the pasture map. Only genuinely-zero tiles (deep ocean,
# glacier, lava) leave the ramp for the single barren fill; there is no "land but no site" middle
# category (that was the sparse-patch model, replaced by per-tile potential).
const FORAGE_OVERLAY_KEY := "forage"
const FORAGE_POOR_COLOR := Color(0.88, 0.80, 0.44, 1.0)     # poorest human-food land — pale wheat
const FORAGE_RICH_COLOR := Color(0.18, 0.72, 0.38, 1.0)     # richest human-food land — lush leaf green
const FORAGE_BARREN_COLOR := Color(0.20, 0.21, 0.24, 1.0)   # NO human food (deep ocean, glacier, lava)
# --- DANGER overlays (Predators Phase 0) -------------------------------------------------------
# TWO derived-danger channels, both per-ENTITY properties the native decoder projects onto tiles
# (max over the herds standing on each hex). Neither is a per-tile field or a two-tone ramp: both
# ride the generic `GRID_COLOR.lerp(overlay_color, value)` path, so empty ground stays grid-colored
# and a hex with a qualifying herd glows. `hunt_danger` (attack × ferocity) is a danger-ORANGE so it
# reads apart from `threat` (attack × aggression), which keeps the harsher threat-RED.
const HUNT_DANGER_OVERLAY_KEY := "hunt_danger"
const HUNT_DANGER_OVERLAY_COLOR := Color(0.93, 0.52, 0.13, 1.0)  # danger orange
const THREAT_OVERLAY_KEY := "threat"
const THREAT_OVERLAY_COLOR := Color(0.85, 0.16, 0.16, 1.0)       # threat red
# Tile "Height" is a relative 0..100 indicator (not meters) so a player can reason
# about line of sight: a higher tile can occlude the tile behind it. Elevation is
# only a normalized 0..1 field, so height rescales the ABOVE-sea-level span into
# 0..100 (at/below sea level reads 0 — nothing occludes over open water). The sea
# level is the ACTIVE map's `sea_level`, streamed per-snapshot in the elevation
# overlay (`_elevation_sea_level`); this constant is only the fallback used until the
# first snapshot arrives (mirrors core_sim's DEFAULT_SEA_LEVEL).
const HEIGHT_DEFAULT_SEA_LEVEL := 0.6
const HEIGHT_BAR_SEGMENTS := 10
# Bright outline/fill used by the Terrain-tab "highlight all tiles of type" tool.
const TERRAIN_HIGHLIGHT_COLOR := Color(1.0, 0.25, 0.9, 1.0)
# ---------------------------------------------------------------------------
# Hex marker stack UX (see clients/godot_thin_client CLAUDE.md — Map markers).
# Two marker classes share a hex: PRIMARY (player bands) own the CENTER spotlight
# as an offset card-stack; SECONDARY (herds / food sites / wondrous sites) ring the
# hex in FIXED corner slots. The split is by source array, not a predicate:
# `_draw_primary_bands` iterates the player-band `units` array, while
# `_compute_secondary_slots` places herds / food sites / wondrous sites.
# ---------------------------------------------------------------------------
# Marker category tags (the classifier key + the value stored per secondary entry).
const MARKER_CATEGORY_BAND := "band"
const MARKER_CATEGORY_WONDER := "wonder"
const MARKER_CATEGORY_FOOD := "food"
const MARKER_CATEGORY_HERD := "herd"

# Primary band token: a settlement-stage glyph over a faction-colored nameplate banner
# (ownership cue). No faction ring or disc — the banner carries ownership; selection is
# conveyed by the selected/hovered hex outline, and the active stacked band reads by
# brightness (back cards darkened) — there is no per-token selection ring. No name label
# yet — the banner is the substrate for one.
const BAND_TOKEN_RADIUS_FACTOR := 0.34       # of hex radius — the spotlight token (was 0.30)
const BAND_TOKEN_OUTLINE_COLOR := Color(0.04, 0.05, 0.06, 0.9)
const BAND_TOKEN_OUTLINE_WIDTH := 2.0
const BAND_FACTION_FALLBACK_COLOR := Color(0.9, 0.9, 0.9, 1.0)  # unknown-faction band tint
# Settlement-stage glyph token: the (opaque, sim-supplied) glyph is drawn with the shared
# drop-shadow helper. Ownership is carried by the banner below, not a ring.
const BAND_STAGE_GLYPH_SIZE_FACTOR := 2.0     # glyph point size as a factor of the token radius
const BAND_STAGE_GLYPH_COLOR := Color(0.99, 0.99, 0.96, 1.0)
# No-stage fallback (pre-stage / missing snapshot — rare; sim assigns nomadic at size 0):
# a small neutral, NON-circular placeholder square in place of the glyph. Never a disc.
const BAND_FALLBACK_MARKER_COLOR := Color(0.55, 0.57, 0.6, 1.0)  # neutral gray, faction-agnostic
const BAND_FALLBACK_MARKER_SIZE_FACTOR := 1.1  # square side as a factor of the token radius
# Faction nameplate banner: a short faction-colored bar under the PRIMARY token (active top
# card only, far-zoom LOD-gated). Reuses the band's faction color as fill so ownership reads
# without a ring/disc. Intentionally wide enough to later host a faction/band NAME LABEL drawn
# on top of the bar — keep the width/height structured for that.
const BAND_BANNER_WIDTH_FACTOR := 2.4         # bar width as a factor of the token radius
const BAND_BANNER_HEIGHT_FACTOR := 0.5        # bar height as a factor of the token radius
const BAND_BANNER_GAP_FACTOR := 0.18          # gap below the glyph as a factor of the token radius
const BAND_BANNER_OUTLINE_COLOR := Color(0.04, 0.05, 0.06, 0.9)  # thin dark outline for legibility
const BAND_BANNER_OUTLINE_WIDTH := 1.0        # ~1px outline
const BAND_BANNER_CORNER_RADIUS_FACTOR := 0.35  # corner radius as a factor of the bar height
const BAND_TASK_ARROW_WIDTH := 2.5           # travel/task destination arrow
# Co-located bands fan into an up-right offset card stack: back cards darkened, the
# active (selected/cycled) band drawn full-brightness on top. Beyond the cap, a `×N` badge.
const BAND_STACK_MAX_CARDS := 3
const BAND_STACK_CARD_STEP := Vector2(0.10, -0.10)   # per-card offset (× hex radius)
# Behind (non-active) cards are multiplied by this tint AND drawn smaller so they read as
# shadowed/recessed *behind* the bright top card (a pseudo-3D depth cue) — this darkening +
# shrink (not the old white ring) is what marks the active band. Tint lever: RGB < 1 darkens,
# alpha < 1 fades. Scale lever: back-card token radius × this factor (< 1 pushes them "back").
const BAND_STACK_BEHIND_TINT := Color(0.28, 0.28, 0.28, 1.0)
const BAND_STACK_BEHIND_SCALE := 0.75   # back-card size vs the front card (perspective shrink)
const BAND_COUNT_BADGE_OFFSET := Vector2(0.34, 0.30)  # from token center (× hex radius), bottom-right

# Secondary edge icons: fixed corner slots around the hex (pointy-top; the top &
# bottom are sharp vertices, so slots hug the upper flanks + sides where a glyph
# reads cleanly and clears the center token + its bottom-right count badge). Filled
# deterministically by category priority (wonder → food → herd) so icons never jump
# frame-to-frame. Offsets are × hex radius from the hex center.
const SECONDARY_SLOT_OFFSETS: Array[Vector2] = [
	Vector2(-0.61, -0.56),   # upper-left
	Vector2(0.61, -0.56),    # upper-right
	Vector2(-0.78, 0.07),    # left flank
	Vector2(0.78, 0.07),     # right flank (also holds the +N overflow chip)
	Vector2(-0.51, 0.66),    # lower-left
	Vector2(0.51, 0.66),     # lower-right
]
const SECONDARY_VISIBLE_CAP := 3             # icons drawn before the +N overflow chip
const SECONDARY_ICON_SIZE_FACTOR := 0.55     # of hex radius (was ~1.05 over a backing disc)
const SECONDARY_ICON_MIN_SIZE := 10.0
const SECONDARY_ICON_COLOR := Color(0.97, 0.98, 0.94, 1.0)
# STARVING-PEN DISTRESS BADGE (docs/plan_corral_managed_population.md). A corralled herd whose keeper
# could not pay this turn's feed is SHRINKING every turn — the drawer must not be the only place that
# says so. The affordance is DRAWN GEOMETRY, never a tint or a glyph: a herd marker is a full-color
# EMOJI, so `modulate` leaves it looking like an ordinary brown animal (measured — see the rejected
# tint below), and a font ⚠ carries emoji presentation and renders as a blob at marker size (the same
# hazard that forced `MagnifierButton` and the line-art policy icons to hand-draw). So:
#   • a DANGER ring around the herd's slot (the same primitive as the food-harvest ring), and
#   • a filled DANGER disc badge on the icon's upper-right with a hand-drawn white "!".
# Driven by `PenStatus.herd_is_starving` — the same test the herd drawer's "⚠ Starving" row uses.
const HERD_DISTRESS_COLOR := HudStyle.DANGER
const HERD_DISTRESS_RING_FACTOR := 0.46        # of hex radius — just outside the food-harvest ring
const HERD_DISTRESS_RING_WIDTH := 2.5
const HERD_DISTRESS_RING_SEGMENTS := 24
# The badge, sized off the icon (not the hex) so it tracks the glyph it annotates at every zoom.
const HERD_DISTRESS_BADGE_RADIUS_FACTOR := 0.38   # of the icon size
const HERD_DISTRESS_BADGE_OFFSET_FACTOR := Vector2(0.42, -0.42)   # of the icon size, from its center
const HERD_DISTRESS_BADGE_RIM_COLOR := Color(0.12, 0.05, 0.05, 0.9)
const HERD_DISTRESS_BADGE_RIM_WIDTH := 1.5
const HERD_DISTRESS_BADGE_SEGMENTS := 16
# The hand-drawn "!" inside the badge: a tapered stem plus a dot, as fractions of the badge radius.
const HERD_DISTRESS_BANG_COLOR := Color(1.0, 1.0, 1.0, 1.0)
const HERD_DISTRESS_BANG_STEM_TOP := -0.55
const HERD_DISTRESS_BANG_STEM_BOTTOM := 0.12
const HERD_DISTRESS_BANG_STEM_WIDTH := 0.24
const HERD_DISTRESS_BANG_DOT_Y := 0.46
const HERD_DISTRESS_BANG_DOT_RADIUS := 0.15
# Legibility without the old dark backing disc: a 1px-offset drop shadow under the glyph.
const MARKER_GLYPH_SHADOW_OFFSET := Vector2(1.0, 1.0)
const MARKER_GLYPH_SHADOW_COLOR := Color(0.0, 0.0, 0.0, 0.6)
# A selected hex containing a herd is indicated ONLY by the hex outline — herds get no
# per-marker ring (it diverged from the outline when a selected herd migrated on turn-advance).
const FOOD_HARVEST_RING_FACTOR := 0.42       # active-harvest ring around a food slot icon
const FOOD_HARVEST_RING_WIDTH := 2.0
# Migration arrow: thinner, and only on the hovered/selected herd tile to cut clutter.
const HERD_MIGRATION_ARROW_COLOR := Color(0.98, 0.58, 0.18, 0.8)
const HERD_MIGRATION_ARROW_WIDTH := 1.6

# Count / overflow badge (shared dark pill: primary `×N`, secondary `+N`).
const MARKER_BADGE_BG := Color(0.05, 0.06, 0.08, 0.9)
const MARKER_BADGE_FG := Color(0.95, 0.97, 1.0, 1.0)
const MARKER_BADGE_FONT_SIZE := 11
const MARKER_BADGE_HEIGHT_FACTOR := 1.15     # pill height as a factor of glyph height
const MARKER_BADGE_PAD_X := 0.0              # a count badge is short: its round end caps ARE its padding

# Selected / hovered hex outline (replaces the old brown-circle selection feel).
const SELECTED_HEX_OUTLINE_COLOR := Color(1.0, 1.0, 1.0, 0.9)
const SELECTED_HEX_OUTLINE_WIDTH := 3.0
const HOVER_HEX_OUTLINE_COLOR := Color(1.0, 1.0, 1.0, 0.22)
const HOVER_HEX_OUTLINE_WIDTH := 1.5

# Zoom level-of-detail: below this hex radius (far zoom, tiny hexes) skip the
# secondary edge icons + overflow/count chips; draw only the primary token.
const ICON_MIN_DETAIL_RADIUS := 16.0
# Out-of-map fill behind the hex grid (matches the direct-path background clear).
const TERRAIN_BG_COLOR := Color(0.3, 0.35, 0.25, 1.0)
const GRID_COLOR := Color(0.06, 0.08, 0.12, 1.0)
const GRID_LINE_COLOR := Color(0.4, 0.4, 0.4, 0.7)
const GRID_LINE_WIDTH := 2.0
const SQRT3 := 1.7320508075688772
const SIN_60 := 0.8660254037844386
# Fog-of-War visibility discriminators on the 0.0/0.5/1.0 visibility encoding
# (Active ≈ 1.0, Discovered ≈ 0.5, Unexplored ≈ 0.0).
const FOW_VISIBLE_THRESHOLD := 0.7  # Above this a tile is Active (full color)
const FOW_EXPLORED_THRESHOLD := 0.3  # Above this a tile is at least Discovered
# Tile-info fields that describe live/current contents. They are stripped from a
# Discovered (remembered, not currently in sight) tile because the player only
# retains the terrain memory, not what is happening on the tile right now.
const FOW_DISCOVERED_HIDDEN_KEYS := [
	"food_module", "food_module_label", "food_module_weight", "food_kind",
	"cultivation_progress", "is_cultivated", "patch_has_owner", "patch_owner",
	"patch_ecology_phase", "patch_biomass", "patch_carrying_capacity",
	"patch_per_worker_yield", "patch_ceiling_sustain", "patch_ceiling_surplus",
	"patch_ceiling_market", "patch_ceiling_eradicate",
	"patch_ceiling_cultivate", "patch_tended_yield",
	# Plant rung 3 (the Field + Sow) — redacted exactly as their rung-2 twins above are: the two
	# build meters are live patch state, and the Sow forecast pair is quoted at the patch's CURRENT
	# biomass. `patch_sow_site_refusal` rides with them: it describes the GROUND (fertility + water,
	# which a remembered tile would arguably still know), but it is only ever read to gate the Sow
	# affordance — and that affordance is already withheld on a hex the player cannot see, so
	# redacting it keeps ONE rule for the whole patch payload rather than a lone exception.
	"patch_field_progress", "patch_is_field",
	"patch_ceiling_sow", "patch_field_yield", "patch_sow_site_refusal",
	"units", "herds", "unit_count", "herd_count",
	"harvest_tasks", "harvest_active", "scout_tasks", "scout_active",
]
# Fallback FoW appearance; overridden by the "fog_of_war" section of
# heightfield_config.json (see _load_fow_config).
const DEFAULT_FOW_MIST_COLOR := Color(0.45, 0.48, 0.55, 1.0)
const DEFAULT_FOW_MIST_BLEND := 0.35
const DEFAULT_FOW_FOG_FILL_COLOR := Color(0.08, 0.08, 0.12, 1.0)
# Shader-path FoW SOFTENING (heightfield_config's "fog_of_war" block; only the blend-shader path reads them —
# the per-hex CPU path is hard-edged by construction). The vis-map is per-hex/NEAREST, so an active↔discovered
# adjacency drew a hard HEXAGONAL brightness step even across uniform water. FOW_DEFAULT_SOFTNESS is the
# cross-edge smoothing reach as a FRACTION OF THE HEX RADIUS (× radius → the fow_soft px uniform, like
# blend_width — so the softness is zoom-invariant); at 0.6 the mist boundary reads as a gradient over most of
# the shared edge's approach. FOW_DEFAULT_NOISE_AMOUNT wisps that boundary with world noise (0 = a clean arc);
# it is enveloped in-shader so it only bites at boundaries and never tints a pure Active/Discovered interior.
const FOW_DEFAULT_SOFTNESS := 0.6
const FOW_DEFAULT_NOISE_AMOUNT := 0.15
# Config bounds. The LOWER bound of both is 0 ON PURPOSE — softness 0 fully disables the smoothing (the raw
# per-hex tint), which blend_probe state 8/W renders as the BEFORE frame of the FoW hex-step fix, and noise 0
# is a clean, unwisped fog line. The UPPER bounds only stop a bad config from swamping the visibility states:
# a softness beyond ~2 radii averages hexes that are nowhere near the fragment, and a noise amount beyond 1
# could push the smoothed scalar clean across a state gap.
const FOW_MAX_SOFTNESS := 2.0
const FOW_MAX_NOISE_AMOUNT := 1.0
const HEIGHTFIELD_CONFIG_PATH := "res://src/data/heightfield_config.json"
const MIN_ZOOM_FACTOR := 1.0
const MAX_ZOOM_FACTOR := 4.0
const MOUSE_ZOOM_STEP := 0.2
# One click of the on-screen zoom rail. Deliberately larger than MOUSE_ZOOM_STEP
# (0.2) so a button press feels like a deliberate step, not a nudge; promote to a
# config lever if it ever wants tuning.
const ZOOM_BUTTON_STEP := 0.5
const KEYBOARD_ZOOM_SPEED := 0.8
const KEYBOARD_PAN_SPEED := 600.0
const PLAYER_FACTION_ID := 0

# --- Band status decorations (food-runway dot, activity glyph, supply links) ---
# Sit relative to the band marker radius so they scale with zoom.
const BAND_FOOD_DOT_RADIUS_FACTOR := 0.28   # of the band marker radius
const BAND_FOOD_DOT_OFFSET_FACTOR := 0.9    # dot center offset up-right from marker center

# --- Scouting-expedition marker (docs/plan_exploration_and_sites.md §2) ---
# A detached party reads as a hollow, flag-marked disc — deliberately distinct from a resident
# band's SOLID faction dot, so an expedition says "party out on a venture, not a settlement-band"
# at a glance. Sized relative to the band marker radius so it scales with zoom.
const EXPEDITION_GLYPH := "⚑"                    # flag motif = a venture staked out on the map
const EXPEDITION_DISC_ALPHA := 0.55              # dark backing disc (glyph legibility over terrain)
const EXPEDITION_RING_FACTOR := 1.02             # faction-tinted outer ring radius, of marker radius
const EXPEDITION_RING_WIDTH := 3.0
const EXPEDITION_GLYPH_SIZE_FACTOR := 1.15       # glyph size, of marker radius
const EXPEDITION_GLYPH_COLOR := Color(0.96, 0.97, 0.92, 1.0)
# Awaiting-orders idle indicator: a pulsing amber (WARN) ring signalling the party has reached its
# objective and needs a command. `expeditionPhase == "awaiting"` drives it; the pulse is animated
# from `_expedition_time` in _process.
const EXPEDITION_PHASE_AWAITING := "awaiting"
const EXPEDITION_AWAITING_RING_FACTOR := 1.35    # pulsing ring base radius, of marker radius
const EXPEDITION_AWAITING_PULSE_AMPLITUDE := 0.22
const EXPEDITION_AWAITING_PULSE_SPEED := 3.2
const EXPEDITION_AWAITING_RING_WIDTH := 2.5
# --- Hunting-expedition marker (PR 2, docs/plan_exploration_and_sites.md §2b) ---
# A hunt party (`expedition_mission == "hunt"`) reads as a bow disc — a clearly different motif from
# the scout's flag — so scout vs hunt parties are distinguishable at a glance.
const EXPEDITION_HUNT_MISSION := "hunt"
const EXPEDITION_HUNT_GLYPH := "🏹"              # bow motif = a hunting party following game
# Hunt phase read: HUNTING (gathering at the herd) shows a small red "working" cue ring; DELIVERING
# and RETURNING (hauling a haul home) show a green food pip. So gathering vs hauling read at a glance.
const EXPEDITION_PHASE_HUNTING := "hunting"
const EXPEDITION_PHASE_DELIVERING := "delivering"
const EXPEDITION_PHASE_RETURNING := "returning"
const EXPEDITION_DELIVER_PIP_FACTOR := 0.34      # green food-pip radius, of marker radius
const EXPEDITION_DELIVER_PIP_OFFSET := 0.85      # pip offset down-right from marker center, of marker radius
const EXPEDITION_GATHER_CUE_FACTOR := 0.30       # red gathering-cue ring radius, of marker radius
const EXPEDITION_GATHER_CUE_OFFSET := 0.85       # cue offset down-right from marker center, of marker radius
const EXPEDITION_GATHER_CUE_WIDTH := 2.0
# Supply-link overlay: faint lines connecting bands sharing a supply network.
const SUPPLY_LINK_COLOR := Color(0.310, 0.878, 0.812, 0.28)  # dim SIGNAL cyan
const SUPPLY_LINK_WIDTH := 2.0
const SUPPLY_NETWORK_SOLO := 0  # supply_network_id 0 == not in a shared network

const OVERLAY_COLORS := {
	"logistics": LOGISTICS_COLOR,
	"sentiment": SENTIMENT_COLOR,
	"corruption": CORRUPTION_COLOR,
	"fog": FOG_COLOR,
	"culture": CULTURE_COLOR,
	"military": MILITARY_COLOR,
	"crisis": CRISIS_COLOR,
	"elevation": ELEVATION_HIGH_COLOR,
	"moisture": Color(0.2, 0.65, 0.95, 1.0),
	"province": Color(0.52, 0.64, 0.78, 1.0),
	# The pasture channel paints through `_pasture_color` (a two-tone ramp plus two off-ramp barren
	# tones), not a single-hue tint; this is the swatch any generic fallback path shows for it.
	PASTURE_OVERLAY_KEY: PASTURE_RICH_COLOR,
	# Both danger channels ride the generic lerp path — empty tiles stay grid-colored, a qualifying
	# herd glows (hunt-danger orange, threat red, so the two read apart).
	HUNT_DANGER_OVERLAY_KEY: HUNT_DANGER_OVERLAY_COLOR,
	THREAT_OVERLAY_KEY: THREAT_OVERLAY_COLOR,
}

const TERRAIN_TAG_KEYS := [
	1 << 0,  # Water
	1 << 1,  # Freshwater
	1 << 2,  # Coastal
	1 << 3,  # Wetland
	1 << 4,  # Fertile
	1 << 5,  # Arid
	1 << 6,  # Polar
	1 << 7,  # Highland
	1 << 8,  # Volcanic
	1 << 9,  # Hazardous
	1 << 10, # Subsurface
	1 << 11, # Hydrothermal
]

const TERRAIN_TAG_COLORS := {
	TERRAIN_TAG_KEYS[0]: Color8(28, 102, 189),   # Water
	TERRAIN_TAG_KEYS[1]: Color8(72, 174, 206),   # Freshwater
	TERRAIN_TAG_KEYS[2]: Color8(64, 176, 150),   # Coastal
	TERRAIN_TAG_KEYS[3]: Color8(70, 140, 96),    # Wetland
	TERRAIN_TAG_KEYS[4]: Color8(192, 198, 96),   # Fertile
	TERRAIN_TAG_KEYS[5]: Color8(210, 166, 84),   # Arid
	TERRAIN_TAG_KEYS[6]: Color8(214, 232, 246),  # Polar
	TERRAIN_TAG_KEYS[7]: Color8(136, 128, 184),  # Highland
	TERRAIN_TAG_KEYS[8]: Color8(216, 102, 72),   # Volcanic
	TERRAIN_TAG_KEYS[9]: Color8(198, 62, 132),   # Hazardous
	TERRAIN_TAG_KEYS[10]: Color8(124, 118, 150), # Subsurface
	TERRAIN_TAG_KEYS[11]: Color8(244, 156, 68),  # Hydrothermal
}

const TERRAIN_TAG_BLEND_WEIGHTS := {
	TERRAIN_TAG_KEYS[0]: 0.92,
	TERRAIN_TAG_KEYS[1]: 0.8,
	TERRAIN_TAG_KEYS[2]: 0.7,
	TERRAIN_TAG_KEYS[3]: 0.66,
	TERRAIN_TAG_KEYS[4]: 0.65,
	TERRAIN_TAG_KEYS[5]: 0.6,
	TERRAIN_TAG_KEYS[6]: 0.7,
	TERRAIN_TAG_KEYS[7]: 0.68,
	TERRAIN_TAG_KEYS[8]: 0.75,
	TERRAIN_TAG_KEYS[9]: 0.45,
	TERRAIN_TAG_KEYS[10]: 0.4,
	TERRAIN_TAG_KEYS[11]: 0.55,
}

const CRISIS_SEVERITY_COLORS := {
	"critical": Color(0.96, 0.28, 0.38, 0.95),
	"warn": Color(0.97, 0.75, 0.28, 0.92),
	"safe": Color(0.5, 0.82, 0.72, 0.85)
}

# Terrain colors and labels loaded from TerrainDefinitions (single source of truth)
var _terrain_colors: Dictionary
var _terrain_labels: Dictionary

func _get_terrain_colors() -> Dictionary:
	if _terrain_colors.is_empty():
		_terrain_colors = TerrainDefinitions.get_colors_dict()
	return _terrain_colors

func _get_terrain_labels() -> Dictionary:
	if _terrain_labels.is_empty():
		for terrain: Dictionary in TerrainDefinitions.get_terrains():
			var tid: int = int(terrain.get("id", -1))
			_terrain_labels[tid] = terrain.get("label", "Unknown")
	return _terrain_labels

const FOOD_MODULE_COLORS := {
	"coastal_littoral": Color(0.98, 0.76, 0.48, 0.9),
	"riverine_delta": Color(0.45, 0.78, 0.92, 0.9),
	"savanna_grassland": Color(0.92, 0.8, 0.52, 0.9),
	"temperate_forest": Color(0.64, 0.86, 0.58, 0.9),
	"boreal_arctic": Color(0.8, 0.88, 0.98, 0.9),
	"montane_highland": Color(0.78, 0.7, 0.9, 0.9),
	"wetland_swamp": Color(0.56, 0.76, 0.64, 0.9),
	"semi_arid_scrub": Color(0.95, 0.68, 0.44, 0.9),
	"coastal_upwelling": Color(0.6, 0.85, 0.98, 0.9),
	"mixed_woodland": Color(0.64, 0.82, 0.72, 0.9)
}

const FOOD_SITE_STYLE_DEFAULT := {
	"color": Color(0.95, 0.82, 0.5, 0.9),
	"shape": "diamond"
}

const FOOD_SITE_STYLES := {
	"littoral": {"color": Color(0.95, 0.74, 0.32, 0.9), "shape": "diamond"},
	"river_garden": {"color": Color(0.4, 0.75, 0.9, 0.9), "shape": "droplet"},
	"savanna_track": {"color": Color(0.92, 0.78, 0.4, 0.9), "shape": "triangle"},
	"forest_forage": {"color": Color(0.52, 0.78, 0.56, 0.9), "shape": "square"},
	"arctic_fishing": {"color": Color(0.78, 0.88, 0.98, 0.9), "shape": "circle"},
	"highland_grove": {"color": Color(0.78, 0.7, 0.9, 0.9), "shape": "diamond"},
	"wetland_harvest": {"color": Color(0.42, 0.66, 0.52, 0.9), "shape": "square"},
	"scrub_roots": {"color": Color(0.9, 0.6, 0.38, 0.9), "shape": "triangle"},
	"upwelling_drying": {"color": Color(0.58, 0.84, 0.94, 0.9), "shape": "droplet"},
	"woodland_cache": {"color": Color(0.6, 0.78, 0.66, 0.9), "shape": "circle"},
	"game_trail": {"color": Color(0.85, 0.5, 0.35, 0.95), "shape": "circle"}
}

const FOOD_MODULE_LABELS := {
	"coastal_littoral": "Coastal Littoral",
	"riverine_delta": "Riverine / Delta",
	"savanna_grassland": "Savanna Grassland",
	"temperate_forest": "Temperate Forest",
	"boreal_arctic": "Boreal / Arctic",
	"montane_highland": "Montane Highland",
	"wetland_swamp": "Wetland / Swamp",
	"semi_arid_scrub": "Semi-Arid Scrub",
	"coastal_upwelling": "Coastal Upwelling",
	"mixed_woodland": "Mixed Woodland",
}

var grid_width: int = 0
var grid_height: int = 0
var _wrap_horizontal: bool = false
var overlay_channels: Dictionary = {}
var overlay_raw_channels: Dictionary = {}
# The active map's sea level on the elevation raster's normalized 0..1 scale, streamed
# per-snapshot. Held here so relative_height_at floors at the correct per-map value.
var _elevation_sea_level: float = HEIGHT_DEFAULT_SEA_LEVEL
# Terrain id to highlight on the map (from the Terrain-tab dropdown); -1 = off.
var _terrain_highlight_id: int = -1
var overlay_channel_labels: Dictionary = {}
var overlay_channel_descriptions: Dictionary = {}
var overlay_placeholder_flags: Dictionary = {}
var overlay_channel_order: PackedStringArray = PackedStringArray()
var culture_layer_map: Dictionary = {}
var active_overlay_key: String = ""
var terrain_overlay: PackedInt32Array = PackedInt32Array()
var terrain_palette: Dictionary = {}
var terrain_tags_overlay: PackedInt32Array = PackedInt32Array()
var terrain_tag_labels: Dictionary = {}
var units: Array = []
var routes: Array = []
var herds: Array = []
var herd_trails: Dictionary = {}
var food_sites: Array = []
var food_site_lookup: Dictionary = {}
# Wondrous Sites the player faction has discovered (per-faction snapshot field). Each entry:
# { x, y, site_id, category, display_name, glyph }. Rendered as glyph markers on the map.
var discovered_sites: Array = []
var discovered_site_lookup: Dictionary = {}
var harvest_sites: Dictionary = {}
var scout_sites: Dictionary = {}
# Forage patches (cultivation/tended state, decoded from ForagePatchState), keyed by
# Vector2i(x, y); read by `_tile_info_at` for the Tile-card cultivation/tended readout.
var forage_patch_lookup: Dictionary = {}
var tile_lookup: Dictionary = {}
# Per-tile habitability (band-independent morale drain, decoded from TileState),
# keyed by Vector2i(x, y); read by `_tile_info_at` for the Tile-card Habitability row.
var tile_habitability: Dictionary = {}
# Per-tile temperature (°, latitude + elevation climate, decoded from TileState),
# keyed by Vector2i(x, y); read by `_tile_info_at` for the Tile-card Climate row.
var tile_temperature: Dictionary = {}
# Per-tile GRAZE — the pasture layer (decoded from TileState: graze_biomass / graze_capacity /
# graze_ecology_phase), keyed by Vector2i(x, y). Read by `_tile_info_at` for the Tile-card Pasture
# rows and by `_build_pasture_legend` for the map-wide standing-stock figure. Entries are stored ONLY
# for tiles that actually carry pasture (capacity > 0), so "no pasture here" is an absent reading —
# the same discipline the sim's GrazeRegistry keeps — and can never be printed as "0 / 0".
var tile_graze: Dictionary = {}
# Per-tile FORAGE capacity — the human-food layer (decoded from TileState.forage_capacity), keyed by
# Vector2i(x, y). Read by `_build_forage_legend` for the Poorest/Average/Richest figures. Stored ONLY
# for tiles that carry human-food potential (capacity > 0), so the map-wide zeros (deep ocean/glacier/
# lava) fall out as the barren count rather than dragging the "poorest" figure to 0.
var tile_forage: Dictionary = {}
var trade_links_overlay: Array = []
var trade_overlay_enabled: bool = false
var selected_trade_entity: int = -1
var crisis_annotations: Array = []
# Per-tile river-edge mask (12 bits, 2 per odd-r direction — see RIVER_DEFAULT_* / the shader's river
# pass), keyed by Vector2i(x, y). Feeds the river-map splatmap's R/G; the shader does the drawing.
var tile_river_edges: Dictionary = {}
# Per-tile river-INFLOW mask (12 bits, 2 per hex CORNER), keyed by Vector2i(x, y): the vertex an edge
# river hands over to the navigable channel at, and with what class. Set on ANY navigable hex a tributary
# joins — a real drainage network joins tributaries to trunks MID-CHAIN, so this is no longer a "chain
# head" flag and the shader must not use it as one (it gates the head taper on the river_channel exit
# count instead). Feeds the river-map splatmap's B/A; the shader draws the channel's inflow SPUR from it.
var tile_river_inflow: Dictionary = {}
# Per-tile river-CHANNEL mask (6 bits, 1 per odd-r direction), keyed by Vector2i(x, y): the sides a
# NAVIGABLE hex's channel flows out through — its upstream/downstream neighbours in its own chain, plus
# (on the last hex only) its exit into the sea/delta. Feeds the R8 river-channel splatmap; the shader arms
# the trunk from it and from nothing else. See RIVER_CHANNEL_MASK for why the terrain cannot answer this.
var tile_river_channel: Dictionary = {}
# Per-tile UNDERLYING terrain id (the "real ground" biome), keyed by Vector2i(x, y). Equals the tile's own
# terrain on ordinary tiles, and the preserved VALLEY biome on a navigable hex (which stamps NavigableRiver
# over the ground the river cut). Feeds the shader's navigable_underlying_map so a navigable hex renders its
# valley as the base, with only a slim bank skirt hugging the channel — the shader reads it on navigable
# hexes only, so non-navigable values are don't-care.
var tile_underlying_terrain: Dictionary = {}
# Debug toggle (Map tab): tint rivers hard so they pop. Pushed to the shader as `river_highlight`.
var highlight_rivers: bool = false

# Hex-grid overlay toggle (H). Stays on MapView: `_draw_hex_grid_overlay` is the ONE grid drawer both
# terrain paths call, and CachedMapRenderer reads it too — it is not terrain-raster state.
var _show_grid_lines: bool = true

var culture_layer_grid: PackedInt32Array = PackedInt32Array()
var highlighted_culture_layer_ids: PackedInt32Array = PackedInt32Array()
var highlighted_culture_layer_set: Dictionary = {}
var highlighted_culture_context: String = ""

var selected_tile: Vector2i = Vector2i(-1, -1)
# Active command-targeting overlay, mirrored from the HUD's pending state via
# `set_targeting`. Drives the reticle / valid-target glow / hover ETA in _draw.
# Keys: active(bool), need("band"|"tile"), command(String), origin_x/origin_y(int).
var _targeting: Dictionary = {}
var _targeting_time: float = 0.0
# Animates the awaiting-orders pulse on expedition markers. Advanced (and a redraw requested)
# only while at least one expedition is in the "awaiting" phase, tracked at marker-rebuild time.
var _expedition_time: float = 0.0
var _has_awaiting_expedition: bool = false

var last_hex_radius: float = 48.0
var last_origin: Vector2 = Vector2.ZERO
var last_map_size: Vector2 = Vector2.ZERO
var last_base_origin: Vector2 = Vector2.ZERO
var base_hex_radius: float = 1.0
var zoom_factor: float = 1.0
# Cached hex point offsets (pre-computed trig values for hex corners)
var _cached_hex_offsets: PackedVector2Array = PackedVector2Array()
var _cached_hex_radius: float = -1.0
# Visible column/row range from last render (for minimap indicator)
var _last_visible_col_start: float = 0.0
var _last_visible_col_end: float = 0.0
var _last_visible_row_start: float = 0.0
var _last_visible_row_end: float = 0.0
var pan_offset: Vector2 = Vector2.ZERO
var base_bounds: Rect2 = Rect2(Vector2.ZERO, Vector2.ONE)
var bounds_dirty: bool = true
# Edges reserved by docked panels (Inspector, Band/City panel). Each reserver
# registers a (edge, size) contribution keyed by a StringName id; the four edge
# totals are the summed sizes per edge (canvas-space px). The map fits and
# recentres into the remaining rect instead of drawing under any reserved strip.
var _reservations: Dictionary = {}
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0
var mouse_pan_active: bool = false
var mouse_pan_button: int = -1

var faction_colors: Dictionary = {
	"Aurora": Color(0.55, 0.85, 1.0, 1.0),
	"Obsidian": Color(0.95, 0.62, 0.2, 1.0),
	"Verdant": Color(0.4, 0.9, 0.55, 1.0),
	0: Color(0.55, 0.85, 1.0, 1.0),
	1: Color(0.95, 0.62, 0.2, 1.0),
	2: Color(0.4, 0.9, 0.55, 1.0)
}

var selected_unit_id: int = -1
var selected_herd_id: String = ""
# Select-then-cycle: which band in the selected tile's stack is active. Advanced by
# re-clicking the selected tile; reset to 0 (top card) on a fresh tile; synced from a
# roster selection via select_occupant so map cycling + roster stay coherent.
var cycle_index: int = 0
var biome_color_buffer: PackedColorArray = PackedColorArray()
var _hovered_tile: Vector2i = Vector2i(-1, -1)
var _fow_enabled: bool = false

# FoW appearance, loaded from heightfield_config.json "fog_of_war" (see _load_fow_config).
var _fow_mist_color: Color = DEFAULT_FOW_MIST_COLOR
var _fow_mist_blend: float = DEFAULT_FOW_MIST_BLEND
var _fow_fog_fill_color: Color = DEFAULT_FOW_FOG_FILL_COLOR
# Shader-path-only FoW boundary softening (see FOW_DEFAULT_* — kills the hard hexagonal mist steps).
var _fow_softness: float = FOW_DEFAULT_SOFTNESS
var _fow_noise_amount: float = FOW_DEFAULT_NOISE_AMOUNT

# 2D Minimap (owned by MinimapController — see ui/MinimapController.gd)
var _minimap: MinimapController = null
# Primary player-band markers (owned by BandMarkerRenderer — see ui/BandMarkerRenderer.gd)
var _band_markers: BandMarkerRenderer = null
# Secondary markers — herds/food/sites (owned by SecondaryMarkerRenderer — see ui/SecondaryMarkerRenderer.gd)
var _secondary_markers: SecondaryMarkerRenderer = null
# Selected-band / selected-herd overlays — range borders, worked-source highlights, the dashed-amber
# pending overlay, the travel destination, the graze-range + pen-footprint rings, and the deferred
# yield-label batch (owned by BandOverlayRenderer — see ui/BandOverlayRenderer.gd).
var _band_overlays: BandOverlayRenderer = null
# Terrain textures + the Approach-B blend shader (owned by TerrainRenderer — see ui/TerrainRenderer.gd).
# The CPU base pass (_draw_terrain_direct) and the _cache_* SubViewport stay on MapView.
var _terrain: TerrainRenderer = null
var _hud_layer: Node = null  # HudLayer reference, set via set_hud_reference() for embedded minimap
var _explored_bounds_world: Rect2 = Rect2()  # World coords of explored area at unit radius (scaled in _clamp_pan_offset)

# Profiling for performance measurement
var _draw_frame_times: Array[float] = []
var _profiling_enabled: bool = false  # Opt-in draw-time profiling; enable manually when profiling

# Cached map rendering (Single-buffer with simple invalidation)
var _map_cache_enabled: bool = true
const MAP_CACHE_BUFFER_MARGIN := 0.5  # 50% buffer on each side

# Single cache (simpler than dual-buffer, avoids sync issues)
var _cache_viewport: SubViewport = null
var _cache_renderer: Node2D = null  # CachedMapRenderer instance
var _cache_texture: ViewportTexture = null
var _cache_pan_offset: Vector2 = Vector2.ZERO  # Pan offset when cache was rendered
var _cache_valid: bool = false
var _cache_display_offset: Vector2 = Vector2.ZERO
var _cache_rendering: bool = false  # Is cache currently rendering?

func _ready() -> void:
	set_process_unhandled_input(true)
	set_process(true)
	# Use nearest-neighbor filtering to prevent seams from bilinear interpolation
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	_load_fow_config()
	_ensure_input_actions()
	_terrain = TerrainRenderer.new(self)
	_terrain.setup()
	_setup_map_cache()
	_minimap = MinimapController.new(self)
	_band_markers = BandMarkerRenderer.new(self)
	_secondary_markers = SecondaryMarkerRenderer.new(self)
	_band_overlays = BandOverlayRenderer.new(self)
	# Note: the MinimapPanel node is created lazily from _minimap.update()
	# This allows Main.gd to set_hud_reference() before the minimap is created


## Load FoW appearance tunables from heightfield_config.json ("fog_of_war" section).
## Falls back to the DEFAULT_FOW_* constants when the file or individual keys are missing.
func _load_fow_config() -> void:
	if not FileAccess.file_exists(HEIGHTFIELD_CONFIG_PATH):
		return
	var file := FileAccess.open(HEIGHTFIELD_CONFIG_PATH, FileAccess.READ)
	if file == null:
		push_warning("[MapView] Failed to open config: " + HEIGHTFIELD_CONFIG_PATH)
		return
	var text := file.get_as_text()
	file.close()
	var json = JSON.parse_string(text)
	if json == null:
		push_warning("[MapView] Failed to parse JSON config: " + HEIGHTFIELD_CONFIG_PATH)
		return
	if not (json is Dictionary and json.has("fog_of_war")):
		return
	var cfg: Dictionary = json["fog_of_war"]
	_fow_mist_color = _color_from_config(cfg.get("mist_color"), DEFAULT_FOW_MIST_COLOR)
	_fow_mist_blend = float(cfg.get("mist_blend", DEFAULT_FOW_MIST_BLEND))
	_fow_fog_fill_color = _color_from_config(cfg.get("fog_fill_color"), DEFAULT_FOW_FOG_FILL_COLOR)
	# Boundary-softening levers (blend-shader path only): a fraction of the hex radius, and the wispiness
	# amplitude. Clamped to the documented bounds — 0 is a legitimate setting on BOTH (softness 0 = smoothing
	# OFF, i.e. the raw per-hex tint the probe's before-frame renders; noise 0 = an unwisped fog line); the
	# upper bounds are what keep a bad config from swamping the visibility states. See the const block.
	_fow_softness = clampf(float(cfg.get("fow_softness", FOW_DEFAULT_SOFTNESS)), 0.0, FOW_MAX_SOFTNESS)
	_fow_noise_amount = clampf(
		float(cfg.get("fow_noise_amount", FOW_DEFAULT_NOISE_AMOUNT)), 0.0, FOW_MAX_NOISE_AMOUNT
	)

## Parse an [r, g, b] (or [r, g, b, a]) config array into a Color, or return the fallback.
func _color_from_config(value, fallback: Color) -> Color:
	if value is Array and value.size() >= 3:
		var alpha := float(value[3]) if value.size() >= 4 else 1.0
		return Color(float(value[0]), float(value[1]), float(value[2]), alpha)
	return fallback


func _setup_map_cache() -> void:
	## Initialize the SubViewport-based map caching system for fast panning
	if not _map_cache_enabled:
		return

	# Create SubViewport for cached rendering
	_cache_viewport = SubViewport.new()
	_cache_viewport.name = "MapCacheViewport"
	_cache_viewport.transparent_bg = false
	_cache_viewport.render_target_update_mode = SubViewport.UPDATE_DISABLED
	_cache_viewport.size = Vector2i(1920, 1080)  # Will be resized on first render
	add_child(_cache_viewport)

	# Create the renderer inside the SubViewport
	var CachedMapRendererScript := preload("res://src/scripts/CachedMapRenderer.gd")
	_cache_renderer = CachedMapRendererScript.new()
	_cache_renderer.name = "CachedMapRenderer"
	_cache_renderer.setup(self)
	_cache_viewport.add_child(_cache_renderer)

	# Connect render completion signal
	_cache_renderer.cache_rendered.connect(_on_cache_rendered)

	# Get the viewport texture
	_cache_texture = _cache_viewport.get_texture()

	print("[MapView] Map cache system initialized")


func _invalidate_map_cache() -> void:
	## Mark the map cache as invalid, forcing a re-render on next draw
	_cache_valid = false


func _render_map_cache() -> void:
	## Render the map to the cache SubViewport
	if _cache_viewport == null or _cache_renderer == null:
		return

	# Calculate buffer size
	var viewport_size := get_viewport_rect().size
	var buffer_size := viewport_size * (1.0 + MAP_CACHE_BUFFER_MARGIN * 2.0)
	_cache_viewport.size = Vector2i(int(buffer_size.x), int(buffer_size.y))

	# Store the pan offset at render time
	_cache_pan_offset = pan_offset

	# Trigger render of the SubViewport
	_cache_renderer.queue_redraw()
	_cache_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	_cache_rendering = true

	# Calculate display offset (the buffer margin)
	_cache_display_offset = viewport_size * MAP_CACHE_BUFFER_MARGIN

	# Mark as valid (texture will be ready next frame)
	_cache_valid = true


func _is_pan_within_cache_buffer() -> bool:
	## Check if current pan is still within the cached buffer bounds
	if not _cache_valid or _cache_viewport == null:
		return false

	var pan_delta := pan_offset - _cache_pan_offset
	var viewport_size := get_viewport_rect().size
	var max_offset := viewport_size * MAP_CACHE_BUFFER_MARGIN

	# Check if pan is within buffer bounds
	return absf(pan_delta.x) <= max_offset.x and absf(pan_delta.y) <= max_offset.y


func _on_cache_rendered() -> void:
	## Called when cache finishes rendering (signal from CachedMapRenderer)
	_cache_rendering = false

func display_snapshot(snapshot: Dictionary) -> Dictionary:
	print("[MapView] display_snapshot called. Keys: ", snapshot.keys())
	if snapshot.is_empty():
		return {}
	var previous_width: int = grid_width
	var previous_height: int = grid_height
	var grid: Dictionary = snapshot.get("grid", {})
	var new_width: int = int(grid.get("width", 0))
	var new_height: int = int(grid.get("height", 0))
	var dimensions_changed: bool = previous_width != new_width or previous_height != new_height
	grid_width = new_width
	grid_height = new_height
	_wrap_horizontal = bool(grid.get("wrap_horizontal", false))

	var overlays: Dictionary = snapshot.get("overlays", {})
	_ingest_overlay_channels(overlays)
	terrain_overlay = PackedInt32Array(overlays.get("terrain", []))
	_terrain.set_grid_terrain(terrain_overlay, grid_width, grid_height)
	_update_biome_color_buffer()
	# Increment minimap data version to trigger rebuild on terrain/visibility changes
	_minimap.bump_data_version()
	# Invalidate map cache when terrain data changes
	_invalidate_map_cache()
	var palette_raw: Variant = overlays.get("terrain_palette", {})
	terrain_palette = palette_raw if typeof(palette_raw) == TYPE_DICTIONARY else {}
	terrain_tags_overlay = PackedInt32Array(overlays.get("terrain_tags", []))
	var tag_labels_raw: Variant = overlays.get("terrain_tag_labels", {})
	terrain_tag_labels = tag_labels_raw if typeof(tag_labels_raw) == TYPE_DICTIONARY else {}
	var culture_layers_variant: Variant = snapshot.get("culture_layers", null)
	if culture_layers_variant is Array:
		for layer_variant in culture_layers_variant:
			if layer_variant is Dictionary:
				var layer: Dictionary = layer_variant
				var id: int = int(layer.get("id", -1))
				if id >= 0:
					culture_layer_map[id] = layer.duplicate(true)
	var removed_layers_variant: Variant = snapshot.get("culture_layer_removed", null)
	if removed_layers_variant is Array:
		for raw_id in removed_layers_variant:
			var id := int(raw_id)
			if culture_layer_map.has(id):
				culture_layer_map.erase(id)
	crisis_annotations = []
	var crisis_annotations_variant: Variant = overlays.get("crisis_annotations", [])
	if crisis_annotations_variant is Array:
		for entry in crisis_annotations_variant:
			if entry is Dictionary:
				crisis_annotations.append((entry as Dictionary).duplicate(true))
	routes = Array(snapshot.get("orders", []))
	food_sites = []
	food_site_lookup.clear()
	harvest_sites.clear()
	scout_sites.clear()
	var food_variant: Variant = snapshot.get("food_modules", [])
	if food_variant is Array:
		for entry in food_variant:
			if not (entry is Dictionary):
				continue
			var site: Dictionary = (entry as Dictionary).duplicate(true)
			food_sites.append(site)
			var x_site: int = int(site.get("x", -1))
			var y_site: int = int(site.get("y", -1))
			# Stamp the tile's terrain so both consumers (map marker + HUD Forage row) resolve the
			# terrain-aware FoodIcons.for_site split from one source and can't disagree (riverine_delta
			# splits fish↔reeds by terrain). Unconditional: for x<0 it's harmless (-1 → fish default).
			site["terrain_id"] = _terrain_id_at(x_site, y_site)
			if x_site >= 0 and y_site >= 0:
				food_site_lookup[Vector2i(x_site, y_site)] = site
		discovered_sites = []
		discovered_site_lookup.clear()
		var sites_variant: Variant = snapshot.get("discovered_sites", [])
		if sites_variant is Array:
			for entry in sites_variant:
				if not (entry is Dictionary):
					continue
				var faction_entry: Dictionary = entry
				if int(faction_entry.get("faction", -1)) != PLAYER_FACTION_ID:
					continue
				var faction_sites: Variant = faction_entry.get("sites", [])
				if not (faction_sites is Array):
					continue
				for site_entry in faction_sites:
					if not (site_entry is Dictionary):
						continue
					var wsite: Dictionary = (site_entry as Dictionary).duplicate(true)
					discovered_sites.append(wsite)
					var wx: int = int(wsite.get("x", -1))
					var wy: int = int(wsite.get("y", -1))
					if wx >= 0 and wy >= 0:
						discovered_site_lookup[Vector2i(wx, wy)] = wsite
		forage_patch_lookup.clear()
		var patch_variant: Variant = snapshot.get("forage_patches", [])
		if patch_variant is Array:
			for entry in patch_variant:
				if not (entry is Dictionary):
					continue
				var patch: Dictionary = (entry as Dictionary).duplicate(true)
				var px: int = int(patch.get("x", -1))
				var py: int = int(patch.get("y", -1))
				if px >= 0 and py >= 0:
					forage_patch_lookup[Vector2i(px, py)] = patch
	var population_variant: Variant = snapshot.get("populations", [])
	if population_variant is Array:
		for entry in population_variant:
			if not (entry is Dictionary):
				continue
			var cohort: Dictionary = entry
			var harvest_variant: Variant = cohort.get("harvest", {})
			if harvest_variant is Dictionary:
				var harvest: Dictionary = (harvest_variant as Dictionary).duplicate(true)
				var hx := int(harvest.get("target_x", -1))
				var hy := int(harvest.get("target_y", -1))
				if hx >= 0 and hy >= 0:
					var key := Vector2i(hx, hy)
					harvest["module_label"] = _food_module_label(String(harvest.get("module", "")))
					var existing: Array = harvest_sites.get(key, [])
					existing.append(harvest)
					harvest_sites[key] = existing
			var scout_variant: Variant = cohort.get("scout", {})
			if scout_variant is Dictionary:
				var scout: Dictionary = (scout_variant as Dictionary).duplicate(true)
				var sx := int(scout.get("target_x", -1))
				var sy := int(scout.get("target_y", -1))
				if sx >= 0 and sy >= 0:
					var scout_key := Vector2i(sx, sy)
					var scout_existing: Array = scout_sites.get(scout_key, [])
					scout_existing.append(scout)
					scout_sites[scout_key] = scout_existing

	tile_lookup.clear()
	tile_habitability.clear()
	tile_temperature.clear()
	tile_graze.clear()
	tile_forage.clear()
	tile_river_edges.clear()
	tile_river_inflow.clear()
	tile_river_channel.clear()
	tile_underlying_terrain.clear()
	if grid_width > 0 and grid_height > 0:
		var total: int = grid_width * grid_height
		culture_layer_grid = PackedInt32Array()
		culture_layer_grid.resize(total)
		culture_layer_grid.fill(-1)
	else:
		culture_layer_grid = PackedInt32Array()
	var tile_entries_variant: Variant = snapshot.get("tiles", [])
	if tile_entries_variant is Array:
		for entry in tile_entries_variant:
			if entry is Dictionary:
				var tile_dict: Dictionary = entry
				var entity_id: int = int(tile_dict.get("entity", -1))
				if entity_id < 0:
					continue
				var x: int = int(tile_dict.get("x", 0))
				var y: int = int(tile_dict.get("y", 0))
				tile_lookup[entity_id] = Vector2i(x, y)
				if tile_dict.has("habitability"):
					tile_habitability[Vector2i(x, y)] = float(tile_dict["habitability"])
				if tile_dict.has("temperature"):
					tile_temperature[Vector2i(x, y)] = float(tile_dict["temperature"])
				# Graze: only a tile whose biome actually carries pasture gets an entry (see
				# `tile_graze`). A zero-capacity tile is a *dead* one, and the Tile card must
				# print nothing there rather than "0 / 0".
				var graze_capacity: float = float(tile_dict.get("graze_capacity", 0.0))
				if graze_capacity > 0.0:
					tile_graze[Vector2i(x, y)] = {
						"biomass": float(tile_dict.get("graze_biomass", 0.0)),
						"capacity": graze_capacity,
						"phase": String(tile_dict.get("graze_ecology_phase", "")),
					}
				# Forage (human-food) potential — only tiles that carry any get an entry, so the
				# barren zeros (deep ocean/glacier/lava) don't drag the legend's "poorest" to 0.
				var forage_capacity: float = float(tile_dict.get("forage_capacity", 0.0))
				if forage_capacity > 0.0:
					tile_forage[Vector2i(x, y)] = forage_capacity
				var river_mask: int = int(tile_dict.get("river_edges", 0))
				if river_mask != 0:
					tile_river_edges[Vector2i(x, y)] = river_mask
				# Where a tributary hands over to a navigable trunk (nonzero on the trunk's FIRST hex only).
				var inflow_mask: int = int(tile_dict.get("river_inflow", 0))
				if inflow_mask != 0:
					tile_river_inflow[Vector2i(x, y)] = inflow_mask
				# Which SIDES a navigable hex's channel flows out through — the sim's word on the trunk's
				# path, and the only thing that arms a trunk arm (see RIVER_CHANNEL_MASK).
				var channel_mask: int = int(tile_dict.get("river_channel", 0))
				if channel_mask != 0:
					tile_river_channel[Vector2i(x, y)] = channel_mask
				# The valley biome the river cut (== terrain on ordinary tiles). Only the shader's navigable
				# pass reads it, but store every tile that carries it so the navigable_underlying_map fills.
				if tile_dict.has("underlying_terrain"):
					tile_underlying_terrain[Vector2i(x, y)] = int(tile_dict["underlying_terrain"])
				if culture_layer_grid.size() > 0:
					if x >= 0 and x < grid_width and y >= 0 and y < grid_height:
						var index: int = y * grid_width + x
						if index >= 0 and index < culture_layer_grid.size():
							culture_layer_grid[index] = int(tile_dict.get("culture_layer", -1))
	# Rebuild the Approach-B blend-shader splatmaps (id-map + FoW vis-map + elev-map + river-map) from the
	# new terrain/fog/elevation/river-edges. Runs AFTER the tile loop, not beside the terrain ingest above:
	# the river-map is built from `tile_river_edges`, which only exists once the tiles have been read.
	_terrain.rebuild_shader_maps()
	_install_province_overlay()
	_rebuild_unit_markers(snapshot)
	_rebuild_herd_markers(snapshot)
	# Removed snapshot ingest logging (noise in normal runs).

	if snapshot.has("trade_links"):
		var trade_variant: Variant = snapshot.get("trade_links")
		if trade_variant is Array:
			update_trade_overlay(trade_variant, trade_overlay_enabled)

	if dimensions_changed:
		zoom_factor = 1.0
		pan_offset = Vector2.ZERO
		mouse_pan_active = false
		mouse_pan_button = -1
	bounds_dirty = dimensions_changed

	_update_layout_metrics()
	_clamp_pan_offset()
	queue_redraw()
	_emit_overlay_legend()
	_minimap.update()

	return {
		"unit_count": units.size(),
		"avg_logistics": _average_overlay("logistics"),
		"avg_sentiment": _average_overlay("sentiment"),
		"avg_corruption": _average_overlay("corruption"),
		"avg_fog": _average_overlay("fog"),
		"avg_culture": _average_overlay("culture"),
		"avg_military": _average_overlay("military"),
		"avg_crisis": _average_overlay("crisis"),
		"dimensions_changed": dimensions_changed,
		"active_overlay": active_overlay_key
	}

func _ingest_overlay_channels(overlays: Variant) -> void:
	var preserve_tag_overlay: bool = (active_overlay_key == "terrain_tags")
	overlay_channels.clear()
	overlay_raw_channels.clear()
	overlay_channel_labels.clear()
	overlay_channel_descriptions.clear()
	overlay_placeholder_flags.clear()
	overlay_channel_order = PackedStringArray()

	var overlay_dict: Dictionary = overlays if overlays is Dictionary else {}
	# Presence-based: keep the fallback default until a snapshot actually carries the
	# per-map value (older native/server builds omit the key).
	if overlay_dict.has("elevation_sea_level"):
		_elevation_sea_level = float(overlay_dict["elevation_sea_level"])
	# The climate-band cut points are a sim-owned per-map constant published beside the
	# sea level (MapSection.climateBands). Presence-based like the sea level: a delta that
	# omits them leaves the last full snapshot's values in place. The native emits all three
	# together or none, so testing one key is enough.
	if overlay_dict.has("climate_polar_max_temp"):
		TileClimate.set_cut_points(
			float(overlay_dict["climate_polar_max_temp"]),
			float(overlay_dict["climate_boreal_max_temp"]),
			float(overlay_dict["climate_temperate_max_temp"]),
		)
	if overlay_dict.has("channels"):
		var channel_variant: Variant = overlay_dict["channels"]
		if channel_variant is Dictionary:
			var channel_dict: Dictionary = channel_variant
			for raw_key in channel_dict.keys():
				var key := String(raw_key)
				var info_variant: Variant = channel_dict[raw_key]
				if not (info_variant is Dictionary):
					continue
				var channel_info: Dictionary = info_variant
				overlay_channels[key] = PackedFloat32Array(channel_info.get("normalized", PackedFloat32Array()))
				overlay_raw_channels[key] = PackedFloat32Array(channel_info.get("raw", PackedFloat32Array()))
				overlay_channel_labels[key] = String(channel_info.get("label", key.capitalize()))
				overlay_channel_descriptions[key] = String(channel_info.get("description", ""))
				overlay_placeholder_flags[key] = bool(channel_info.get("placeholder", false))

	var placeholder_variant: Variant = overlay_dict.get("placeholder_channels", PackedStringArray())
	if placeholder_variant is PackedStringArray:
		var placeholder_array: PackedStringArray = placeholder_variant
		for raw_placeholder_key in placeholder_array:
			var placeholder_key := String(raw_placeholder_key)
			overlay_placeholder_flags[placeholder_key] = true

	var order_variant: Variant = overlay_dict.get("channel_order", PackedStringArray())
	overlay_channel_order = PackedStringArray()
	if order_variant is PackedStringArray:
		var order_array: PackedStringArray = order_variant
		for raw_channel_key in order_array:
			overlay_channel_order.append(String(raw_channel_key))
	if overlay_channel_order.size() == 0:
		var keys: Array = overlay_channels.keys()
		keys.sort()
		for key in keys:
			overlay_channel_order.append(String(key))

	var tag_channel_available: bool = false
	if overlays is Dictionary:
		tag_channel_available = overlays.has("terrain_tags")

	_ensure_default_overlay_channel()

	if overlay_channels.is_empty():
		active_overlay_key = ""
		return

	if preserve_tag_overlay and tag_channel_available:
		active_overlay_key = "terrain_tags"
	else:
		active_overlay_key = ""
func _draw() -> void:
	var _profile_start := Time.get_ticks_usec() if _profiling_enabled else 0

	if grid_width == 0 or grid_height == 0:
		return

	_update_layout_metrics()
	_clamp_pan_offset()
	# Recalculate last_origin after clamp (pan_offset may have wrapped)
	last_origin = last_base_origin + pan_offset

	var radius: float = last_hex_radius
	var origin: Vector2 = last_origin
	var viewport_size := _get_adjusted_viewport_size()
	_apply_view_clip(viewport_size)

	# Pre-compute hex point offsets for this radius (eliminates per-hex trig)
	_update_hex_offset_cache(radius)

	# Update minimap indicator values
	var hex_col_width := SQRT3 * radius
	_last_visible_col_start = (0.0 - origin.x) / hex_col_width
	_last_visible_col_end = (viewport_size.x - origin.x) / hex_col_width
	var hex_row_height := 1.5 * radius
	_last_visible_row_start = (0.0 - origin.y) / hex_row_height
	_last_visible_row_end = (viewport_size.y - origin.y) / hex_row_height

	# Visible logical col/row span (for the shader-branch grid + drives the direct path's own ranges).
	var col_start: int = int((-origin.x) / hex_col_width) - 2
	var col_end: int = int((viewport_size.x - origin.x) / hex_col_width) + 2
	var row_start: int = maxi(0, int((-origin.y) / hex_row_height) - 2)
	var row_end: int = mini(grid_height, int((viewport_size.y - origin.y) / hex_row_height) + 2)
	if not _wrap_horizontal:
		col_start = maxi(0, col_start)
		col_end = mini(grid_width, col_end)

	# === TERRAIN RENDERING ===
	if _terrain.shader_active():
		# Approach B: the whole-map blend shader draws the base terrain on the behind-quad; MapView only
		# adds grid lines on top here. The CPU cache is bypassed (the shader is a single cheap GPU draw).
		_terrain.update_shader_quad(radius, origin, viewport_size)
		_draw_hex_grid_overlay(radius, origin, col_start, col_end, row_start, row_end)
	else:
		_terrain.hide_shader_quad()
		# === CACHED TERRAIN RENDERING (per-hex textures / solid / overlay — blend OFF or non-textured) ===
		var use_cache := _map_cache_enabled and _cache_viewport != null and _cache_texture != null
		var cache_needs_render := false

		if use_cache:
			# Check if we need to re-render the cache
			if not _cache_valid or not _is_pan_within_cache_buffer():
				cache_needs_render = true
				_render_map_cache()

		# If cache is valid and doesn't need re-render, use it
		# Otherwise fall back to direct rendering (SubViewport won't be ready until next frame)
		var using_cached_render := use_cache and _cache_valid and not cache_needs_render
		var pan_delta := pan_offset - _cache_pan_offset

		if using_cached_render:
			# Draw the cached texture with offset
			var draw_pos := -_cache_display_offset + pan_delta
			draw_texture_rect(_cache_texture, Rect2(draw_pos, Vector2(_cache_viewport.size)), false)
		else:
			# Fallback: Direct rendering (used when cache is re-rendering or disabled)
			_draw_terrain_direct(radius, origin, viewport_size)

	# === OVERLAYS (always drawn fresh) ===
	# These need to respond to hover, selection, and other dynamic state
	_draw_terrain_highlight(radius, origin, viewport_size)
	_draw_trade_overlay(radius, origin)
	# (No river draw here: Minor/Major rivers are painted by terrain_blend.gdshader's river pass, off the
	# per-tile river-edge mask — the water is drawn exactly on the edge the future crossing cost applies to.)
	_draw_crisis_annotations(radius, origin)

	# Selected + hovered hex outlines (drawn under the markers).
	_draw_tile_selection_highlight(radius, origin)

	# Selected player band: highlight what it's working (forage tiles / hunted herds) and
	# its assignable reach (work-range ring). Drawn before the
	# unit/herd markers so those sit on top of the tile tints. Its per-source yield LABELS are the
	# exception — they are queued here and flushed at the very end of _draw (see
	# _band_overlays.flush_yield_labels).
	_band_overlays.draw_band_work_highlights(radius, origin)

	# Selected herd: its grazing range (the ground that sets its carrying capacity), drawn over the
	# tile tints / Pasture overlay but under the herd markers so the animal still reads on top.
	_band_overlays.draw_herd_range_highlights(radius, origin)
	# Selected CORRALLED herd: its fenced pen footprint (a distinct enclosure tint). A corralled herd
	# draws no roam-range above, so exactly one of the two ever renders.
	_band_overlays.draw_pen_footprint_highlight(radius, origin)

	_draw_supply_links(radius, origin)
	_band_markers.draw_primary_bands(radius, origin)

	_secondary_markers.compute_slots()
	for herd in herds:
		_secondary_markers.draw_herd(herd, radius, origin)
	for site in food_sites:
		_secondary_markers.draw_food_site(site, radius, origin)
	for wsite in discovered_sites:
		_secondary_markers.draw_discovered_site(wsite, radius, origin)
	_secondary_markers.draw_secondary_overflow(radius, origin)

	_secondary_markers.draw_harvest_markers(radius, origin)
	_secondary_markers.draw_scout_markers(radius, origin)

	for order in routes:
		_draw_route(order, radius, origin)

	_draw_targeting(radius, origin)

	# TOPMOST: the selected band's per-source yield labels, collected during the overlay renderer's
	# draw_band_work_highlights and held back to here. They annotate the map, so they must survive
	# every layer above the tile tints — herd/food glyphs, rings, band→herd links and the dashed
	# pending overlays all used to scribble across the text. This call MUST stay LAST.
	_band_overlays.flush_yield_labels()

	# Profiling output
	if _profiling_enabled:
		var elapsed: float = (Time.get_ticks_usec() - _profile_start) / 1000.0
		_draw_frame_times.append(elapsed)
		if _draw_frame_times.size() >= 100:
			var total: float = 0.0
			for t: float in _draw_frame_times:
				total += t
			var avg: float = total / _draw_frame_times.size()
			print("[MapView] Avg draw time (100 frames): %.2f ms" % avg)
			_draw_frame_times.clear()

## Highlights all hexes of a given terrain id (Terrain-tab dropdown). Pass -1 to clear.
func set_terrain_highlight(terrain_id: int) -> void:
	if _terrain_highlight_id == terrain_id:
		return
	_terrain_highlight_id = terrain_id
	queue_redraw()

## Overlay pass: outline + tint every visible tile matching `_terrain_highlight_id`.
## Draws map-wide (ignores Fog of War) so it doubles as a worldgen debugging tool.
func _draw_terrain_highlight(radius: float, origin: Vector2, viewport_size: Vector2) -> void:
	if _terrain_highlight_id < 0 or terrain_overlay.is_empty() or grid_width == 0:
		return
	var hex_col_width := SQRT3 * radius
	var hex_row_height := 1.5 * radius
	var col_start: int = int((-origin.x) / hex_col_width) - 2
	var col_end: int = int((viewport_size.x - origin.x) / hex_col_width) + 2
	var row_start: int = maxi(0, int((-origin.y) / hex_row_height) - 2)
	var row_end: int = mini(grid_height, int((viewport_size.y - origin.y) / hex_row_height) + 2)
	if not _wrap_horizontal:
		col_start = maxi(0, col_start)
		col_end = mini(grid_width, col_end)
	var fill := TERRAIN_HIGHLIGHT_COLOR
	fill.a = 0.35
	var fill_colors := PackedColorArray([fill, fill, fill, fill, fill, fill])
	for y in range(row_start, row_end):
		for logical_x in range(col_start, col_end):
			var data_x: int = posmod(logical_x, grid_width) if _wrap_horizontal else logical_x
			if not _wrap_horizontal and (logical_x < 0 or logical_x >= grid_width):
				continue
			if _terrain_id_at(data_x, y) != _terrain_highlight_id:
				continue
			var center: Vector2 = _hex_center(logical_x, y, radius, origin)
			var pts := _hex_points(center, radius)
			draw_polygon(pts, fill_colors)
			var outline := PackedVector2Array([pts[0], pts[1], pts[2], pts[3], pts[4], pts[5], pts[0]])
			draw_polyline(outline, TERRAIN_HIGHLIGHT_COLOR, 2.5, true)

func _draw_terrain_direct(radius: float, origin: Vector2, viewport_size: Vector2) -> void:
	## Direct terrain rendering (fallback when cache is disabled or unavailable)
	# Draw background
	draw_rect(Rect2(Vector2.ZERO, viewport_size), Color(0.3, 0.35, 0.25, 1.0))

	# Determine if using textured rendering
	var mgr = get_node_or_null("/root/TerrainTextureManager")
	var use_textures: bool = mgr != null and mgr.use_terrain_textures and mgr.terrain_textures != null and active_overlay_key == ""

	# Calculate visible range
	var hex_col_width := SQRT3 * radius
	var hex_row_height := 1.5 * radius

	var col_start: int = int((-origin.x) / hex_col_width) - 2
	var col_end: int = int((viewport_size.x - origin.x) / hex_col_width) + 2
	var row_start: int = maxi(0, int((-origin.y) / hex_row_height) - 2)
	var row_end: int = mini(grid_height, int((viewport_size.y - origin.y) / hex_row_height) + 2)

	# Handle horizontal wrapping
	if not _wrap_horizontal:
		col_start = maxi(0, col_start)
		col_end = mini(grid_width, col_end)

	# Draw hexes
	for y in range(row_start, row_end):
		for logical_x in range(col_start, col_end):
			var data_x: int = posmod(logical_x, grid_width) if _wrap_horizontal else logical_x
			if not _wrap_horizontal and (logical_x < 0 or logical_x >= grid_width):
				continue

			var center: Vector2 = _hex_center(logical_x, y, radius, origin)

			if use_textures:
				var vstate := _visibility_state_at(data_x, y)  # one FoW lookup per tile
				if vstate == "unexplored":
					var fog := _fow_fog_fill_color
					var fog_points := _hex_points(center, radius)
					draw_polygon(fog_points, PackedColorArray([fog, fog, fog, fog, fog, fog]))
				else:
					var terrain_id: int = _terrain_id_at(data_x, y)
					_terrain.draw_hex_textured_direct(center, terrain_id, radius, _fow_texture_tint_for_state(vstate))
			else:
				var final_color: Color = _tile_color(data_x, y)
				var polygon_points := _hex_points(center, radius)
				draw_polygon(polygon_points, PackedColorArray([final_color, final_color, final_color, final_color, final_color, final_color]))

	# Draw grid lines on top of all terrain (batched, shared with the shader path).
	_draw_hex_grid_overlay(radius, origin, col_start, col_end, row_start, row_end)



func update_trade_overlay(trade_links: Array, enabled: bool = trade_overlay_enabled) -> void:
	trade_links_overlay = []
	if trade_links is Array:
		for entry in trade_links:
			if entry is Dictionary:
				trade_links_overlay.append((entry as Dictionary).duplicate(true))
	trade_overlay_enabled = enabled
	queue_redraw()

func set_trade_overlay_enabled(enabled: bool) -> void:
	trade_overlay_enabled = enabled
	queue_redraw()

func set_trade_overlay_selection(entity_id: int) -> void:
	selected_trade_entity = entity_id
	if trade_overlay_enabled:
		queue_redraw()

func set_culture_layer_highlight(layer_ids: PackedInt32Array, context_label: String = "") -> void:
	highlighted_culture_layer_ids = PackedInt32Array(layer_ids)
	if highlighted_culture_layer_ids.is_empty():
		highlighted_culture_context = ""
	else:
		highlighted_culture_context = context_label
	highlighted_culture_layer_set.clear()
	for id_value in highlighted_culture_layer_ids:
		highlighted_culture_layer_set[int(id_value)] = true
	queue_redraw()
	_emit_overlay_legend()

func set_overlay_channel(key: String) -> void:
	if key == "terrain_tags":
		if active_overlay_key == key:
			return
		active_overlay_key = key
		_invalidate_map_cache()  # Overlay changes require fresh cache render
		queue_redraw()
		_emit_overlay_legend()
		return
	if key == "":
		active_overlay_key = ""
		_invalidate_map_cache()  # Overlay changes require fresh cache render
		queue_redraw()
		_emit_overlay_legend()
		return
	if not overlay_channels.has(key):
		return
	if active_overlay_key == key:
		return
	active_overlay_key = key
	_invalidate_map_cache()  # Overlay changes require fresh cache render
	queue_redraw()
	_emit_overlay_legend()

func set_fow_enabled(enabled: bool) -> void:
	if _fow_enabled == enabled:
		return
	_fow_enabled = enabled
	# When enabling FoW, ensure we're in terrain view (no overlay)
	if _fow_enabled and active_overlay_key != "":
		active_overlay_key = ""
	_terrain.rebuild_shader_maps()  # refresh the blend-shader vis-map for the new FoW state
	_invalidate_map_cache()  # FoW changes require fresh cache render
	queue_redraw()
	_emit_overlay_legend()
	_minimap.update()  # Rebuild minimap with/without FoW (also sets _explored_bounds_world)
	_clamp_pan_offset()  # Clamp pan to explored bounds when FoW enabled

func is_fow_enabled() -> bool:
	return _fow_enabled

func _is_tile_visible(x: int, y: int) -> bool:
	# Returns true if tile should show entities (Active visibility)
	# When FoW is disabled, all tiles are visible
	if not _fow_enabled:
		return true
	var vis: float = _visibility_value_at(x, y)
	return vis > FOW_VISIBLE_THRESHOLD  # Active tiles only

## Convert grid bounds to world-space bounds for pan clamping.
## Similar to _compute_bounds() but only for the explored region.
func _compute_explored_bounds_world(grid_bounds: Rect2i, radius: float) -> Rect2:
	if grid_bounds.size.x <= 0 or grid_bounds.size.y <= 0:
		return Rect2()

	var min_x := INF
	var max_x := -INF
	var min_y := INF
	var max_y := -INF

	for col in range(grid_bounds.position.x, grid_bounds.position.x + grid_bounds.size.x):
		for row in range(grid_bounds.position.y, grid_bounds.position.y + grid_bounds.size.y):
			var axial := _offset_to_axial(col, row)
			var center := _axial_center(axial.x, axial.y, radius)
			min_x = min(min_x, center.x - radius)
			max_x = max(max_x, center.x + radius)
			min_y = min(min_y, center.y - radius)
			max_y = max(max_y, center.y + radius)

	if min_x == INF:
		return Rect2()

	return Rect2(Vector2(min_x, min_y), Vector2(max_x - min_x, max_y - min_y))

func _draw_crisis_annotations(radius: float, origin: Vector2) -> void:
	if active_overlay_key != "crisis":
		return
	if crisis_annotations.is_empty():
		return
	for entry_variant in crisis_annotations:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var severity := String(entry.get("severity", "safe"))
		var color: Color = CRISIS_SEVERITY_COLORS.get(severity, CRISIS_COLOR)
		var stroke_color: Color = color
		stroke_color.a = max(color.a, 0.9)
		var fill_color: Color = color
		fill_color.a = min(color.a, 0.45)
		var coords: Array[Vector2] = []
		var path_variant: Variant = entry.get("path", PackedInt32Array())
		if path_variant is PackedInt32Array:
			var packed: PackedInt32Array = path_variant
			var length: int = packed.size()
			if length < 2:
				continue
			for idx in range(0, length, 2):
				if idx + 1 >= length:
					break
				var col := int(packed[idx])
				var row := int(packed[idx + 1])
				coords.append(_hex_center(col, row, radius, origin))
		elif path_variant is Array:
			var arr: Array = path_variant
			if arr.is_empty():
				continue
			for step in arr:
				if step is Array and step.size() >= 2:
					var col := int(step[0])
					var row := int(step[1])
					coords.append(_hex_center(col, row, radius, origin))
		if coords.is_empty():
			continue
		var stroke_width: float = clamp(radius * 0.18, 2.0, 8.0)
		if coords.size() == 1:
			var center: Vector2 = coords[0]
			var halo_color: Color = fill_color
			halo_color.a = max(fill_color.a, 0.35)
			draw_circle(center, radius * 0.55, halo_color)
			var core_color: Color = stroke_color
			core_color.a = max(stroke_color.a, 0.85)
			draw_circle(center, radius * 0.32, core_color)
		else:
			var polyline := PackedVector2Array()
			for point in coords:
				polyline.append(point)
			draw_polyline(polyline, stroke_color, stroke_width, true)
			var head: Vector2 = coords[coords.size() - 1]
			var tail: Vector2 = coords[0]
			var head_radius: float = clamp(radius * 0.28, 4.0, 12.0)
			var tail_radius: float = clamp(radius * 0.2, 3.0, 10.0)
			draw_circle(head, head_radius, stroke_color)
			var tail_color: Color = fill_color
			tail_color.a = max(fill_color.a, 0.55)
			draw_circle(tail, tail_radius, tail_color)
		var label: String = String(entry.get("label", ""))
		if label != "":
			var anchor: Vector2 = coords[coords.size() - 1]
			var font_size: int = int(round(clamp(radius * 0.5, 14.0, 26.0)))
			_draw_label(anchor + Vector2(radius * 0.3, -radius * 0.22), label, -1.0, font_size, Color(0.95, 0.96, 0.98, 0.95))

func _draw_trade_overlay(radius: float, origin: Vector2) -> void:
	if not trade_overlay_enabled:
		return
	if trade_links_overlay.is_empty():
		return
	if tile_lookup.is_empty():
		return

	for entry in trade_links_overlay:
		if not (entry is Dictionary):
			continue
		var link: Dictionary = entry
		var from_tile: int = int(link.get("from_tile", -1))
		var to_tile: int = int(link.get("to_tile", -1))
		if not tile_lookup.has(from_tile) or not tile_lookup.has(to_tile):
			continue
		var from_pos: Vector2i = tile_lookup[from_tile]
		var to_pos: Vector2i = tile_lookup[to_tile]
		var start: Vector2 = _hex_center(from_pos.x, from_pos.y, radius, origin)
		var end: Vector2 = _hex_center(to_pos.x, to_pos.y, radius, origin)
		var knowledge_variant: Variant = link.get("knowledge", {})
		var openness: float = 0.0
		var leak_timer: int = 0
		if knowledge_variant is Dictionary:
			var knowledge_dict: Dictionary = knowledge_variant
			openness = float(knowledge_dict.get("openness", 0.0))
			leak_timer = int(knowledge_dict.get("leak_timer", 0))
		var throughput: float = float(link.get("throughput", 0.0))
		var intensity: float = clamp(abs(throughput) * 0.25, 0.0, 2.5)
		var opacity: float = clamp(0.25 + openness * 0.6, 0.3, 0.95)
		var base_color: Color = Color(0.95, 0.74, 0.22, opacity)
		var width: float = 2.0 + intensity
		var entity_id: int = int(link.get("entity", -1))
		if entity_id == selected_trade_entity:
			base_color = Color(0.3, 0.95, 0.7, 0.95)
			width += 2.0

		draw_line(start, end, base_color, width)

		if leak_timer <= 1:
			var midpoint: Vector2 = start.lerp(end, 0.5)
			draw_circle(midpoint, 4.5, Color(1.0, 0.35, 0.28, 0.85))

func _unhandled_input(event: InputEvent) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	# While a command is targeting, Esc / right-click back out of it (instead of
	# panning), matching the targeting-mode contract.
	if _targeting.get("active", false):
		if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
			emit_signal("targeting_cancel_requested")
			_mark_input_handled()
			return
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_RIGHT:
			emit_signal("targeting_cancel_requested")
			_mark_input_handled()
			return
	if event is InputEventKey and event.pressed and event.keycode == KEY_C:
		_fit_map_to_view()
		_mark_input_handled()
		return
	if event is InputEventKey and event.pressed and event.keycode == KEY_H:
		_show_grid_lines = not _show_grid_lines
		_invalidate_map_cache()  # grid lines are baked into the cached texture; force a re-render
		queue_redraw()
		_mark_input_handled()
		return
	if event is InputEventKey and event.pressed and event.keycode == KEY_T:
		_terrain.toggle_terrain_textures()
		_mark_input_handled()
		return
	if event is InputEventMouseButton:
		var mouse_event: InputEventMouseButton = event
		if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
			_apply_zoom(MOUSE_ZOOM_STEP, get_local_mouse_position())
			_mark_input_handled()
			return
		elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
			_apply_zoom(-MOUSE_ZOOM_STEP, get_local_mouse_position())
			_mark_input_handled()
			return
		elif (mouse_event.button_index == MOUSE_BUTTON_MIDDLE or mouse_event.button_index == MOUSE_BUTTON_RIGHT):
			if mouse_event.pressed:
				_begin_mouse_pan(mouse_event.button_index)
			else:
				_end_mouse_pan(mouse_event.button_index)
			_mark_input_handled()
			return
		elif mouse_event.button_index == MOUSE_BUTTON_LEFT and mouse_event.pressed:
			var local_position: Vector2 = get_local_mouse_position()
			if not _is_local_point_in_view(local_position):
				return
			_update_layout_metrics()
			var offset := _point_to_offset(local_position)
			var col: int = offset.x
			var row: int = offset.y
			handle_hex_click(col, row, mouse_event.button_index)
			var herd_hit: Dictionary = _herd_at_point(local_position)
			if mouse_event.double_click and not herd_hit.is_empty():
				var shortcut_id := String(herd_hit.get("id", ""))
				if shortcut_id != "":
					# Double-click a herd -> quick-assign idle hunters (Sustain). The old
					# shift+double-click scout shortcut was retired with the scout command.
					emit_signal("herd_quick_hunt_requested", shortcut_id)
			_mark_input_handled()
			return
	elif event is InputEventMouseMotion:
		var motion: InputEventMouseMotion = event
		if mouse_pan_active:
			_apply_pan(motion.relative)
			_mark_input_handled()
		else:
			var local_pos: Vector2 = get_local_mouse_position()
			if not _is_local_point_in_view(local_pos):
				# Hovering the Inspector's reserved strip: no map tooltip.
				if _hovered_tile != Vector2i(-1, -1):
					_hovered_tile = Vector2i(-1, -1)
					emit_signal("tile_hovered", {})
				return
			_update_layout_metrics()
			var offset := _point_to_offset(local_pos)
			if offset != _hovered_tile:
				_hovered_tile = offset
				if offset.x < 0 or offset.y < 0:
					emit_signal("tile_hovered", {})
				elif _fow_enabled and _visibility_state_at(offset.x, offset.y) == "unexplored":
					# Never-seen tiles: no hover tooltip (they are inspectable via click).
					emit_signal("tile_hovered", {})
				else:
					# Active tiles get full info; Discovered tiles are redacted to
					# remembered terrain by _apply_visibility_to_info.
					var info := _apply_visibility_to_info(_tile_info_at(offset.x, offset.y), offset.x, offset.y)
					emit_signal("tile_hovered", info)
				queue_redraw()
	elif event is InputEventPanGesture:
		var gesture: InputEventPanGesture = event
		if gesture.alt_pressed:
			return
		_apply_pan(-gesture.delta)
		_mark_input_handled()
	elif event is InputEventMagnifyGesture:
		var magnify: InputEventMagnifyGesture = event
		var amount: float = (magnify.factor - 1.0) * KEYBOARD_ZOOM_SPEED
		if not is_zero_approx(amount):
			_apply_zoom(amount, get_local_mouse_position())
			_mark_input_handled()
## Faint links between the player's bands that share a supply network (bands
## auto-share goods by reach, grouped server-side by `supply_network_id`). Drawn
## as a simple chain through each network's members so the player can see who is
## pooling food. Solo bands (id 0) and non-player bands are ignored.
func _draw_supply_links(radius: float, origin: Vector2) -> void:
	var networks: Dictionary = {}  # supply_network_id -> Array[Vector2] of centers
	for unit in units:
		if not _is_player_unit(unit):
			continue
		var network_id: int = int(unit.get("supply_network_id", SUPPLY_NETWORK_SOLO))
		if network_id == SUPPLY_NETWORK_SOLO:
			continue
		var pos: Array = Array(unit.get("pos", []))
		if pos.size() != 2:
			continue
		var center: Vector2 = _hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)
		var members: Array = networks.get(network_id, [])
		members.append(center)
		networks[network_id] = members
	for network_id in networks:
		var members: Array = networks[network_id]
		if members.size() < 2:
			continue
		# Chain the members in draw order — enough to read the grouping for the
		# small networks these form, without an all-pairs mesh.
		for i in range(members.size() - 1):
			var a: Vector2 = members[i]
			var b: Vector2 = members[i + 1]
			# Skip wrap artifacts (a segment spanning most of the map width).
			if abs(a.x - b.x) > last_map_size.x * 0.4:
				continue
			draw_line(a, b, SUPPLY_LINK_COLOR, SUPPLY_LINK_WIDTH)
## Coordinator push (Hud.labor_pending_changed → Main → here): the per-band optimistic pending
## map, stored by _band_overlays; the selected band's pending shows in a dashed-amber style.
## THIS SEAM IS PUBLIC AND NAME-BOUND — Main.gd wires the HUD signal to it via has_method /
## Callable(map_view, "set_labor_pending") and tools/map_preview.gd calls it on the MapView, so
## the name and signature must stay put even though the state now lives in the helper.
func set_labor_pending(pending: Dictionary) -> void:
	_band_overlays.set_labor_pending(pending)
	queue_redraw()

func _herd_by_id(herd_id: String) -> Dictionary:
	if herd_id == "":
		return {}
	for herd in herds:
		if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
			return herd
	return {}

## Band's wrapped column: the copy of `col` nearest the viewport centre (matches
## `_hex_center_wrapped`), so highlights render contiguous with the band across the seam.
func _band_effective_col(col: int, radius: float, origin: Vector2) -> int:
	if not (_wrap_horizontal and grid_width > 0):
		return col
	var viewport_size: Vector2 = _get_adjusted_viewport_size()
	var center_world_x: float = viewport_size.x * 0.5 - origin.x
	var col_width: float = SQRT3 * radius
	var center_col: float = center_world_x / col_width
	var wrap_offset: int = int(round((center_col - float(col)) / float(grid_width)))
	return col + wrap_offset * grid_width

## Shortest signed column delta from `from_col` to `to_col`, honoring horizontal wrap, so a
## target tile renders adjacent to the band rather than across the whole map.
## Mirrors the sim's `grid_utils::shortest_delta_x` exactly: keep the direct delta when it is
## within half the width, else shift by one width. The exact-half tie (`abs(d) == width/2`)
## keeps the DIRECT signed value (so `-width/2` stays negative), matching the sim — NOT `round()`'s
## half-away-from-zero (which flipped the sign at the antipode and pointed the travel line the wrong seam direction).
func _wrapped_col_delta(from_col: int, to_col: int) -> int:
	var d := to_col - from_col
	if _wrap_horizontal and grid_width > 0:
		# Integer half-width mirrors the sim's `w / 2` truncation.
		var half_width := grid_width / 2
		if d > half_width:
			d -= grid_width
		elif d < -half_width:
			d += grid_width
	return d

func _fill_hex(col: int, row: int, radius: float, origin: Vector2, fill: Color) -> void:
	var center := _hex_center(col, row, radius, origin)
	var pts := _hex_points(center, radius)
	draw_polygon(pts, PackedColorArray([fill, fill, fill, fill, fill, fill]))

func _outline_hex(col: int, row: int, radius: float, origin: Vector2, color: Color, width: float) -> void:
	var center := _hex_center(col, row, radius, origin)
	var pts := _hex_points(center, radius)
	var outline := PackedVector2Array([pts[0], pts[1], pts[2], pts[3], pts[4], pts[5], pts[0]])
	draw_polyline(outline, color, width, true)

## White outline on the selected hex + a faint outline on the hovered hex (skipped when
## hover == selection). Replaces the old brown-circle-as-selection feel; the hex-shape
## outline is the sole selection cue — there is NO per-token ring, and the active band in a
## stack reads by full brightness over its darkened/shrunk back cards.
func _draw_tile_selection_highlight(radius: float, origin: Vector2) -> void:
	if selected_tile.x >= 0 and selected_tile.y >= 0:
		_outline_hex(selected_tile.x, selected_tile.y, radius, origin, SELECTED_HEX_OUTLINE_COLOR, SELECTED_HEX_OUTLINE_WIDTH)
	if _hovered_tile.x >= 0 and _hovered_tile.y >= 0 and _hovered_tile != selected_tile:
		_outline_hex(_hovered_tile.x, _hovered_tile.y, radius, origin, HOVER_HEX_OUTLINE_COLOR, HOVER_HEX_OUTLINE_WIDTH)
func _draw_label(pos: Vector2, text: String, max_width: float, font_size: int, color: Color) -> void:
	var font: Font = ThemeDB.fallback_font
	if font != null:
		draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, max_width, font_size, color)
## Draw a marker glyph with a subtle drop shadow (replaces the old dark backing disc):
## the glyph once offset in near-black, then again on top, centered on `center`.
func _draw_marker_glyph(center: Vector2, glyph: String, size: int, color: Color) -> void:
	var font: Font = ThemeDB.fallback_font
	if font == null or glyph == "":
		return
	var text_size: Vector2 = font.get_string_size(glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size)
	var baseline := Vector2(center.x - text_size.x * 0.5, center.y + size * 0.34)
	draw_string(font, baseline + MARKER_GLYPH_SHADOW_OFFSET, glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size, MARKER_GLYPH_SHADOW_COLOR)
	draw_string(font, baseline, glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size, color)

## Sprite sibling of `_draw_marker_glyph`: a bundled marker texture in a `size`×`size` box centered
## on `center`, wearing the SAME drop-shadow treatment (once offset in near-black, then again on
## top) so a sprite marker and an emoji marker sit on the map identically.
## The sprite is drawn UNTINTED by default (`modulate` = white) — see the herd-marker comment in
## `SecondaryMarkerRenderer.draw_herd`: distress reads as ring + badge geometry, never as a modulate.
## `modulate` exists for the ONE case where a tint is structural rather than semantic: the band
## card-stack's behind cards, which recede via `BAND_STACK_BEHIND_TINT` exactly as the stage GLYPH
## path does (`BandMarkerRenderer._draw_band_token`). Do not use it to encode state.
func _draw_marker_sprite(center: Vector2, tex: Texture2D, size: int, modulate: Color = Color.WHITE) -> void:
	if tex == null or size <= 0:
		return
	var box := Rect2(center - Vector2(size, size) * 0.5, Vector2(size, size))
	draw_texture_rect(tex, Rect2(box.position + MARKER_GLYPH_SHADOW_OFFSET, box.size), false, MARKER_GLYPH_SHADOW_COLOR)
	draw_texture_rect(tex, box, false, modulate)
## The shared rounded-pill PLATE: a dark rounded-rect (draw_rect body + two end-cap circles) centered
## on `center`, sized to an already-measured `text_size` plus `pad_x` of symmetric horizontal padding.
## Single source of truth for the pill look — used by the `×N`/`+N` count badges (`_draw_count_pill`,
## no extra padding: the end caps are its padding) and by the on-tile yield labels
## (`BandOverlayRenderer._draw_yield_label`, padded so the plate hugs the text+glyph run).
func _draw_pill_plate(center: Vector2, text_size: Vector2, pad_x: float, bg: Color) -> void:
	var half_w: float = text_size.x * 0.5 + pad_x
	var half_h: float = text_size.y * 0.5 * MARKER_BADGE_HEIGHT_FACTOR
	draw_rect(Rect2(center.x - half_w, center.y - half_h, half_w * 2.0, half_h * 2.0), bg)
	draw_circle(Vector2(center.x - half_w, center.y), half_h, bg)
	draw_circle(Vector2(center.x + half_w, center.y), half_h, bg)

## A small dark rounded pill with centered text — shared by the primary `×N` count
## badge and the secondary `+N` overflow chip.
func _draw_count_pill(center: Vector2, text: String) -> void:
	var font: Font = ThemeDB.fallback_font
	if font == null or text == "":
		return
	var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, MARKER_BADGE_FONT_SIZE)
	_draw_pill_plate(center, text_size, MARKER_BADGE_PAD_X, MARKER_BADGE_BG)
	draw_string(font, Vector2(center.x - text_size.x * 0.5, center.y + text_size.y * 0.32), text, HORIZONTAL_ALIGNMENT_LEFT, -1, MARKER_BADGE_FONT_SIZE, MARKER_BADGE_FG)
func _draw_route(order: Dictionary, radius: float, origin: Vector2) -> void:
	var path: Array = order.get("path", [])
	if path.is_empty():
		return
	var color: Color = faction_colors.get(order.get("faction", ""), Color(0.95, 0.9, 0.6, 0.8))
	var points: Array[Vector2] = []
	for waypoint in path:
		if waypoint.size() != 2:
			continue
		points.append(_hex_center(int(waypoint[0]), int(waypoint[1]), radius, origin))
	if points.size() < 2:
		return
	for i in range(points.size() - 1):
		draw_line(points[i], points[i + 1], color, 3.0)

func _overlay_array(key: String) -> PackedFloat32Array:
	var variant: Variant = overlay_channels.get(key, null)
	if variant is PackedFloat32Array:
		return variant
	return PackedFloat32Array()

func _overlay_raw_array(key: String) -> PackedFloat32Array:
	var variant: Variant = overlay_raw_channels.get(key, null)
	if variant is PackedFloat32Array:
		return variant
	return PackedFloat32Array()

func _average_overlay(key: String) -> float:
	return _average(_overlay_raw_array(key))

func _value_at_overlay(key: String, x: int, y: int) -> float:
	return _value_at(_overlay_array(key), x, y)

## Relative 0..100 "Height" for a tile, for the tile panels. Elevation is surfaced
## only as the normalized 0..1 ElevationField raster, so this reads the RAW elevation
## channel (the per-frame min/max-normalized channel would distort cross-tile
## comparison) and rescales the above-sea-level span into 0..100 — sea level and below
## clamp to 0. Returns -1 when no elevation data has streamed yet so callers can omit
## the row.
func relative_height_at(x: int, y: int) -> int:
	var raster: PackedFloat32Array = _overlay_raw_array("elevation")
	if raster.is_empty():
		return -1
	var normalized: float = _value_at(raster, x, y)
	var sea_level: float = clampf(_elevation_sea_level, 0.0, 0.999)
	var above_sea: float = (normalized - sea_level) / (1.0 - sea_level)
	return int(round(clampf(above_sea, 0.0, 1.0) * 100.0))

## Formats a relative height (0..100) as a number plus a filled/empty bar, e.g.
## "78  ▰▰▰▰▰▰▰▱▱▱", so two tiles can be compared at a glance. Single source of truth
## shared by every tile panel.
func format_height(height: int) -> String:
	var clamped: int = clampi(height, 0, 100)
	var filled: int = int(round(float(clamped) / 100.0 * HEIGHT_BAR_SEGMENTS))
	var bar: String = ""
	for i in HEIGHT_BAR_SEGMENTS:
		bar += "▰" if i < filled else "▱"
	return "%d  %s" % [clamped, bar]

## Fog of War reads the RAW visibility channel, never the min-max normalized one.
## The channel carries a discrete encoding (0.0 = Unexplored, 0.5 = Discovered,
## 1.0 = Active) and the FoW thresholds are tuned to it. normalize_overlay()
## rescales per frame, so whenever a frame lacks either an unexplored (0.0) or an
## active (1.0) tile the 0.5 "discovered" value collapses to 0.0 and the remembered
## terrain wrongly renders as black. Reading raw keeps the encoding intact.
func _visibility_array() -> PackedFloat32Array:
	return _overlay_raw_array("visibility")

func _visibility_value_at(x: int, y: int) -> float:
	return _value_at(_visibility_array(), x, y)

## Three-state Fog of War classification for a tile: "active", "discovered", or
## "unexplored". Returns "" when FoW is disabled so callers render full info.
func _visibility_state_at(x: int, y: int) -> String:
	if not _fow_enabled:
		return ""
	var vis := _visibility_value_at(x, y)
	if vis > FOW_VISIBLE_THRESHOLD:
		return "active"
	if vis > FOW_EXPLORED_THRESHOLD:
		return "discovered"
	return "unexplored"

## Tag tile info with its FoW state and strip fields the player cannot currently
## know. Active tiles (and FoW-off) keep full info; Discovered tiles keep only the
## remembered terrain (biome/tags); Unexplored tiles keep just their coordinates.
func _apply_visibility_to_info(info: Dictionary, x: int, y: int) -> Dictionary:
	var state := _visibility_state_at(x, y)
	if state == "":
		return info
	info["visibility_state"] = state
	if state == "unexplored":
		return {"x": info.get("x", x), "y": info.get("y", y), "visibility_state": state}
	if state == "discovered":
		for key in FOW_DISCOVERED_HIDDEN_KEYS:
			info.erase(key)
	return info

## Vertex-color tint for a TEXTURED tile given its already-computed FoW state.
## Pure function of `state` (no per-tile visibility lookup) so the hot draw loops
## can classify each tile once via _visibility_state_at() and derive both the
## hide decision (state == "unexplored") and this tint from that single value.
## Discovered tiles are tinted toward the mist color (remembered/cloudy while
## keeping their texture); Active tiles (and FoW off, state == "") draw full.
func _fow_texture_tint_for_state(state: String) -> Color:
	if state == "discovered":
		return Color.WHITE.lerp(_fow_mist_color, _fow_mist_blend)
	return Color.WHITE

func _value_at(data: PackedFloat32Array, x: int, y: int) -> float:
	if data.is_empty() or grid_width == 0:
		return 0.0
	var index: int = y * grid_width + x
	if index < 0 or index >= data.size():
		return 0.0
	return clamp(float(data[index]), 0.0, 1.0)

func _terrain_id_at(x: int, y: int) -> int:
	if terrain_overlay.is_empty() or grid_width == 0:
		return -1
	var index: int = y * grid_width + x
	if index < 0 or index >= terrain_overlay.size():
		return -1
	return int(terrain_overlay[index])

func _rebuild_unit_markers(snapshot: Dictionary) -> void:
	units = []
	_has_awaiting_expedition = false
	var population_variant: Variant = snapshot.get("populations", [])
	if not (population_variant is Array):
		return
	var counter := 1
	var label_cache: Dictionary = {}
	for entry_variant in population_variant:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant

		# Use current position if available, otherwise fall back to home tile lookup
		var current_x: int = int(entry.get("current_x", -1))
		var current_y: int = int(entry.get("current_y", -1))
		var is_traveling: bool = bool(entry.get("is_traveling", false))

		if current_x < 0 or current_y < 0:
			# Fall back to home tile lookup
			var home_id: int = int(entry.get("home", -1))
			if home_id < 0 or not tile_lookup.has(home_id):
				continue
			var coords: Vector2i = tile_lookup[home_id]
			current_x = coords.x
			current_y = coords.y

		var label: String = String(entry.get("label", ""))
		if label == "":
			label = "Band %d" % counter
		while label_cache.has(label):
			counter += 1
			label = "Band %d" % counter
		label_cache[label] = true
		var marker := {
			"entity": int(entry.get("entity", -1)),
			"faction": entry.get("faction", PLAYER_FACTION_ID),
			"pos": [current_x, current_y],
			"size": int(entry.get("size", 0)),
			"id": label,
			"is_traveling": is_traveling,
			# Travel destination tile (valid only while `is_traveling`; `0,0` otherwise). Drives the
			# wrap-aware destination reticle + line the selected traveling unit draws (band OR
			# expedition) in BandOverlayRenderer._draw_travel_destination.
			"travel_target_x": int(entry.get("travel_target_x", 0)),
			"travel_target_y": int(entry.get("travel_target_y", 0)),
			"turns_of_food": float(entry.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS)),
			# Band food ledger (food/turn) — total income across worked sources vs total consumption.
			# Carried onto the marker so the allocation panel's ledger footer reads them off the
			# selected-unit copy (the per-source actual/sustainable yields ride inside labor_assignments).
			"food_income": float(entry.get("food_income", 0.0)),
			"food_consumption": float(entry.get("food_consumption", 0.0)),
			# The ledger's THIRD term: the food this band paid this turn to feed the pens it keeps
			# (a corralled herd cannot graze). It comes straight off the larder and is in neither of
			# the two rows above, so the Food line's net rate must subtract it — see DetailFormat.band_net_food.
			"pen_feed_upkeep": float(entry.get("pen_feed_upkeep", 0.0)),
			"morale": float(entry.get("morale", 1.0)),
			"morale_delta": float(entry.get("morale_delta", 0.0)),
			"morale_cause": int(entry.get("morale_cause", 0)),
			# Civilization Wellbeing (docs/plan_civ_wellbeing.md): productivity, discontent,
			# migration counters, and the four signed Layer-1 morale contributions that feed
			# the band drawer's itemized breakdown + "people leaving" alert reason.
			"output_multiplier": float(entry.get("output_multiplier", 1.0)),
			"discontent_fraction": float(entry.get("discontent_fraction", 0.0)),
			"last_emigrated": int(entry.get("last_emigrated", 0)),
			"last_immigrated": int(entry.get("last_immigrated", 0)),
			"morale_settling": float(entry.get("morale_settling", 0.0)),
			"morale_terrain": float(entry.get("morale_terrain", 0.0)),
			"morale_climate": float(entry.get("morale_climate", 0.0)),
			"morale_unrest": float(entry.get("morale_unrest", 0.0)),
			# Data-driven settlement stage (icon glyph + label). The icon becomes the band's
			# map token; empty icon → neutral non-circular fallback marker (square; ownership is
			# on the banner, no disc). Label surfaces in tooltip/roster.
			"settlement_stage_id": String(entry.get("settlement_stage_id", "")),
			"settlement_stage_label": String(entry.get("settlement_stage_label", "")),
			"settlement_stage_icon": String(entry.get("settlement_stage_icon", "")),
			"activity": String(entry.get("activity", "")),
			# Fauna-pursuit sub-mode (single/sustain/surplus/market/eradicate); flows to the
			# drawer + roster so "Cancel <Mode> Hunt" can label a live hunting band.
			"hunt_mode": String(entry.get("hunt_mode", "")),
			"supply_network_id": int(entry.get("supply_network_id", 0)),
			# Early-Game Labor (slice 3b): what the band is working + its reach, for the
			# selected-band map highlights (work-range ring / worked forage tiles / hunted
			# herds) AND the allocation panel's Population/Workers/Idle header (the drawer
			# reads _selected_unit, which is a copy of this marker — so these must be carried
			# here or the panel reads 0). `scout_reveal_radius` (now the band's sight-range
			# bonus, not a reveal-disc radius) is still carried but no longer drawn.
			"work_range": int(entry.get("work_range", 0)),
			# Hunt reach (work_range + hunt leash): the herd-hunt affordance offers a LOCAL hunt within
			# this hex distance, a hunting EXPEDITION beyond it (Hud._build_herd_assign_controls).
			"hunt_reach": int(entry.get("hunt_reach", 0)),
			"scout_reveal_radius": int(entry.get("scout_reveal_radius", 0)),
			"working_age": int(entry.get("working_age", 0)),
			"idle_workers": int(entry.get("idle_workers", 0)),
			# Age structure of THIS band (children / working / elders). Distinct from `working_age`
			# above, which counts assignable workers — hence the `age_` prefix on all three.
			# FRACTIONAL, like every other Scalar on this block: the decoder runs them through
			# `fixed64_to_f64`, and truncating here zeroes every remainder, so `Hud._apportion_people`
			# has nothing left to redistribute and the PEOPLE header undercounts the band.
			"age_children": float(entry.get("age_children", 0.0)),
			"age_working": float(entry.get("age_working", 0.0)),
			"age_elders": float(entry.get("age_elders", 0.0)),
			# Scouting expedition (docs/plan_exploration_and_sites.md §2): a detached party is a
			# cohort tagged Expedition flowing through this same populations[] array. These three
			# discriminator fields drive the distinct expedition marker (_draw_band_token →
			# _draw_expedition_body), its awaiting-orders idle indicator, and the HUD expedition
			# panel. Default false/"" so
			# resident-band markers are unaffected.
			"is_expedition": bool(entry.get("is_expedition", false)),
			"expedition_mission": String(entry.get("expedition_mission", "")),
			"expedition_phase": String(entry.get("expedition_phase", "")),
			# The band that outfitted this party (entity bits; 0 for a normal band) — the Band/City
			# panel groups a band's active expeditions by home_band_entity == band.entity.
			"home_band_entity": int(entry.get("home_band_entity", 0)),
			# Hunt expedition (PR 2): the herd (fauna_id) a hunt party follows; "" for scouts.
			"expedition_target_herd": String(entry.get("expedition_target_herd", "")),
			# Hunt party take policy (sustain|surplus|market|eradicate; "" for scouts) + carry cap.
			"expedition_hunt_policy": String(entry.get("expedition_hunt_policy", "")),
			"expedition_carry_cap": float(entry.get("expedition_carry_cap", 0.0)),
			# Next-delivery forecast (the in-flight raid twin): the detail panel's "Next delivery" line
			# reads these off `_selected_unit` (the marker), so they MUST ride the marker or the panel
			# renders nothing while the Parties-zone row (raw dict) shows the token — guarded by
			# marker_field_guard (fractional round-trip for the projected float).
			"expedition_eta_turns": int(entry.get("expedition_eta_turns", 0)),
			"expedition_projected_delivery": float(entry.get("expedition_projected_delivery", 0.0)),
			"expedition_recurring": bool(entry.get("expedition_recurring", false)),
			# Hard party-size cap (from the expedition config); the resident-band outfit stepper
			# clamps its max to min(idle_workers, this).
			"max_expedition_party_size": int(entry.get("max_expedition_party_size", 0)),
				# Global expedition/labor config levers echoed on every cohort. They ride the marker
				# because the targeting flow carries a copy of the band dict, and the pre-launch
				# forecast reads its threshold + the local-hunt preview its take rate off it. Neither
				# computes an expedition's trip length: that is a PURE LOOKUP into the target herd's
				# sim-simulated `hunt_trip_estimates` (the client never divides a carry cap by a
				# rate). `expedition_viability_warn_turns` = the viable/not-viable threshold on
				# turns_to_fill, `hunt_per_worker_provisions` = the RESIDENT-BAND local-hunt take
				# rate, which IS arithmetic. Band = flow arithmetic; expedition = lookup.
				"hunt_per_worker_provisions": float(entry.get("hunt_per_worker_provisions", 0.0)),
				"expedition_viability_warn_turns": int(entry.get("expedition_viability_warn_turns", 0)),
				# Per-worker carry: the pre-launch forecast shows the HAUL a filled pack delivers as
				# party × this lever (the same blessed party×lever arithmetic as the band ceiling, NOT
				# the turns-to-fill lookup). 0 when absent → no haul rendered.
				"expedition_per_worker_carry": float(entry.get("expedition_per_worker_carry", 0.0)),
				# Band move speed (tiles/turn). The hunt-expedition forecast's round-trip TRAVEL turns
				# are ceil(2 × hex_distance(band, herd) / this), added to the herd's hunting turns for
				# the total trip length (and the per-turn averaging denominator). 0/absent → travel 0.
				"band_move_tiles_per_turn": float(entry.get("band_move_tiles_per_turn", 0.0)),
			"labor_assignments": (entry.get("labor_assignments", []) as Array).duplicate(true) if entry.get("labor_assignments", []) is Array else [],
		}
		var stores_variant: Variant = entry.get("stores", {})
		if stores_variant is Dictionary:
			marker["stores"] = (stores_variant as Dictionary).duplicate(true)

		# Add destination info for units with active assignments
		var harvest_variant: Variant = entry.get("harvest", {})
		if harvest_variant is Dictionary:
			var harvest: Dictionary = harvest_variant as Dictionary
			marker["harvest"] = harvest.duplicate(true)
			marker["dest_x"] = int(harvest.get("target_x", -1))
			marker["dest_y"] = int(harvest.get("target_y", -1))
			marker["travel_task_kind"] = String(harvest.get("kind", "harvest"))
		var scout_variant: Variant = entry.get("scout", {})
		if scout_variant is Dictionary:
			var scout: Dictionary = scout_variant as Dictionary
			marker["scout"] = scout.duplicate(true)
			if not marker.has("dest_x") or int(marker.get("dest_x", -1)) < 0:
				marker["dest_x"] = int(scout.get("target_x", -1))
				marker["dest_y"] = int(scout.get("target_y", -1))
				marker["travel_task_kind"] = "scout"
		var stockpile_variant: Variant = entry.get("accessible_stockpile", {})
		if stockpile_variant is Dictionary:
			marker["accessible_stockpile"] = (stockpile_variant as Dictionary).duplicate(true)
		if bool(marker.get("is_expedition", false)) \
				and String(marker.get("expedition_phase", "")) == EXPEDITION_PHASE_AWAITING:
			_has_awaiting_expedition = true
		units.append(marker)
		counter += 1

func _rebuild_herd_markers(snapshot: Dictionary) -> void:
	herds = []
	var herd_variant: Variant = snapshot.get("herds", [])
	if not (herd_variant is Array):
		herd_trails.clear()
		return
	var active_ids := {}
	for entry in herd_variant:
		if entry is Dictionary:
			var herd_dict: Dictionary = (entry as Dictionary).duplicate(true)
			herds.append(herd_dict)
			var herd_id := String(herd_dict.get("id", ""))
			if herd_id != "":
				active_ids[herd_id] = true
				_update_herd_trail(herd_id, herd_dict)
	var stale_ids := herd_trails.keys()
	for herd_id in stale_ids:
		if not active_ids.has(herd_id):
			herd_trails.erase(herd_id)

## Select a subject chosen from the HUD selection list (no hex click). `kind` is
## "unit" (id = entity_id int), "herd" (id = herd_id String) or **"land"** (no id — the tile
## itself). Sets `selected_unit_id`/`selected_herd_id` (and syncs `cycle_index`) so the picked
## occupant becomes the active/top stack card and the hex selection outline reflects it — there is
## no per-token ring; selection is the hex outline.
##
## "LAND" IS A REAL SUBJECT, SO IT MUST CLEAR THE OCCUPANT SELECTION — picking a band clears the
## herd, and picking the land clears both. Without it `refresh_selection_payload` still sees
## `selected_unit_id >= 0` and answers `kind: "unit"` every snapshot, which restores the band and
## silently steals a deliberately-chosen land selection back (the land was unselectable on any
## occupied hex). `selected_tile` is deliberately untouched — the land IS that tile — and so is
## `cycle_index`, so re-clicking the hex on the map still cycles the band stack from where it was.
func select_occupant(kind: String, id) -> void:
	if kind == "unit":
		selected_unit_id = int(id)
		selected_herd_id = ""
		# Surface the roster-picked band as the top stack card, and seed cycling from it.
		cycle_index = _cycle_index_for_unit(selected_unit_id)
	elif kind == "herd":
		selected_herd_id = String(id)
		selected_unit_id = -1
	elif kind == "land":
		selected_unit_id = -1
		selected_herd_id = ""
	queue_redraw()

## The band's position within the stack on its own tile — so a roster selection shows it
## on top and map re-click cycling continues from it. Returns 0 if not found.
func _cycle_index_for_unit(entity_id: int) -> int:
	for unit in units:
		if int(unit.get("entity", -1)) != entity_id:
			continue
		var pos: Array = Array(unit.get("pos", []))
		if pos.size() != 2:
			return 0
		var here := _units_on_tile(int(pos[0]), int(pos[1]))
		for i in range(here.size()):
			if int((here[i] as Dictionary).get("entity", -1)) == entity_id:
				return i
		return 0
	return 0

## Re-resolve the current selection against the freshly-rebuilt markers/tiles so the
## HUD panel can refresh after a snapshot without the user reselecting the hex.
## Returns {"kind": "unit"|"herd"|"tile"|"none", "data": {...}}, mirroring the payload
## shape each selection path emits. Selection is conveyed by the hex outline (no
## per-token ring): a selected band/herd that no longer exists in the new snapshot has
## its selected id cleared and falls through to its tile ("tile") or "none".
func refresh_selection_payload() -> Dictionary:
	if selected_unit_id >= 0:
		for unit in units:
			if int(unit.get("entity", -1)) == selected_unit_id:
				# A FOREIGN band can WALK INTO the fog while selected. Keeping it selected would stream
				# its live state into the panel off a band the player can no longer see, so the
				# selection drops with its marker (mirrors the selected-herd rule). Your own band is
				# never dropped.
				if _unit_hidden_by_fog(unit as Dictionary):
					break
				var payload: Dictionary = (unit as Dictionary).duplicate(true)
				var pos := Array(payload.get("pos", []))
				var ux := int(pos[0]) if pos.size() == 2 else selected_tile.x
				var uy := int(pos[1]) if pos.size() == 2 else selected_tile.y
				payload["tile_info"] = _tile_info_at(ux, uy)
				return {"kind": "unit", "data": payload}
		# The selected band left/expired — clear the selection and fall through.
		selected_unit_id = -1
	if selected_herd_id != "":
		for herd in herds:
			if String(herd.get("id", "")) == selected_herd_id:
				var payload: Dictionary = (herd as Dictionary).duplicate(true)
				var hx := int(payload.get("x", selected_tile.x))
				var hy := int(payload.get("y", selected_tile.y))
				# A migratory herd can WALK OUT of sight while selected. Keeping it selected would
				# stream live biomass/ecology (and a live hunt forecast) off a herd the player can no
				# longer see, so the selection drops with the marker and the hex falls back to its
				# tile card — which now states the hex is out of sight.
				if not _is_tile_visible(hx, hy):
					break
				payload["tile_info"] = _tile_info_at(hx, hy)
				return {"kind": "herd", "data": payload}
		selected_herd_id = ""
	if selected_tile.x >= 0 and selected_tile.y >= 0:
		var info := _apply_visibility_to_info(
			_tile_info_at(selected_tile.x, selected_tile.y), selected_tile.x, selected_tile.y
		)
		return {"kind": "tile", "data": info}
	return {"kind": "none"}

func _handle_entity_selection(col: int, row: int) -> void:
	# Check for units on this tile
	var units_here := _units_on_tile(col, row)
	if not units_here.is_empty():
		# Select-then-cycle: cycle_index picks which band in the stack is active.
		var unit: Dictionary = units_here[clampi(cycle_index, 0, units_here.size() - 1)]
		selected_unit_id = int(unit.get("entity", -1))
		selected_herd_id = ""
		var unit_payload: Dictionary = (unit as Dictionary).duplicate(true)
		var pos := Array(unit_payload.get("pos", []))
		var unit_col := col
		var unit_row := row
		if pos.size() == 2:
			unit_col = int(pos[0])
			unit_row = int(pos[1])
		unit_payload["tile_info"] = _tile_info_at(unit_col, unit_row)
		emit_signal("unit_selected", unit_payload)
		queue_redraw()
		return

	# Check for herds on this tile
	var herds_here := _herds_on_tile(col, row)
	if not herds_here.is_empty():
		var herd: Dictionary = herds_here[0]
		selected_unit_id = -1
		selected_herd_id = String(herd.get("id", ""))
		var herd_payload: Dictionary = (herd as Dictionary).duplicate(true)
		var herd_col: int = int(herd_payload.get("x", col))
		var herd_row: int = int(herd_payload.get("y", row))
		herd_payload["tile_info"] = _tile_info_at(herd_col, herd_row)
		emit_signal("herd_selected", herd_payload)
		queue_redraw()
		return
	if selected_unit_id != -1 or selected_herd_id != "":
		selected_unit_id = -1
		selected_herd_id = ""
		emit_signal("selection_cleared")
		selected_tile = Vector2i(-1, -1)
		queue_redraw()

func _update_herd_trail(herd_id: String, herd: Dictionary) -> void:
	if herd_id == "":
		return
	var x := int(herd.get("x", -1))
	var y := int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	var current := Vector2i(x, y)
	var trail: Array = herd_trails.get(herd_id, [])
	if trail.is_empty() or trail[trail.size() - 1] != current:
		trail.append(current)
	var max_len := int(herd.get("route_length", trail.size()))
	if max_len > 0:
		while trail.size() > max_len:
			trail.remove_at(0)
	herd_trails[herd_id] = trail

func _draw_herd_trail(herd_id: String, radius: float, origin: Vector2) -> void:
	if herd_id == "":
		return
	if not herd_trails.has(herd_id):
		return
	var trail: Array = herd_trails[herd_id]
	if trail.size() < 2:
		return
	var points := PackedVector2Array()
	for tile in trail:
		if tile is Vector2i:
			points.append(_hex_center(tile.x, tile.y, radius, origin))
	if points.size() >= 2:
		draw_polyline(points, Color(0.97, 0.69, 0.25, 0.6), 2.0)

func _draw_arrowhead(start: Vector2, end: Vector2, color: Color, size: float = 8.0) -> void:
	var direction := end - start
	if direction.length() <= 0.1:
		return
	var norm := direction.normalized()
	var ortho := Vector2(-norm.y, norm.x)
	var tip := end
	var base_point := tip - norm * size
	var left := base_point + ortho * (size * 0.5)
	var right := base_point - ortho * (size * 0.5)
	var pts := PackedVector2Array([tip, left, right])
	draw_polygon(pts, PackedColorArray([color, color, color]))

func _emit_tile_selection(col: int, row: int) -> void:
	if col < 0 or row < 0 or col >= grid_width or row >= grid_height:
		return
	selected_tile = Vector2i(col, row)
	var info := _apply_visibility_to_info(_tile_info_at(col, row), col, row)
	emit_signal("tile_selected", info)
	queue_redraw()

## Hit-test a band MARKER under the pointer. Fog-gated: a marker that isn't drawn can't be clicked, so
## a foreign band under the fog can't be picked out of an apparently-empty hex.
func _unit_at_point(point: Vector2) -> Dictionary:
	for unit in units:
		if _unit_hidden_by_fog(unit):
			continue
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		var center := _hex_center_wrapped(int(position[0]), int(position[1]), last_hex_radius, last_origin)
		if center.distance_to(point) <= last_hex_radius * 0.55:
			return unit
	return {}

## Hit-test a herd MARKER under the pointer (the double-click quick-hunt shortcut). Fog-gated like
## `_herds_on_tile`: a marker that isn't drawn can't be clicked, so an unseen herd can't be quick-hunted.
func _herd_at_point(point: Vector2) -> Dictionary:
	for herd in herds:
		var x := int(herd.get("x", -1))
		var y := int(herd.get("y", -1))
		if x < 0 or y < 0 or not _is_tile_visible(x, y):
			continue
		var center := _hex_center_wrapped(x, y, last_hex_radius, last_origin)
		if center.distance_to(point) <= last_hex_radius * 0.45:
			return herd
	return {}

func _tile_info_at(col: int, row: int) -> Dictionary:
	var info: Dictionary = {
		"x": col,
		"y": row,
	}
	if col < 0 or row < 0 or col >= grid_width or row >= grid_height:
		return info
	var terrain_id := _terrain_id_at(col, row)
	info["terrain_id"] = terrain_id
	info["terrain_label"] = String(_get_terrain_labels().get(terrain_id, "Terrain %d" % terrain_id))
	var relative_height := relative_height_at(col, row)
	if relative_height >= 0:
		info["relative_height"] = relative_height
		info["height_display"] = format_height(relative_height)
	var tile_key := Vector2i(col, row)
	if discovered_site_lookup.has(tile_key):
		var wsite: Dictionary = discovered_site_lookup[tile_key]
		info["site_name"] = String(wsite.get("display_name", ""))
	if tile_habitability.has(tile_key):
		info["habitability"] = float(tile_habitability[tile_key])
	if tile_temperature.has(tile_key):
		info["temperature"] = float(tile_temperature[tile_key])
	# Pasture (graze). Deliberately NOT in FOW_DISCOVERED_HIDDEN_KEYS: like the biome, the height and
	# the habitability above it, the grass is a property of the GROUND — you can see a steppe from a
	# ridge, and it is remembered, not live contents. (Occupants are what a remembered tile redacts.)
	if tile_graze.has(tile_key):
		var graze: Dictionary = tile_graze[tile_key]
		info["graze_biomass"] = float(graze.get("biomass", 0.0))
		info["graze_capacity"] = float(graze.get("capacity", 0.0))
		info["graze_ecology_phase"] = String(graze.get("phase", ""))
	# Hex-edge rivers (the 12-bit Minor/Major mask, 2 bits per odd-r direction). Deliberately NOT in
	# FOW_DISCOVERED_HIDDEN_KEYS: a river is permanent geography, like the terrain label and a
	# discovered Wondrous Site, so a remembered tile still reports it. Never-seen tiles are already
	# handled by the `unexplored` redaction. Formatted for text by ui/RiverEdges.gd.
	info["river_edges"] = int(tile_river_edges.get(tile_key, 0))
	var mask := _tag_mask_at(col, row)
	info["tags_mask"] = mask
	var tag_labels := _tag_names_for_mask(mask)
	info["tag_labels"] = tag_labels
	var tags_text := "none"
	if not tag_labels.is_empty():
		tags_text = ", ".join(tag_labels)
	info["tags_text"] = tags_text
	var module_entry := _food_module_entry_at(col, row)
	var module_key := ""
	var module_weight := 0.0
	if not module_entry.is_empty():
		module_key = String(module_entry.get("module", ""))
		module_weight = float(module_entry.get("seasonal_weight", 0.0))
		var kind := String(module_entry.get("kind", ""))
		if kind != "":
			info["food_kind"] = kind
	info["food_module"] = module_key
	info["food_module_label"] = _food_module_label(module_key)
	info["food_module_weight"] = module_weight
	# Forage-patch cultivation/tended state (intensification ladder). Read by
	# Hud._tile_terrain_lines for the "Cultivation N%" / "🌾 Tended Patch" row.
	if forage_patch_lookup.has(tile_key):
		var patch: Dictionary = forage_patch_lookup[tile_key]
		info["cultivation_progress"] = float(patch.get("cultivation_progress", 0.0))
		info["is_cultivated"] = bool(patch.get("is_cultivated", false))
		info["patch_has_owner"] = bool(patch.get("has_owner", false))
		info["patch_owner"] = int(patch.get("owner", 0))
		info["patch_ecology_phase"] = String(patch.get("ecology_phase", ""))
		# Standing forage stock vs the patch's ceiling — "how much there is", the patch
		# counterpart to a herd's Biomass row (Hud._tile_terrain_lines renders both).
		info["patch_biomass"] = float(patch.get("biomass", 0.0))
		info["patch_carrying_capacity"] = float(patch.get("carrying_capacity", 0.0))
		# Pre-commit yield forecast (food/turn at the patch's current biomass, at
		# output_multiplier 1.0). Read by Hud._build_forage_assign_controls to show the live
		# "Expected yield" row and to cap the forager stepper at the patch's max-useful workers.
		info["patch_per_worker_yield"] = float(patch.get("per_worker_yield", 0.0))
		info["patch_ceiling_sustain"] = float(patch.get("ceiling_sustain", 0.0))
		info["patch_ceiling_surplus"] = float(patch.get("ceiling_surplus", 0.0))
		info["patch_ceiling_market"] = float(patch.get("ceiling_market", 0.0))
		info["patch_ceiling_eradicate"] = float(patch.get("ceiling_eradicate", 0.0))
		# The Cultivate investment rung: the dip yield while the patch is being prepared, and the
		# tended yield it pays afterwards. Hud._build_forage_assign_controls turns the pair into the
		# pre-commit "Preparing: +X → then +Y" forecast.
		info["patch_ceiling_cultivate"] = float(patch.get("ceiling_cultivate", 0.0))
		info["patch_tended_yield"] = float(patch.get("tended_yield", 0.0))
		# Plant RUNG 3 — the Field + the Sow verb (the twin of the herd's Corral block). The patch
		# carries TWO independent build meters: `cultivation_progress` (rung 2, above) and
		# `field_progress` here. Hud._tile_terrain_lines renders the meters; the Sow forecast pair
		# drives `_build_forage_assign_controls`' "Preparing: +X → then +Y", exactly as the Cultivate
		# pair does one rung down.
		info["patch_field_progress"] = float(patch.get("field_progress", 0.0))
		info["patch_is_field"] = bool(patch.get("is_field", false))
		info["patch_ceiling_sow"] = float(patch.get("ceiling_sow", 0.0))
		info["patch_field_yield"] = float(patch.get("field_yield", 0.0))
		# WHY this ground will not take seed ("" = it will). The client cannot re-derive this — it has
		# neither the per-biome capacity table nor the hydrology — so the sim ships the reason itself.
		info["patch_sow_site_refusal"] = String(patch.get("sow_site_refusal", ""))
		# WHAT GROWS HERE — the tile's named plant composition (share-descending, already sorted
		# server-side; never re-sorted here). Deliberately NOT in FOW_DISCOVERED_HIDDEN_KEYS: it is
		# a pure function of the BIOME, like the terrain label or the river edges, so a remembered
		# tile still knows what grows there. Never-seen tiles are covered by the `unexplored`
		# redaction, and nothing on the patch can change it.
		info["patch_composition"] = patch.get("composition", [])
		# THE COMMITTED CROP — "" while the patch is the wild mixed basket above, else the single
		# species this patch was committed to by Cultivate/Sow. Unlike the composition it IS patch
		# state (a band's doing), but it rides beside it because the tile card renders exactly one of
		# the two rows; the Forage line it sits under is already past the discovered early-return, so
		# a remembered tile never reports it and it needs no FOW_DISCOVERED_HIDDEN_KEYS entry.
		info["patch_committed_species"] = String(patch.get("committed_species", ""))
		info["patch_committed_display_name"] = String(patch.get("committed_display_name", ""))
	var units_here := _units_on_tile(col, row)
	var herds_here := _herds_on_tile(col, row)
	info["units"] = units_here
	info["herds"] = herds_here
	info["unit_count"] = units_here.size()
	info["herd_count"] = herds_here.size()
	var harvest_here: Variant = harvest_sites.get(Vector2i(col, row), null)
	if harvest_here is Array and not harvest_here.is_empty():
		var harvest_array: Array = []
		for entry in harvest_here:
			if entry is Dictionary:
				harvest_array.append((entry as Dictionary).duplicate(true))
		info["harvest_tasks"] = harvest_array
		info["harvest_active"] = harvest_array.size()
	var scout_here: Variant = scout_sites.get(Vector2i(col, row), null)
	if scout_here is Array and not scout_here.is_empty():
		var scout_array: Array = []
		for entry in scout_here:
			if entry is Dictionary:
				scout_array.append((entry as Dictionary).duplicate(true))
		info["scout_tasks"] = scout_array
		info["scout_active"] = scout_array.size()
	var nearest_unit := _nearest_unit_sample(col, row)
	if not nearest_unit.is_empty():
		info["nearest_unit_distance"] = nearest_unit.get("distance", -1)
		info["nearest_unit_label"] = nearest_unit.get("label", "")
		info["nearest_unit_id"] = nearest_unit.get("id", "")
	return info

## The bands standing on a hex — the single chokepoint for unit-by-coordinate lookups (Occupants
## roster, band-selection click, stack cycling), fog-gated by `_unit_hidden_by_fog`: a FOREIGN band on
## an unseen hex is neither listed nor selectable, while your OWN band is always both (it may well be
## standing on an Unexplored tile — see `_unit_hidden_by_fog`).
func _units_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
	for unit in units:
		if _unit_hidden_by_fog(unit):
			continue
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		if int(position[0]) == col and int(position[1]) == row:
			matches.append((unit as Dictionary).duplicate(true))
	return matches

## The herds standing on a hex — FOG-GATED through the SAME `_is_tile_visible` test the herd RENDERER
## uses (`_draw_herd`), so a herd you cannot see is neither listed nor targetable. This is the single
## chokepoint for herd-by-coordinate lookups: the Occupants roster, the herd-selection click, the
## hunt-target click resolution and the pre-launch trip forecast all read the herds through here (via
## `_tile_info_at` → `tile_info.herds`), so gating HERE makes "you can only hunt/forecast what you can
## actually see" true by construction. The server still exports every herd unfiltered (a wire-level
## leak, tracked separately), so this client gate is LOAD-BEARING, not cosmetic — do not bypass it by
## reading `herds` by coordinate somewhere else.
func _herds_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
	if not _is_tile_visible(col, row):
		return matches
	for herd in herds:
		var x := int(herd.get("x", -1))
		var y := int(herd.get("y", -1))
		if x == col and y == row:
			matches.append((herd as Dictionary).duplicate(true))
	return matches

func _nearest_unit_sample(col: int, row: int) -> Dictionary:
	if units.is_empty():
		return {}
	var best_distance: int = -1
	var best_unit: Dictionary = {}
	for entry in units:
		if not (entry is Dictionary):
			continue
		# Fog: never sample a foreign band the player can't see — the "nearest unit" readout would
		# otherwise leak its label AND its distance (a bearing on an invisible band).
		if _unit_hidden_by_fog(entry as Dictionary):
			continue
		var pos_array: Array = Array(entry.get("pos", []))
		if pos_array.size() != 2:
			continue
		var ux := int(pos_array[0])
		var uy := int(pos_array[1])
		var distance: int = abs(col - ux) + abs(row - uy)
		if distance < 0:
			continue
		if best_distance < 0 or distance < best_distance:
			best_distance = distance
			best_unit = entry
	if best_distance < 0 or best_unit.is_empty():
		return {}
	var summary := {
		"distance": best_distance,
		"label": String(best_unit.get("id", best_unit.get("entity", "Band"))),
		"id": best_unit.get("entity", best_unit.get("id", "")),
	}
	return summary

func _food_module_entry_at(col: int, row: int) -> Dictionary:
	var key := Vector2i(col, row)
	if food_site_lookup.has(key):
		return (food_site_lookup[key] as Dictionary).duplicate(true)
	return {}

func _food_harvest_active(col: int, row: int) -> bool:
	return harvest_sites.has(Vector2i(col, row))

func _selected_tile_matches_food(col: int, row: int, module_key: String) -> bool:
	if module_key == "":
		return false
	return selected_tile.x == col and selected_tile.y == row

func _tag_names_for_mask(mask: int) -> PackedStringArray:
	var names := PackedStringArray()
	if mask == 0:
		return names
	for raw_bit in TERRAIN_TAG_KEYS:
		var bit: int = int(raw_bit)
		if (mask & bit) == 0:
			continue
		var label_value: Variant = terrain_tag_labels.get(bit, "")
		var label := String(label_value)
		if label == "":
			label = _tag_label_for_mask(bit)
		names.append(label)
	return names

func _food_module_label(module_key: String) -> String:
	if module_key == "":
		return "None"
	return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

func _culture_layer_at(x: int, y: int) -> int:
	if culture_layer_grid.is_empty() or grid_width == 0:
		return -1
	var index: int = y * grid_width + x
	if index < 0 or index >= culture_layer_grid.size():
		return -1
	return int(culture_layer_grid[index])

func _is_culture_layer_highlighted(layer_id: int) -> bool:
	if highlighted_culture_layer_set.is_empty():
		return true
	return highlighted_culture_layer_set.has(layer_id)

func _elevation_color(value: float) -> Color:
	var t: float = clampf(value, 0.0, 1.0)
	if t <= 0.5:
		return ELEVATION_LOW_COLOR.lerp(ELEVATION_MID_COLOR, t * 2.0)
	return ELEVATION_MID_COLOR.lerp(ELEVATION_HIGH_COLOR, (t - 0.5) * 2.0)

func _desaturate_color(c: Color, factor: float) -> Color:
	# Convert to grayscale luminance and blend back
	var gray: float = c.r * 0.299 + c.g * 0.587 + c.b * 0.114
	return Color(
		lerpf(c.r, gray, factor),
		lerpf(c.g, gray, factor),
		lerpf(c.b, gray, factor),
		c.a
	)

func _tile_color(x: int, y: int) -> Color:
	if active_overlay_key == "":
		var terrain_id := _terrain_id_at(x, y)
		var base_color: Color = GRID_COLOR
		if terrain_id >= 0:
			base_color = _terrain_color_for_id(terrain_id)
		# Apply Fog of War modifiers if enabled
		# Visibility values: Active ≈ 1.0, Discovered ≈ 0.5, Unexplored ≈ 0.0
		if _fow_enabled:
			var vis: float = _visibility_value_at(x, y)
			if vis > FOW_VISIBLE_THRESHOLD:  # Active - full terrain color
				return base_color
			elif vis > 0.0:  # Explored but not active - show terrain with foggy overlay
				# Light mist effect that preserves terrain recognition
				return base_color.lerp(_fow_mist_color, _fow_mist_blend)
			else:  # Unexplored - dark fog
				return _fow_fog_fill_color
		return base_color
	if active_overlay_key == "terrain_tags":
		var mask := _tag_mask_at(x, y)
		if mask == 0:
			return GRID_COLOR
		var tag_color: Color = _tag_color_for_mask(mask)
		return GRID_COLOR.lerp(tag_color, 0.92)
	var overlay_value: float = _value_at_overlay(active_overlay_key, x, y)
	var overlay_color: Color = OVERLAY_COLORS.get(active_overlay_key, LOGISTICS_COLOR)
	if active_overlay_key == "culture" and not highlighted_culture_layer_set.is_empty():
		var layer_id: int = _culture_layer_at(x, y)
		if not _is_culture_layer_highlighted(layer_id):
			overlay_value *= 0.15
			var muted := GRID_COLOR.lerp(overlay_color, overlay_value)
			return muted.darkened(0.35)
		var highlighted := GRID_COLOR.lerp(overlay_color, overlay_value)
		return highlighted.lightened(0.12)
	if active_overlay_key == "elevation":
		var gradient_color: Color = _elevation_color(overlay_value)
		var blend: float = clampf(overlay_value * 0.85 + 0.15, 0.0, 1.0)
		return GRID_COLOR.lerp(gradient_color, blend)
	if active_overlay_key == PASTURE_OVERLAY_KEY:
		return _pasture_color(x, y, overlay_value)
	if active_overlay_key == FORAGE_OVERLAY_KEY:
		return _forage_color(x, y, overlay_value)
	return GRID_COLOR.lerp(overlay_color, overlay_value)

## Pasture overlay color for one tile. `normalized` is the tile's graze capacity as a fraction of the
## map's RICHEST pasture (the native decoder scales it against the max, not min-max — see
## `snapshot_dict`), so 1.0 is the best pasture on this map and 0.0 means NO pasture at all.
##
## Zero leaves the ramp: a tile that carries no pasture is a categorically different fact from a poor
## one, and painting both dark would let the overlay lie about exactly the thing it exists to show.
## Water is split out from dead land (Water terrain tag) because "the sea has no grass" is not a
## finding — burying it in the same tone as a glacier would drown the real dead ground.
func _pasture_color(x: int, y: int, normalized: float) -> Color:
	if normalized <= 0.0:
		if (_tag_mask_at(x, y) & PASTURE_WATER_TAG) != 0:
			return PASTURE_WATER_COLOR
		return PASTURE_DEAD_COLOR
	return _pasture_ramp_color(normalized)

## The pasture ramp itself: the HUE carries the capacity (straw → grass). The barren tones sit OFF
## this ramp entirely, so the map's poorest real pasture still reads unmistakably AS pasture without
## any floor fudge. Shared by the map paint and the legend swatches, so they cannot drift apart.
func _pasture_ramp_color(normalized_capacity: float) -> Color:
	return PASTURE_POOR_COLOR.lerp(PASTURE_RICH_COLOR, clampf(normalized_capacity, 0.0, 1.0))

## Forage overlay color for one tile. `normalized` is the tile's human-food capacity as a fraction
## of the map's RICHEST forage tile (the native decoder scales it against the max — see
## `snapshot_dict`), so 1.0 is the best human-food land on this map and 0.0 means genuinely none.
##
## Twin of `_pasture_color`, but WATER is not an off-category: a coastal shelf with fishing
## potential is a positive value and rides the ramp, so it lights up here where it is barren on the
## pasture map. Only genuinely-zero tiles (deep ocean, glacier, lava) leave the ramp for the single
## barren fill — the `x`/`y` are unused (no water/dead split), kept for the overlay-color signature.
func _forage_color(_x: int, _y: int, normalized: float) -> Color:
	if normalized <= 0.0:
		return FORAGE_BARREN_COLOR
	return _forage_ramp_color(normalized)

## The forage ramp: HUE carries the capacity (wheat → leaf green). Kept a distinct green from the
## pasture ramp so the two food webs read as different layers. Shared by the map paint and the
## legend swatches, so they cannot drift apart.
func _forage_ramp_color(normalized_capacity: float) -> Color:
	return FORAGE_POOR_COLOR.lerp(FORAGE_RICH_COLOR, clampf(normalized_capacity, 0.0, 1.0))

func _terrain_color_for_id(terrain_id: int) -> Color:
	var colors := _get_terrain_colors()
	if colors.has(terrain_id):
		return colors[terrain_id]
	return Color(0.2, 0.2, 0.2, 1.0)

func _update_biome_color_buffer() -> void:
	if grid_width <= 0 or grid_height <= 0 or terrain_overlay.is_empty():
		biome_color_buffer = PackedColorArray()
		return
	var total: int = grid_width * grid_height
	biome_color_buffer = PackedColorArray()
	biome_color_buffer.resize(total)
	for idx in range(total):
		var terrain_id := 0
		if idx < terrain_overlay.size():
			terrain_id = int(terrain_overlay[idx])
		biome_color_buffer[idx] = _terrain_color_for_id(terrain_id)

func _tag_mask_at(x: int, y: int) -> int:
	if terrain_tags_overlay.is_empty() or grid_width == 0:
		return 0
	var index: int = y * grid_width + x
	if index < 0 or index >= terrain_tags_overlay.size():
		return 0
	return int(terrain_tags_overlay[index])

func _tag_color_for_mask(mask: int) -> Color:
	var color := GRID_COLOR
	var applied := false
	for raw_bit in TERRAIN_TAG_KEYS:
		var bit: int = int(raw_bit)
		if (mask & bit) == 0:
			continue
		var tag_color: Color = TERRAIN_TAG_COLORS.get(bit, Color.WHITE)
		var weight: float = float(TERRAIN_TAG_BLEND_WEIGHTS.get(bit, 0.6))
		color = color.lerp(tag_color, weight)
		applied = true
	if not applied:
		return GRID_COLOR
	return color

func _tag_label_for_mask(mask: int) -> String:
	if terrain_tag_labels.has(mask):
		return str(terrain_tag_labels[mask])
	for key in terrain_tag_labels.keys():
		if int(key) == mask:
			return str(terrain_tag_labels[key])
	return "Tag %d" % mask

func _compare_tag_rows(a: Dictionary, b: Dictionary) -> bool:
	var a_count: int = int(a.get("count", 0))
	var b_count: int = int(b.get("count", 0))
	if a_count == b_count:
		return int(a.get("mask", 0)) < int(b.get("mask", 0))
	return a_count > b_count

func _tag_coverage_rows() -> Array:
	var rows: Array = []
	if terrain_tags_overlay.is_empty() or grid_width <= 0 or grid_height <= 0:
		return rows
	var total_tiles: int = grid_width * grid_height
	if total_tiles <= 0:
		return rows
	var counts: Dictionary = {}
	var limit: int = min(terrain_tags_overlay.size(), total_tiles)
	for idx in range(limit):
		var mask: int = int(terrain_tags_overlay[idx])
		if mask == 0:
			continue
		for raw_bit in TERRAIN_TAG_KEYS:
			var bit: int = int(raw_bit)
			if (mask & bit) != 0:
				counts[bit] = int(counts.get(bit, 0)) + 1
	for raw_bit in counts.keys():
		var bit_value: int = int(raw_bit)
		var count: int = int(counts[raw_bit])
		var percent: float = 0.0
		if total_tiles > 0:
			percent = (float(count) / float(total_tiles)) * 100.0
		rows.append({
			"mask": bit_value,
			"label": _tag_label_for_mask(bit_value),
			"count": count,
			"percent": percent,
		})
	rows.sort_custom(Callable(self, "_compare_tag_rows"))
	return rows

func _tag_overlay_stats() -> Dictionary:
	var rows: Array = _tag_coverage_rows()
	if rows.is_empty():
		return {"has_values": false}
	return {
		"has_values": true,
		"coverage": rows,
		"tile_total": grid_width * grid_height,
	}

func _build_tag_legend() -> Dictionary:
	var coverage: Array = _tag_coverage_rows()
	var coverage_lookup: Dictionary = {}
	for entry in coverage:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		coverage_lookup[int(entry.get("mask", 0))] = entry
	var rows: Array = []
	for raw_bit in TERRAIN_TAG_KEYS:
		var mask: int = int(raw_bit)
		var label: String = _tag_label_for_mask(mask)
		var entry: Dictionary = coverage_lookup.get(mask, {})
		var percent_val: float = float(entry.get("percent", 0.0))
		var count: int = int(entry.get("count", 0))
		var value_text := ""
		if percent_val > 0.0:
			value_text = "%.1f%%" % percent_val
		var display_label := "%s (%d)" % [label, count] if count > 0 else label
		rows.append({
			"color": TERRAIN_TAG_COLORS.get(mask, Color.WHITE),
			"label": display_label,
			"value_text": value_text,
		})
	return {
		"key": "terrain_tags",
		"title": "Terrain Tags",
		"description": "Tiles blend colors for all active environmental tags.",
		"rows": rows,
		"stats": {
			"tile_total": grid_width * grid_height,
		},
	}

func terrain_palette_entries() -> Array:
	var ids: Array = []
	if terrain_palette.size() > 0:
		ids = Array(terrain_palette.keys())
	else:
		ids = Array(_get_terrain_colors().keys())
	ids.sort()
	var labels := _get_terrain_labels()
	var entries: Array = []
	for raw_id in ids:
		var id := int(raw_id)
		var label := ""
		if terrain_palette.has(id):
			label = str(terrain_palette[id])
		if label == "":
			label = labels.get(id, "Unknown")
		var color := _terrain_color_for_id(id)
		entries.append({
			"id": id,
			"label": label,
			"color": color,
		})
	return entries

func present_terrain_ids() -> PackedInt32Array:
	## Distinct terrain ids actually present on the current map, sorted ascending,
	## computed from the per-tile ids `TerrainRenderer._cached_terrain_ids` caches in `display_snapshot`.
	## Empty before the first snapshot (no per-tile terrain cached yet) — callers
	## fall back to the full palette in that case.
	var seen: Dictionary = {}
	for raw_id in _terrain.cached_terrain_ids():
		seen[int(raw_id)] = true
	var ids: Array = seen.keys()
	ids.sort()
	return PackedInt32Array(ids)

func _emit_overlay_legend() -> void:
	emit_signal("overlay_legend_changed", _legend_for_current_view())

func refresh_overlay_legend() -> void:
	_emit_overlay_legend()

func overlay_stats_for_key(key: String) -> Dictionary:
	if key == "terrain_tags":
		return _tag_overlay_stats()
	if not overlay_channels.has(key):
		return {}
	if key == "culture" and not highlighted_culture_layer_set.is_empty():
		var selection := _culture_selection_data()
		if bool(selection.get("valid", false)):
			return selection.get("stats", {})
	var normalized: PackedFloat32Array = _overlay_array(key)
	var raw: PackedFloat32Array = _overlay_raw_array(key)
	return _overlay_stats(normalized, raw)

func _legend_for_current_view() -> Dictionary:
	if active_overlay_key == "":
		return _build_terrain_legend()
	if active_overlay_key == "terrain_tags":
		return _build_tag_legend()
	if not overlay_channels.has(active_overlay_key):
		return {}
	if active_overlay_key == PASTURE_OVERLAY_KEY:
		return _build_pasture_legend()
	if active_overlay_key == FORAGE_OVERLAY_KEY:
		return _build_forage_legend()
	if active_overlay_key == "culture" and not highlighted_culture_layer_set.is_empty():
		var selection := _culture_selection_data()
		if bool(selection.get("valid", false)):
			var normalized: PackedFloat32Array = selection.get("normalized", PackedFloat32Array())
			var raw: PackedFloat32Array = selection.get("raw", PackedFloat32Array())
			var stats: Dictionary = selection.get("stats", {})
			var tile_count: int = int(stats.get("tile_count", stats.get("raw_count", 0)))
			var context_label: String = highlighted_culture_context
			if context_label == "" and tile_count > 0:
				context_label = "Selection (%d tiles)" % tile_count
			return _build_scalar_overlay_legend("culture", normalized, raw, stats, context_label)
	return _build_scalar_overlay_legend(active_overlay_key)

## Legend for the PASTURE channel. It cannot use `_build_scalar_overlay_legend`, because that one
## reports min/avg/max over EVERY tile — and here the map-wide minimum is 0 (the sea), which would
## report the world's poorest pasture as "0" and say nothing about the ground that has none. So the
## rows are: the barren tones (off-ramp, counted), then Poor/Average/Rich measured over the tiles
## that ACTUALLY carry pasture. The map-wide standing stock (biomass ÷ capacity) rides in the
## description — the "how eaten-down is it?" question the capacity ramp deliberately does not answer.
func _build_pasture_legend() -> Dictionary:
	var raw: PackedFloat32Array = _overlay_raw_array(PASTURE_OVERLAY_KEY)
	var max_capacity: float = 0.0
	for value in raw:
		var capacity := float(value)
		if is_finite(capacity):
			max_capacity = maxf(max_capacity, capacity)

	var pasture_min: float = INF
	var pasture_max: float = 0.0
	var pasture_sum: float = 0.0
	var pasture_tiles: int = 0
	var biomass_sum: float = 0.0
	for entry in tile_graze.values():
		var patch: Dictionary = entry
		var capacity: float = float(patch.get("capacity", 0.0))
		if capacity <= 0.0:
			continue
		pasture_tiles += 1
		pasture_min = minf(pasture_min, capacity)
		pasture_max = maxf(pasture_max, capacity)
		pasture_sum += capacity
		biomass_sum += float(patch.get("biomass", 0.0))

	# Every land tile the map knows about, minus the ones carrying pasture = the DEAD ground. Water is
	# excluded (its emptiness is not a finding), and it is counted off the Water terrain tag, which is
	# server truth — the same test `_pasture_color` paints with, so the legend can't disagree with the map.
	var water_tiles: int = 0
	var land_tiles: int = 0
	for y in grid_height:
		for x in grid_width:
			if (_tag_mask_at(x, y) & PASTURE_WATER_TAG) != 0:
				water_tiles += 1
			else:
				land_tiles += 1
	var dead_tiles: int = maxi(land_tiles - pasture_tiles, 0)

	var description := "Graze capacity — the ANIMAL-edible stock (grass and browse; humans cannot digest it)."
	if pasture_sum > 0.0:
		description += "\nStanding stock %d%% of capacity across %d pasture tiles." % [
			int(round(biomass_sum / pasture_sum * 100.0)), pasture_tiles
		]

	var rows: Array = []
	if pasture_tiles == 0:
		rows.append({
			"color": PASTURE_DEAD_COLOR,
			"label": "No pasture anywhere",
			"value_text": "Awaiting graze telemetry",
		})
	else:
		var avg_capacity: float = pasture_sum / float(pasture_tiles)
		rows.append({
			"color": _pasture_color_for_capacity(pasture_min, max_capacity),
			"label": "Poorest pasture",
			"value_text": _format_pasture_capacity(pasture_min),
		})
		rows.append({
			"color": _pasture_color_for_capacity(avg_capacity, max_capacity),
			"label": "Average pasture",
			"value_text": _format_pasture_capacity(avg_capacity),
		})
		rows.append({
			"color": _pasture_color_for_capacity(pasture_max, max_capacity),
			"label": "Richest pasture",
			"value_text": _format_pasture_capacity(pasture_max),
		})
	# Kept SHORT: the legend panel clips a long row label, and "the ground here carries no pasture at
	# all" is the one row that must never be half-read.
	rows.append({
		"color": PASTURE_DEAD_COLOR,
		"label": "Barren ground",
		"value_text": "%d tiles" % dead_tiles,
	})
	rows.append({
		"color": PASTURE_WATER_COLOR,
		"label": "Water",
		"value_text": "%d tiles" % water_tiles,
	})
	return {
		"key": PASTURE_OVERLAY_KEY,
		"title": String(overlay_channel_labels.get(PASTURE_OVERLAY_KEY, "Pasture")),
		"description": description,
		"rows": rows,
		"stats": {
			"min": (0.0 if pasture_tiles == 0 else pasture_min),
			"max": pasture_max,
			"avg": (0.0 if pasture_tiles == 0 else pasture_sum / float(pasture_tiles)),
		},
	}

## The legend swatch for a given capacity: re-normalizes against the map's richest pasture exactly as
## the decoder does for the map, then paints through the SAME ramp (`_pasture_ramp_color`).
func _pasture_color_for_capacity(capacity: float, max_capacity: float) -> Color:
	if max_capacity <= 0.0:
		return PASTURE_DEAD_COLOR
	return _pasture_ramp_color(capacity / max_capacity)

func _format_pasture_capacity(capacity: float) -> String:
	return "%.0f graze" % capacity

## Legend for the FORAGE channel — the human-food twin of `_build_pasture_legend`. It cannot use
## `_build_scalar_overlay_legend` for the same reason pasture can't: the map-wide minimum is 0 (every
## barren tile — deep ocean/glacier/lava), which would report the world's poorest forage as "0". So
## the rows are Poorest/Average/Richest measured over the tiles that ACTUALLY carry human food, then
## the barren "No forage" count. The description carries the honest gathering-sites sub-count — the
## tiles you can actually work today, a subset of the potential — so the ramp reads as POTENTIAL
## without pretending the rest of the land is worthless.
func _build_forage_legend() -> Dictionary:
	var raw: PackedFloat32Array = _overlay_raw_array(FORAGE_OVERLAY_KEY)
	var max_capacity: float = 0.0
	for value in raw:
		var capacity := float(value)
		if is_finite(capacity):
			max_capacity = maxf(max_capacity, capacity)

	var forage_min: float = INF
	var forage_max: float = 0.0
	var forage_sum: float = 0.0
	var forage_tiles: int = 0
	for entry in tile_forage.values():
		var capacity: float = float(entry)
		if capacity <= 0.0:
			continue
		forage_tiles += 1
		forage_min = minf(forage_min, capacity)
		forage_max = maxf(forage_max, capacity)
		forage_sum += capacity

	# Every tile the map knows about, minus the ones carrying human food = the barren ground (deep
	# ocean, glacier, lava). Unlike pasture there is no water/land split here — coastal shelves carry
	# forage and ride the ramp, so "water" is not an off-category; only genuinely-zero tiles are.
	var total_tiles: int = maxi(grid_width, 0) * maxi(grid_height, 0)
	var barren_tiles: int = maxi(total_tiles - forage_tiles, 0)

	var description := "The HUMAN-edible potential of this land — seeds, nuts, tubers, fruit, and fish."
	# Gathering sites = the tiles you can actually forage today (a subset of the potential above).
	description += "\nGathering sites: %d tiles." % food_sites.size()

	var rows: Array = []
	if forage_tiles == 0:
		rows.append({
			"color": FORAGE_BARREN_COLOR,
			"label": "No forage anywhere",
			"value_text": "Awaiting forage telemetry",
		})
	else:
		var avg_capacity: float = forage_sum / float(forage_tiles)
		rows.append({
			"color": _forage_color_for_capacity(forage_min, max_capacity),
			"label": "Poorest forage",
			"value_text": _format_forage_capacity(forage_min),
		})
		rows.append({
			"color": _forage_color_for_capacity(avg_capacity, max_capacity),
			"label": "Average forage",
			"value_text": _format_forage_capacity(avg_capacity),
		})
		rows.append({
			"color": _forage_color_for_capacity(forage_max, max_capacity),
			"label": "Richest forage",
			"value_text": _format_forage_capacity(forage_max),
		})
	# Kept SHORT (the legend panel clips). Deep ocean, glacier and lava — the only ground that truly
	# yields no human food.
	rows.append({
		"color": FORAGE_BARREN_COLOR,
		"label": "No forage",
		"value_text": "%d tiles" % barren_tiles,
	})
	return {
		"key": FORAGE_OVERLAY_KEY,
		"title": String(overlay_channel_labels.get(FORAGE_OVERLAY_KEY, "Forage")),
		"description": description,
		"rows": rows,
		"stats": {
			"min": (0.0 if forage_tiles == 0 else forage_min),
			"max": forage_max,
			"avg": (0.0 if forage_tiles == 0 else forage_sum / float(forage_tiles)),
		},
	}

## Legend swatch for a given forage capacity: re-normalizes against the map's richest patch exactly
## as the decoder does for the map, then paints through the SAME ramp (`_forage_ramp_color`).
func _forage_color_for_capacity(capacity: float, max_capacity: float) -> Color:
	if max_capacity <= 0.0:
		return FORAGE_BARREN_COLOR
	return _forage_ramp_color(capacity / max_capacity)

func _format_forage_capacity(capacity: float) -> String:
	return "%.0f food" % capacity

func _build_terrain_legend() -> Dictionary:
	var present_ids: PackedInt32Array = present_terrain_ids()
	if present_ids.is_empty():
		# Pre-first-snapshot fallback: no per-tile terrain cached yet, so list the
		# full palette (as before) rather than render a blank legend.
		var fallback_rows: Array = []
		for entry in terrain_palette_entries():
			if typeof(entry) != TYPE_DICTIONARY:
				continue
			fallback_rows.append({
				"color": entry.get("color", Color.WHITE),
				"label": str(entry.get("label", "")),
				"value_text": "#%02d" % int(entry.get("id", 0)),
				# No per-tile counts pre-snapshot; carry 0 so the panel's count
				# sort has a numeric field (rows fall back to name order).
				"count": 0,
			})
		return {
			"key": "terrain",
			"title": "Terrain Types",
			"description": "Biome palette applied directly to tiles.",
			"rows": fallback_rows,
			"stats": {},
		}
	# Count tiles per present biome in a single pass over the cached terrain ids.
	var counts: Dictionary = {}
	for raw_id in _terrain.cached_terrain_ids():
		var counted_id := int(raw_id)
		counts[counted_id] = int(counts.get(counted_id, 0)) + 1
	var labels := _get_terrain_labels()
	var rows: Array = []
	for id in present_ids:
		var label := ""
		if terrain_palette.has(id):
			label = str(terrain_palette[id])
		if label == "":
			label = labels.get(id, "Unknown")
		var tile_count := int(counts.get(id, 0))
		rows.append({
			"color": _terrain_color_for_id(id),
			"label": label,
			"value_text": "%d tiles" % tile_count,
			# Numeric tile count so the legend panel can sort by count without
			# parsing value_text.
			"count": tile_count,
		})
	return {
		"key": "terrain",
		"title": "Terrain Types",
		"description": "Biomes present on this map (%d)." % present_ids.size(),
		"rows": rows,
		"stats": {},
	}

func _build_scalar_overlay_legend(
		key: String,
		normalized_override: Variant = null,
		raw_override: Variant = null,
		stats_override: Dictionary = {},
		context_label: String = ""
	) -> Dictionary:
	var normalized: PackedFloat32Array
	if normalized_override != null and normalized_override is PackedFloat32Array:
		normalized = normalized_override
	else:
		normalized = _overlay_array(key)
	var raw: PackedFloat32Array
	if raw_override != null and raw_override is PackedFloat32Array:
		raw = raw_override
	else:
		raw = _overlay_raw_array(key)
	var stats: Dictionary = stats_override
	if stats_override.is_empty():
		stats = _overlay_stats(normalized, raw)
	var overlay_color: Color = OVERLAY_COLORS.get(key, LOGISTICS_COLOR)
	var label: String = String(overlay_channel_labels.get(key, key.capitalize()))
	var description: String = String(overlay_channel_descriptions.get(key, ""))
	var placeholder: bool = bool(overlay_placeholder_flags.get(key, false))
	var rows: Array = []
	if context_label != "":
		if description != "":
			description = "%s\n%s" % [description, context_label]
		else:
			description = context_label
	var has_values: bool = bool(stats.get("has_values", false))
	var raw_range: float = float(stats.get("raw_range", 0.0))

	if placeholder and not has_values:
		rows.append({
			"color": GRID_COLOR,
			"label": "No data",
			"value_text": "Channel awaiting telemetry",
		})
	elif key == "crisis" and not has_values:
		rows.append({
			"color": GRID_COLOR,
			"label": "No active crises",
			"value_text": "Awaiting crisis incidents",
		})
	elif not has_values:
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, 0.2),
			"label": "No variation",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
	elif raw_range <= 0.0001:
		var tint: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, tint),
			"label": "Uniform",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
	else:
		var low_t: float = clamp(float(stats.get("normalized_min", 0.0)), 0.0, 1.0)
		var mid_t: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
		var high_t: float = clamp(float(stats.get("normalized_max", 0.0)), 0.0, 1.0)
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, low_t),
			"label": "Low",
			"value_text": _format_legend_value(float(stats.get("raw_min", 0.0))),
		})
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, mid_t),
			"label": "Average",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, high_t),
			"label": "High",
			"value_text": _format_legend_value(float(stats.get("raw_max", 0.0))),
		})

	return {
		"key": key,
		"title": label,
		"description": description,
		"rows": rows,
		"stats": {
			"min": stats.get("raw_min", 0.0),
			"max": stats.get("raw_max", 0.0),
			"avg": stats.get("raw_avg", 0.0),
		},
		"placeholder": placeholder,
	}

func _overlay_stats(normalized: PackedFloat32Array, raw: PackedFloat32Array) -> Dictionary:
	var n_min: float = INF
	var n_max: float = -INF
	var n_sum: float = 0.0
	var n_count: int = 0
	for value in normalized:
		var v: float = float(value)
		if not is_finite(v):
			continue
		n_min = min(n_min, v)
		n_max = max(n_max, v)
		n_sum += v
		n_count += 1
	if n_count == 0:
		n_min = 0.0
		n_max = 0.0

	var r_min: float = INF
	var r_max: float = -INF
	var r_sum: float = 0.0
	var r_count: int = 0
	for value in raw:
		var rv: float = float(value)
		if not is_finite(rv):
			continue
		r_min = min(r_min, rv)
		r_max = max(r_max, rv)
		r_sum += rv
		r_count += 1
	if r_count == 0:
		r_min = 0.0
		r_max = 0.0

	var has_values: bool = n_count > 0 and r_count > 0
	var raw_avg: float = 0.0
	if r_count > 0:
		raw_avg = r_sum / float(r_count)
	var normalized_avg: float = 0.0
	if n_count > 0:
		normalized_avg = n_sum / float(n_count)

	return {
		"normalized_min": n_min,
		"normalized_max": n_max,
		"normalized_avg": normalized_avg,
		"raw_min": r_min,
		"raw_max": r_max,
		"raw_avg": raw_avg,
		"raw_range": r_max - r_min,
		"has_values": has_values,
		"normalized_count": n_count,
		"raw_count": r_count,
	}

func _culture_selection_data() -> Dictionary:
	if highlighted_culture_layer_set.is_empty():
		return {"valid": false}
	if culture_layer_grid.is_empty():
		return {"valid": false}
	var normalized_src: PackedFloat32Array = _overlay_array("culture")
	if normalized_src.is_empty():
		return {"valid": false}
	var raw_src: PackedFloat32Array = _overlay_raw_array("culture")
	var limit: int = min(normalized_src.size(), culture_layer_grid.size())
	if limit <= 0:
		return {"valid": false}
	var selected_norm: Array = []
	var selected_raw: Array = []
	for idx in range(limit):
		var layer_id: int = int(culture_layer_grid[idx])
		if not highlighted_culture_layer_set.has(layer_id):
			continue
		selected_norm.append(normalized_src[idx])
		if raw_src.size() > idx:
			selected_raw.append(raw_src[idx])
		else:
			selected_raw.append(normalized_src[idx])
	if selected_norm.is_empty():
		return {"valid": false}
	var norm_packed := PackedFloat32Array(selected_norm)
	var raw_packed := PackedFloat32Array(selected_raw)
	var stats := _overlay_stats(norm_packed, raw_packed)
	stats["tile_count"] = selected_norm.size()
	return {
		"valid": true,
		"normalized": norm_packed,
		"raw": raw_packed,
		"stats": stats,
	}

func _install_province_overlay() -> void:
	if overlay_channels.has("province"):
		return
	if grid_width <= 0 or grid_height <= 0:
		return
	if culture_layer_map.is_empty() or culture_layer_grid.is_empty():
		return
	var province_raw := PackedFloat32Array()
	var total: int = grid_width * grid_height
	province_raw.resize(total)
	province_raw.fill(-1.0)
	var regional_owner: Dictionary = {}
	for layer_dict in culture_layer_map.values():
		if not (layer_dict is Dictionary):
			continue
		var scope := String(layer_dict.get("scope", ""))
		if scope == "Regional":
			var id: int = int(layer_dict.get("id", -1))
			var owner: int = int(layer_dict.get("owner", -1))
			if id >= 0:
				regional_owner[id] = owner
	if regional_owner.is_empty():
		return
	var layer_to_province: Dictionary = {}
	for idx in range(total):
		var layer_id: int = int(culture_layer_grid[idx])
		if layer_id < 0:
			continue
		if layer_to_province.has(layer_id):
			province_raw[idx] = float(layer_to_province[layer_id])
			continue
		var province_id: int = _resolve_province_for_layer(layer_id, regional_owner)
		layer_to_province[layer_id] = province_id
		province_raw[idx] = float(province_id)
	var province_seq: Dictionary = {}
	var seq: int = 0
	for value in province_raw:
		var pid := int(value)
		if pid < 0:
			continue
		if province_seq.has(pid):
			continue
		province_seq[pid] = seq
		seq += 1
	var province_norm := PackedFloat32Array()
	province_norm.resize(total)
	var denom: float = max(float(seq - 1), 1.0)
	for i in range(total):
		var pid := int(province_raw[i])
		if pid < 0 or seq <= 0:
			province_norm[i] = 0.0
		elif seq == 1:
			province_norm[i] = 0.5
		else:
			var idx_val: int = int(province_seq.get(pid, 0))
			province_norm[i] = float(idx_val) / denom
	_add_overlay_channel(
		"province",
		province_norm,
		province_raw,
		"Provinces",
		"Province/territory partitions"
	)

func _resolve_province_for_layer(layer_id: int, regional_owner: Dictionary) -> int:
	var guard := 0
	var current := layer_id
	while current > 0 and guard < 32:
		if regional_owner.has(current):
			return int(regional_owner[current])
		if not culture_layer_map.has(current):
			break
		var layer: Dictionary = culture_layer_map[current]
		current = int(layer.get("parent", -1))
		guard += 1
	return -1

func _add_overlay_channel(key: String, normalized: PackedFloat32Array, raw: PackedFloat32Array, label: String, description: String = "") -> void:
	overlay_channels[key] = normalized
	overlay_raw_channels[key] = raw
	overlay_channel_labels[key] = label
	overlay_channel_descriptions[key] = description
	overlay_placeholder_flags[key] = false
	if overlay_channel_order.find(key) == -1:
		overlay_channel_order.append(key)

func _ensure_default_overlay_channel() -> void:
	if grid_width <= 0 or grid_height <= 0:
		return
	var total: int = grid_width * grid_height
	var zeros := PackedFloat32Array()
	zeros.resize(total)
	zeros.fill(0.0)
	_add_overlay_channel("", zeros, zeros, "No Overlay", "Base map without overlays")

func _format_legend_value(value: float) -> String:
	return "%0.3f" % value

func set_terrain_mode(_enabled: bool) -> void:
	set_overlay_channel("")

## Debug toggle (Map tab): tint the shader's river bands hard so they pop against the terrain.
## Pushed to the blend shader as `river_highlight` on the next TerrainRenderer.update_shader_quad.
func set_highlight_rivers(enabled: bool) -> void:
	highlight_rivers = enabled
	queue_redraw()

func toggle_terrain_mode() -> void:
	set_overlay_channel("")

## Terrain-texture seams for callers outside MapView (the Inspector / HUD). Thin pass-throughs to
## TerrainRenderer, which owns the toggle — same shape as the MinimapController seams.
func get_terrain_textures_enabled() -> bool:
	return _terrain.get_terrain_textures_enabled()

func enable_terrain_textures(enabled: bool) -> void:
	_terrain.enable_terrain_textures(enabled)

func _average(data: PackedFloat32Array) -> float:
	if data.is_empty():
		return 0.0
	var total: float = 0.0
	for value in data:
		total += float(value)
	return total / data.size()

func _hex_center(col: int, row: int, radius: float, origin: Vector2) -> Vector2:
	var axial := _offset_to_axial(col, row)
	return origin + _axial_center(axial.x, axial.y, radius)

func _hex_center_wrapped(col: int, row: int, radius: float, origin: Vector2) -> Vector2:
	## Like _hex_center but wraps column to nearest visible position when horizontal wrapping enabled.
	## Use for individual markers (food sites, units). Do NOT use for connected lines (rivers, routes).
	var effective_col: int = col
	if _wrap_horizontal and grid_width > 0:
		# Find the viewport center in hex column space
		var viewport_size: Vector2 = _get_adjusted_viewport_size()
		var center_world_x: float = viewport_size.x * 0.5 - origin.x
		var col_width: float = SQRT3 * radius
		var center_col: float = center_world_x / col_width

		# Wrap col to be within grid_width/2 of center_col
		var offset: int = int(round((center_col - float(col)) / float(grid_width)))
		effective_col = col + offset * grid_width

	var axial := _offset_to_axial(effective_col, row)
	return origin + _axial_center(axial.x, axial.y, radius)

func _axial_center(q: int, r: int, radius: float) -> Vector2:
	var fq := float(q)
	var fr := float(r)
	var x: float = radius * (SQRT3 * fq + SQRT3 * 0.5 * fr)
	var y: float = radius * (1.5 * fr)
	return Vector2(x, y)

func _offset_to_axial(col: int, row: int) -> Vector2i:
	# odd-r horizontal layout (flat-top hexes)
	var q := col - ((row - (row & 1)) >> 1)
	var r := row
	return Vector2i(q, r)

func _axial_to_offset(q: int, r: int) -> Vector2i:
	var col: int = q + ((r - (r & 1)) >> 1)
	return Vector2i(col, r)

func _hex_points(center: Vector2, radius: float, closed: bool = false) -> PackedVector2Array:
	# Use cached offsets if available (avoids trig per hex)
	if radius == _cached_hex_radius and not _cached_hex_offsets.is_empty():
		var points := PackedVector2Array()
		points.resize(7 if closed else 6)
		for i in range(6):
			points[i] = center + _cached_hex_offsets[i]
		if closed:
			points[6] = points[0]
		return points

	# Fallback to computing (used when radius changes)
	var points := PackedVector2Array()
	for i in range(6):
		var angle := deg_to_rad(60.0 * float(i) + 30.0)
		points.append(center + Vector2(radius * cos(angle), radius * sin(angle)))
	if closed:
		points.append(points[0])
	return points


func _update_hex_offset_cache(radius: float) -> void:
	## Pre-compute hex corner offsets for the given radius (eliminates per-hex trig)
	if radius == _cached_hex_radius:
		return
	_cached_hex_offsets.resize(6)
	for i in range(6):
		var angle := deg_to_rad(60.0 * float(i) + 30.0)
		_cached_hex_offsets[i] = Vector2(radius * cos(angle), radius * sin(angle))
	_cached_hex_radius = radius

func _get_adjusted_viewport_size() -> Vector2:
	var viewport_size: Vector2 = get_viewport_rect().size
	var canvas_scale := get_viewport().get_canvas_transform().get_scale()
	if canvas_scale.x != 0.0 and canvas_scale.y != 0.0:
		# Account for global canvas (camera) scaling so hit-testing matches the drawn map
		viewport_size /= canvas_scale
	# Exclude every reserved edge strip: the map treats the remaining rect as its
	# entire viewport, and the node is translated by the leading insets (see
	# set_reserved_inset), so nothing renders behind a docked panel.
	viewport_size.x = max(viewport_size.x - _inset_left - _inset_right, 1.0)
	viewport_size.y = max(viewport_size.y - _inset_top - _inset_bottom, 1.0)
	return viewport_size

func _update_layout_metrics() -> void:
	if grid_width <= 0 or grid_height <= 0:
		return
	var viewport_size: Vector2 = _get_adjusted_viewport_size()
	if viewport_size.x <= 0.0 or viewport_size.y <= 0.0:
		return
	if bounds_dirty:
		base_bounds = _compute_bounds(1.0)
		bounds_dirty = false
	if base_bounds.size.x <= 0.0 or base_bounds.size.y <= 0.0:
		return
	var radius_from_width: float = viewport_size.x / base_bounds.size.x
	var radius_from_height: float = viewport_size.y / base_bounds.size.y
	base_hex_radius = max(radius_from_width, radius_from_height)
	last_hex_radius = clamp(base_hex_radius * zoom_factor, base_hex_radius * MIN_ZOOM_FACTOR, base_hex_radius * MAX_ZOOM_FACTOR)
	var scaled_bounds := Rect2(base_bounds.position * last_hex_radius, base_bounds.size * last_hex_radius)
	last_map_size = scaled_bounds.size
	last_base_origin = (viewport_size - last_map_size) * 0.5 - scaled_bounds.position
	last_origin = last_base_origin + pan_offset

func _clamp_pan_offset() -> void:
	if last_map_size.x <= 0.0 or last_map_size.y <= 0.0:
		return
	var viewport_size: Vector2 = _get_adjusted_viewport_size()

	# When horizontal wrapping is enabled, X pans infinitely (wraps around)
	if _wrap_horizontal:
		# Wrap pan_offset.x to stay within one map width for numerical stability
		# This doesn't affect rendering but keeps the value reasonable
		pan_offset.x = fposmod(pan_offset.x + last_map_size.x * 0.5, last_map_size.x) - last_map_size.x * 0.5

		# Y axis still clamps normally (poles are boundaries)
		var delta_y: float = viewport_size.y - last_map_size.y
		if delta_y <= 0.0:
			var max_pan_y: float = -delta_y / 2.0
			var min_pan_y: float = delta_y / 2.0
			pan_offset.y = clamp(pan_offset.y, min_pan_y, max_pan_y)
		else:
			pan_offset.y = 0.0
		return

	# Non-wrapping mode: use FoW bounds if enabled
	var effective_size: Vector2
	var bounds_offset: Vector2 = Vector2.ZERO  # Offset of explored region from map center

	if _fow_enabled and _explored_bounds_world.size.x > 0:
		# _explored_bounds_world is stored at unit radius - scale to current zoom
		var scaled_explored_size := _explored_bounds_world.size * last_hex_radius
		var scaled_explored_position := _explored_bounds_world.position * last_hex_radius
		effective_size = scaled_explored_size
		# Calculate offset: how much to shift pan center from full map center to explored center
		# A positive bounds_offset shifts the allowed pan range in that direction
		# base_bounds is at unit radius - scale to current zoom
		var full_map_position := base_bounds.position * last_hex_radius
		var full_map_center := full_map_position + last_map_size * 0.5
		var explored_center := scaled_explored_position + scaled_explored_size * 0.5
		# To center on explored region: pan needs to shift hexes so explored_center is at screen center
		# Since explored is upper-left of full map, we need positive pan to bring it into view
		bounds_offset = full_map_center - explored_center
	else:
		effective_size = last_map_size

	# Calculate pan limits based on keeping viewport within effective bounds
	var delta_x: float = viewport_size.x - effective_size.x
	var delta_y: float = viewport_size.y - effective_size.y

	# For X axis:
	if delta_x <= 0.0:
		# Effective area is wider than viewport - allow panning within bounds
		var max_pan_x: float = -delta_x / 2.0 + bounds_offset.x
		var min_pan_x: float = delta_x / 2.0 + bounds_offset.x
		pan_offset.x = clamp(pan_offset.x, min_pan_x, max_pan_x)
	else:
		# Effective area is narrower - center on it
		pan_offset.x = bounds_offset.x

	# For Y axis:
	if delta_y <= 0.0:
		# Effective area is taller than viewport - allow panning within bounds
		var max_pan_y: float = -delta_y / 2.0 + bounds_offset.y
		var min_pan_y: float = delta_y / 2.0 + bounds_offset.y
		pan_offset.y = clamp(pan_offset.y, min_pan_y, max_pan_y)
	else:
		# Effective area is shorter - center on it
		pan_offset.y = bounds_offset.y

func get_world_center() -> Vector2:
	return last_origin + last_map_size * 0.5

func get_hex_radius() -> float:
	return last_hex_radius

func _compute_bounds(radius: float) -> Rect2:
	var min_x := INF
	var max_x := -INF
	var min_y := INF
	var max_y := -INF
	for col in range(grid_width):
		for row in range(grid_height):
			var axial := _offset_to_axial(col, row)
			var center := _axial_center(axial.x, axial.y, radius)
			min_x = min(min_x, center.x - radius)
			max_x = max(max_x, center.x + radius)
			min_y = min(min_y, center.y - radius)
			max_y = max(max_y, center.y + radius)
	if min_x == INF:
		return Rect2(Vector2.ZERO, Vector2.ONE)
	return Rect2(Vector2(min_x, min_y), Vector2(max_x - min_x, max_y - min_y))

func _point_to_offset(point: Vector2) -> Vector2i:
	if grid_width <= 0 or grid_height <= 0:
		return Vector2i(-1, -1)
	var radius: float = max(last_hex_radius, 0.0001)
	var relative: Vector2 = (point - last_origin) / radius
	var qf: float = (SQRT3 / 3.0) * relative.x - (1.0 / 3.0) * relative.y
	var rf: float = (2.0 / 3.0) * relative.y
	var axial: Vector2i = _cube_round(qf, rf)
	var offset := _axial_to_offset(axial.x, axial.y)
	if _wrap_horizontal:
		offset.x = posmod(offset.x, grid_width)
	return offset

func _cube_round(qf: float, rf: float) -> Vector2i:
	var sf: float = -qf - rf
	var rq: float = round(qf)
	var rr: float = round(rf)
	var rs: float = round(sf)

	var q_diff: float = abs(rq - qf)
	var r_diff: float = abs(rr - rf)
	var s_diff: float = abs(rs - sf)

	if q_diff > r_diff and q_diff > s_diff:
		rq = -rr - rs
	elif r_diff > s_diff:
		rr = -rq - rs
	else:
		rs = -rq - rr

	return Vector2i(int(rq), int(rr))

func _process(delta: float) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if mouse_pan_active and mouse_pan_button != -1 and not Input.is_mouse_button_pressed(mouse_pan_button):
		mouse_pan_active = false
		mouse_pan_button = -1
	var pan_input := Vector2(
		Input.get_action_strength("map_pan_right") - Input.get_action_strength("map_pan_left"),
		Input.get_action_strength("map_pan_down") - Input.get_action_strength("map_pan_up")
	)
	if pan_input != Vector2.ZERO:
		if pan_input.length_squared() > 1.0:
			pan_input = pan_input.normalized()
		_apply_pan(pan_input * KEYBOARD_PAN_SPEED * delta)
	var zoom_direction: float = Input.get_action_strength("map_zoom_in") - Input.get_action_strength("map_zoom_out")
	if not is_zero_approx(zoom_direction):
		var viewport_center: Vector2 = get_viewport_rect().size * 0.5
		_apply_zoom(zoom_direction * KEYBOARD_ZOOM_SPEED * delta, viewport_center)
	# Animate the targeting overlay (pulsing glow / reticle) while a command is
	# being targeted.
	if _targeting.get("active", false):
		_targeting_time += delta
		queue_redraw()
	# Animate the awaiting-orders pulse on any expedition idle at its objective.
	if _has_awaiting_expedition:
		_expedition_time += delta
		queue_redraw()

## Mirror the HUD's pending command-targeting state so the map can draw the
## reticle / valid-target glow / hover ETA. Pass {} to clear.
func set_targeting(info: Dictionary) -> void:
	_targeting = info if info is Dictionary else {}
	if not bool(_targeting.get("active", false)):
		_targeting_time = 0.0
	queue_redraw()

func _draw_targeting(radius: float, origin: Vector2) -> void:
	if not bool(_targeting.get("active", false)):
		return
	var need := String(_targeting.get("need", ""))
	var pulse: float = 0.5 + 0.5 * sin(_targeting_time * 3.2)
	var cyan := HudStyle.SIGNAL
	if need == "band":
		# Only the player's own bands can fulfill a harvest/hunt, so only they get
		# the valid-target glow / ETA — not other factions' visible units.
		for unit in units:
			if not _is_player_unit(unit):
				continue
			var pos: Array = Array(unit.get("pos", []))
			if pos.size() != 2:
				continue
			var center: Vector2 = _hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)
			var ring_radius: float = radius * (0.62 + 0.10 * pulse)
			var ring_color := Color(cyan.r, cyan.g, cyan.b, 0.5 + 0.35 * pulse)
			draw_arc(center, ring_radius, 0, TAU, 32, ring_color, 2.5)
		if _hovered_tile.x >= 0 and _hovered_tile.y >= 0:
			for unit in units:
				if not _is_player_unit(unit):
					continue
				var hpos: Array = Array(unit.get("pos", []))
				if hpos.size() == 2 and int(hpos[0]) == _hovered_tile.x and int(hpos[1]) == _hovered_tile.y:
					_draw_targeting_hover_label(unit, radius, origin)
					break
	elif need == "herd":
		# Quarry targeting: glow the herds that are valid targets + reticle the hovered hex, so it
		# reads "click on a herd".
		# `min_distance` is the outfitting band's `hunt_reach`, and this test is the RENDER-SIDE
		# MIRROR of `Hud._is_expedition_quarry` — a herd within reach is a LOCAL hunt, not a party's
		# job, and `Hud._try_pick_quarry` refuses it. The halo must never promise a target the pick
		# will refuse, nor hide one it would accept, so the two tests must be changed together.
		# Absent (every other targeting mode omits the key) it defaults to 0 and admits everything.
		var min_distance := int(_targeting.get("min_distance", 0))
		for herd in herds:
			if not bool(herd.get("huntable", false)):
				continue
			var hx := int(herd.get("x", -1))
			var hy := int(herd.get("y", -1))
			# Fog-gated like the herd marker itself: glowing a herd you can't see would BE the leak
			# (it would draw a "valid target here" halo onto an empty-looking fogged hex).
			if hx < 0 or hy < 0 or not _is_tile_visible(hx, hy):
				continue
			# An UNKNOWN distance (`-1`, origin missing) skips too — `_is_expedition_quarry` also
			# refuses one, so the mirror holds at the degenerate end as well.
			if _targeting_distance(hx, hy) <= min_distance:
				continue
			var hcenter: Vector2 = _hex_center_wrapped(hx, hy, radius, origin)
			var hring_radius: float = radius * (0.55 + 0.10 * pulse)
			var hring_color := Color(cyan.r, cyan.g, cyan.b, 0.5 + 0.35 * pulse)
			draw_arc(hcenter, hring_radius, 0, TAU, 32, hring_color, 2.5)
		if _hovered_tile.x >= 0 and _hovered_tile.y >= 0:
			var herd_reticle: Vector2 = _hex_center_wrapped(_hovered_tile.x, _hovered_tile.y, radius, origin)
			_draw_reticle(herd_reticle, radius * 0.82, cyan, pulse)
	elif need == "tile":
		if _hovered_tile.x >= 0 and _hovered_tile.y >= 0:
			var reticle_center: Vector2 = _hex_center_wrapped(_hovered_tile.x, _hovered_tile.y, radius, origin)
			_draw_reticle(reticle_center, radius * 0.82, cyan, pulse)

func _is_player_unit(unit: Dictionary) -> bool:
	return int(unit.get("faction", PLAYER_FACTION_ID)) == PLAYER_FACTION_ID

## THE unit fog rule — one definition, used by every unit draw/lookup/hit-test:
##     hidden == tile not currently visible AND the unit is not ours.
##
## YOUR OWN UNITS ARE ALWAYS SHOWN, including on an Unexplored hex. That exception is load-bearing,
## not a courtesy: the sim deliberately excludes expeditions from fog reveal (`calculate_visibility`
## runs `Without<Expedition>` — discovery is comm-range gated), so a scouting party ROUTINELY stands on
## an Unexplored tile. A plain visibility gate would erase your own expedition from the map at exactly
## the moment you are using it. A unit with no position can't be fog-tested, so it stays visible.
func _unit_hidden_by_fog(unit: Dictionary) -> bool:
	if _is_player_unit(unit):
		return false
	var pos: Array = Array(unit.get("pos", []))
	if pos.size() != 2:
		return false
	return not _is_tile_visible(int(pos[0]), int(pos[1]))

func _draw_reticle(center: Vector2, r: float, color: Color, pulse: float) -> void:
	var a := Color(color.r, color.g, color.b, 0.7 + 0.3 * pulse)
	draw_arc(center, r, 0, TAU, 40, a, 2.0)
	var g: float = r * 0.5
	draw_line(center + Vector2(-r, 0), center + Vector2(-g, 0), a, 2.0)
	draw_line(center + Vector2(g, 0), center + Vector2(r, 0), a, 2.0)
	draw_line(center + Vector2(0, -r), center + Vector2(0, -g), a, 2.0)
	draw_line(center + Vector2(0, g), center + Vector2(0, r), a, 2.0)

func _draw_targeting_hover_label(unit: Dictionary, radius: float, origin: Vector2) -> void:
	var pos: Array = Array(unit.get("pos", []))
	if pos.size() != 2:
		return
	var center: Vector2 = _hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)
	var text: String = str(unit.get("id", "Band"))
	var dist := _targeting_distance(int(pos[0]), int(pos[1]))
	if dist >= 0:
		text += " · %d tiles" % dist
	var font: Font = ThemeDB.fallback_font
	if font == null:
		return
	var font_size := 13
	var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var pad := Vector2(8, 5)
	var box_pos: Vector2 = center + Vector2(radius * 0.7, -radius * 0.7 - text_size.y - pad.y * 2)
	box_pos.x = clampf(box_pos.x, 4.0, _get_adjusted_viewport_size().x - text_size.x - pad.x * 2 - 4.0)
	box_pos.y = maxf(box_pos.y, 4.0)
	var rect := Rect2(box_pos, text_size + pad * 2)
	draw_rect(rect, Color(0.03, 0.055, 0.06, 0.95))
	draw_rect(rect, HudStyle.SIGNAL, false, 1.0)
	draw_string(font, box_pos + Vector2(pad.x, pad.y + text_size.y * 0.8), text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color(0.87, 0.98, 0.96))

## True odd-r hex distance between two offset (col,row) tiles, mirroring the sim's
## `hex_distance_wrapped` (offset→axial via _offset_to_axial, then cube distance). Callers
## must first bring both tiles into a common column frame (e.g. via _wrapped_col_delta /
## _band_effective_col) so the seam is handled before this row-parity-sensitive conversion.
func _hex_distance(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
	var a := _offset_to_axial(a_col, a_row)
	var b := _offset_to_axial(b_col, b_row)
	var dq: int = a.x - b.x
	var dr: int = a.y - b.y
	return int((abs(dq) + abs(dr) + abs(dq + dr)) / 2)

## Wrap-aware hex distance from the targeting ORIGIN to (col,row), the render-side mirror of
## Hud._hex_distance_wrapped (which Hud._is_expedition_quarry — the authoritative quarry pick —
## routes through). Bring the target into the origin's column frame via _wrapped_col_delta BEFORE
## the row-parity-sensitive offset→axial conversion (the same pre-wrap the work-range rings use), so
## a herd across the horizontal wrap seam measures the SHORT way round. Without this the herd-glow
## filter could halo a herd the pick refuses (or hide one it accepts) near the seam. Returns -1 when
## the origin (or the target) is unknown, matching the Hud helper.
func _targeting_distance(col: int, row: int) -> int:
	var ox := int(_targeting.get("origin_x", -1))
	var oy := int(_targeting.get("origin_y", -1))
	if ox < 0 or oy < 0 or col < 0 or row < 0:
		return -1
	var eff_col := ox + _wrapped_col_delta(ox, col)
	return _hex_distance(ox, oy, eff_col, row)

func _apply_pan(delta: Vector2) -> void:
	if delta == Vector2.ZERO:
		return
	var pan_x_before := pan_offset.x
	pan_offset += delta
	_update_layout_metrics()
	_clamp_pan_offset()

	# Detect horizontal wrap: if actual change differs significantly from delta,
	# the fposmod wrap occurred and we need to invalidate the cache
	if _wrap_horizontal and last_map_size.x > 0:
		var actual_x_change: float = pan_offset.x - pan_x_before
		var wrap_occurred: bool = abs(actual_x_change - delta.x) > last_map_size.x * 0.5
		if wrap_occurred:
			_invalidate_map_cache()

	queue_redraw()
	_minimap.queue_indicator_redraw()

func _apply_zoom(delta_zoom: float, pivot: Vector2) -> void:
	if is_zero_approx(delta_zoom):
		return
	_update_layout_metrics()
	var previous_zoom: float = zoom_factor
	var previous_radius: float = max(last_hex_radius, 0.0001)
	var previous_origin: Vector2 = last_origin
	zoom_factor = clamp(zoom_factor + delta_zoom, MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
	if is_equal_approx(zoom_factor, previous_zoom):
		return
	var unit_position: Vector2 = (pivot - previous_origin) / previous_radius
	_update_layout_metrics()
	var new_radius: float = last_hex_radius
	var new_base_origin: Vector2 = last_base_origin
	pan_offset = pivot - new_base_origin - unit_position * new_radius
	_clamp_pan_offset()
	_update_layout_metrics()
	_invalidate_map_cache()  # Zoom changes require fresh cache render
	queue_redraw()
	_minimap.queue_indicator_redraw()
	# Reaching here means the factor actually changed (the no-op / clamped-equal
	# cases early-returned above), so the readout only updates on a real change.
	emit_signal("zoom_changed", zoom_factor)

## Public zoom API — the on-screen zoom rail routes through the same `_apply_zoom`
## path the trackpad/wheel uses, so there is exactly one map-zoom code path.
## `direction` is +1 (in) / -1 (out); the pivot is the map center so button-zoom
## doesn't drift the view.
func zoom_step(direction: int) -> void:
	_apply_zoom(float(direction) * ZOOM_BUTTON_STEP, _viewport_center_pivot())

## Absolute zoom setter — jump straight to a target `zoom_factor` (clamped to
## [MIN,MAX]), pivoting on the map centre. Reuses the single `_apply_zoom` path by
## expressing the target as a relative delta, so the hex-radius recompute, pan-clamp,
## cache invalidation, redraw and the `zoom_changed` HUD-readout emit all happen
## exactly as they do for a wheel/rail zoom. Used to seat the startup zoom on a new
## world reveal; a no-op when already at the target (the delta early-returns).
func set_zoom_factor(target: float) -> void:
	var clamped: float = clamp(target, MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
	_apply_zoom(clamped - zoom_factor, _viewport_center_pivot())

func _viewport_center_pivot() -> Vector2:
	# Local coords (matches _apply_zoom's pivot space); respects the inspector inset.
	return _get_adjusted_viewport_size() * 0.5

## Public alias for the fit-to-view action (the `C` hotkey), so the zoom rail's
## `⊡` button and Main's wiring can call it without reaching a private method.
func fit_to_view() -> void:
	_fit_map_to_view()

func _begin_mouse_pan(button_index: int) -> void:
	mouse_pan_active = true
	mouse_pan_button = button_index

func _end_mouse_pan(button_index: int) -> void:
	if mouse_pan_active and mouse_pan_button == button_index:
		mouse_pan_active = false
		mouse_pan_button = -1

func _mark_input_handled() -> void:
	var viewport := get_viewport()
	if viewport != null:
		viewport.set_input_as_handled()

func _ensure_input_actions() -> void:
	var action_keys := {
		"map_pan_left": KEY_A,
		"map_pan_right": KEY_D,
		"map_pan_up": KEY_W,
		"map_pan_down": KEY_S,
		"map_zoom_in": KEY_E,
		"map_zoom_out": KEY_Q,
	}
	for action in action_keys.keys():
		if not InputMap.has_action(action):
			InputMap.add_action(action)
		var keycode: int = action_keys[action]
		var needs_event: bool = true
		for existing_event in InputMap.action_get_events(action):
			if existing_event is InputEventKey and existing_event.keycode == keycode:
				needs_event = false
				break
		if needs_event:
			var key_event := InputEventKey.new()
			key_event.keycode = keycode
			key_event.physical_keycode = keycode
			InputMap.action_add_event(action, key_event)

func _fit_map_to_view() -> void:
	zoom_factor = 1.0
	pan_offset = Vector2.ZERO
	_update_layout_metrics()
	_clamp_pan_offset()
	# Mirror _apply_zoom: the fit changes last_hex_radius, so the cached terrain
	# render must be dropped too or the map keeps drawing at the pre-fit zoom while
	# markers redraw at the new radius (also fixes the `C` hotkey's stale-icon gap).
	_invalidate_map_cache()
	queue_redraw()
	_minimap.queue_indicator_redraw()
	emit_signal("zoom_changed", zoom_factor)

func handle_hex_click(col: int, row: int, button_index: int) -> void:
	# Only handle left mouse button clicks. Right-clicks and other buttons are intentionally ignored.
	if button_index != MOUSE_BUTTON_LEFT:
		return

	if col < 0 or col >= grid_width or row < 0 or row >= grid_height:
		return

	# Select-then-cycle: re-clicking the current tile with >1 band advances the active
	# band through the stack; any fresh tile resets to the top band. Computed before
	# _emit_tile_selection overwrites selected_tile.
	var bands_here := _units_on_tile(col, row)
	if Vector2i(col, row) == selected_tile and bands_here.size() > 1:
		cycle_index = (cycle_index + 1) % bands_here.size()
	else:
		cycle_index = 0

	var terrain_id: int = _terrain_id_at(col, row)
	emit_signal("hex_selected", col, row, terrain_id)
	_emit_tile_selection(col, row)

	_handle_entity_selection(col, row)

## The single shared hex-grid-line drawer for MapView's own canvas — called by BOTH the shader-terrain
## branch (base terrain is the behind-quad) and _draw_terrain_direct (blend-off per-hex path), so the
## grid renders identically regardless of the terrain path. Each hex paints only its right + lower edges
## (boundary rows/cols add their unshared edges), and every visible edge is batched into one draw_multiline.
func _draw_hex_grid_overlay(radius: float, origin: Vector2, col_start: int, col_end: int, row_start: int, row_end: int) -> void:
	if not _show_grid_lines or radius < 12.0:
		return
	_update_hex_offset_cache(radius)  # idempotent; ensures _cached_hex_offsets is valid for this radius
	if _cached_hex_offsets.size() < 6:
		return
	var o := _cached_hex_offsets
	# draw_multiline consumes points as INDEPENDENT PAIRS (a,b, c,d, …), so push each
	# edge's two endpoints. Batches every visible grid edge into ONE draw call.
	var segs := PackedVector2Array()
	for y in range(row_start, row_end):
		for logical_x in range(col_start, col_end):
			if not _wrap_horizontal and (logical_x < 0 or logical_x >= grid_width):
				continue
			var c: Vector2 = _hex_center(logical_x, y, radius, origin)
			var p0 := c + o[0]
			var p1 := c + o[1]
			var p2 := c + o[2]
			var p3 := c + o[3]
			var p4 := c + o[4]
			var p5 := c + o[5]
			segs.push_back(p5)
			segs.push_back(p0)
			segs.push_back(p0)
			segs.push_back(p1)
			segs.push_back(p1)
			segs.push_back(p2)
			# Map's north boundary: the top row has no neighbour above to draw its upper edges.
			if y == 0:
				segs.push_back(p3)
				segs.push_back(p4)
				segs.push_back(p4)
				segs.push_back(p5)
			# Map's west boundary (non-wrapping): column 0 has no western neighbour.
			if not _wrap_horizontal and logical_x == 0:
				segs.push_back(p2)
				segs.push_back(p3)
	if not segs.is_empty():
		draw_multiline(segs, GRID_LINE_COLOR, GRID_LINE_WIDTH)

# --- End Terrain Texture System ---

# --- 2D Minimap System (uses shared MinimapPanel) ---

## Set reference to HUD layer for minimap integration.
## Must be called before the minimap is first created (lazily on the first
## _minimap.update()) for embedded mode to work.
func set_hud_reference(hud: Node) -> void:
	_hud_layer = hud

## True when a local-space point lies in the map's usable area rather than a strip
## reserved by a docked panel. The node is translated by the leading (left/top)
## insets, so local origin (0,0) is the usable top-left and the adjusted viewport
## size is its extent — a point outside that rect is under a reserved edge (left,
## top, right, OR bottom) even though the cover-fit map mathematically extends
## there. The map ignores input outside it.
func _is_local_point_in_view(local_pos: Vector2) -> bool:
	var adjusted: Vector2 = _get_adjusted_viewport_size()
	return local_pos.x >= 0.0 and local_pos.y >= 0.0 and local_pos.x <= adjusted.x and local_pos.y <= adjusted.y

## Clip this node's drawing to its usable rect (in local space, i.e. after the
## node's translation). Because the map is cover-fit, its content is wider than
## the reduced viewport and would otherwise overflow left into the Inspector's
## strip; clipping confines every draw command (terrain, overlays, markers) to
## the usable width.
func _apply_view_clip(usable_size: Vector2) -> void:
	var ci := get_canvas_item()
	if _inset_left > 0.0 or _inset_right > 0.0 or _inset_top > 0.0 or _inset_bottom > 0.0:
		RenderingServer.canvas_item_set_custom_rect(ci, true, Rect2(Vector2.ZERO, usable_size))
		RenderingServer.canvas_item_set_clip(ci, true)
	else:
		RenderingServer.canvas_item_set_clip(ci, false)
		RenderingServer.canvas_item_set_custom_rect(ci, false, Rect2())

## Reserve a strip of one edge for a docked panel (keyed by reserver id). The
## map's viewport shrinks by the summed per-edge sizes (canvas-space px) and the
## node is translated by the leading (left/top) insets, so the whole map system
## behaves as if the window were that much smaller — nothing draws behind a
## panel. `edge` is a Godot Side const (SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM);
## `size <= 0` releases the reserver's strip.
func set_reserved_inset(id: StringName, edge: int, size: float) -> void:
	if size <= 0.0:
		if not _reservations.has(id):
			return
		_reservations.erase(id)
	else:
		_reservations[id] = {"edge": edge, "size": size}
	_recompute_insets()
	position = Vector2(_inset_left, _inset_top)
	_update_layout_metrics()
	_clamp_pan_offset()
	_invalidate_map_cache()
	queue_redraw()
	_minimap.queue_indicator_redraw()

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
func focus_on_tile(col: int, row: int) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if last_hex_radius <= 0:
		return
	var target_col := col
	var target_row := row
	if _wrap_horizontal:
		# When wrapping, find the closest logical column to current view center
		# First, wrap target to [0, grid_width)
		target_col = posmod(target_col, grid_width)

		# Find current view center column using direct hex geometry
		# (consistent with indicator drawing)
		var viewport_size := _get_adjusted_viewport_size()
		var hex_width := SQRT3 * last_hex_radius
		var current_center_col := (viewport_size.x * 0.5 - last_origin.x) / hex_width

		# Find closest logical column: target_col, target_col - grid_width, or target_col + grid_width
		var dist_direct := absf(float(target_col) - current_center_col)
		var dist_minus := absf(float(target_col - grid_width) - current_center_col)
		var dist_plus := absf(float(target_col + grid_width) - current_center_col)

		if dist_minus < dist_direct and dist_minus < dist_plus:
			target_col = target_col - grid_width
		elif dist_plus < dist_direct and dist_plus < dist_minus:
			target_col = target_col + grid_width
		# else: use target_col as-is
	else:
		target_col = clampi(target_col, 0, grid_width - 1)
	target_row = clampi(target_row, 0, grid_height - 1)

	# Get the screen position of target hex at base origin (before any panning)
	var hex_center_at_base := _hex_center(target_col, target_row, last_hex_radius, last_base_origin)

	# Calculate pan_offset to center this hex in the viewport:
	# viewport_center = hex_center_at_base + pan_offset
	# Therefore: pan_offset = viewport_center - hex_center_at_base
	var viewport_size := _get_adjusted_viewport_size()
	var viewport_center := viewport_size * 0.5
	pan_offset = viewport_center - hex_center_at_base

	_clamp_pan_offset()
	_update_layout_metrics()
	queue_redraw()
	# Panning only moves the viewport; the minimap image is unchanged, so just
	# refresh the indicator instead of running the full rebuild-check path.
	_minimap.queue_indicator_redraw()

## Centre the view on a tile AND select it (as if the hex were clicked), so a jump
## from the turn-orb attention popover lands on a *selected* tile — the Tile card +
## Occupants roster populate, not just a recentre. Select first, then centre.
func focus_and_select_tile(col: int, row: int) -> void:
	handle_hex_click(col, row, MOUSE_BUTTON_LEFT)
	focus_on_tile(col, row)

# --- End 2D Minimap System ---
