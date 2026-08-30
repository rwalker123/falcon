extends Node2D
class_name MapView

const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

signal hex_selected(col: int, row: int, terrain_id: int)
signal tile_selected(info: Dictionary)
signal overlay_legend_changed(legend: Dictionary)
## A snapshot has been ingested, so the channel ROSTER may have changed and `active_overlay_key` has
## just been cleared by `_ingest_overlay_channels`. Distinct from `overlay_legend_changed`, which
## also fires on every ordinary channel change — and the distinction is what tells the minimap's
## picker apart: on THIS it re-asserts the channel the player chose (nothing else will), and on a
## bare legend change it ADOPTS whatever is painted, because some other caller decided that.
## Emitted at the END of `display_snapshot` rather than inside the ingest, so a listener asking
## `has_terrain_tag_data()` sees this frame's tags and not the last one's.
signal overlay_channels_ingested()
signal unit_selected(unit: Dictionary)
signal herd_selected(herd: Dictionary)
## Double-click on a herd (Early-Game Labor slice 3b): a convenience that assigns the
## player band's idle workers to hunt this herd (Main → Hud.quick_assign_hunters). The
## old shift+double-click "scout" shortcut was retired with the single-task scout command.
signal herd_quick_hunt_requested(herd_id: String)
signal tile_hovered(info: Dictionary)
signal selection_cleared()
## The select-then-cycle click reached the LAND stop of an OCCUPIED hex. Carries no payload: the
## `_emit_tile_selection` one call earlier in the same click already handed the HUD this hex's
## `tile_info` (the guarantee `selection_cleared` relies on too), so the land subject is fully
## described by what the HUD already holds. Distinct from `selection_cleared` because the HUD must
## treat it as a DELIBERATE choice — `Hud.show_land_selection` records the choice tile so
## `SelectionCardController._resolve_auto_selected_subject` does not auto-pick the first band back.
signal land_selected()
signal next_turn_requested(steps: int)
signal targeting_cancel_requested()
## Emitted whenever the map zoom factor changes (rail button, wheel, Q/E, or fit).
## The HUD renders the live zoom readout from it. `_apply_zoom` emits only on a
## real change (it early-returns on a no-op); `_fit_map_to_view` also emits after
## resetting zoom + pan, so a fit re-syncs the readout even when already at 1.0×.
signal zoom_changed(zoom_factor: float)

## The two channels this renderer paints WITHOUT consulting `OVERLAY_COLORS` and that own no single
## hue of their own: the empty key (terrain art, or the fog tones over it) and the terrain-tag blend
## (one colour per tag, mixed per tile). Named here because `paints_with_overlay_color` is derived
## from them, and because `_tile_color` branches on both. `OverlayChannels` declares the same two
## keys for the picker's roster; the registry reads this renderer duck-typed and never the other way
## round, so each side spells its own.
const NO_OVERLAY_KEY := ""
const TERRAIN_TAGS_OVERLAY_KEY := "terrain_tags"

## Tint for an overlay channel with no row in OVERLAY_COLORS — a channel the native decoder
## publishes that this table has never heard of still ramps in SOMETHING rather than reading as
## unpainted. (It was `LOGISTICS_COLOR` until the logistics channel was removed with the rest of the
## trade substrate — see `docs/plan_contact_and_logistics.md`; the colour was always doing this
## second job.)
static var OVERLAY_FALLBACK_COLOR := Color(0.15, 0.45, 1.0, 1.0)
static var SENTIMENT_COLOR := Color(1.0, 0.35, 0.25, 1.0)
static var CORRUPTION_COLOR := Color(0.92, 0.58, 0.18, 1.0)
static var CULTURE_COLOR := Color(0.72, 0.36, 0.88, 1.0)
static var MILITARY_COLOR := Color(0.36, 0.7, 0.43, 1.0)
static var CRISIS_COLOR := Color(0.92, 0.24, 0.46, 1.0)
static var ELEVATION_LOW_COLOR := Color(0.16, 0.32, 0.78, 1.0)
static var ELEVATION_MID_COLOR := Color(0.97, 0.82, 0.32, 1.0)
static var ELEVATION_HIGH_COLOR := Color(0.78, 0.14, 0.18, 1.0)
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
static var PASTURE_POOR_COLOR := Color(0.85, 0.78, 0.42, 1.0)    # marginal grazing — dry straw
static var PASTURE_RICH_COLOR := Color(0.13, 0.62, 0.24, 1.0)    # the reference pasture — deep grass green
static var PASTURE_DEAD_COLOR := Color(0.34, 0.30, 0.38, 1.0)    # land that carries NO pasture (glacier/lava/rock)
static var PASTURE_WATER_COLOR := Color(0.10, 0.16, 0.28, 1.0)   # water — no pasture, and not ground at all
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
static var FORAGE_POOR_COLOR := Color(0.88, 0.80, 0.44, 1.0)     # poorest human-food land — pale wheat
static var FORAGE_RICH_COLOR := Color(0.18, 0.72, 0.38, 1.0)     # richest human-food land — lush leaf green
static var FORAGE_BARREN_COLOR := Color(0.20, 0.21, 0.24, 1.0)   # NO human food (deep ocean, glacier, lava)

## Every channel `_tile_color` paints through a path of ITS OWN rather than the generic
## `GRID_COLOR.lerp(OVERLAY_COLORS[key], value)`. Two of them still have an `OVERLAY_COLORS` row that
## describes what they paint (the pasture and forage ramps climb to it); the other two have none and
## can have none, having no single hue. `paints_with_overlay_color` is this list, and it is what a
## caller wearing a channel's colour as a READOUT asks before trusting `overlay_color_for`.
const SPECIAL_PAINT_OVERLAY_KEYS: Array[String] = [
	NO_OVERLAY_KEY,
	TERRAIN_TAGS_OVERLAY_KEY,
	PASTURE_OVERLAY_KEY,
	FORAGE_OVERLAY_KEY,
]
# --- DANGER overlays (Predators Phase 0) -------------------------------------------------------
# TWO derived-danger channels, both per-ENTITY properties the native decoder projects onto tiles
# (max over the herds standing on each hex). Neither is a per-tile field or a two-tone ramp: both
# ride the generic `GRID_COLOR.lerp(overlay_color, value)` path, so empty ground stays grid-colored
# and a hex with a qualifying herd glows. `hunt_danger` (attack × ferocity) is a danger-ORANGE so it
# reads apart from `threat` (attack × aggression), which keeps the harsher threat-RED.
const HUNT_DANGER_OVERLAY_KEY := "hunt_danger"
static var HUNT_DANGER_OVERLAY_COLOR: Color = Color()  # DERIVED: HudStyle.HUNT_DANGER_ACCENT
const THREAT_OVERLAY_KEY := "threat"
static var THREAT_OVERLAY_COLOR: Color = Color()       # DERIVED: HudStyle.THREAT_ACCENT
# --- READY FOR IMPROVEMENT (the aggregate ⌃) ----------------------------------------------------------
# The map-wide view of the per-source `⌃` mark: every source that could climb a rung right now, at
# once. Synthesized client-side from the patches, the herds and the faction's knowledge row (see
# `ReadyForImprovement`) — there is no wire raster, and there could not be, the answer being about what the
# PLAYER's faction knows.
#
# **IT IS `HudStyle.HEALTHY` — THE THEMED "well-supplied / good" GREEN.** The aggregate is a reading
# about LAND — these hexes hold ground and herds that could carry more — and that is the same good
# news every other healthy mark on this client states in green. **The per-source `⌃` badge keeps
# `SIGNAL`**, and the two being different marks is deliberate rather than a drift: a badge pinned to
# one source and a dim wash over a whole map are not one vocabulary said twice, and painting the map
# in the badge's ink made a near-white cream blow the lit hexes out against the grid. It is still
# DERIVED rather than authored: a hand-picked hue would agree with the palette in one theme and fight
# it in three. Amber would be wrong twice over — it is the trouble channel on this map, and teaching a
# player to read good news in it is how a warning stops being read.
static var READY_FOR_IMPROVEMENT_OVERLAY_COLOR: Color = Color()  # DERIVED: HudStyle.HEALTHY

## **CHANNELS THIS RENDERER SYNTHESIZES ON DEMAND — `{key: builder method}`.** A channel in this table
## is NOT built during the snapshot ingest; it is built the first time each frame that something asks
## for it, which in practice is `set_overlay_channel` accepting it — and the picker re-asserts the
## painted channel on every `overlay_channels_ingested`, so a channel the player is HOLDING is rebuilt
## once per turn, automatically, through a seam that already existed.
##
## **A MEASUREMENT PUT IT HERE, NOT A PREFERENCE.** `province` is derived eagerly beside the markers
## because it is a partition over TILES; `ready_for_improvement` is a `RungGates` evaluation per SOURCE, and
## the sim seeds a forage patch on every food-module tile carrying any human-edible capacity with no
## cap in the capture. `map_preview`'s scale probe walks a full-size 256×192 world at **~7 µs a
## source — 342 ms** for the ceiling of 49,152. Paying that on every turn boundary for a channel
## nobody has selected is not a constant worth tuning; it is work that should not happen.
##
## **THE TABLE IS THE POINT.** §6b forbids a second `if key ==` in the render path, so this is a
## registry and `_realize_deferred_overlay` names nothing: a second expensive channel is one row.
const DEFERRED_OVERLAY_BUILDERS := {
	ReadyForImprovement.CHANNEL_KEY: "_build_ready_for_improvement_channel",
}
# Tile "Height" is a relative 0..100 indicator (not meters) so a player can reason
# about line of sight: a higher tile can occlude the tile behind it. Elevation is
# only a normalized 0..1 field, so height rescales the ABOVE-sea-level span into
# 0..100 (at/below sea level reads 0 — nothing occludes over open water). The sea
# level is the ACTIVE map's `sea_level`, streamed per-snapshot in the elevation
# overlay (`_elevation_sea_level`); this constant is only the fallback used until the
# first snapshot arrives (mirrors core_sim's DEFAULT_SEA_LEVEL).
const HEIGHT_DEFAULT_SEA_LEVEL := 0.6
const HEIGHT_BAR_SEGMENTS := 10
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

# ---------------------------------------------------------------------------
# Occupant kinds — the shared vocabulary of `select_occupant`, `_occupants_on_tile` and the
# select-then-cycle click. The VALUES are the wire contract with the HUD: `HudSelectionState`'s
# `SUBJECT_UNIT`/`SUBJECT_HERD`/`SUBJECT_LAND` arrive here through
# `Hud.roster_occupant_selected` → `Main._on_hud_roster_occupant_selected`, so they must match.
# ---------------------------------------------------------------------------
const OCCUPANT_KIND_UNIT := "unit"
const OCCUPANT_KIND_HERD := "herd"
const OCCUPANT_KIND_LAND := "land"
# The keys of an `_occupants_on_tile` entry: which kind it is, and the underlying marker dict.
const OCCUPANT_KEY_KIND := "kind"
const OCCUPANT_KEY_DATA := "data"
# The LAND's cycle entry carries no marker dict — the hex IS its identity, and `selected_tile`
# already names it — so it is the one entry whose `data` is empty by construction.
const LAND_CYCLE_ENTRY := {OCCUPANT_KEY_KIND: OCCUPANT_KIND_LAND, OCCUPANT_KEY_DATA: {}}

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
# Driven by `PenStatus.herd_is_starving` — the same test that marks the herd drawer's `Fed:` row.
## DERIVED from `HudStyle.DANGER` in `apply_palette` — a `const` here would be a parse error against a
## themed `static var`, and an initializer would freeze at the palette loaded before the theme.
static var HERD_DISTRESS_COLOR: Color = Color()
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
# Migration TRAIL: the same amber, dimmer than the arrow — where the herd has BEEN reads under
# where it is going.
const HERD_TRAIL_COLOR := Color(0.97, 0.69, 0.25, 0.6)
const HERD_TRAIL_WIDTH := 2.0

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
#
# THE TWO FOOD WEBS SPLIT ON STOCK vs CAPACITY, NOT ON WEB (issue #462). Each web's CAPACITY is
# remembered and each web's BIOMASS — plus the ecology phase, which is classified FROM that biomass —
# is redacted, so `graze_biomass` / `patch_biomass` and both phases are in this list while both webs
# still state a ceiling on a remembered card. Why a capacity can be shown on a hex the player cannot
# see without leaking anything is in `.claude/rules/client/land-readouts.md` → "Fog splits a stock
# from its CAPACITY"; the short form is that the sim recomputes `K` from the tile every turn and no
# player action moves it, so the value we are sent for an unseen hex IS the value that hex last showed.
#
# # ⛔ WHICH FORAGE CAPACITY: THE GAIN IS REDACTED, NOT THE CEILING
#
# **`patch_carrying_capacity` IS IN THIS LIST**, and that is not a retreat from the split above — it
# is the split applied to a field that stopped being ground. It is the PATCH's ceiling, the tile's own
# `K` times the interpolated `field_capacity_gain`, so a standing Field publishes ~2.53× its biome's
# base while `patch_is_field` and `patch_field_progress` are redacted two lines down. That ratio is a
# finer reading of the ladder position than the boolean being hidden, and it moves continuously as the
# meter fills, so a remembered hex was handing over exactly what the redaction exists to withhold.
#
# **`patch_tile_capacity` is what a remembered hex renders instead** — the ground's own `K`, a pure
# function of the tile's terrain with no rung in it, which a Discovered tile knows by definition.
# ONE reader answers "what capacity does this card show" for both states: `DetailFormat.patch_capacity`
# prefers the patch's ceiling and falls back to the tile's. Nothing else may do that `or` itself.
#
# **`graze_capacity` IS UNAFFECTED AND STAYS OUT OF THIS LIST — do not "fix" the asymmetry.** The
# animal web's density multipliers land on `Herd::carrying_capacity`, a different field entirely;
# `GrazePatch`'s capacity is still the tile's biome-derived graze ceiling and no rung moves it. The
# graze row really is ground, so redacting it would delete a true reading from every remembered card
# for the sake of a symmetry the sim does not have.
const FOW_DISCOVERED_HIDDEN_KEYS := [
	"food_module", "food_module_label", "food_module_weight", "food_kind",
	"patch_cultivation_progress", "patch_is_cultivated", "patch_has_owner", "patch_owner",
	"patch_ecology_phase", "patch_biomass",
	# THE PATCH'S CEILING, redacted because the FIELD RUNG IS IN IT — see this list's header. Its
	# fog-safe twin `patch_tile_capacity` is deliberately NOT here: that one is terrain.
	"patch_carrying_capacity",
	# The ANIMAL web's stock, redacted under the same rule as the plant web's above — grass on a hex
	# you cannot see is drawn down by herds you cannot see. Its `graze_capacity` twin stays.
	"graze_biomass", "graze_ecology_phase",
	# The phase BANDS and the two growth terms travel with the stock they describe: all four are read
	# only to compose the harvest-floor instrument, which a hex the player cannot currently see does not
	# render — one rule for the whole patch payload, as with the dips and the crews below.
	"patch_collapse_fraction", "patch_stressed_fraction",
	"patch_per_worker_biomass", "patch_regrowth_samples",
	"patch_per_worker_yield", "patch_tended_yield",
	# Plant rung 3 (the Field + Sow) — redacted exactly as their rung-2 twins above are: the two
	# build meters are live patch state, and the Sow forecast pair is quoted at the patch's CURRENT
	# biomass. `patch_sow_site_refusal` rides with them: it describes the GROUND (fertility + water,
	# which a remembered tile would arguably still know), but it is only ever read to gate the Sow
	# affordance — and that affordance is already withheld on a hex the player cannot see, so
	# redacting it keeps ONE rule for the whole patch payload rather than a lone exception.
	"patch_field_progress", "patch_is_field",
	"patch_field_yield", "patch_sow_site_refusal",
	# The two build meters' WORK absolutes and the source's turn/gear pair (`plan_unit_costed_work.md`
	# §8) travel with the fractions they decompose: they are live build state, and a remembered tile
	# knows a build's price no better than it knows its progress.
	"patch_cultivation_work_done", "patch_cultivation_work_cost",
	"patch_field_work_done", "patch_field_work_cost",
	"patch_build_turns_remaining", "patch_build_work_from_gear",
	# WHERE THIS PATCH SITS IN THE WINNING BAND'S BUILD QUEUE (§4.6b). It rides the same winner as the
	# pair above and is redacted with them: a queue position is live state about a band's declared
	# work, which a remembered tile knows no better than it knows the countdown it explains.
	"patch_build_queue_position",
	# …and WHY that queue is blocked here. It names a gate refusing a band's declared job right
	# now, so it is redacted with the countdown it explains rather than with the ground readings: a
	# remembered tile knows no more about a live refusal than it knows the date behind it.
	"patch_build_blocked_reason",
	# …and WHAT THAT BUILD IS BEING RAISED WITH. A resolved builders kit is a fact about a band's
	# declared job, redacted with the queue position and the cause it rides beside.
	"patch_build_kit_id",
	# WHERE THE QUEUED ENTRY IS TAKING THIS PATCH, and what is left of the climb
	# (`docs/plan_standing_upkeep.md` §2.8). Both are facts about a band's DECLARED job — the
	# destination it named and the legs still owed from where the patch stands — so they are redacted
	# with the queue position and the countdown they belong to. A remembered tile knows where the
	# ground has been taken no better than it knows how far along the job is.
	"patch_build_destination_rung", "patch_build_legs",
	# …and WHERE THE PATCH ITSELF STANDS on that ladder. It is redacted for the same reason
	# `patch_carrying_capacity` above is: `plant:tended` / `plant:field` state exactly the rung
	# `patch_is_cultivated` and `patch_is_field` are struck out two entries up to hide, so leaving it
	# out would hand a merely-remembered hex the redaction's own answer in one token. It travels with
	# the destination rather than with the ground readings because it IS the ladder, not the terrain
	# under it — `patch_tile_capacity` is what a remembered hex has instead.
	"patch_current_rung",
	# …and WHAT THE GROUND WILL CARRY at that destination. It rides the destination it belongs to, and
	# it must: the figure is `tile K x the rung's field_capacity_gain`, so publishing it on a remembered
	# hex hands over the same interpolated ladder position `patch_carrying_capacity` is redacted to
	# hide — one rung further up. A hex the player cannot see renders no floor instrument at all, so
	# nothing reads it there either.
	"patch_build_destination_capacity",
	# The estimate's per-source TERM travels under the same rule as the answer beside it — it is a
	# figure about a build being worked, and a remembered tile knows no more about that than it knows
	# the progress. (The gear half of the estimate is not here at all: it rides the band's kit row.)
	"patch_build_work_per_worker_turn",
	# THE TILE'S PER-BIOMASS YIELD VECTOR (docs/plan_harvest_floor.md §5) — what one unit of this
	# patch's standing crop is worth in each account, plus the two investment rungs' non-food payoff
	# twins. **It replaced the six per-policy row dicts**, which could only answer four floors; the
	# client composes `max(0, B − floor·K) × rate` at any floor from these three plus `patch_biomass`
	# and `patch_carrying_capacity` (both redacted above, which is why the composition answers nothing
	# on a remembered hex — the ceiling has no `K` to be a fraction of). Redacted for the same reason every
	# forecast field is — each describes live patch state a remembered tile does not know — and
	# redacting them is also what keeps a remembered tile reading "no forecast" rather than a stale
	# one: `SourceForecast.forecast_is_known` reads the vector's PRESENCE, so the answer comes for free.
	"patch_provisions_per_biomass", "patch_fodder_per_biomass",
	# The MATERIAL account's two vectors ride with the two scalars above, under the one rule the
	# whole patch payload follows: they are per-biomass rates read only to compose a forecast at the
	# patch's CURRENT stock, which a hex the player cannot see does not render.
	"patch_material_per_biomass", "patch_per_worker_material",
	"patch_tended_fodder", "patch_field_fodder",
	# THE STANDING UPKEEP (`docs/plan_standing_upkeep.md` §2) — what holding this patch's rung demands
	# every turn, what its keepers paid, what went unmet, and the hands that would meet it. Live patch
	# state, redacted under the one rule the whole patch payload follows.
	"patch_upkeep_demand", "patch_upkeep_supplied", "patch_upkeep_shortfall",
	"patch_upkeep_workers_needed",
	# …and what that shortfall is COSTING the meter, which is the same fact one step on. The two
	# per-rung `*_upkeep_demand` figures beside it are deliberately NOT here: since the plant rungs
	# moved onto `scaled_by: source_load` they no longer read identically on every patch, but the
	# scale is the tile's own forage capacity — TERRAIN, which a Discovered tile remembers — so the
	# figure sent for an unseen hex is the figure that hex last showed, exactly as
	# `patch_tile_capacity` they are struck through does. (Emphatically NOT
	# `patch_carrying_capacity`, which carries the rung and is redacted above.) This one is derived
	# from a shortfall a remembered tile has no way to observe.
	"patch_meter_rot_per_turn",
	# THE NEGLECT GRACE. Live patch state by construction — it counts the turns of upkeep SHORTFALL —
	# and redacting it is what keeps a remembered tile from counting down a lapse it has no way to
	# observe.
	"patch_has_neglect_grace", "patch_neglect_grace_remaining",
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
# `zoom_factor` is a MULTIPLE OF THE COVER FIT, not an absolute hex size: `_update_layout_metrics`
# sizes `base_hex_radius` so the map COVERS the viewport and MIN_ZOOM_FACTOR (1.0) is that fit. So
# this cap says "how close a single hex can get", and what it buys depends on the panel — on a
# hi-DPI / high-resolution display the cover fit already yields a small `base_hex_radius`, so at the
# old 4.0 hexes stayed small even at full zoom-in (issue #375). 7.0 adds six more ZOOM_BUTTON_STEP
# (0.5) clicks; terrain, labels and markers were checked to still read at the new maximum
# (`map_preview`'s `map_max_zoom` state, which asserts it is sitting at this const).
const MAX_ZOOM_FACTOR := 7.0
const MOUSE_ZOOM_STEP := 0.2
# One click of the on-screen zoom rail — also the RUNG SPACING of the ladder
# `zoom_step` snaps to (see it for why the rail is a ladder). Deliberately larger
# than MOUSE_ZOOM_STEP (0.2) so a button press feels like a deliberate step, not a
# nudge; promote to a config lever if it ever wants tuning.
const ZOOM_BUTTON_STEP := 0.5
# Tolerance, IN RUNGS, for deciding whether `zoom_factor` is already sitting on a
# ladder rung. Absorbs the float drift left by the pivot math in `_apply_zoom`, so a
# factor a hair below a rung still counts as ON it and one click moves a WHOLE rung
# rather than degenerating into a near-zero step.
const ZOOM_RUNG_EPSILON := 0.001
const KEYBOARD_ZOOM_SPEED := 0.8
const KEYBOARD_PAN_SPEED := 600.0
# The smallest interface scale this node will take the reciprocal of. `ClientSettings` clamps the
# setting to a far higher UI_SCALE_MIN, so this is the guard for a hand-edited config file only —
# without it a 0 would make the counter-scale infinite. See `_apply_ui_scale`.
const MIN_UI_SCALE := 0.01
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
# DENIAL raid (docs/plan_denial_raid.md) — a third mission, and a third marker: it engages like a hunt
# party but brings nothing home, so wearing the bow would read as a hunt on the map. 💀 is the mark it
# wears everywhere else (the footer button, the parties row), so the three surfaces agree.
const EXPEDITION_DENY_MISSION := "deny"
const EXPEDITION_DENY_GLYPH := "💀"
# TRADE shipment (arc #527) — a fourth mission and a fourth marker. It carries goods to another band
# and comes home empty, so neither the bow nor the skull says what it is: 📦 is the mark it wears on
# its footer button and its parties row too, so the three surfaces agree. Its phase decorations stay
# OFF (`is_hunt` gates those) — the green pip is a HAUL cue, and a shipment's haul is going the other
# way.
const EXPEDITION_TRADE_MISSION := "trade"
const EXPEDITION_TRADE_GLYPH := "📦"
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
## DERIVED: SIGNAL at `SUPPLY_LINK_OPACITY`. It was a hand-written copy of the console cyan, which
## would have stayed teal under every other theme.
const SUPPLY_LINK_OPACITY := 0.28
static var SUPPLY_LINK_COLOR: Color = Color()
const SUPPLY_LINK_WIDTH := 2.0
const SUPPLY_NETWORK_SOLO := 0  # supply_network_id 0 == not in a shared network

## Channel key -> the tint its ramp climbs to. BUILT IN `apply_palette`, not here: every value in it is
## a themed colour, and a dictionary initializer runs at script load, before any theme is installed —
## it would freeze the whole table at the default palette. Empty until the first `apply_palette`.
static var OVERLAY_COLORS := {}

# ---- theme installation ----------------------------------------------------
## Install one theme's MAP ramps. Called by `HudPalette.apply()` AFTER `HudStyle.apply_palette`, which
## the derivations below depend on.
##
## `p` carries the **16 AUTHORED** ramp colours. The five values under `--- derived ---` are the map
## side of HUD colours (so the two surfaces speak one danger/alert language) plus the table built out
## of them, and they are assigned HERE rather than as initializers: an initializer runs at script load,
## before any theme is installed, and would silently freeze at the default palette.
##
## The elevation ramp stays THREE stops — only the colours change, from a blue/yellow/red heatmap to a
## hypsometric lowland-green -> tan -> bone tint.
static func apply_palette(p: Dictionary) -> void:
	SENTIMENT_COLOR = p["SENTIMENT_COLOR"]
	CORRUPTION_COLOR = p["CORRUPTION_COLOR"]
	CULTURE_COLOR = p["CULTURE_COLOR"]
	MILITARY_COLOR = p["MILITARY_COLOR"]
	CRISIS_COLOR = p["CRISIS_COLOR"]
	OVERLAY_FALLBACK_COLOR = p["OVERLAY_FALLBACK_COLOR"]
	ELEVATION_LOW_COLOR = p["ELEVATION_LOW_COLOR"]
	ELEVATION_MID_COLOR = p["ELEVATION_MID_COLOR"]
	ELEVATION_HIGH_COLOR = p["ELEVATION_HIGH_COLOR"]
	PASTURE_POOR_COLOR = p["PASTURE_POOR_COLOR"]
	PASTURE_RICH_COLOR = p["PASTURE_RICH_COLOR"]
	PASTURE_DEAD_COLOR = p["PASTURE_DEAD_COLOR"]
	PASTURE_WATER_COLOR = p["PASTURE_WATER_COLOR"]
	FORAGE_POOR_COLOR = p["FORAGE_POOR_COLOR"]
	FORAGE_RICH_COLOR = p["FORAGE_RICH_COLOR"]
	FORAGE_BARREN_COLOR = p["FORAGE_BARREN_COLOR"]
	# --- derived ---
	THREAT_OVERLAY_COLOR = HudStyle.THREAT_ACCENT
	HUNT_DANGER_OVERLAY_COLOR = HudStyle.HUNT_DANGER_ACCENT
	READY_FOR_IMPROVEMENT_OVERLAY_COLOR = HudStyle.HEALTHY
	HERD_DISTRESS_COLOR = HudStyle.DANGER
	SUPPLY_LINK_COLOR = Color(HudStyle.SIGNAL, SUPPLY_LINK_OPACITY)
	OVERLAY_COLORS = {
		"sentiment": SENTIMENT_COLOR,
		"corruption": CORRUPTION_COLOR,
		"culture": CULTURE_COLOR,
		"military": MILITARY_COLOR,
		"crisis": CRISIS_COLOR,
		"elevation": ELEVATION_HIGH_COLOR,
		"moisture": Color(0.2, 0.65, 0.95, 1.0),
		"province": Color(0.52, 0.64, 0.78, 1.0),
		# The pasture channel paints through `_pasture_color` (a two-tone ramp plus two off-ramp barren
		# tones), not a single-hue tint; this is the swatch any generic fallback path shows for it.
		PASTURE_OVERLAY_KEY: PASTURE_RICH_COLOR,
		# And the forage channel paints through `_forage_color` (a wheat→green ramp plus one barren
		# tone) for the same reason; this is the swatch any generic fallback path shows for it. Without
		# a row it fell to `OVERLAY_FALLBACK_COLOR`, so the minimap's legend button wore a blue that
		# appears nowhere on the forage map.
		FORAGE_OVERLAY_KEY: FORAGE_RICH_COLOR,
		# Both danger channels ride the generic lerp path — empty tiles stay grid-colored, a qualifying
		# herd glows (hunt-danger orange, threat red, so the two read apart).
		HUNT_DANGER_OVERLAY_KEY: HUNT_DANGER_OVERLAY_COLOR,
		THREAT_OVERLAY_KEY: THREAT_OVERLAY_COLOR,
		# The aggregate ⌃ rides the generic lerp too — a hex with an offer wears a dim wash of this hue
		# (`ReadyForImprovement.TILE_READY`), every other stays grid-coloured. Without a row it would
		# paint `OVERLAY_FALLBACK_COLOR`, a blue meaning nothing, and — because the fallback is what the
		# legend button would then state as well — the picker's roster-wide face guard would pass on it,
		# the two agreeing honestly about a colour that says nothing. What the row buys is the HEALTHY
		# GREEN, and that is what `map_preview` asserts by name.
		ReadyForImprovement.CHANNEL_KEY: READY_FOR_IMPROVEMENT_OVERLAY_COLOR,
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
## **THE ROAD NETWORK — the roads in the GROUND** (arc #532, `.claude/rules/core_sim/routes.md`), one
## entry per road, in the sim's own ledger order. World state like `units` / `herds`, which is why it
## lives here and not on `AnnotationRenderer`: that renderer draws it through the `_view` back-ref,
## the same way it reads `units` and `herds` today.
##
## ⛔ **THIS IS NOT `AnnotationRenderer._routes`, AND THE TWO MUST NOT MERGE.** That field — and
## `map_preview`'s `"routes"` annotation state — is the per-faction ORDER PATH overlay: waypoints a
## player's own movement orders are following. A road is a world object with a fixed stamped path
## that outlives every band that walks it. The obvious name was already taken by the other thing, so
## the road network is spelled `road_network` everywhere in the client.
## The key `_ingest_road_network` stamps the ZIPPED path onto each road dict under — an `Array` of
## `Vector2i` built once at ingest from the wire's two packed halves, so neither the draw pass nor a
## hover re-zips it. Named because the producer and its readers are different scripts and a typo in a
## `get` there is a silently empty polyline.
const ROAD_TILES_KEY := "tiles"
var road_network: Array = []
## …and the same roads keyed by the tiles they RUN OVER (`{Vector2i: Array[road]}`), so
## `_tile_info_at` can answer "what roads cross this hex" without walking every path every hover. One
## road appears under each of its own tiles; a hex may carry more than one road.
var road_tile_lookup: Dictionary = {}
# Forage patches (cultivation/tended state, decoded from ForagePatchState), keyed by
# Vector2i(x, y); read by `_tile_info_at` for the Tile-card cultivation/tended readout.
var forage_patch_lookup: Dictionary = {}
## Player faction knowledge, pushed from the HUD — see `set_faction_knowledge`.
var faction_knowledge: Dictionary = {}
## The `ready_for_improvement` channel's cached model (`ReadyForImprovement.derive`) — the raster its channel was
## built from plus the counts and unworked tiles its legend states. Empty until the frame builds it,
## and on a world with no sources at all.
var _ready_for_improvement: Dictionary = {}
## **A COPY of the knowledge row the cached model was derived AGAINST**, and it is a copy for the same
## reason `reset_world_state` rebinds rather than clears: `faction_knowledge` is the HUD's own dict
## held by reference, so storing the reference would compare a row against itself and never fire.
var _ready_for_improvement_knowledge: Dictionary = {}
## Which deferred channels this frame has NOT built yet (`{key: true}`) — see
## `DEFERRED_OVERLAY_BUILDERS` and `_realize_deferred_overlay`.
var _deferred_overlay_pending: Dictionary = {}
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
# Select-then-cycle: which member of the selected tile's cycle is active — an index into
# `_selection_cycle_on_tile` (every band, then every herd, then the LAND), not into the bands alone.
# Advanced by re-clicking the selected tile; reset to 0 (top card) on a fresh tile; synced from a
# roster selection via select_occupant so map cycling + roster stay coherent.
var cycle_index: int = 0
var biome_color_buffer: PackedColorArray = PackedColorArray()
var _hovered_tile: Vector2i = Vector2i(-1, -1)
# Fails CLOSED: fog ON until a snapshot's `fog_enabled` says otherwise. `Main._ready` used to seat
# this true before the first world rendered; that seat is gone now that the sim owns the flag, so the
# DEFAULT has to carry it — a `false` default would draw one fully-revealed frame on load, which is
# the leak this whole arc closed. Matches the server's `SimulationConfig.fog_enabled` default.
var _fow_enabled: bool = true

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
# Map ANNOTATIONS — crisis annotations, the Terrain-tab highlight, order
# routes and the command-targeting overlay, plus that family's own state (owned by AnnotationRenderer
# — see ui/AnnotationRenderer.gd). Its five PUBLIC seams keep same-named pass-throughs on MapView
# because every one of them is reached reflectively; see the header of that file.
var _annotations: AnnotationRenderer = null
# Terrain textures + the Approach-B blend shader (owned by TerrainRenderer — see ui/TerrainRenderer.gd).
# The CPU base pass (_draw_terrain_direct) and the _cache_* SubViewport stay on MapView.
var _terrain: TerrainRenderer = null
var _hud_layer: Node = null  # HudLayer reference, set via set_hud_reference() for embedded minimap
var _explored_bounds_world: Rect2 = Rect2()  # World coords of explored area at unit radius (scaled in _clamp_pan_offset)

# Profiling for performance measurement. `_profiling_enabled` is seated from the SAME flag as the
# per-snapshot profile (`TurnProfile.ENV_FLAG`) so one switch governs both halves of the client's
# turn cost — the ingest line below and this rolling `_draw` average.
var _draw_frame_times: Array[float] = []
var _profiling_enabled: bool = false
## Frames folded into one rolling `_draw` average before it is printed. At 60 fps that is a line
## roughly every 1.7 s — often enough to watch a change land, rare enough not to drown the ingest
## lines it shares a console with.
const DRAW_PROFILE_WINDOW_FRAMES := 100

# Phase labels for `display_snapshot`'s breakdown. `Main` prefixes them with `display.` when it
# splices `last_display_profile` into its line, so these stay short and unqualified.
const PROFILE_OVERLAYS := "overlays"   # channel ingest + terrain raster + biome colours + cache kill
const PROFILE_LAYERS := "layers"       # palette/tags, culture layers, crisis annotations, routes
const PROFILE_SITES := "sites"         # food / discovered / forage / harvest / scout ingests
# `layers` and `sites` each cover several unrelated ingests and were the two biggest surprises in the
# first live run (layers 31.6 ms, sites 9.8 ms), so each is broken down one level further. The parent
# still reports the whole block; these say which part of it.
const PROFILE_LAYERS_TAGS := "layers.tags"                # terrain palette + the terrain_tags full-grid conversion + tag labels
const PROFILE_LAYERS_CULTURE := "layers.culture"          # the culture_layer_map merge + removals
const PROFILE_LAYERS_CRISIS := "layers.crisis"            # AnnotationRenderer.set_crisis_annotations
const PROFILE_LAYERS_ROUTES := "layers.routes"            # AnnotationRenderer.set_routes (the `orders` array)
const PROFILE_LAYERS_ROAD_NETWORK := "layers.road_network"  # _ingest_road_network (the `routes` SECTION — the roads in the ground)
const PROFILE_SITES_FOOD := "sites.food"                  # food_modules ingest + the terrain_id stamp
const PROFILE_SITES_DISCOVERED := "sites.discovered"      # the per-faction discovered-site ingest
const PROFILE_SITES_FORAGE := "sites.forage"              # the forage_patches ingest
const PROFILE_SITES_POPULATIONS := "sites.populations"    # the populations harvest/scout target extraction
const PROFILE_TILES := "tiles"         # the full-grid per-tile GDScript loop
const PROFILE_SHADER := "shader"       # the six full-grid splatmap rebuilds
const PROFILE_MARKERS := "markers"     # province overlay + unit + herd markers
const PROFILE_TAIL := "tail"           # layout/clamp/redraw, legend, minimap, metrics

# ---------------------------------------------------------------------------
# Change-manifest sections `display_snapshot` gates its blocks on
# ---------------------------------------------------------------------------
# Named constants rather than inline strings because these are a CONTRACT with the native decoder
# (`native/src/snapshot/cache.rs`, `bridge/decoder.rs`), and a typo here reads as "this section
# never changed" — a block that silently stops rebuilding, which is the exact failure this whole
# arc has been chasing. `SnapshotSections.changed` answers `true` for any frame with no manifest, so
# a full snapshot, a resync and a pre-manifest native build all still repaint everything.
const SECTION_TILES := "tiles"
## The tile fields the terrain SPLATMAPS are built from, reported apart from `tiles` by the decoder
## so 600 tiles moving their graze biomass costs no texture rebuild.
const SECTION_TILES_RIVERS := "tiles.rivers"
const SECTION_CULTURE_LAYERS := "culture_layers"
const SECTION_FOOD_MODULES := "food_modules"
const SECTION_DISCOVERED_SITES := "discovered_sites"
const SECTION_FORAGE_PATCHES := "forage_patches"
const SECTION_POPULATIONS := "populations"
## The ROADS-IN-THE-GROUND section (arc #532). Named `routes` because that is the wire's own name
## for it and the manifest carries that spelling; the client-side NOUN is `road_network`, to keep it
## clear of `AnnotationRenderer`'s order-path `_routes`.
const SECTION_ROUTES := "routes"
const SECTION_OVERLAY_TERRAIN := "overlays.terrain"
const SECTION_OVERLAY_VISIBILITY := "overlays.visibility"
const SECTION_OVERLAY_ELEVATION := "overlays.elevation"

## Everything `TerrainRenderer.rebuild_shader_maps` reads: the terrain id grid, the FoW visibility
## raster, the elevation raster, and the per-tile river edge/inflow/channel + underlying-terrain
## masks. Six full-grid `PackedByteArray`s (7.7 ms measured), so it must not ride on a tile moving
## its biomass.
## A plain `Array`, not a `PackedStringArray`: a constructor call is not a constant expression in
## GDScript, so the packed form cannot be a `const` at all (it parses, then fails to compile).
const SHADER_INPUT_SECTIONS := [
	SECTION_OVERLAY_TERRAIN,
	SECTION_OVERLAY_VISIBILITY,
	SECTION_OVERLAY_ELEVATION,
	SECTION_TILES_RIVERS,
]

## Everything `MinimapController._rebuild_image` reads: the terrain id grid and the visibility
## raster (the FoW *toggle* it tracks itself). Verified against that function rather than assumed —
## the minimap image is a full-grid pixel loop, and a stale one is a visibly frozen map.
const MINIMAP_INPUT_SECTIONS := [
	SECTION_OVERLAY_TERRAIN,
	SECTION_OVERLAY_VISIBILITY,
]

## Cost breakdown of the LAST `display_snapshot`, published for `Main` to fold into its per-turn
## `[TurnProfile]` line. Inert (records nothing) unless profiling is on.
var last_display_profile: TurnProfile = null

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
	_profiling_enabled = TurnProfile.is_enabled()
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
	_annotations = AnnotationRenderer.new(self)
	_apply_ui_scale()
	ClientSettings.changed.connect(_apply_ui_scale)
	# Note: the MinimapPanel node is created lazily from _minimap.update()
	# This allows Main.gd to set_hud_reference() before the minimap is created


## THE MAP IS IMMUNE TO THE INTERFACE SCALE, and this is what makes it so.
##
## `UiScaler` sets the window's `content_scale_factor`, which shrinks the LOGICAL viewport so every
## UI control re-lays-out larger on screen. Counter-scaling this node by the reciprocal leaves the
## world drawn at exactly the same on-screen size — the chrome grows around a map that holds still,
## which is what the old whole-canvas interface scale got wrong (it zoomed the map too).
##
## MapView owns its own compensation because it already reads `ClientSettings` live for the pan/zoom
## multipliers; `UiScaler` deliberately holds no handle to the map and must not grow one.
##
## The counter-scale leaves `screen_size_local()` CONSTANT (the viewport shrinks by `s`, the local
## units grow by `s`), so the map's own metrics only actually move when a reserved inset is in play —
## a docked panel drawn larger eats more of the map. Recomputing them unconditionally is cheap and
## keeps the two cases from needing to be told apart.
func _apply_ui_scale() -> void:
	var ui_scale: float = ClientSettings.ui_scale
	if ui_scale < MIN_UI_SCALE:
		# Only reachable from a hand-edited config file — the Options slider and `ClientSettings`
		# both clamp to UI_SCALE_MIN. Refuse rather than divide by ~0 and explode the map.
		push_warning("[MapView] ignoring an unusable interface scale: %f" % ui_scale)
		return
	var counter_scale := Vector2.ONE / ui_scale
	# **A NO-OP IS FREE, AND IT HAS TO BE.** `ClientSettings.changed` is ONE signal shared by all four
	# setters, and `MenuShell._make_speed_slider_row` writes on every `value_changed` — so dragging the
	# PAN SPEED slider with the pause menu open runs this function once per step of the drag. Each run
	# would otherwise invalidate the map cache, which is the whole-map re-render the cache exists to
	# avoid, for a scale that did not move.
	#
	# **The test is the TRANSFORM ACTUALLY IN EFFECT, not a remembered copy of `ui_scale`.** `scale` is
	# what must be true when this returns; a cached previous value would go stale the moment anything
	# else wrote the node's scale, and the early-out would then skip the correction that was the point.
	#
	# It short-circuits the `_ready`-time call too, and nothing is lost there: `scale` already IS
	# `Vector2.ONE` at the default scale, `_cache_valid` starts `false` so there is no cache to
	# invalidate, and `_update_layout_metrics()` early-returns while `grid_width`/`grid_height` are 0 —
	# the first snapshot recomputes all of it.
	if scale.is_equal_approx(counter_scale):
		return
	scale = counter_scale
	_invalidate_map_cache()
	_update_layout_metrics()
	queue_redraw()


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

	# Calculate buffer size. LOCAL units, like every other length here: the cached texture is drawn
	# back into this node's own space (`draw_texture_rect` in `_draw`), so it must be sized in it.
	var viewport_size := screen_size_local()
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
	var viewport_size := screen_size_local()
	var max_offset := viewport_size * MAP_CACHE_BUFFER_MARGIN

	# Check if pan is within buffer bounds
	return absf(pan_delta.x) <= max_offset.x and absf(pan_delta.y) <= max_offset.y


func _on_cache_rendered() -> void:
	## Called when cache finishes rendering (signal from CachedMapRenderer)
	_cache_rendering = false

func display_snapshot(snapshot: Dictionary) -> Dictionary:
	if snapshot.is_empty():
		return {}
	# Per-turn cost breakdown for this ingest, published on `last_display_profile` for `Main` to
	# splice into the one `[TurnProfile]` line it prints (see TurnProfile.gd). Inert unless the
	# profile flag is on; the labels here render as `display.<label>`.
	last_display_profile = TurnProfile.start()
	var profile: TurnProfile = last_display_profile
	var t_overlays: int = profile.begin(PROFILE_OVERLAYS)
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
	# NEVER gated: the scalar raster channels (sentiment, corruption, culture, military) move on
	# nearly every turn, and this is also where the sea level and climate cut points land.
	_ingest_overlay_channels(overlays)
	# The TERRAIN grid is the opposite case — it moves only when the map is (re)generated, and it
	# costs a full-grid PackedInt32Array conversion plus a biome-colour rebuild. `dimensions_changed`
	# rides alongside because a resize must repaint whatever the manifest says.
	if dimensions_changed or SnapshotSections.changed(snapshot, SECTION_OVERLAY_TERRAIN):
		terrain_overlay = PackedInt32Array(overlays.get("terrain", []))
		_terrain.set_grid_terrain(terrain_overlay, grid_width, grid_height)
		_update_biome_color_buffer()
	# The minimap image is a full-grid pixel loop over exactly terrain + visibility, so bumping the
	# version on a turn that moved neither only buys a redundant rebuild.
	if dimensions_changed or SnapshotSections.any_changed(snapshot, MINIMAP_INPUT_SECTIONS):
		_minimap.bump_data_version()
	# Deliberately NOT gated, unlike the minimap beside it — and MEASURED, because "it is only a
	# flag write" is true of this line and says nothing about what the flag costs.
	#
	# **In the shipped configuration this whole path is dead.** The cache is reached only from the
	# `else` branch of `_draw`'s terrain block, i.e. when `_terrain.shader_active()` is FALSE; with
	# the Approach-B blend shader on (the default) the base terrain is one GPU draw and
	# `_render_map_cache` is unreachable. Instrumenting `CachedMapRenderer._draw` across a full
	# live session produced ZERO calls. So gating this would buy exactly nothing by default.
	#
	# With the shader off it is NOT cheap: the same instrumentation, forcing the cached branch,
	# measured **70-72 ms per cached render** on an 80x52 map. It does not fire every turn (the
	# re-render is also gated on the pan buffer in `_draw`), so that is a per-render cost at an
	# unmeasured cadence, not a per-turn one — but it is large enough that anyone turning terrain
	# blending off should measure before assuming this line is free.
	#
	# The reason it stays ungated is correctness, not cost: `CachedMapRenderer._draw` paints
	# `_tile_color`, which follows the ACTIVE OVERLAY channel, so with an overlay selected the
	# cached map's colours move whenever that channel does and a gate on terrain/fog would freeze
	# them. If it ever needs gating, key it on the active overlay channel — `active_overlay_key`
	# empty means terrain/fog are the only inputs — never on terrain/fog alone.
	_invalidate_map_cache()
	profile.end(PROFILE_OVERLAYS, t_overlays)
	var t_layers: int = profile.begin(PROFILE_LAYERS)
	var t_layers_tags: int = profile.begin(PROFILE_LAYERS_TAGS)
	var palette_raw: Variant = overlays.get("terrain_palette", {})
	terrain_palette = palette_raw if typeof(palette_raw) == TYPE_DICTIONARY else {}
	terrain_tags_overlay = PackedInt32Array(overlays.get("terrain_tags", []))
	var tag_labels_raw: Variant = overlays.get("terrain_tag_labels", {})
	terrain_tag_labels = tag_labels_raw if typeof(tag_labels_raw) == TYPE_DICTIONARY else {}
	profile.end(PROFILE_LAYERS_TAGS, t_layers_tags)
	var t_layers_culture: int = profile.begin(PROFILE_LAYERS_CULTURE)
	# One gate for both halves: the decoder names `culture_layers` when the section's diff carried
	# EITHER changed rows or removed ids (removals set the section's changed flag), so a frame that
	# does not name it has neither to apply.
	if SnapshotSections.changed(snapshot, SECTION_CULTURE_LAYERS):
		_ingest_culture_layers(snapshot)
	profile.end(PROFILE_LAYERS_CULTURE, t_layers_culture)
	var t_layers_crisis: int = profile.begin(PROFILE_LAYERS_CRISIS)
	_annotations.set_crisis_annotations(overlays.get("crisis_annotations", []))
	profile.end(PROFILE_LAYERS_CRISIS, t_layers_crisis)
	var t_layers_routes: int = profile.begin(PROFILE_LAYERS_ROUTES)
	_annotations.set_routes(snapshot.get("orders", []))
	profile.end(PROFILE_LAYERS_ROUTES, t_layers_routes)
	var t_layers_roads: int = profile.begin(PROFILE_LAYERS_ROAD_NETWORK)
	# **THE ROADS IN THE GROUND** — a different section from the order paths one line up, and a
	# different KIND of thing (see `road_network`). Gated on the section's own name in the delta
	# manifest: the decoder republishes the whole section whenever any road moves and names it, so a
	# frame that does not name it carries the roads it already had.
	if SnapshotSections.changed(snapshot, SECTION_ROUTES):
		_ingest_road_network(snapshot.get("routes", []))
	profile.end(PROFILE_LAYERS_ROAD_NETWORK, t_layers_roads)
	profile.end(PROFILE_LAYERS, t_layers)
	var t_sites: int = profile.begin(PROFILE_SITES)
	# Four independent ingests, each now gated on the section IT reads and each clearing its own
	# lookups inside that gate. **The clear and the refill must stay together**: these blocks get
	# erasure for free by wiping the lookup first, so a gate that skipped the refill but not the
	# clear would publish an empty world — food markers, forage sites and harvest targets all gone.
	# They were also nested inside `if food_variant is Array`, which the file's own comment called
	# an accidental coupling; gating them separately forces the untangle, because a block must never
	# be gated on a key it does not read.
	if SnapshotSections.changed(snapshot, SECTION_FOOD_MODULES):
		var t_sites_food: int = profile.begin(PROFILE_SITES_FOOD)
		_ingest_food_modules(snapshot)
		profile.end(PROFILE_SITES_FOOD, t_sites_food)
	if SnapshotSections.changed(snapshot, SECTION_DISCOVERED_SITES):
		var t_sites_discovered: int = profile.begin(PROFILE_SITES_DISCOVERED)
		_ingest_discovered_sites(snapshot)
		profile.end(PROFILE_SITES_DISCOVERED, t_sites_discovered)
	if SnapshotSections.changed(snapshot, SECTION_FORAGE_PATCHES):
		var t_sites_forage: int = profile.begin(PROFILE_SITES_FORAGE)
		_ingest_forage_patches(snapshot)
		profile.end(PROFILE_SITES_FORAGE, t_sites_forage)
	# `populations` is named on essentially every turn (a cohort's size or morale always moves), so
	# this gate is not expected to skip. It is here for the same reason the others are: the two
	# lookups it clears are ITS lookups, and leaving the clear outside the gate is how the next
	# person accidentally wipes the harvest markers.
	if SnapshotSections.changed(snapshot, SECTION_POPULATIONS):
		var t_sites_populations: int = profile.begin(PROFILE_SITES_POPULATIONS)
		_ingest_population_sites(snapshot)
		profile.end(PROFILE_SITES_POPULATIONS, t_sites_populations)
	profile.end(PROFILE_SITES, t_sites)

	var t_tiles: int = profile.begin(PROFILE_TILES)
	# Two paths into the SAME per-tile ingest (`_ingest_tile`), which is what keeps them from
	# drifting: the full rebuild wipes the lookups and replays every row, the incremental one
	# replays only the rows the delta carried.
	#
	# The incremental path needs all four of these to hold, and each rules out a real case:
	# a manifest (so this is a delta from a decoder that publishes one, and `tile_updates` is the
	# sparse list rather than the whole world), no resize (a grid resize invalidates
	# `culture_layer_grid`'s indexing outright), lookups already built by a previous frame, and an
	# actual sparse list to walk. Anything else — full snapshot, resync, first frame, new world —
	# falls through to the full rebuild, which is the only thing that can establish the baseline.
	var tile_updates_variant: Variant = snapshot.get("tile_updates", null)
	var tiles_incremental: bool = (
		not dimensions_changed
		and SnapshotSections.has_manifest(snapshot)
		and _tile_lookups_ready()
		and tile_updates_variant is Array
	)
	if tiles_incremental:
		# 6.9 ms of full-grid loop to learn about ~600 changed rows out of 4,160. Skipped entirely
		# when the delta moved no tile at all.
		if SnapshotSections.changed(snapshot, SECTION_TILES):
			for entry in (tile_updates_variant as Array):
				if entry is Dictionary:
					_ingest_tile(entry)
	else:
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
					_ingest_tile(entry)
	profile.end(PROFILE_TILES, t_tiles)
	# Rebuild the Approach-B blend-shader splatmaps (id-map + FoW vis-map + elev-map + river-map) from the
	# new terrain/fog/elevation/river-edges. Runs AFTER the tile loop, not beside the terrain ingest above:
	# the river-map is built from `tile_river_edges`, which only exists once the tiles have been read.
	var t_shader: int = profile.begin(PROFILE_SHADER)
	# Six full-grid `PackedByteArray` rebuilds, 7.7 ms measured. Its inputs are terrain / fog /
	# elevation / the river + underlying-terrain masks and NOTHING else — which is why the decoder
	# reports `tiles.rivers` apart from `tiles`: 600 tiles moving their graze biomass must not force
	# the splatmaps to be rebuilt, and before the manifest existed there was no way to tell the two
	# apart.
	if dimensions_changed or SnapshotSections.any_changed(snapshot, SHADER_INPUT_SECTIONS):
		_terrain.rebuild_shader_maps()
	profile.end(PROFILE_SHADER, t_shader)
	var t_markers: int = profile.begin(PROFILE_MARKERS)
	_install_province_overlay()
	_rebuild_unit_markers(snapshot)
	_rebuild_herd_markers(snapshot)
	profile.end(PROFILE_MARKERS, t_markers)

	var t_tail: int = profile.begin(PROFILE_TAIL)
	if dimensions_changed:
		zoom_factor = 1.0
		pan_offset = Vector2.ZERO
		mouse_pan_active = false
		mouse_pan_button = -1
	bounds_dirty = dimensions_changed

	_update_layout_metrics()
	_clamp_pan_offset()
	queue_redraw()
	# **EVERY DEFERRED CHANNEL IS STALE AGAIN**, and this is the last moment it can be said: the
	# listener below re-asserts the painted channel, which is what REBUILDS the one the player is
	# actually holding. See `DEFERRED_OVERLAY_BUILDERS`.
	_reset_deferred_overlays()
	# BEFORE the legend: a listener that re-asserts a channel here wants the legend that follows to
	# describe the channel it just re-asserted, not the cleared one.
	overlay_channels_ingested.emit()
	_emit_overlay_legend()
	_minimap.update()

	# Built into a local first so the five `_average_overlay` full-grid passes land inside the tail
	# measurement rather than escaping it after the last `profile.end`.
	var metrics: Dictionary = {
		"unit_count": units.size(),
		"avg_sentiment": _average_overlay("sentiment"),
		"avg_corruption": _average_overlay("corruption"),
		"avg_culture": _average_overlay("culture"),
		"avg_military": _average_overlay("military"),
		"avg_crisis": _average_overlay("crisis"),
		"dimensions_changed": dimensions_changed,
		"active_overlay": active_overlay_key
	}
	profile.end(PROFILE_TAIL, t_tail)
	return metrics

## Are the per-tile lookups in a state the incremental path may patch rather than rebuild?
##
## `culture_layer_grid` is the honest witness: it is the one lookup sized to the grid, so a size
## that no longer matches means either nothing has been ingested yet or the grid moved under us —
## in both cases the sparse list cannot reconstruct what is missing.
func _tile_lookups_ready() -> bool:
	if grid_width <= 0 or grid_height <= 0:
		return false
	return culture_layer_grid.size() == grid_width * grid_height


## Fold ONE tile row into the per-tile lookups.
##
## **Every conditional insert has an explicit `erase` on its else branch, and that is not
## defensive — it is the whole difference between the two callers.** The full rebuild gets erasure
## for free by clearing the lookups first, so a tile whose graze capacity fell to 0 simply never
## gets re-added. The incremental path never clears, so without the `erase` a tile that LOST its
## pasture (or its river, or its habitability reading) would keep answering with the value it had
## the turn before, forever — stale entries accumulating silently, which is the same bug class as
## the frozen `tiles` array this arc started from. After a clear, `erase` on an absent key is a
## no-op, so the two paths produce byte-identical lookups from the same rows.
##
## A tile's `(x, y)` is fixed for the life of the world (the grid does not move; a resize forces the
## full rebuild), so `tile_lookup`'s entity → cell entry can be overwritten in place without
## orphaning the previous cell.
func _ingest_tile(tile_dict: Dictionary) -> void:
	var entity_id: int = int(tile_dict.get("entity", -1))
	if entity_id < 0:
		return
	var x: int = int(tile_dict.get("x", 0))
	var y: int = int(tile_dict.get("y", 0))
	var cell := Vector2i(x, y)
	tile_lookup[entity_id] = cell
	if tile_dict.has("habitability"):
		tile_habitability[cell] = float(tile_dict["habitability"])
	else:
		tile_habitability.erase(cell)
	if tile_dict.has("temperature"):
		tile_temperature[cell] = float(tile_dict["temperature"])
	else:
		tile_temperature.erase(cell)
	# Graze: only a tile whose biome actually carries pasture gets an entry (see `tile_graze`). A
	# zero-capacity tile is a *dead* one, and the Tile card must print nothing there rather than
	# "0 / 0" — so a tile whose capacity fell to zero must lose its entry, not keep a stale one.
	var graze_capacity: float = float(tile_dict.get("graze_capacity", 0.0))
	if graze_capacity > 0.0:
		tile_graze[cell] = {
			"biomass": float(tile_dict.get("graze_biomass", 0.0)),
			"capacity": graze_capacity,
			"phase": String(tile_dict.get("graze_ecology_phase", "")),
		}
	else:
		tile_graze.erase(cell)
	# Forage (human-food) potential — only tiles that carry any get an entry, so the barren zeros
	# (deep ocean/glacier/lava) don't drag the legend's "poorest" to 0.
	var forage_capacity: float = float(tile_dict.get("forage_capacity", 0.0))
	if forage_capacity > 0.0:
		tile_forage[cell] = forage_capacity
	else:
		tile_forage.erase(cell)
	var river_mask: int = int(tile_dict.get("river_edges", 0))
	if river_mask != 0:
		tile_river_edges[cell] = river_mask
	else:
		tile_river_edges.erase(cell)
	# Where a tributary hands over to a navigable trunk (nonzero on the trunk's FIRST hex only).
	var inflow_mask: int = int(tile_dict.get("river_inflow", 0))
	if inflow_mask != 0:
		tile_river_inflow[cell] = inflow_mask
	else:
		tile_river_inflow.erase(cell)
	# Which SIDES a navigable hex's channel flows out through — the sim's word on the trunk's path,
	# and the only thing that arms a trunk arm (see RIVER_CHANNEL_MASK).
	var channel_mask: int = int(tile_dict.get("river_channel", 0))
	if channel_mask != 0:
		tile_river_channel[cell] = channel_mask
	else:
		tile_river_channel.erase(cell)
	# The valley biome the river cut (== terrain on ordinary tiles). Only the shader's navigable pass
	# reads it, but store every tile that carries it so the navigable_underlying_map fills.
	if tile_dict.has("underlying_terrain"):
		tile_underlying_terrain[cell] = int(tile_dict["underlying_terrain"])
	else:
		tile_underlying_terrain.erase(cell)
	# Written per index rather than refilled: on the incremental path only the changed tiles' cells
	# move, and the grid is re-created (and re-filled with -1) by the full path whenever it resizes.
	if culture_layer_grid.size() > 0:
		if x >= 0 and x < grid_width and y >= 0 and y < grid_height:
			var index: int = y * grid_width + x
			if index >= 0 and index < culture_layer_grid.size():
				culture_layer_grid[index] = int(tile_dict.get("culture_layer", -1))


## Merge the frame's culture layers into `culture_layer_map` by id, then apply its removals.
##
## **The layer dictionaries are HELD BY REFERENCE, not copied.** Both readers
## (`_install_province_overlay`, `_resolve_province_for_layer`) only `get` off them, and a layer
## whose data moved arrives as a NEW dictionary in the republished array, which this loop overwrites
## — so a copy would buy nothing and cost an allocation per layer per turn. See
## `.claude/rules/client/turn-profiling.md` → "Snapshot sub-trees are held by reference".
func _ingest_culture_layers(snapshot: Dictionary) -> void:
	var culture_layers_variant: Variant = snapshot.get("culture_layers", null)
	if culture_layers_variant is Array:
		for layer_variant in culture_layers_variant:
			if layer_variant is Dictionary:
				var layer: Dictionary = layer_variant
				var id: int = int(layer.get("id", -1))
				if id >= 0:
					culture_layer_map[id] = layer
	var removed_layers_variant: Variant = snapshot.get("culture_layer_removed", null)
	if removed_layers_variant is Array:
		for raw_id in removed_layers_variant:
			var id := int(raw_id)
			if culture_layer_map.has(id):
				culture_layer_map.erase(id)


## Refill `food_sites` / `food_site_lookup` from the frame's food modules.
##
## **The one ingest here that must copy, and it copies SHALLOWLY.** It stamps `terrain_id` onto each
## row, and writing into a row we do not own would write into the decoder's cached world (see the
## rule file). A shallow `duplicate()` is enough because the stamp is a top-level key — nothing
## nested is touched — and it leaves the row's nested values shared rather than re-allocated.
func _ingest_food_modules(snapshot: Dictionary) -> void:
	food_sites = []
	food_site_lookup.clear()
	var food_variant: Variant = snapshot.get("food_modules", [])
	if not (food_variant is Array):
		return
	for entry in food_variant:
		if not (entry is Dictionary):
			continue
		var site: Dictionary = (entry as Dictionary).duplicate()
		food_sites.append(site)
		var x_site: int = int(site.get("x", -1))
		var y_site: int = int(site.get("y", -1))
		# Stamp the tile's terrain so both consumers (map marker + HUD Forage row) resolve the
		# terrain-aware FoodIcons.for_site split from one source and can't disagree (riverine_delta
		# splits fish↔reeds by terrain). Unconditional: for x<0 it's harmless (-1 → fish default).
		site["terrain_id"] = _terrain_id_at(x_site, y_site)
		if x_site >= 0 and y_site >= 0:
			food_site_lookup[Vector2i(x_site, y_site)] = site


## Refill `discovered_sites` / `discovered_site_lookup` with the PLAYER faction's wonder sites.
##
## Rows held by reference: `SecondaryMarkerRenderer.compute_slots` / `_wonder_renders` only read
## them, and nothing stamps anything on.
func _ingest_discovered_sites(snapshot: Dictionary) -> void:
	discovered_sites = []
	discovered_site_lookup.clear()
	var sites_variant: Variant = snapshot.get("discovered_sites", [])
	if not (sites_variant is Array):
		return
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
			var wsite: Dictionary = site_entry
			discovered_sites.append(wsite)
			var wx: int = int(wsite.get("x", -1))
			var wy: int = int(wsite.get("y", -1))
			if wx >= 0 and wy >= 0:
				discovered_site_lookup[Vector2i(wx, wy)] = wsite


## Refill `forage_patch_lookup` from the frame's forage patches.
##
## **The largest of the held-by-reference wins.** A patch row carries ~25 scalars *and* a nested
## `composition` array of per-species dictionaries, so the deep copy this replaced re-allocated the
## whole flora roster of every patch on the map, every turn it changed. `_tile_info_at` copies the
## values it wants out onto `tile_info`, and `HudBandLaborState.set_forage_patches` — fed the same
## array by `Main` — has always held these rows by reference.
func _ingest_forage_patches(snapshot: Dictionary) -> void:
	forage_patch_lookup.clear()
	var patch_variant: Variant = snapshot.get("forage_patches", [])
	if not (patch_variant is Array):
		return
	for entry in patch_variant:
		if not (entry is Dictionary):
			continue
		var patch: Dictionary = entry
		var px: int = int(patch.get("x", -1))
		var py: int = int(patch.get("y", -1))
		if px >= 0 and py >= 0:
			forage_patch_lookup[Vector2i(px, py)] = patch


## Refill `harvest_sites` / `scout_sites` from each cohort's harvest + scout targets.
##
## The harvest entry is copied for the same reason the food row is — it gets `module_label` stamped
## on — and shallowly, for the same reason. The scout entry is stamped with nothing, so it is held
## by reference.
func _ingest_population_sites(snapshot: Dictionary) -> void:
	harvest_sites.clear()
	scout_sites.clear()
	var population_variant: Variant = snapshot.get("populations", [])
	if not (population_variant is Array):
		return
	for entry in population_variant:
		if not (entry is Dictionary):
			continue
		var cohort: Dictionary = entry
		var harvest_variant: Variant = cohort.get("harvest", {})
		if harvest_variant is Dictionary:
			var hx := int((harvest_variant as Dictionary).get("target_x", -1))
			var hy := int((harvest_variant as Dictionary).get("target_y", -1))
			if hx >= 0 and hy >= 0:
				var harvest: Dictionary = (harvest_variant as Dictionary).duplicate()
				var key := Vector2i(hx, hy)
				harvest["module_label"] = _food_module_label(String(harvest.get("module", "")))
				var existing: Array = harvest_sites.get(key, [])
				existing.append(harvest)
				harvest_sites[key] = existing
		var scout_variant: Variant = cohort.get("scout", {})
		if scout_variant is Dictionary:
			var scout: Dictionary = scout_variant
			var sx := int(scout.get("target_x", -1))
			var sy := int(scout.get("target_y", -1))
			if sx >= 0 and sy >= 0:
				var scout_key := Vector2i(sx, sy)
				var scout_existing: Array = scout_sites.get(scout_key, [])
				scout_existing.append(scout)
				scout_sites[scout_key] = scout_existing


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
	_annotations.draw_terrain_highlight(radius, origin, viewport_size)
	# (No river draw here: Minor/Major rivers are painted by terrain_blend.gdshader's river pass, off the
	# per-tile river-edge mask — the water is drawn exactly on the edge the future crossing cost applies to.)
	_annotations.draw_crisis_annotations(radius, origin)

	# THE ROADS IN THE GROUND (arc #532), drawn HERE — above the tile tints and BENEATH every marker,
	# overlay ring and selection outline below. A road is infrastructure in the ground rather than
	# something standing on it, so nothing that stands on the map may be painted over by it; drawing
	# it with the annotations rather than at the end (where the ORDER-PATH routes go, two different
	# things — see `MapView.road_network`) is what puts it in that layer.
	_annotations.draw_road_network(radius, origin)

	# SECONDARY MARKER SLOTS ARE COMPUTED HERE, not beside the marker draws below, because the
	# worked-source marks dock a ring to the SOURCE's own marker and therefore need its slot before
	# they can draw. This is a PURE computation over `discovered_sites` / `food_sites` / `herds` /
	# `last_hex_radius`, none of which mutate during `_draw`, so hoisting it above the overlay pass is
	# behaviour-neutral for the marker draws that still read the result further down.
	_secondary_markers.compute_slots()

	# Every player band's worked sources — a ring on each source's OWN marker, bold for the selected
	# band and thin for the rest, plus a faint tile outline as the far-zoom/overflow fallback. NOT
	# selection-gated: this is what makes "what are my people doing" answerable without clicking.
	_band_overlays.draw_worked_source_marks(radius, origin)

	# Selected player band: its assignable reach (the three range borders), the band→herd links, the
	# optimistic pending overlay and the travel destination — the things SELECTION buys, on top of the
	# always-on marks above. Its per-source yield LABELS are the exception — they are queued here and
	# flushed at the very end of _draw (see _band_overlays.flush_yield_labels).
	_band_overlays.draw_band_work_highlights(radius, origin)

	# Selected herd: its grazing range (the ground that sets its carrying capacity), drawn over the
	# tile tints / Pasture overlay but under the herd markers so the animal still reads on top.
	_band_overlays.draw_herd_range_highlights(radius, origin)
	# Selected CORRALLED herd: its fenced pen footprint (a distinct enclosure tint). A corralled herd
	# draws no roam-range above, so exactly one of the two ever renders.
	_band_overlays.draw_pen_footprint_highlight(radius, origin)

	# Selected + hovered hex outlines: the TOPMOST tile border, drawn after every per-tile overlay
	# border above — each of those stamps an outline on EVERY tile of its disk, so a selection drawn
	# earlier is erased on any tile inside one. Still under the markers, so tokens read on top.
	_draw_tile_selection_highlight(radius, origin)

	_draw_supply_links(radius, origin)
	_band_markers.draw_primary_bands(radius, origin)

	# (Slots were computed above, before the worked-source marks that dock to them.)
	for herd in herds:
		_secondary_markers.draw_herd(herd, radius, origin)
	for site in food_sites:
		_secondary_markers.draw_food_site(site, radius, origin)
	for wsite in discovered_sites:
		_secondary_markers.draw_discovered_site(wsite, radius, origin)
	# The chip reports what the cap hid, so it needs the mark pass's roll-up (threaded across here so
	# neither renderer holds the other).
	_secondary_markers.set_hidden_source_state(_band_overlays.hidden_source_state())
	_secondary_markers.draw_secondary_overflow(radius, origin)

	_secondary_markers.draw_harvest_markers(radius, origin)
	_secondary_markers.draw_scout_markers(radius, origin)

	_annotations.draw_routes(radius, origin)

	_annotations.draw_targeting(radius, origin)

	# TOPMOST: the selected band's per-source yield labels, collected during the overlay renderer's
	# draw_band_work_highlights and held back to here. They annotate the map, so they must survive
	# every layer above the tile tints — herd/food glyphs, rings, band→herd links and the dashed
	# pending overlays all used to scribble across the text. This call MUST stay LAST.
	_band_overlays.flush_yield_labels()

	# Profiling output — same `[TurnProfile]` prefix and `label=ms` shape as the per-snapshot line,
	# so one grep collects the whole client-side turn picture.
	if _profiling_enabled:
		var elapsed: float = float(Time.get_ticks_usec() - _profile_start) / TurnProfile.USEC_PER_MSEC
		_draw_frame_times.append(elapsed)
		if _draw_frame_times.size() >= DRAW_PROFILE_WINDOW_FRAMES:
			var total: float = 0.0
			for t: float in _draw_frame_times:
				total += t
			var avg: float = total / _draw_frame_times.size()
			print("%s %s" % [TurnProfile.LINE_PREFIX, TurnProfile.ENTRY_FORMAT % ["draw.avg%d" % DRAW_PROFILE_WINDOW_FRAMES, avg]])
			_draw_frame_times.clear()

## Highlights all hexes of a given terrain id (Terrain-tab dropdown). Pass -1 to clear.
## THIN PASS-THROUGH to AnnotationRenderer, and the NAME cannot move: TerrainPanel.gd pushes it via
## has_method/call, so a rename would silently do nothing rather than error. The renderer reports
## whether the id actually changed, preserving the old setter's no-op early-out.
func set_terrain_highlight(terrain_id: int) -> void:
	if _annotations.set_terrain_highlight(terrain_id):
		queue_redraw()

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
	# **BUILD A DEFERRED CHANNEL BEFORE ANYTHING ELSE LOOKS AT IT**, including the `overlay_channels`
	# test below, which would otherwise refuse a channel this renderer has simply not built yet. It is
	# a table lookup that names no channel (`DEFERRED_OVERLAY_BUILDERS`), so it is not the second
	# `if key ==` §6b forbids, and every key not in that table falls straight through.
	_realize_deferred_overlay(key)
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

## Seat the RENDER CACHE for fog of war. `_fow_enabled` is not an authority — the sim owns
## `fog_enabled` and `Main._sync_fog_of_war` pushes it here off every snapshot (the early-out makes
## that per-turn call free). This stays a plain public setter only because the OFFLINE harnesses
## (`tools/map_preview.gd`, `tools/blend_probe.gd`) drive fog states with no server to ask.
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

func _unhandled_input(event: InputEvent) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	# A wheel or trackpad gesture over a card the player is scrolling must move THAT card and not the
	# map underneath it. The GUI pass stops a LEFT press for us and stops none of these three, so the
	# map declines them itself. It only DECLINES — the event is left unhandled, so whatever the pointer
	# is really over stays free to answer it. See `_pointer_claimed_by_ui`.
	if _is_pointer_navigation_input(event) and _pointer_claimed_by_ui():
		return
	# While a command is targeting, Esc / right-click back out of it (instead of
	# panning), matching the targeting-mode contract.
	if _annotations.is_targeting_active():
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
			_apply_zoom(MOUSE_ZOOM_STEP * ClientSettings.zoom_speed_multiplier, get_local_mouse_position())
			_mark_input_handled()
			return
		elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
			_apply_zoom(-MOUSE_ZOOM_STEP * ClientSettings.zoom_speed_multiplier, get_local_mouse_position())
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
		_apply_pan(-gesture.delta * ClientSettings.pan_speed_multiplier)
		_mark_input_handled()
	elif event is InputEventMagnifyGesture:
		var magnify: InputEventMagnifyGesture = event
		var amount: float = (magnify.factor - 1.0) * KEYBOARD_ZOOM_SPEED * ClientSettings.zoom_speed_multiplier
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

## WORLD BOUNDARY (`Main._reset_per_world_state`): the snapshot about to be applied describes a
## DIFFERENT world — a `new_game`, or a `map_size` rebuild, which regenerates in place with no scene
## reload. Everything `display_snapshot` clears-and-refills per snapshot heals itself and is
## deliberately absent here; what is listed below is the remainder, audited case by case:
##
##   • `herd_trails` — APPENDED per herd id and pruned only when an id is ABSENT from the snapshot,
##     so a herd id the new world happens to reuse inherits the old world's path and its trail leaps
##     across the map. This is the cache that made the bug visible.
##   • `culture_layer_map` — MERGED by layer id, erased only on an explicit `culture_layer_removed`,
##     so a layer id the new world reuses shows the old world's layer.
##   • the selection triplet + `cycle_index` — entity ids / a herd id / a tile belonging to the old
##     world. Left alone, `refresh_selection_payload` resolves them against the NEW world's arrays and
##     hands the HUD a different subject under the same id. Cleared silently rather than through
##     `selection_cleared`: `Main._apply_snapshot` ends in `_refresh_hud_selection`, which reads the
##     cleared state and drops the HUD card the same frame.
##   • the culture highlight and the labor-pending overlay — pushed IN from the Inspector and the HUD
##     respectively, both keyed by ids the new world reuses.
##   • the annotation family's world-keyed draw caches (`AnnotationRenderer.reset_world_state`).
##
## Not cleared, deliberately: `active_overlay_key`, the terrain highlight id
## and the texture/grid toggles are VIEW preferences (or keyed on stable terrain ids), not world data.
func reset_world_state() -> void:
	# PUSHED IN from the HUD and keyed by tracks the new world reuses — the third shape
	# `.claude/rules/core_sim/world-handoff.md` names as needing a clear. A new world knows nothing,
	# and stale knowledge here would mark its wild sources as ready to climb.
	# REBIND, never `.clear()`: this dict is the HUD's OWN row held BY REFERENCE
	# (`FactionReadouts.faction_tracks()` returns `_intensification_knowledge[faction]` uncopied,
	# `Hud` emits that same object on `faction_knowledge_changed`, `set_faction_knowledge` stores it
	# as-is), so clearing would reach back through the reference and empty the live knowledge strip.
	# Today only the call ORDER in `Main._reset_per_world_state` — HUD reset before MapView's —
	# masks that; dropping the reference is what makes it correct regardless of order. Same idiom as
	# `BandOverlayRenderer.reset_world_state`'s `_labor_pending = {}`.
	faction_knowledge = {}
	# The model derived FROM that row goes with it — and so does the copy the staleness test compares
	# against, or the first push into the new world would match the old world's row and be skipped.
	_ready_for_improvement = {}
	_ready_for_improvement_knowledge = {}
	_reset_deferred_overlays()
	herd_trails.clear()
	# The roads of a world we are about to stop showing. Both halves, together: the lookup holds the
	# same road dicts the array does, so clearing one alone would leave a hover answering off a world
	# that is gone.
	road_network = []
	road_tile_lookup = {}
	culture_layer_map.clear()
	selected_unit_id = -1
	selected_herd_id = ""
	selected_tile = Vector2i(-1, -1)
	cycle_index = 0
	highlighted_culture_layer_ids = PackedInt32Array()
	highlighted_culture_layer_set.clear()
	highlighted_culture_context = ""
	_annotations.reset_world_state()
	_band_overlays.reset_world_state()
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

## A connected tile path (a herd's migration trail, an order route) unwrapped into ONE continuous
## column frame, so a polyline through it follows the seam-crossing path that was actually walked
## instead of shooting the long way back across the whole map. `tiles` holds DATA columns — what a
## snapshot publishes, so a herd stepping over the seam records `95` then `0`, and a raw
## `_hex_center` per point draws a segment the full width of the map at nearly constant row.
##
## The frame is anchored on the LAST tile via
## `_band_effective_col` (the copy `_hex_center_wrapped` puts a MARKER on, so a trail's head lands
## on its herd) and every earlier step is placed by the SHORTEST wrapped delta, which is at most
## half a map width — so no segment CAN span the map and this needs none of the
## `0.4 * last_map_size.x` skip that the DISCONNECTED links use (supply links, the migration arrow,
## the band task arrow). A path that genuinely circles the world draws longer than one map width,
## which is the truth about it.
##
## **Do NOT wrap the points individually.** `_hex_center_wrapped` on each one snaps every point
## toward the viewport centre on its own and tears a seam-crossing path into two halves — the same
## reason the range disks walk `eff_col + delta` from a resolved anchor (see `map-renderers.md`
## → "THE SELECTION OUTLINE WRAPS": a data column wraps, a resolved one does not).
func _unwrapped_path_points(tiles: Array, radius: float, origin: Vector2) -> PackedVector2Array:
	var points := PackedVector2Array()
	if tiles.is_empty():
		return points
	points.resize(tiles.size())
	var last: int = tiles.size() - 1
	var eff_col: int = _band_effective_col(int(tiles[last].x), radius, origin)
	points[last] = _hex_center(eff_col, int(tiles[last].y), radius, origin)
	# Walk BACKWARDS off that anchor: subtract the forward delta i -> i+1 from the frame already placed.
	for i in range(last - 1, -1, -1):
		eff_col -= _wrapped_col_delta(int(tiles[i].x), int(tiles[i + 1].x))
		points[i] = _hex_center(eff_col, int(tiles[i].y), radius, origin)
	return points

func _fill_hex(col: int, row: int, radius: float, origin: Vector2, fill: Color) -> void:
	var center := _hex_center(col, row, radius, origin)
	var pts := _hex_points(center, radius)
	draw_polygon(pts, PackedColorArray([fill, fill, fill, fill, fill, fill]))

func _outline_hex(col: int, row: int, radius: float, origin: Vector2, color: Color, width: float) -> void:
	_outline_hex_at(_hex_center(col, row, radius, origin), radius, color, width)

## Seam-aware twin of `_outline_hex`: stamps the outline on the copy of `col` the viewport is
## actually over. Use it for a SINGLE tile named by its DATA column (the selection, the hover);
## `_outline_hex` stays right for a tile whose caller already resolved an effective column
## (`_band_effective_col`, the range disks) or that came off a logical-column draw loop.
func _outline_hex_wrapped(col: int, row: int, radius: float, origin: Vector2, color: Color, width: float) -> void:
	_outline_hex_at(_hex_center_wrapped(col, row, radius, origin), radius, color, width)

func _outline_hex_at(center: Vector2, radius: float, color: Color, width: float) -> void:
	var pts := _hex_points(center, radius)
	var outline := PackedVector2Array([pts[0], pts[1], pts[2], pts[3], pts[4], pts[5], pts[0]])
	draw_polyline(outline, color, width, true)

## White outline on the selected hex + a faint outline on the hovered hex (skipped when
## hover == selection). Replaces the old brown-circle-as-selection feel; the hex-shape
## outline is the sole selection cue — there is NO per-token ring, and the active band in a
## stack reads by full brightness over its darkened/shrunk back cards.
##
## **BOTH OUTLINES WRAP.** `selected_tile` / `_hovered_tile` hold DATA columns — `_point_to_offset`
## posmods the pick — while the terrain loop draws each column at whatever LOGICAL copy the
## viewport is over, so on a wrapping map an unwrapped outline is stamped a whole map-width away
## and the click reads as "the selection didn't take" on every tile the seam pushed into a copy.
func _draw_tile_selection_highlight(radius: float, origin: Vector2) -> void:
	if selected_tile.x >= 0 and selected_tile.y >= 0:
		_outline_hex_wrapped(selected_tile.x, selected_tile.y, radius, origin, SELECTED_HEX_OUTLINE_COLOR, SELECTED_HEX_OUTLINE_WIDTH)
	if _hovered_tile.x >= 0 and _hovered_tile.y >= 0 and _hovered_tile != selected_tile:
		_outline_hex_wrapped(_hovered_tile.x, _hovered_tile.y, radius, origin, HOVER_HEX_OUTLINE_COLOR, HOVER_HEX_OUTLINE_WIDTH)
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

		# **THE MARKER IS A STRUCTURAL COPY OF THE COHORT, NOT AN ALLOWLIST OF IT.**
		#
		# A band reaches the Band panel by TWO paths carrying two dicts: the per-snapshot refresh hands
		# over the decoder's cohort dict, a click on this marker hands over THIS copy. This used to be a
		# hand-listed literal naming ~45 of the cohort's keys, and it leaked THREE times — `hunt_mode`,
		# then `working_age`/`idle_workers`, then the Minimal TOE's six, the last of which made a band's
		# `Kit` row vanish when you clicked its map icon and took the ⚠ zero-effective-attack warning
		# silently with it. Every leak had the same shape: the decoder grew a field, the panel read it, and
		# nobody remembered this list. Enumerating what to KEEP cannot be made safe by care; enumerating
		# what to ADD can, because the addition is the thing being written.
		#
		# **`duplicate()` IS SHALLOW, AND THAT IS THE CORRECT DEPTH.** `duplicate(true)` would re-allocate
		# `labor_assignments` / `stores` / `harvest` / `scout` for every band every frame, which is the
		# per-turn cost `turn-profiling.md` spent a pass removing ("snapshot sub-trees are HELD BY
		# REFERENCE"). The four sub-trees that DO need isolating are re-stamped with their own deep copies
		# below, exactly as they always were, so nothing about nested aliasing moves here and
		# `snapshot_alias_guard`'s "MapView must not write into the decoder's cached world" is untouched:
		# every stamp below lands in this copy's own top level.
		#
		# **IT PRESERVES ABSENCE, which is the property two readouts depend on.** `duplicate()` reproduces
		# the cohort's key set exactly — present stays present and ABSENT STAYS ABSENT — so
		# `DetailFormat.band_states_kit` (a bare `has()`) and `SourceForecast.hunt_gate_model` (which
		# early-returns blank without `hunter_attack`, so a defaulted `attack 0` cannot refuse every hunt in
		# the game) keep their tests and now get the SAME answer on both paths. The hand list was what
		# destroyed absence semantics, by dropping keys the cohort actually had.
		#
		# **NO COERCIONS RIDE THE COPY.** The literal wrapped every field in `int()` / `float()` / `String()`,
		# which is where the age-bracket narrowing bug lived; a duplicate carries the decoder's own types,
		# so nothing can narrow in transit. The coercions that survive are on the STAMPS below, and they
		# defend against a hand-built FIXTURE rather than against the decoder (see each one).
		var marker := entry.duplicate()

		# --- THE MAP-ONLY STAMPS: everything the marker has that the cohort does not ------------------
		# Keep `marker_field_guard.MARKER_STAMPED_KEYS` in step with these — it asserts the partition is
		# total, so a stamp added here and not named there fails the guard rather than the eye.
		#
		# The RESOLVED tile. The cohort carries `current_x`/`current_y`, which may be absent or negative
		# for a band that has never moved; `pos` is those resolved through the home-tile fallback above,
		# and it is what every map draw and the drawer's "Position:" row read.
		marker["pos"] = [current_x, current_y]
		# The DE-DUPLICATED display name. The cohort's own `label` can be empty or repeated across bands;
		# `id` is this run's unique, stable-per-frame name ("Band 3"), which the Occupants drawer titles
		# itself with. `label` survives the copy untouched beside it.
		marker["id"] = label

		# --- SUB-TREE ISOLATION, unchanged from the literal this replaced ----------------------------
		# The shallow copy shares these two Array/Dictionary instances with the decoder's cached frame,
		# so they are re-stamped with their own deep copies exactly as they always were. This is the ONE
		# thing `duplicate(true)` on the whole dict would have bought, and buying it here instead keeps
		# the cost to the sub-trees that need it rather than every sub-tree on the cohort.
		var assignments_variant: Variant = entry.get("labor_assignments", [])
		if assignments_variant is Array:
			marker["labor_assignments"] = (assignments_variant as Array).duplicate(true)
		var stores_variant: Variant = entry.get("stores", {})
		if stores_variant is Dictionary:
			marker["stores"] = (stores_variant as Dictionary).duplicate(true)

		# The travel DESTINATION, derived from whichever task sub-tree the band actually has — harvest
		# first, scout as the fallback. **Gated on `has`, not on `get(…, {}) is Dictionary`**: the old
		# spelling took its own empty default down the branch, so every band on the map carried a
		# fabricated `harvest: {}` plus a `dest_x: -1` / `travel_task_kind: "harvest"` for a journey it
		# was not making. Both readers already treat a negative destination as "none"
		# (`BandMarkerRenderer._draw_travel_destination`), so absence reads identically and the marker
		# stops claiming a task sub-tree its cohort never had.
		#
		# The `duplicate(true)` on each is the sub-tree isolation above, for the same reason.
		if entry.get("harvest", null) is Dictionary:
			var harvest: Dictionary = entry["harvest"]
			marker["harvest"] = harvest.duplicate(true)
			marker["dest_x"] = int(harvest.get("target_x", -1))
			marker["dest_y"] = int(harvest.get("target_y", -1))
			marker["travel_task_kind"] = String(harvest.get("kind", "harvest"))
		if entry.get("scout", null) is Dictionary:
			var scout: Dictionary = entry["scout"]
			marker["scout"] = scout.duplicate(true)
			if int(marker.get("dest_x", -1)) < 0:
				marker["dest_x"] = int(scout.get("target_x", -1))
				marker["dest_y"] = int(scout.get("target_y", -1))
				marker["travel_task_kind"] = "scout"
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
## occupied hex). `selected_tile` is deliberately untouched — the land IS that tile — while
## `cycle_index` follows the pick, so re-clicking the hex on the map continues the cycle from it.
func select_occupant(kind: String, id) -> void:
	if kind == OCCUPANT_KIND_UNIT:
		selected_unit_id = int(id)
		selected_herd_id = ""
		# Surface the roster-picked band as the active stack card, and seed cycling from it.
		cycle_index = _occupant_cycle_index(kind, id)
	elif kind == OCCUPANT_KIND_HERD:
		selected_herd_id = String(id)
		selected_unit_id = -1
		cycle_index = _occupant_cycle_index(kind, id)
	elif kind == OCCUPANT_KIND_LAND:
		selected_unit_id = -1
		selected_herd_id = ""
		# The land is the cycle's LAST stop, so seeding from it makes the next map re-click advance
		# to the top occupant — the same coherence a roster band/herd row already gets.
		cycle_index = _occupant_cycle_index(kind, id)
	queue_redraw()

## The subject's position within its own tile's selection cycle — so a roster selection shows it
## as the active card and map re-click cycling continues from it. Covers ALL THREE kinds: a roster
## herd click leaves `cycle_index` pointing at that herd, and a land-row click at the land, so the
## next map click on the hex advances to the member after it rather than restarting at the top of
## the cycle. Returns 0 if not found.
func _occupant_cycle_index(kind: String, id) -> int:
	var tile := _occupant_home_tile(kind, id)
	if tile.x < 0 or tile.y < 0:
		return 0
	var cycle := _selection_cycle_on_tile(tile.x, tile.y)
	for i in range(cycle.size()):
		if _occupant_matches(cycle[i] as Dictionary, kind, id):
			return i
	return 0

## The hex a selection subject sits on, read from the unfiltered source arrays (`units`/`herds`) so
## the lookup works from an id alone. The LAND carries no id — it IS the selected hex — so it
## answers `selected_tile`. `(-1, -1)` when the subject is unknown or carries no position.
func _occupant_home_tile(kind: String, id) -> Vector2i:
	if kind == OCCUPANT_KIND_LAND:
		return selected_tile
	if kind == OCCUPANT_KIND_UNIT:
		for unit in units:
			if int((unit as Dictionary).get("entity", -1)) != int(id):
				continue
			var pos: Array = Array((unit as Dictionary).get("pos", []))
			if pos.size() != 2:
				return Vector2i(-1, -1)
			return Vector2i(int(pos[0]), int(pos[1]))
	elif kind == OCCUPANT_KIND_HERD:
		for herd in herds:
			if String((herd as Dictionary).get("id", "")) != String(id):
				continue
			return Vector2i(int((herd as Dictionary).get("x", -1)), int((herd as Dictionary).get("y", -1)))
	return Vector2i(-1, -1)

## Does this cycle entry name the `(kind, id)` subject? The one place the two identity vocabularies
## (a band's int `entity`, a herd's String `id`) are compared. The LAND has NO id — a hex holds
## exactly one land entry — so matching its kind is the whole test.
func _occupant_matches(entry: Dictionary, kind: String, id) -> bool:
	if String(entry.get(OCCUPANT_KEY_KIND, "")) != kind:
		return false
	if kind == OCCUPANT_KIND_LAND:
		return true
	var data: Dictionary = entry.get(OCCUPANT_KEY_DATA, {})
	if kind == OCCUPANT_KIND_UNIT:
		return int(data.get("entity", -1)) == int(id)
	return String(data.get("id", "")) == String(id)

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

## Select ONE member of the hex's cycle — `occupant_index` picks which of `occupants` (bands, then
## herds, then the LAND), and becomes the stored `cycle_index`. BOTH the list and the index are
## PARAMETERS rather than reads of `_selection_cycle_on_tile`/`cycle_index`, because the caller's
## `_emit_tile_selection` runs first and can re-enter `select_occupant` synchronously (the HUD's
## fresh-hex auto-pick relays `roster_occupant_selected` → `Main` → `select_occupant`), which
## rewrites `cycle_index` to the FIRST occupant mid-click. Carrying the click's own list and index
## through the call makes the selection immune to that re-entrancy, and guarantees the index is
## applied to the very list it was computed against.
func _handle_entity_selection(col: int, row: int, occupants: Array, occupant_index: int) -> void:
	if not occupants.is_empty():
		# Select-then-cycle: the index picks which member of the cycle is active.
		cycle_index = clampi(occupant_index, 0, occupants.size() - 1)
		var entry: Dictionary = occupants[cycle_index]
		var data: Dictionary = entry.get(OCCUPANT_KEY_DATA, {})
		var kind := String(entry.get(OCCUPANT_KEY_KIND, ""))
		if kind == OCCUPANT_KIND_LAND:
			# The LAND stop of an occupied hex. It emits NEITHER occupant signal — there is no
			# occupant — and clears both ids, which is what makes the next snapshot's
			# `refresh_selection_payload` answer `kind: "tile"`. `land_selected` is what tells the HUD
			# this was CHOSEN rather than merely emptied; without it the HUD's fresh-hex auto-pick
			# takes the selection straight back to the first band and the land stop is invisible.
			selected_unit_id = -1
			selected_herd_id = ""
			emit_signal("land_selected")
			queue_redraw()
			return
		# The payload IS the entry's data, uncopied: `_units_on_tile`/`_herds_on_tile` already made
		# each entry a private deep copy, and `occupants` is a click-local temporary discarded when
		# the click returns. So stamping `tile_info` below mutates nothing the decoder or any other
		# surface holds — the "never write into a held snapshot sub-tree" rule is satisfied by that
		# first copy.
		if kind == OCCUPANT_KIND_UNIT:
			selected_unit_id = int(data.get("entity", -1))
			selected_herd_id = ""
			var unit_payload: Dictionary = data
			var pos := Array(unit_payload.get("pos", []))
			var unit_col := col
			var unit_row := row
			if pos.size() == 2:
				unit_col = int(pos[0])
				unit_row = int(pos[1])
			unit_payload["tile_info"] = _tile_info_at(unit_col, unit_row)
			emit_signal("unit_selected", unit_payload)
		else:
			selected_unit_id = -1
			selected_herd_id = String(data.get("id", ""))
			var herd_payload: Dictionary = data
			var herd_col: int = int(herd_payload.get("x", col))
			var herd_row: int = int(herd_payload.get("y", row))
			herd_payload["tile_info"] = _tile_info_at(herd_col, herd_row)
			emit_signal("herd_selected", herd_payload)
		queue_redraw()
		return

	cycle_index = 0
	if selected_unit_id != -1 or selected_herd_id != "":
		selected_unit_id = -1
		selected_herd_id = ""
		emit_signal("selection_cleared")
		# The OCCUPANT selection clears; the TILE selection does NOT. The click that reached here ran
		# _emit_tile_selection one call earlier and selected this hex, and the land card is what the hex
		# falls back to (refresh_selection_payload → {"kind": "tile"}, Hud.clear_selection → select_land).
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
	var tiles: Array = []
	for tile in trail:
		if tile is Vector2i:
			tiles.append(tile)
	if tiles.size() < 2:
		return
	# The trail holds DATA columns, so it MUST be unwrapped into one frame before it is connected —
	# see `_unwrapped_path_points`.
	draw_polyline(_unwrapped_path_points(tiles, radius, origin), HERD_TRAIL_COLOR, HERD_TRAIL_WIDTH)

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
	# Pasture (graze). SPLIT across FOW_DISCOVERED_HIDDEN_KEYS rather than kept whole: `graze_capacity`
	# is a property of the GROUND — you can read a steppe's carrying capacity from a ridge, and the
	# biome above it is already remembered — while `graze_biomass` and the phase derived from it are
	# live stock, drawn down every turn by herds a remembered tile cannot see. The forage patch below
	# is split on the same line, which is the point: the two webs are stocks on one piece of ground and
	# a rule that separates them separates a stock from its capacity, never one web from the other
	# (issue #462).
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
		# **`patch_`-PREFIXED, like every other patch field** (issue #442). This pair was the ONE
		# exception in this cross-ref — stamped bare while its rung-3 twins `patch_field_progress` /
		# `patch_is_field` were prefixed — and that inconsistency had to be written down in
		# `RungGates.forage_gates_from_patch` and re-learned by every reader. `SourceForecast`'s
		# improvement helpers spell a key as `prefix + name` uniformly, so the exception would have
		# meant a done-test that silently answered "not built" on a tended patch.
		info["patch_cultivation_progress"] = float(patch.get("cultivation_progress", 0.0))
		info["patch_is_cultivated"] = bool(patch.get("is_cultivated", false))
		info["patch_has_owner"] = bool(patch.get("has_owner", false))
		info["patch_owner"] = int(patch.get("owner", 0))
		info["patch_ecology_phase"] = String(patch.get("ecology_phase", ""))
		# Standing forage stock vs the patch's ceiling — "how much there is", the patch
		# counterpart to a herd's Biomass row (Hud._tile_terrain_lines renders both).
		info["patch_biomass"] = float(patch.get("biomass", 0.0))
		info["patch_carrying_capacity"] = float(patch.get("carrying_capacity", 0.0))
		# …AND THE GROUND UNDER IT — the tile's own `K`, with no `field_capacity_gain` folded in. The
		# ceiling above moves when the player builds, so it is redacted on a remembered hex and THIS is
		# what such a card states (`FOW_DISCOVERED_HIDDEN_KEYS` header). Both cross: the client cannot
		# derive either from the other, holding neither the gain nor the ladder position it
		# interpolates on. `DetailFormat.patch_capacity` is the ONE reader that picks between them.
		info["patch_tile_capacity"] = float(patch.get("tile_capacity", 0.0))
		# WHERE THE PHASE WORD ABOVE CHANGES HANDS — `classify_ecology_phase`'s own cut points, as
		# fractions of `patch_carrying_capacity`, i.e. the units the escapement floor is in. The harvest
		# floor chart draws them as horizontal zones BEHIND the floor line, which is only honest because
		# the two share an axis (`SourceForecast.phase_zones`).
		info["patch_collapse_fraction"] = float(patch.get("collapse_fraction", 0.0))
		info["patch_stressed_fraction"] = float(patch.get("stressed_fraction", 0.0))
		# THE TWO GROWTH TERMS THE WHOLE FLOOR INSTRUMENT RESTS ON, and they were decoded off the wire
		# but never carried across to `tile_info` — where every forage compose sheet reads its patch. The
		# omission was invisible in the preview harnesses (their fixture adapter seeds both), and against
		# a live sim it silently removed the chart from every patch: `floor_chart_model` answers
		# `known == false` without a curve, and a missing throughput prices no crew, so both worker
		# targets vanished too. A patch's `per_worker_biomass` folds the tile's seasonal weight in, so its
		# `0` in a dead season is a reading; `regrowth_samples` is the sim's own sampled curve, never
		# re-fitted here.
		info["patch_per_worker_biomass"] = float(patch.get("per_worker_biomass", 0.0))
		info["patch_regrowth_samples"] = patch.get("regrowth_samples", PackedFloat32Array())
		# Pre-commit yield forecast (food/turn at the patch's current biomass, at
		# output_multiplier 1.0). Read by Hud._build_forage_assign_controls to show the live
		# "Expected yield" row and to cap the forager stepper at the patch's max-useful workers.
		info["patch_per_worker_yield"] = float(patch.get("per_worker_yield", 0.0))
		# The Cultivate investment rung: the dip yield while the patch is being prepared, and the
		# tended yield it pays afterwards. Hud._build_forage_assign_controls turns the pair into the
		# pre-commit "Preparing: +X → then +Y" forecast.
		info["patch_tended_yield"] = float(patch.get("tended_yield", 0.0))
		# Plant RUNG 3 — the Field + the Sow verb (the twin of the herd's Corral block). The patch
		# carries TWO independent build meters: `cultivation_progress` (rung 2, above) and
		# `field_progress` here. Hud._tile_terrain_lines renders the meters; the Sow forecast pair
		# drives `_build_forage_assign_controls`' "Preparing: +X → then +Y", exactly as the Cultivate
		# pair does one rung down.
		info["patch_field_progress"] = float(patch.get("field_progress", 0.0))
		info["patch_is_field"] = bool(patch.get("is_field", false))
		info["patch_field_yield"] = float(patch.get("field_yield", 0.0))
		# THE BUILD, PRICED IN WORK (docs/plan_unit_costed_work.md §8) — the two plant rungs' absolutes
		# beside the fractions above, plus the ONE turn estimate and gear saving the source carries (at
		# most one improvement is ever in flight on one patch). `work_done / work_cost` IS the
		# `*_progress` fraction; both travel because the tile card and the compose sheet state the
		# SIZE of the job, which a fraction structurally cannot. The COST rides even on a patch nobody
		# is building: it is the resolved price of that job here, which is what lets the sheet quote a
		# rung before the player commits.
		info["patch_cultivation_work_done"] = float(patch.get("cultivation_work_done", 0.0))
		info["patch_cultivation_work_cost"] = float(patch.get("cultivation_work_cost", 0.0))
		info["patch_field_work_done"] = float(patch.get("field_work_done", 0.0))
		info["patch_field_work_cost"] = float(patch.get("field_work_cost", 0.0))
		# **THE NEGATIVES MUST SURVIVE THE COPY AS THEMSELVES** — `-1` no estimate, `-2` the meter
		# holds, `-3` the meter rots, `-4` the queue is blocked on it, `-5` it is queued and the sim has
		# not looked yet. A `0` default here would hand every unworked patch a "this build lands next
		# turn" reading, and the int cast is what keeps the whole family intact; nothing on this path may
		# collapse one negative into another (`SourceForecast.build_turns_remaining`), which is what put
		# the `⚠ Stalled` hazard on a build queued one command ago.
		info["patch_build_turns_remaining"] = int(patch.get(
			"build_turns_remaining", SourceForecast.BUILD_TURNS_NO_ESTIMATE))
		info["patch_build_work_from_gear"] = float(patch.get("build_work_from_gear", 0.0))
		# **WHERE THIS PATCH SITS IN THE WINNING BAND'S QUEUE** — 0-based, `-1` = queued nowhere. The
		# countdown two lines up is a CHAINED date (everything ahead of this entry plus its own span
		# at the full builders pool), and this is what makes that number explicable. Its default is
		# the sentinel, never `0`, which would put every unqueued patch at the head of a queue.
		info["patch_build_queue_position"] = int(patch.get(
			"build_queue_position", SourceForecast.NOT_IN_ANY_BUILD_QUEUE))
		# **WHY THAT QUEUE IS BLOCKED HERE** — `""` when this patch is not a blocked build, else the
		# sim's own cause key for the conjunct that refused (`escapement`, `knowledge`, `no_crop`,
		# `site`, …). It crosses BESIDE the `-4` it explains: a countdown sentinel with no cause beside
		# it is the state the field exists to end, and the client cannot re-derive the gate.
		info["patch_build_blocked_reason"] = String(patch.get("build_blocked_reason", ""))
		# **WHAT THIS PATCH'S BUILD IS BEING RAISED WITH** — the builders kit the winning band's queue
		# entry RESOLVES to, `""` when nobody has it queued. It rides the queue position and the cause
		# above because it is the same entry's property. **This line was MISSING while the decoder
		# emitted the key**, which is the plant web's second-wiring bug for the fourth time: the
		# compose sheet reads `build_kit_id` out of `tile_info` and there only, so every forage build
		# read as carrying no kit at all.
		info["patch_build_kit_id"] = String(patch.get("build_kit_id", ""))
		# **WHERE THE ENTRY IS TAKING THIS PATCH, AND WHAT IS LEFT OF THE CLIMB** (§2.8). A queue entry
		# names a DESTINATION rung rather than a single rung, so a `sow` declared on untended ground is
		# a two-leg climb that holds the head of the queue through its Cultivate leg. The legs travel
		# WHOLE — each row's `work_remaining` is the leg's owing from where the patch stands NOW and
		# each `turns_remaining` is chained behind the legs above it, so nothing on this path may
		# narrow, re-order or re-derive them.
		info["patch_build_destination_rung"] = String(patch.get("build_destination_rung", ""))
		# **WHERE THE PATCH STANDS RIGHT NOW**, in the destination's own `<branch>:<id>` spelling —
		# the one field a consumer asks instead of reading this web's private `is_cultivated` /
		# `is_field` pair, so a third food web costs it nothing. Read it BESIDE the destination: that
		# one is a fact about a band's declared job and is `""` when nobody queued this patch, while
		# every patch stands on a rung. The `""` default is the honest answer for a fixture the wire
		# never touched; `SourceForecast.rung_above_branch_floor` reads an unnamed rung as "not
		# improved" rather than guessing which rung was meant.
		info["patch_current_rung"] = String(patch.get("current_rung", ""))
		# **WHAT THIS PATCH WILL CARRY AT THAT DESTINATION** — the ceiling the rung buys, which is what
		# says why the take falls while the build runs: the escapement floor is a fraction of `K` and the
		# rung raises `K`, so the floor climbs underneath the player every turn. Its default is
		# `NO_BUILD_DESTINATION_CAPACITY` and never `0`, which would tell the player that improving any
		# unqueued patch would leave it holding nothing.
		info["patch_build_destination_capacity"] = float(patch.get("build_destination_capacity",
			SourceForecast.NO_BUILD_DESTINATION_CAPACITY))
		info["patch_build_legs"] = patch.get("build_legs", [])
		# **THE ESTIMATE'S PER-SOURCE TERM, so the compose sheet can price a crew the player is
		# PROPOSING.** The turn count above is the sim's answer for the crew already here; this is
		# what the sheet's stepper and floor slider evaluate `turns(workers)` from — see
		# `SourceForecast.build_turns_at`, which takes the gear half off the band's kit row. It
		# defaults to zero work, which that form reads as "no estimate" rather than as a build about
		# to land.
		info["patch_build_work_per_worker_turn"] = float(patch.get(
			"build_work_per_worker_turn", SourceForecast.BUILD_WORK_NONE))
		# WHY this ground will not take seed ("" = it will). The client cannot re-derive this — it has
		# neither the per-biome capacity table nor the hydrology — so the sim ships the reason itself.
		info["patch_sow_site_refusal"] = String(patch.get("sow_site_refusal", ""))
		# THE TILE'S PER-BIOMASS YIELD VECTOR (docs/plan_harvest_floor.md §5) — what ONE UNIT of this
		# patch's standing crop is worth in each account, at the patch's own basket-averaged rates.
		# **This is the patch's whole ceiling representation now**: with `patch_biomass` and
		# `patch_carrying_capacity` above — the PATCH's ceiling, since the floor is a fraction of the
		# stand actually standing here — the client composes the ceiling at ANY floor
		# (`SourceForecast.escapement_room`). The six per-policy row dicts it replaced — and the six
		# flat `patch_ceiling_*` scalars before them — are retired `(deprecated)` wire slots, so nothing
		# can read one representation while the sim pays the other. The vector's PRESENCE is what tells
		# `SourceForecast` "the wire describes this source" apart from "the source pays nothing at this
		# floor" — the #426 distinction, now answered by a rate rather than a row.
		info["patch_provisions_per_biomass"] = float(patch.get("provisions_per_biomass", 0.0))
		info["patch_fodder_per_biomass"] = float(patch.get("fodder_per_biomass", 0.0))
		# THE THIRD ACCOUNT, AS A VECTOR — what one unit of this patch's crop is made of, and what one
		# gatherer brings home per turn, per material. They are the plant twins of the herd's pair and
		# they cross here for the reason `patch_per_worker_biomass` does: the compose sheet composes
		# `min(workers × per_worker, ceiling(floor))` per material off `tile_info`, so a decoded field
		# this list omits is silently absent on the PLANT web while the animal web reads it fine (a
		# herd dict travels whole). Reported from play on a 56% tobacco tile whose PER TURN box named
		# the fodder and never the tobacco — the third time an appended patch field reached the panel
		# through only one of its two wirings. `patch_crossref_guard` is what makes it the last.
		# Never summed into one "materials/turn" figure: that is the retired trade axis under a new
		# name (`SourceForecast.material_rows_of`).
		info["patch_material_per_biomass"] = patch.get("material_per_biomass", [])
		info["patch_per_worker_material"] = patch.get("per_worker_material", [])
		# The two investment rungs' FODDER payoff twins, each quoted at ITS OWN rung (#433). Their
		# `*_trade` siblings went with arc #527's yield axis; a cash crop's payoff is now a per-material
		# vector on the COMPOSITION entry, which travels whole in `patch_composition`.
		info["patch_tended_fodder"] = float(patch.get("tended_fodder", 0.0))
		info["patch_field_fodder"] = float(patch.get("field_fodder", 0.0))
		# THE STANDING UPKEEP (`docs/plan_standing_upkeep.md` §2) — the four numbers that say what this
		# patch costs to HOLD: the rung's per-turn demand, what the keepers supplied out of this turn's
		# budget, what went unmet (**the sim's own field — never `demand − supplied` here**, since the
		# shortfall IS what the meter decays by) and the hands that would meet the demand.
		# `SourceForecast.upkeep_state` is the ONE reader of the four.
		info["patch_upkeep_demand"] = float(patch.get("upkeep_demand", 0.0))
		info["patch_upkeep_supplied"] = float(patch.get("upkeep_supplied", 0.0))
		info["patch_upkeep_shortfall"] = float(patch.get("upkeep_shortfall", 0.0))
		info["patch_upkeep_workers_needed"] = int(patch.get("upkeep_workers_needed", 0))
		# **THE PRE-COMMIT RATE, PER RUNG** — what holding each plant rung costs per turn, published
		# whether or not a build is running (the `*_work_cost` rule). The compose sheet's closed form
		# nets the BUILD crew's output against the rate of the rung it is pricing; the source-level
		# `patch_upkeep_demand` above is `0` on a patch with nothing started, which is what made the
		# stepper quote a finish date for a build that could never advance.
		#
		# **Deliberately NOT in `FOW_DISCOVERED_HIDDEN_KEYS`, and the reason had to be re-argued when
		# both plant rungs moved onto `scaled_by: source_load`.** The pair is no longer the ladder's
		# bare number — it is struck through this patch's own TENDER-LOAD, so it differs patch to
		# patch. What keeps it fog-safe is that the load is `tile forage capacity /
		# capacity_per_tender`: a pure function of the tile's TERRAIN, which a Discovered tile
		# remembers by definition, so the figure sent for an unseen hex is the figure that hex last
		# showed. That is the same argument `patch_tile_capacity` rides on — and the reason
		# `patch_carrying_capacity` beside it IS redacted, since that one carries the Field rung's
		# own gain. The second reason this pair used to carry is gone: the closed form no longer reads it at all (`docs/plan_standing_upkeep.md`
		# §2.4 — the keeping pool owes the rate at every fullness, so a build crew nets the ROT
		# instead), so redacting it could no longer cost the estimate its term. The surviving reason
		# stands on its own, and the field is a PRICE on the offered face rather than a term.
		info["patch_cultivation_upkeep_demand"] = float(patch.get("cultivation_upkeep_demand", 0.0))
		info["patch_field_upkeep_demand"] = float(patch.get("field_upkeep_demand", 0.0))
		# **WHAT THE AT-RISK METER IS LOSING PER TURN** — the term the compose sheet's closed form
		# nets. Unlike the pair above it IS live patch state: it exists only because this band's keeping
		# pool came up short past the rung's grace, so it is redacted with the shortfall it is derived
		# from. Nothing is lost by that — a remembered tile's whole build payload is redacted, so the
		# estimate already answers `BUILD_TURNS_NO_ESTIMATE` there for want of a cost.
		info["patch_meter_rot_per_turn"] = float(patch.get("meter_rot_per_turn", 0.0))
		# THE NEGLECT GRACE — the COUNTDOWN to the ground reverting, with its own presence bool.
		# `has_neglect_grace == false` means nothing is built here to lose (the common case, a wild
		# patch), and it is what keeps the honest "reverting NOW" zero from reading as "nothing at
		# risk": every reader tests the bool BEFORE the number. Both travel `patch_`-prefixed like the
		# rest of the payload.
		info["patch_has_neglect_grace"] = bool(patch.get("has_neglect_grace", false))
		info["patch_neglect_grace_remaining"] = int(patch.get("neglect_grace_remaining", 0))
		# WHAT GROWS HERE — the tile's named plant composition (share-descending, already sorted
		# server-side; never re-sorted here). It is the patch's STANDING basket: seeded from the
		# biome, then REWEIGHTED as a commitment's build lands (issue #433 — a Tended Patch weeds the
		# favored share up, a Field takes the tile whole). Deliberately NOT in
		# FOW_DISCOVERED_HIDDEN_KEYS: what a tile grows is ground knowledge like the terrain label or
		# the river edges, so a remembered tile still knows it — at worst it remembers the mix as it
		# last stood. Never-seen tiles are covered by the `unexplored` redaction.
		info["patch_composition"] = patch.get("composition", [])
		# THE COMMITTED CROP — "" while nothing has been committed here, else the single species this
		# patch was committed to by Cultivate/Sow. It rides BESIDE the composition rather than
		# replacing it: the commitment is recorded on the first worked turn, ~25 turns before the
		# basket above it moves at all, so the two answer different questions and the tile card renders
		# both rows. Unlike the composition it IS patch state (a band's doing), but the Forage line it
		# sits under is already past the discovered early-return, so a remembered tile never reports it
		# and it needs no FOW_DISCOVERED_HIDDEN_KEYS entry.
		info["patch_committed_species"] = String(patch.get("committed_species", ""))
		info["patch_committed_display_name"] = String(patch.get("committed_display_name", ""))
	# THE ROADS CROSSING THIS HEX (arc #532) — the tile card's road readout reads its rows out of
	# here and nowhere else, the forage patch's own cross-ref idiom. Stamped BEFORE the fog split
	# below and deliberately NOT in `FOW_DISCOVERED_HIDDEN_KEYS`: a road is permanent geography like
	# the terrain label and the river edges, so a remembered hex still reports the road that crosses
	# it — which is exactly the `Discovered` gate the sim publishes these rows under.
	info["roads"] = _roads_on_tile(col, row)
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

## **INGEST THE ROAD NETWORK** — the `routes` section, kept whole for the map draw and indexed by
## tile for the tile card. A whole-section replace on both wire paths, so this rebuilds both from
## scratch: a road that reverted to nothing is PRUNED sim-side and simply stops arriving, and a
## merge that kept stale entries would leave a road drawn on the map for the life of the world.
##
## The path arrives as the wire's own two packed halves (`path_x` / `path_y`, zipped by index), which
## is what the decoder carries rather than N sub-arrays; it is zipped ONCE here, into the `Vector2i`
## tiles both consumers want.
func _ingest_road_network(raw: Variant) -> void:
	road_network = []
	road_tile_lookup = {}
	if not (raw is Array):
		return
	for entry in raw:
		if not (entry is Dictionary):
			continue
		var road: Dictionary = (entry as Dictionary).duplicate(true)
		var xs := PackedInt32Array(road.get("path_x", PackedInt32Array()))
		var ys := PackedInt32Array(road.get("path_y", PackedInt32Array()))
		var tiles: Array[Vector2i] = []
		# **THE SHORTER HALF BOUNDS THE WALK.** The two are the same length by construction on the
		# wire; taking the min rather than either one is what keeps a truncated frame from indexing
		# past the end, which is a crash rather than a wrong drawing.
		for i in range(mini(xs.size(), ys.size())):
			tiles.append(Vector2i(xs[i], ys[i]))
		road[ROAD_TILES_KEY] = tiles
		road_network.append(road)
		for tile in tiles:
			if not road_tile_lookup.has(tile):
				road_tile_lookup[tile] = []
			(road_tile_lookup[tile] as Array).append(road)

## The roads crossing a hex — the tile card's cross-ref, read through `_tile_info_at`. The rows come
## back BY REFERENCE into `road_network` rather than duplicated: nothing downstream writes to a road,
## and a per-hover deep copy of every path would be paid on every mouse move.
##
## **NOT fog-gated here, and that is deliberate**: the sim already publishes a road only to a faction
## that has explored at least one of its tiles (`Discovered`, not `Active` — a road does not wander
## off, so remembering one is remembering something true), and `_apply_visibility_to_info` drops the
## whole payload on an UNEXPLORED hex. Gating on `_is_tile_visible` the way the herd list does would
## hide a road the player is standing next to the moment they look away.
func _roads_on_tile(col: int, row: int) -> Array:
	var found: Variant = road_tile_lookup.get(Vector2i(col, row), null)
	return found if found is Array else []

## EVERYTHING standing on a hex, as one ordered stack of `{kind, data}` entries: every band first,
## then every herd. That order is the click contract — bands still win the first click on a shared
## hex — and the stack is the OCCUPANT half of what re-clicking cycles through, so a herd under a
## band is reachable. Built from `_units_on_tile` + `_herds_on_tile` rather than re-matching
## coordinates, so BOTH fog gates (a foreign band under fog, a herd on an unseen hex) hold here by
## construction. Occupant-only on purpose — the land is not an occupant, and adding it here would
## falsify both the name and the fog claim; `_selection_cycle_on_tile` is where it joins.
func _occupants_on_tile(col: int, row: int) -> Array:
	var occupants: Array = []
	for unit in _units_on_tile(col, row):
		occupants.append({OCCUPANT_KEY_KIND: OCCUPANT_KIND_UNIT, OCCUPANT_KEY_DATA: unit})
	for herd in _herds_on_tile(col, row):
		occupants.append({OCCUPANT_KEY_KIND: OCCUPANT_KIND_HERD, OCCUPANT_KEY_DATA: herd})
	return occupants

## The full select-then-cycle ring for a hex: every occupant (bands, then herds), then the LAND
## LAST. Membership is exactly what the tile panel lists, so re-clicking reaches every row of it —
## on a one-animal hex the click toggles herd ↔ land, on a band + two herds hex it runs
## band → herd A → herd B → land → band.
##
## The land goes LAST so the FIRST click on a fresh hex still lands on the top occupant, which is
## also the HUD's own fresh-hex precedence (`_resolve_auto_selected_subject`: first unit → first
## herd → land). And it joins ONLY when the hex has an occupant: on a bare hex the cycle would
## otherwise be `[land]`, which would route the click through the land branch instead of
## `_handle_entity_selection`'s clear branch and retire the `selection_cleared` path that
## `tile_panel_deselect_keeps_tile` guards. An empty hex has an EMPTY cycle, exactly as before.
func _selection_cycle_on_tile(col: int, row: int) -> Array:
	var cycle := _occupants_on_tile(col, row)
	if not cycle.is_empty():
		cycle.append(LAND_CYCLE_ENTRY)
	return cycle

## Where the CURRENT selection sits in this cycle — the anchor a re-click advances from.
## Derived from the selected ids rather than read straight off `cycle_index` for two reasons: it
## keeps the map click coherent with a panel roster-row click (pick Wildlife row 3 in the panel,
## re-click the hex, get row 4 — not row 1), and it survives the occupant array reordering between
## snapshots. Falls back to the stored `cycle_index` when nothing on the hex is selected.
##
## NEITHER id set means the LAND is what is selected, and this only ever runs for the ALREADY
## selected tile (its one caller is inside `handle_hex_click`'s `== selected_tile` guard), so that
## state names the land stop of THIS hex. Answering the land's own index is what makes the next
## click advance OFF the land to the first occupant instead of falling back to `cycle_index`.
func _selected_cycle_index(cycle: Array) -> int:
	var land_selected := selected_unit_id < 0 and selected_herd_id == ""
	for i in range(cycle.size()):
		var entry: Dictionary = cycle[i]
		if land_selected:
			if String(entry.get(OCCUPANT_KEY_KIND, "")) == OCCUPANT_KIND_LAND:
				return i
			continue
		if selected_unit_id >= 0 and _occupant_matches(entry, OCCUPANT_KIND_UNIT, selected_unit_id):
			return i
		if selected_herd_id != "" and _occupant_matches(entry, OCCUPANT_KIND_HERD, selected_herd_id):
			return i
	return clampi(cycle_index, 0, maxi(cycle.size() - 1, 0))

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
	if active_overlay_key == NO_OVERLAY_KEY:
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
	if active_overlay_key == TERRAIN_TAGS_OVERLAY_KEY:
		var mask := _tag_mask_at(x, y)
		if mask == 0:
			return GRID_COLOR
		var tag_color: Color = _tag_color_for_mask(mask)
		return GRID_COLOR.lerp(tag_color, 0.92)
	var overlay_value: float = _value_at_overlay(active_overlay_key, x, y)
	var overlay_color: Color = OVERLAY_COLORS.get(active_overlay_key, OVERLAY_FALLBACK_COLOR)
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

## The legend for whatever channel is active — the PULL side of `overlay_legend_changed`, which
## carries the identical dict. The minimap's overlay picker renders its own copy of it and can open
## long after the last push, so it needs to be able to ask.
func current_overlay_legend() -> Dictionary:
	return _legend_for_current_view()

## Does this world carry terrain-tag data? `terrain_tags` has no wire raster — it is assembled from
## the per-tile tag masks — so `OverlayChannels` asks this before offering the channel at all.
func has_terrain_tag_data() -> bool:
	return not terrain_tags_overlay.is_empty() or not terrain_tag_labels.is_empty()

## The tint a channel paints the map in — what the minimap's legend button wears as its face when the
## channel has no icon of its own. A channel with no row takes `OVERLAY_FALLBACK_COLOR`, which is a
## RAMP TARGET for an unknown wire channel and NOT a description of anything: a caller wearing this
## as a readout must ask `has_overlay_color` / `paints_with_overlay_color` first, or it will state a
## colour the map paints nowhere.
func overlay_color_for(key: String) -> Color:
	return OVERLAY_COLORS.get(key, OVERLAY_FALLBACK_COLOR)

## Does this channel have a row of its OWN in `OVERLAY_COLORS` — i.e. is `overlay_color_for` handing
## back that channel's real tint rather than the fallback? The channels painted through a path of
## their own still answer `true` when their ramp climbs to a named colour (pasture, forage).
func has_overlay_color(key: String) -> bool:
	return OVERLAY_COLORS.has(key)

## Does `_tile_color` paint this channel with its `OVERLAY_COLORS` value — the generic
## `GRID_COLOR.lerp(overlay_color, value)`? False for exactly `SPECIAL_PAINT_OVERLAY_KEYS`, each of
## which paints through a ramp or a blend of its own.
func paints_with_overlay_color(key: String) -> bool:
	return not SPECIAL_PAINT_OVERLAY_KEYS.has(key)

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
	# **THE KEY SHOWS WHAT THE MAP IS ACTUALLY DRAWN WITH.** With terrain textures on, a flat colour
	# swatch names a biome the player cannot match to anything on screen — the hexes are painted art,
	# not the palette entry. `hex_texture_for` is the very texture the blend-OFF renderer stamps on a
	# hex, so the swatch is a picture of that tile. Gated on the `T` toggle, because with textures OFF
	# the map really is flat `_tile_color` fills and the palette swatch is the honest answer; and it
	# answers `null` for any id the atlas has no layer for, which `OverlayLegend` falls back from.
	var textured: bool = _terrain.get_terrain_textures_enabled()
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
			"texture": _terrain.hex_texture_for(id) if textured else null,
			"label": label,
			"value_text": "%d tiles" % tile_count,
			# Numeric tile count so a consumer can sort by count without parsing value_text.
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
	var overlay_color: Color = OVERLAY_COLORS.get(key, OVERLAY_FALLBACK_COLOR)
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

## **THE AGGREGATE ⌃ CHANNEL** (`docs/plan_knowledge_screen.md` §7) — the ready-source model and the
## raster built from it, published as an ordinary channel through the `province` seam.
##
## **IT IS OFFERED WHETHER OR NOT ANYTHING IS READY, and the empty state is the teaching one.** A
## channel that appeared the turn the first discovery landed would be the map lighting up for an
## unlock, which §7 rules out by name; and a roster row that comes and goes is a row a player cannot
## learn. With nothing ready the legend says so in a sentence, which is a better answer than a missing
## channel. What it is gated on is a WORLD — `has_ready_for_improvement_data`, no grid or no source of either
## web, and there is nothing for the channel to be about at all.
func _build_ready_for_improvement_channel() -> void:
	# Stamped whether or not a channel is built: a world with no sources still ANSWERED for this row,
	# and leaving the stamp behind would make the next knowledge push compare against a world that is
	# gone.
	_ready_for_improvement_knowledge = faction_knowledge.duplicate()
	if not has_ready_for_improvement_data():
		_ready_for_improvement = {}
		return
	_ready_for_improvement = ReadyForImprovement.derive(self)
	_add_overlay_channel(
		ReadyForImprovement.CHANNEL_KEY,
		_ready_for_improvement[ReadyForImprovement.MODEL_NORMALIZED],
		_ready_for_improvement[ReadyForImprovement.MODEL_RAW],
		ReadyForImprovement.CHANNEL_LABEL,
		ReadyForImprovement.CHANNEL_DESCRIPTION)

## Forget every deferred channel's last build. Called at the END of an ingest, because the sources a
## deferred channel is derived from have just been replaced.
func _reset_deferred_overlays() -> void:
	_deferred_overlay_pending = {}
	for key in DEFERRED_OVERLAY_BUILDERS:
		_deferred_overlay_pending[key] = true

## Build a deferred channel, unless this frame has already built it.
##
## **IT NAMES NO CHANNEL, and that is the requirement it exists to satisfy** (`plan_knowledge_screen`
## §6b): a new deferred channel is one row in `DEFERRED_OVERLAY_BUILDERS`, never a second `if key ==`
## in the render path. Every other key falls straight through, which is what makes it safe as
## `set_overlay_channel`'s first line — `set_terrain_mode`, the fog clear and every offline harness
## drive that function with keys this table has never heard of.
func _realize_deferred_overlay(key: String) -> void:
	if not bool(_deferred_overlay_pending.get(key, false)):
		return
	_deferred_overlay_pending[key] = false
	call(String(DEFERRED_OVERLAY_BUILDERS[key]))

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

## The screen's extent expressed in THIS node's own local units — the map's "how big is the window,
## in the units I draw in". `get_global_transform_with_canvas()` composes the global canvas transform
## (camera scaling) with the node's OWN scale, which is where the interface-scale compensation lives,
## so this one division covers both and the two can never be applied twice or forgotten separately.
## Public because `CachedMapRenderer` sizes its SubViewport off it.
func screen_size_local() -> Vector2:
	var viewport_size: Vector2 = get_viewport_rect().size
	var to_screen := get_global_transform_with_canvas().get_scale()
	if to_screen.x == 0.0 or to_screen.y == 0.0:
		return viewport_size
	return viewport_size / to_screen

## The summed reserved strips per axis (left+right, top+bottom), converted into LOCAL units.
## `set_reserved_inset` receives widths measured in CANVAS units (a docked panel's width), which is
## also the space this node's `position` lives in — but `_get_adjusted_viewport_size` subtracts them
## from a LOCAL extent, and the interface scale makes those two spaces differ. Converted at the point
## of USE rather than at set time, because the scale can change after a panel has docked.
func _reserved_inset_span_local() -> Vector2:
	var span := Vector2(_inset_left + _inset_right, _inset_top + _inset_bottom)
	var to_screen := get_global_transform_with_canvas().get_scale()
	if to_screen.x == 0.0 or to_screen.y == 0.0:
		return span
	return span / to_screen

func _get_adjusted_viewport_size() -> Vector2:
	# In LOCAL units, so hit-testing matches the drawn map under any canvas (camera) scaling and
	# under the interface scale's counter-scale.
	var viewport_size: Vector2 = screen_size_local()
	# Exclude every reserved edge strip: the map treats the remaining rect as its
	# entire viewport, and the node is translated by the leading insets (see
	# set_reserved_inset), so nothing renders behind a docked panel.
	var inset_span: Vector2 = _reserved_inset_span_local()
	viewport_size.x = max(viewport_size.x - inset_span.x, 1.0)
	viewport_size.y = max(viewport_size.y - inset_span.y, 1.0)
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
		_apply_pan(pan_input * KEYBOARD_PAN_SPEED * ClientSettings.pan_speed_multiplier * delta)
	var zoom_direction: float = Input.get_action_strength("map_zoom_in") - Input.get_action_strength("map_zoom_out")
	if not is_zero_approx(zoom_direction):
		# `_apply_zoom`'s pivot is in LOCAL coords, so the centre is measured in them too.
		var viewport_center: Vector2 = screen_size_local() * 0.5
		_apply_zoom(zoom_direction * KEYBOARD_ZOOM_SPEED * ClientSettings.zoom_speed_multiplier * delta, viewport_center)
	# Animate the targeting overlay (pulsing glow / reticle) while a command is
	# being targeted.
	if _annotations.is_targeting_active():
		_annotations.advance_targeting_time(delta)
		queue_redraw()
	# Animate the awaiting-orders pulse on any expedition idle at its objective.
	if _has_awaiting_expedition:
		_expedition_time += delta
		queue_redraw()

## Mirror the HUD's pending command-targeting state so the map can draw the reticle / valid-target
## glow / hover ETA. Pass {} to clear. THIN PASS-THROUGH to AnnotationRenderer, and the NAME cannot
## move: Main.gd connects the HUD's `targeting_changed` signal to it BY NAME (has_method +
## Callable(map_view, "set_targeting")), so a rename would silently do nothing rather than error.
func set_targeting(info: Dictionary) -> void:
	_annotations.set_targeting(info)
	queue_redraw()

## SECONDARY-SLOT PASS-THROUGHS. `BandOverlayRenderer` docks its worked-source marks to the slot a
## source's marker drew in, and a renderer reaches its siblings through MapView rather than holding
## one another (the same convention as `_hex_center` / `_herd_by_id` / `_fill_hex`). The KEY builders
## come through too, so a mark and the marker it rides can never disagree about a source's identity.
## The player faction's {track: progress} knowledge row (`Hud.faction_knowledge_changed` → `Main`).
## MapView holds the patches and the herds already; this is the one input the ready mark needs that it
## cannot see, and it is deliberately the RAW row rather than a derived answer, so `RungGates` stays
## the single place the rung rules are written.
## **AND IT STALES THE `ready_for_improvement` CHANNEL WHEN THE ROW ACTUALLY MOVES.** The knowledge push
## arrives AFTER the map's own ingest — `Main._apply_snapshot` calls `display_snapshot` first and the
## HUD fan-out that emits `faction_knowledge_changed` after it — so a channel built only during the
## ingest would state the PREVIOUS turn's knowledge for the whole turn a discovery lands on, which is
## the one turn it is wrong on and the one turn a player looks.
##
## Marking it stale is free; the REBUILD happens only where the answer is about to be read — here when
## the channel is the one being painted, and otherwise at whatever later moment asks for it.
func set_faction_knowledge(knowledge: Dictionary) -> void:
	faction_knowledge = knowledge if knowledge is Dictionary else {}
	if faction_knowledge == _ready_for_improvement_knowledge:
		queue_redraw()
		return
	_ready_for_improvement = {}
	_deferred_overlay_pending[ReadyForImprovement.CHANNEL_KEY] = true
	if active_overlay_key == ReadyForImprovement.CHANNEL_KEY:
		_realize_deferred_overlay(ReadyForImprovement.CHANNEL_KEY)
		_emit_overlay_legend()
	queue_redraw()

## Is there anything for the aggregate ⌃ channel to be ABOUT — the registry row's `available`
## predicate, and a question about the WORLD rather than about the count or the raster. A world with
## sources offers the channel even when nothing is ready (the legend says so), and it must answer
## without building anything: the picker asks it on every ingest, which is exactly the moment the
## build is being kept off.
func has_ready_for_improvement_data() -> bool:
	return grid_width > 0 and grid_height > 0 \
		and not (forage_patch_lookup.is_empty() and herds.is_empty())

## The `facts` legend lines for that channel — the registry row names this method, and
## `OverlayChannels.facts_for` calls it with no arguments.
##
## It REALIZES the channel, because a legend can be asked for one the map is not painting; past that
## the answer is off the cached model, so opening the legend costs a scan of the unworked list (tens)
## and never a second pass over the sources.
func ready_for_improvement_facts() -> PackedStringArray:
	_realize_deferred_overlay(ReadyForImprovement.CHANNEL_KEY)
	return ReadyForImprovement.facts(self, _ready_for_improvement)

func secondary_slot_of(key: String) -> int:
	return _secondary_markers.slot_of(key)

func secondary_slot_center(tile_center: Vector2, slot: int, radius: float) -> Vector2:
	return _secondary_markers.slot_center(tile_center, slot, radius)

func secondary_food_key(x: int, y: int) -> String:
	return _secondary_markers.food_key(x, y)

func secondary_herd_key(herd_id: String) -> String:
	return _secondary_markers.herd_key(herd_id)

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

## Public zoom API — the on-screen zoom rail. `direction` is +1 (in) / -1 (out); the
## pivot is the map center so button-zoom doesn't drift the view. It still expresses
## its move as a delta through `_apply_zoom`, so there is exactly one map-zoom code
## path (the `set_zoom_factor` idiom).
##
## The rail is a LADDER: rungs sit every `ZOOM_BUTTON_STEP` from `MIN_ZOOM_FACTOR`,
## and a click moves to the ADJACENT rung rather than adding a delta to wherever
## `zoom_factor` happens to be. Two decisions, both deliberate:
##
## - **Unscaled by `zoom_speed_multiplier`.** That slider means *speed* and belongs to
##   the CONTINUOUS inputs — wheel, pinch, Q·E — which still read it. The rail is a
##   DISCRETE, deliberate step, which is what `ZOOM_BUTTON_STEP`'s own comment already
##   says it is for. Scaling it made the ladder unpredictable: at the slider's max (3.0)
##   each click became 1.5, so the rail ran 1.0 → 2.5 → 4.0 → 5.5 → 7.0 with no 6.0 or
##   6.5, and from the startup zoom it ran a DIFFERENT ladder (2.0 → 3.5 → 5.0 → 6.5).
##   Same precedent as mouse-drag pan, which is deliberately 1:1.
## - **Snapped, not accumulated.** The wheel and pinch use their own step and leave
##   `zoom_factor` off-grid (3.27, say); without snapping the readout could never get
##   back onto the ladder. From 3.27 a `+1` goes to 3.5 and a `-1` to 3.0 — the adjacent
##   rung in the direction of travel — so one click always restores a round readout.
##
## `MAX_ZOOM_FACTOR` need not lie on the ladder; the clamp just makes the topmost click
## a short one. At either limit the delta is 0 and `_apply_zoom`'s `is_zero_approx`
## early-out makes the click a clean no-op, with no spurious `zoom_changed`.
func zoom_step(direction: int) -> void:
	var rungs: float = (zoom_factor - MIN_ZOOM_FACTOR) / ZOOM_BUTTON_STEP
	# The epsilon nudges an on-rung value off the floor/ceil boundary in the direction of
	# travel, so "already on a rung" advances a whole rung instead of returning itself.
	var target_rung: float = (floorf(rungs + ZOOM_RUNG_EPSILON) + 1.0) if direction > 0 \
		else (ceilf(rungs - ZOOM_RUNG_EPSILON) - 1.0)
	var target: float = clampf(
		MIN_ZOOM_FACTOR + target_rung * ZOOM_BUTTON_STEP, MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
	_apply_zoom(target - zoom_factor, _viewport_center_pivot())

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

## **THE POINTER-DRIVEN NAVIGATION INPUTS — the three the GUI pass does NOT stop for us.** A wheel
## button, a trackpad pan gesture and a pinch gesture all reach `_unhandled_input` over a UI card that
## a LEFT press cannot get past, which is why they need `_pointer_claimed_by_ui` and the other buttons
## do not. Measured in Godot 4.7 over a `MOUSE_FILTER_STOP` card, pushing each through
## `Viewport.push_input`: LEFT is consumed by the GUI pass; wheel, pan and magnify all survive it.
func _is_pointer_navigation_input(event: InputEvent) -> bool:
	if event is InputEventPanGesture or event is InputEventMagnifyGesture:
		return true
	if event is InputEventMouseButton:
		var button: InputEventMouseButton = event
		return button.button_index == MOUSE_BUTTON_WHEEL_UP or button.button_index == MOUSE_BUTTON_WHEEL_DOWN
	return false

## **DOES A CONTROL CLAIM THE PIXEL UNDER THE POINTER?** The claim is `MOUSE_FILTER_STOP` — the
## contract this client already states for presses (`band-city-panel.md`, the `PanelRoot` autopsy:
## every pixel a STOP control claims is a pixel of dead map). A **PASS** control does not claim its
## pixel and must not block the map, because several HUD containers are PASS over visually empty
## space and blocking on those would kill map navigation across large dead regions.
##
## **IT WALKS THE ANCESTORS, and that is what makes the test agree with Godot's own routing.**
## `gui_get_hovered_control` answers the INNERMOST pickable control, which inside a card is routinely a
## PASS row rather than the STOP card around it — measured: a PASS child of a STOP panel is reported as
## hovered, and a LEFT press there is still eaten by the STOP ancestor. So a leaf-only reading would
## declare most of a card's own surface unclaimed. The walk mirrors `Viewport::_gui_call_input`: up the
## Control chain, stopping at the first STOP, at a `top_level` node, or where the chain leaves Controls
## (a `CanvasLayer` parent), which is what keeps a full-screen `MOUSE_FILTER_IGNORE` root — `PanelRoot`
## is one — from ever appearing in it.
func _pointer_claimed_by_ui() -> bool:
	var viewport := get_viewport()
	if viewport == null:
		return false
	var control: Control = viewport.gui_get_hovered_control()
	while control != null:
		if control.mouse_filter == Control.MOUSE_FILTER_STOP:
			return true
		if control.is_set_as_top_level():
			return false
		control = control.get_parent() as Control
	return false

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

	# Select-then-cycle: re-clicking the current tile advances the selection through everything the
	# tile panel lists — every band, every herd, then the LAND — and any fresh tile resets to the top
	# of it. Computed before _emit_tile_selection overwrites selected_tile — and held in a LOCAL
	# rather than written to `cycle_index` here, because _emit_tile_selection can re-enter
	# select_occupant (see _handle_entity_selection) and clobber the member mid-click.
	var cycle := _selection_cycle_on_tile(col, row)
	var next_index := 0
	if Vector2i(col, row) == selected_tile and cycle.size() > 1:
		next_index = (_selected_cycle_index(cycle) + 1) % cycle.size()

	var terrain_id: int = _terrain_id_at(col, row)
	emit_signal("hex_selected", col, row, terrain_id)
	_emit_tile_selection(col, row)

	# The cycle built above is passed through rather than rebuilt, so the index is applied to the
	# very list it was computed against (and each occupant is deep-copied once per click, not twice).
	_handle_entity_selection(col, row, cycle, next_index)

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

## The rect a floating surface may open into: the viewport, less every edge a docked panel has
## reserved. In CANVAS units, which is the space the reservations arrive in and the space a HUD
## `Control` positions in — deliberately NOT `_reserved_inset_span_local()`, which converts the same
## numbers into the map's own counter-scaled units for the cover-fit maths (`interface-scale.md`).
## The raw `get_viewport_rect()` is correct here for the same reason, and is not the defect that file
## warns about: this answer is consumed by a Control, never by map geometry.
func unreserved_screen_rect() -> Rect2:
	var full: Vector2 = get_viewport_rect().size
	return Rect2(
		Vector2(_inset_left, _inset_top),
		Vector2(
			maxf(0.0, full.x - _inset_left - _inset_right),
			maxf(0.0, full.y - _inset_top - _inset_bottom)))

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
