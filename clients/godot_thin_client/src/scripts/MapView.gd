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

# Selected / hovered hex outline (replaces the old brown-circle selection feel).
const SELECTED_HEX_OUTLINE_COLOR := Color(1.0, 1.0, 1.0, 0.9)
const SELECTED_HEX_OUTLINE_WIDTH := 3.0
const HOVER_HEX_OUTLINE_COLOR := Color(1.0, 1.0, 1.0, 0.22)
const HOVER_HEX_OUTLINE_WIDTH := 1.5

# Zoom level-of-detail: below this hex radius (far zoom, tiny hexes) skip the
# secondary edge icons + overflow/count chips; draw only the primary token.
const ICON_MIN_DETAIL_RADIUS := 16.0
# Edge blending (Approach B — per-pixel biome-blend shader; see CLAUDE.md → Edge Blending):
# below this hex radius the shader renders base-only (no blend) so far-zoom tiny hexes don't shimmer.
# The flat↔flat seam is ALWAYS A CONTINUOUS WEIGHTED MIX: a symmetric signed-distance splat weight (the
# LEAD term), perturbed by world value noise (organic meander) and — optionally, weakly — by the two
# textures' zero-centred luminance (a detail-following NUDGE), then feathered through a smoothstep into a
# mix factor. Two rejected predecessors were both 1-bit picks in disguise: the original dither
# (`neighbour if p > vnoise(...)`) — user-rejected as chunky blobs — and height blending with
# blend_height_influence 4.0, where the luma term dwarfed the distance weight and degenerated into
# winner-takes-all-by-luminance: dark soil punched torn holes deep into prairie-hex interiors ("this isn't
# a blend at all"). Hence the invariant: the WEIGHTS LEAD, the heights only NUDGE, and the last step is a mix.
# The fallbacks below mirror terrain_config.json (blend_width/blend_soft/blend_height_influence/
# blend_noise_scale/blend_noise_amount/feature_noise_cell). The two noises stay DECOUPLED: the blend noise
# wobbles ONLY the flat↔flat seam, while feature_noise_cell grains the shoreline reach/wisp + canopy
# treeline + peak footline — so retuning the seam can never move a coastline, treeline or footline.
# UNITS DIFFER ON PURPOSE: blend_noise_scale is a FRACTION OF THE HEX RADIUS (× radius → px, like
# blend_width), so the wobble's cell stays the same fraction of a hex at EVERY zoom. A raw-px cell did not:
# a hex is ~45px on screen in-game but several times that in a zoomed-in preview frame, so one fixed 6px
# cell meant a far coarser grain per hex in the game than in the preview it was judged in (the "preview
# doesn't match the game" report). feature_noise_cell stays a RAW PIXEL size (the shore/treeline/footline
# look is tuned in px).
const EDGE_BLEND_MIN_RADIUS := ICON_MIN_DETAIL_RADIUS
# REACH — how far across the seam the two biomes are allowed to mix (× radius → the blend_band px uniform).
# This is the ecotone's width. The user wants a SHALLOW transition confined to the hex edge, so it is kept
# small: 0.25·radius ≈ 19px at the on-screen r≈75, a shallow band that never reaches a hex interior.
const EDGE_BLEND_DEFAULT_WIDTH := 0.25
# FEATHER SOFTNESS — half-width (in seam-weight units, so 0..0.5 is the meaningful span) of the smoothstep
# that converts the perturbed seam weight into the mix factor. SMALL (≈0.03) ⇒ the mix snaps between the two
# biomes wherever the noise/detail carries the weight past 0.5, reading as a fine crisp stipple; LARGE
# (≈0.35) ⇒ a smooth gradient where the noise only leans the crossfade. It is never 0 in the shader
# (BLEND_SOFT_MIN floors it) so the smoothstep can't degenerate into a hard step.
const EDGE_BLEND_DEFAULT_SOFT := 0.35
const EDGE_BLEND_MAX_SOFT := 0.5  # past 0.5 the feather spans the whole weight range — no seam left
# HEIGHT INFLUENCE — the detail-following NUDGE: the two textures' zero-centred luminances bend the boundary
# toward the darker/lighter side so it follows their own tufts/grains. It is deliberately WEAK — typical luma
# deviations are ±0.3, so at 0.25 the nudge moves the weight by ≤ ~0.08, a fraction of the 0..1 distance
# weight it perturbs. It must NEVER out-vote that weight: at 4.0 the term dominated and the blend degenerated
# into a luminance-driven winner-takes-all that tore holes in hex interiors (user-rejected). 0 = pure
# distance+noise feather. EDGE_BLEND_MAX_HEIGHT_INFLUENCE is the hard ceiling that keeps it a nudge.
const EDGE_BLEND_DEFAULT_HEIGHT_INFLUENCE := 0.25
const EDGE_BLEND_MAX_HEIGHT_INFLUENCE := 0.5
# WOBBLE CELL — the seam-perturbation noise cell as a FRACTION of the hex radius (→ blend_noise_cell px).
# It is the WAVELENGTH at which the boundary meanders: coarse (≈0.25·radius ≈ 19px at r=75) gives a few
# organic lobes per hex edge, which is what stops the seam reading as the straight hex polyline; very fine
# (≈0.05) turns it into a per-pixel speckle instead.
const EDGE_BLEND_DEFAULT_NOISE_SCALE := 0.25
# WOBBLE AMOUNT — amplitude of that perturbation, ADDED to the seam weight (never thresholded against it —
# this is not a dither) and enveloped so it dies at both ends of the band. 0.30 swings the boundary by ±0.15
# of the weight range at the seam: a visible meander, still far short of reaching a hex interior.
const EDGE_BLEND_DEFAULT_NOISE_AMOUNT := 0.3
const EDGE_BLEND_DEFAULT_FEATURE_NOISE_CELL := 6.0
# RUGGED-LAND ELIGIBILITY (config `blend_rugged_land` → the shader's blend_rugged_land uniform). The bare seam
# gate is SAME-CLASS, so a rugged biome's base floor would never blend and would end in a razor-straight
# hexagon against its neighbour — the "rolling hills are CUT OFF at the hex edge" report (a peak biome's base
# IS the whole ground under its relief overlay). ON widens the LAND half of the gate to "both sides are land"
# (flat↔rugged and rugged↔rugged blend, through the EXISTING flat levers; land↔water stays hard and water
# keeps its depth field, so no frame without a rugged hex moves — verified bit-identical).
# SHIPPED ON, but only after the whole rugged roster was swept for SHREDDING (the height term tearing holes in
# a structured texture's interior — see EDGE_BLEND_DEFAULT_HEIGHT_INFLUENCE): every rugged biome was rendered
# as an ISOLATED hex surrounded by a contrasting one, in a flat field AND in a rugged field
# (tools/blend_probe.tscn state R). A straight band seam cannot show shredding — never judge this on one.
const EDGE_BLEND_DEFAULT_RUGGED_LAND := true
# --- WATER↔WATER seam levers (terrain_config's "water_blend" block) ---
# Blend eligibility is SAME-CLASS (see CLAUDE.md → Edge Blending): flat↔flat AND water↔water both blend;
# land↔water stays hard (that seam is the shoreline). The five levers above are tuned for LAND, where the
# textures carry detail for the height term to interlock on. Water textures are smooth and low-variance, so
# a land-width seam there just draws a clean soft-edged HEXAGON: the wobble is the only thing dissolving the
# silhouette, and at land amplitude/reach it cannot. Ocean depth also grades gradually in nature, so water
# gets its OWN wider/softer/wobblier reach (rendered side-by-side at r≈77: the land levers left visible hex
# outlines on a deep-ocean patch, these dissolved them). Only these three differ — the wobble CELL and the
# height nudge stay shared (a finer cell would speckle; the height term is a no-op on flat water anyway).
const WATER_BLEND_DEFAULT_WIDTH := 0.45          # reach (× radius → px), vs 0.25 on land
const WATER_BLEND_DEFAULT_SOFT := 0.45           # feather half-width, vs 0.35 on land (capped as land is)
const WATER_BLEND_DEFAULT_NOISE_AMOUNT := 0.45   # wobble amplitude, vs 0.30 on land
# Shoreline (land↔water coasts): a continuous profile — land → sand → surf → water — built from a SIGNED
# coast coordinate that straddles the shared edge, so no boundary in that chain is a hard step (see the
# shader's shoreline block for the three rejected passes this replaced). The three reaches are fractions of
# the hex radius (× radius → px band, like blend_width); fallbacks mirror terrain_config's "shore" block.
# Universal for now (every land↔water edge gets it, no per-biome gating).
# SAND_WIDTH is the sand's reach INLAND and is deliberately SHORT (0.25 < the 0.4 the first land-side beach
# used): the sand must fade into the land art, not bury it. There is NO sand on the water side at all — the
# sand↔foam blend is bought instead with FOAM_INLAND_WIDTH, the distance the surf washes UP the beach and
# crossfades with the sand.
const SHORE_DEFAULT_SAND_WIDTH := 0.25
const SHORE_DEFAULT_FOAM_INLAND_WIDTH := 0.15
const SHORE_DEFAULT_FOAM_WIDTH := 0.41
# The faint SECOND surf line out over open water. Both levers are fractions of the hex radius, exactly like
# FOAM_WIDTH — the wisp used to be a fixed multiple of the surf's reach, which chained the two together so
# the surf could not be shortened without dragging the wisp in with it. Config must keep the band clear of
# the surf (centre − half > foam_width) or the two merge into one wide white smear; the shipped values put
# the wisp at 0.42–0.68·r against a surf that dies at 0.41·r. wisp_half_width 0 turns the wisp off.
const SHORE_DEFAULT_WISP_CENTER_WIDTH := 0.55
const SHORE_DEFAULT_WISP_HALF_WIDTH := 0.13
# THE WATERLINE BASE CROSS-FADE — the half-reach (fraction of the hex radius) over which the LAND base
# texture and the WATER base texture cross-fade through the coastline. Without it the base STEPS at the
# waterline (raw land meeting raw water on a cliff coast), and the opaque surf peak was the only thing
# hiding that step — which is why the surf could never be muted. This is a WET EDGE, not an ecotone: it is
# deliberately well under the sand's 0.25 reach, so no land texture reads out to sea and no water texture
# reads up the beach. Chosen on `blend_probe` state SURF's foam-off step check over the CLIFF coast (the
# worst case: deep_ocean has no beach either), where 0.08 already dissolves the step, 0.14 reads as a natural
# wet-rock rim, and 0.20 starts ghosting land pebbles out into the water — so 0.14 ships.
# 0 disables it (and then FOAM_OPACITY must go back to 1). See the shader's waterline_band.
const SHORE_DEFAULT_WATERLINE_WIDTH := 0.14
# The surf's PEAK opacity (and, scaled with it, the offshore wisp's) — a translucent highlight instead of the
# opaque white ring the foam had to be while it was covering the base step above.
const SHORE_DEFAULT_FOAM_OPACITY := 0.55
const SHORE_DEFAULT_FOAM_COLOR := Vector3(0.690, 0.761, 0.804)
const SHORE_DEFAULT_BEACH_COLOR := Vector3(0.847, 0.733, 0.541)
# Canopy overlay (forest = grass floor + overhanging tree crowns): overhang reach + treeline softness
# are fractions of the hex radius (× radius → px); texture_scale is the world-UV multiplier (1.0 = one
# crown tile per hex). It is an INDEPENDENT density knob from base_texture_scale (the base biome is
# sampled in continuous world space at its own base_scale, ~0.25 ≈ one base tile per 4 hexes), NOT
# matched to it. Fallbacks mirror terrain_config's "canopy" block.
const CANOPY_DEFAULT_OVERHANG_WIDTH := 0.5
const CANOPY_DEFAULT_SOFTNESS_WIDTH := 0.45
const CANOPY_DEFAULT_TEXTURE_SCALE := 1.0
# Base biome-texture world-UV scale (top-level terrain_config "base_texture_scale"): the base biome is
# sampled in CONTINUOUS world space (like the canopy), so one texture tile spans ~1/base_scale hex-rows
# and adjacent hexes show DIFFERENT regions of it — killing the per-hex identical-repeat grid that any
# detailed (non-homogeneous) texture showed. ~0.25 → one texture spans ~4 hexes: the grid disappears but
# features aren't tiny. Smaller = a texture covers MORE hexes (zoomed-in look), larger = fewer.
const BASE_DEFAULT_TEXTURE_SCALE := 0.25
# Canopy LOD gate, DECOUPLED from the flat↔flat blend gate (EDGE_BLEND_MIN_RADIUS). Set WELL BELOW it so
# the canopy pass keeps running at far zoom — interior forest density (D=1) persists into a distinct
# darker-green forest mass (the edge overhang naturally shrinks to nothing as hexes shrink). Trilinear
# mipmap filtering on the crown array (TerrainTextureManager) keeps that far-zoom mass smooth, not shimmery.
const CANOPY_DEFAULT_MIN_RADIUS := 3.0
# Peak overlay (highland/volcanic relief = flat rocky floor + overhanging faceted peaks + cast shadow):
# the mountain-drama analog of the canopy overlay. Overhang reach + softness are fractions of the hex
# radius (× radius → px, like canopy); texture_scale is the world-UV multiplier (1.0 = one peak tile per
# hex); shadow_length is the cast-shadow reach fraction (× radius → px). prominence/strength are unit
# scalars; light_dir points TOWARD the light in canvas space (+y is DOWN, so top-left = negative,negative).
# Fallbacks mirror terrain_config's "peaks" block.
const PEAK_DEFAULT_OVERHANG_WIDTH := 0.6
const PEAK_DEFAULT_SOFTNESS_WIDTH := 0.4
const PEAK_DEFAULT_TEXTURE_SCALE := 0.2
const PEAK_DEFAULT_MIN_RADIUS := 3.0
const PEAK_DEFAULT_SHADOW_LENGTH := 0.5
const PEAK_DEFAULT_SHADOW_STRENGTH := 0.45
const PEAK_DEFAULT_MIN_PROMINENCE := 0.35
const PEAK_DEFAULT_LIGHT_DIR := Vector2(-0.7, -0.7)  # top-left; canvas +y is DOWN, so negative,negative
# Elev-map fallback (0..255 = relative height 0..100): used when a snapshot lacks an elevation raster
# (relative_height_at returns -1) so peak relief still renders in preview/rehydrated frames.
const PEAK_ELEV_FALLBACK := 200
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

# --- Band status decorations (food-days dot, activity glyph, supply links) ---
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
# Selected-player-band labor highlights (Early-Game Labor slice 3b). Distinct styles so
# the layers read apart: the work-range reach (thin cyan outlines = "reachable"), the worked
# forage tiles (strong green fill = "being worked"), and the hunted herds (red ring + link).
# (Scouting no longer draws a disc — it extends the band's real sight, visible directly in the
# fog; `scout_reveal_radius` is now a sight-range bonus, not a reveal-disc radius.)
# See _draw_band_work_highlights.
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"
# Work-range ring: outline every in-range tile (Chebyshev square), no fill — reads as the
# reachable-forage cluster without competing with the worked-tile fills.
const WORK_RANGE_OUTLINE := Color(0.310, 0.878, 0.812, 0.34)  # faint SIGNAL cyan
const WORK_RANGE_OUTLINE_WIDTH := 1.5
# Worked forage tiles: strong green fill + bold outline (the tiles actually being harvested).
const FORAGE_WORKED_FILL := Color(0.30, 0.80, 0.30, 0.34)
const FORAGE_WORKED_OUTLINE := Color(0.46, 0.96, 0.46, 0.95)
const FORAGE_WORKED_OUTLINE_WIDTH := 3.0
# Hunted herds: red ring on the herd tile + a thin band→herd link (the herd can sit well
# outside the work-range ring — hunt reach = work_range + leash).
const HUNT_WORKED_COLOR := Color(0.92, 0.34, 0.30, 0.95)
const HUNT_WORKED_RING_FACTOR := 0.62   # of hex radius
const HUNT_WORKED_RING_WIDTH := 3.0
const HUNT_WORKED_LINK_COLOR := Color(0.92, 0.34, 0.30, 0.60)
const HUNT_WORKED_LINK_WIDTH := 2.5
# On-tile per-source yield annotations on the selected band's worked forage tiles / hunted herds:
# the assignment's `actual_yield` (food/turn) as a small drop-shadow label above the tile center
# (reusing `_draw_marker_glyph`), sign-formatted to 2 decimals, food-income green — with a WARN-amber
# `⚠` overhunting flag when `actual > sustainable + ε` (mirrors the allocation panel; forage is
# renewable so never trips). ε/decimals mirror Hud's `OVERHUNT_EPSILON`/`YIELD_DECIMALS` (separate
# script, so named here rather than shared). LOD-suppressed below ICON_MIN_DETAIL_RADIUS.
# Font scales with the hex radius (clamped) so the label reads at any zoom, not just tiny at big hexes.
const YIELD_LABEL_SIZE_FACTOR := 0.16     # of hex radius
const YIELD_LABEL_MIN_FONT := 11
const YIELD_LABEL_MAX_FONT := 24
const YIELD_LABEL_OFFSET_FACTOR := 0.78   # above the tile center, as a fraction of the hex radius
const YIELD_LABEL_DECIMALS := 2
const YIELD_OVERHUNT_EPSILON := 0.001
const YIELD_OVERHUNT_FLAG := "⚠"
# Optimistic PENDING actions (Early-Game Labor slice 3b UX): a distinct amber DASHED style
# (clearly apart from the solid confirmed green/cyan/blue/red) marks a just-issued assign/move
# that the snapshot hasn't confirmed yet. Ties to the amber "· pending" rows in the HUD panel.
const LABOR_PENDING_COLOR := Color(0.98, 0.80, 0.30, 0.98)  # amber/gold
const LABOR_PENDING_WIDTH := 2.6
const LABOR_PENDING_DASH := 10.0
const LABOR_PENDING_GAP := 7.0
const LABOR_PENDING_LINK_ALPHA := 0.7
# Travel destination (selected traveling band/expedition): a thin cyan line from the unit's
# current tile to the wrapped-nearest destination hex + a target reticle on that hex, so the
# player sees where it is headed. Distinct from the pending-amber style — this is a confirmed,
# in-progress move reported by the snapshot (`is_traveling` + `travel_target_x/y`).
const TRAVEL_DEST_COLOR := Color(0.310, 0.878, 0.812, 0.85)  # SIGNAL cyan
const TRAVEL_DEST_LINE_WIDTH := 2.0
const TRAVEL_DEST_LINE_ALPHA := 0.6           # line reads fainter than the reticle
const TRAVEL_DEST_RETICLE_FACTOR := 0.62      # reticle radius as a factor of hex radius
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
var tile_lookup: Dictionary = {}
# Per-tile habitability (band-independent morale drain, decoded from TileState),
# keyed by Vector2i(x, y); read by `_tile_info_at` for the Tile-card Habitability row.
var tile_habitability: Dictionary = {}
# Per-tile temperature (°, latitude + elevation climate, decoded from TileState),
# keyed by Vector2i(x, y); read by `_tile_info_at` for the Tile-card Climate row.
var tile_temperature: Dictionary = {}
var trade_links_overlay: Array = []
var trade_overlay_enabled: bool = false
var selected_trade_entity: int = -1
var crisis_annotations: Array = []
var hydrology_rivers: Array = []
var highlight_rivers: bool = false

# Terrain texture system for 2D view (textures loaded via TerrainTextureManager autoload)
var _hex_texture_cache: Dictionary = {}  # terrain_id -> ImageTexture (hex-masked)
var _hex_texture_size: int = 128  # Size of cached hex textures
var _show_grid_lines: bool = true
var _terrain_grid_width: int = 0
var _terrain_grid_height: int = 0
var _cached_terrain_ids: PackedInt32Array = PackedInt32Array()
var _hex_alpha_mask: PackedByteArray = PackedByteArray()  # Pre-computed hex mask for texture rendering
var _terrain_blend_class: Dictionary = {}  # terrain_id -> "flat"|"water"|"rugged" (edge-blend eligibility)
# Approach B — per-pixel biome-blend shader (terrain_blend.gdshader). A whole-map quad child renders
# the blended terrain behind MapView's own draws; MapView feeds it the biome array + a per-hex id-map
# (splatmap) + the exact hex-layout uniforms. Supersedes A's baked-overlay dither when use_edge_blending.
var _terrain_blend_quad: Node2D = null
var _terrain_blend_material: ShaderMaterial = null
var _terrain_blend_ready: bool = false
var _terrain_id_map_tex: ImageTexture = null   # RGBA8: R=terrain id, G=blend_class code (0 water/1 flat/2 rugged), B=canopy code (0=none else layer+1), A=peak code (0=none else layer+1)
var _terrain_vis_map_tex: ImageTexture = null  # R8: 0 unexplored / 0.5 discovered / 1 active
var _terrain_elev_map_tex: ImageTexture = null # R8: per-hex relative height (0..255 = 0..100), for peak prominence + shadow scaling
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
# Reused StyleBoxFlat for the band nameplate banner — lazily created once, then only its
# per-call properties (bg_color, corner radius) are updated in `_draw_band_banner`, so the
# draw path allocates no StyleBox per primary tile per frame.
var _band_banner_box: StyleBoxFlat = null
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
# Per-frame SECONDARY marker slot assignments (rebuilt in _compute_secondary_slots):
# entry-key(String) -> edge slot index (int, -1 = overflowed/hidden); tile -> overflow count.
var _secondary_slot_lookup: Dictionary = {}
var _secondary_overflow: Dictionary = {}
# Optimistic pending-labor map (per band entity), pushed from the HUD via set_labor_pending.
# Drawn for the selected band in a distinct dashed-amber style until the snapshot confirms.
var _labor_pending: Dictionary = {}
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

# 2D Minimap (uses shared MinimapPanel component)
const MinimapPanelScript := preload("res://src/scripts/ui/MinimapPanel.gd")
var _minimap_2d: Node = null  # MinimapPanel instance
var _minimap_2d_image: Image = null
var _minimap_2d_last_grid_size: Vector2i = Vector2i.ZERO
var _minimap_2d_data_version: int = 0  # Incremented when terrain/visibility changes
var _minimap_2d_last_data_version: int = -1  # Last version used for rebuild
var _minimap_2d_last_fow_enabled: bool = false  # Track FoW state changes
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
	_init_terrain_rendering()
	_setup_map_cache()
	# Note: _setup_2d_minimap() is now called lazily from _update_2d_minimap()
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
	_cached_terrain_ids = terrain_overlay
	_terrain_grid_width = grid_width
	_terrain_grid_height = grid_height
	_update_biome_color_buffer()
	# Increment minimap data version to trigger rebuild on terrain/visibility changes
	_minimap_2d_data_version += 1
	# Invalidate map cache when terrain data changes
	_invalidate_map_cache()
	# Rebuild the Approach-B blend-shader splatmaps (id-map + FoW vis-map) from the new terrain/fog.
	_rebuild_terrain_shader_maps()
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
	hydrology_rivers = []
	var rivers_variant: Variant = overlays.get("hydrology_rivers", [])
	if rivers_variant is Array:
		for entry in rivers_variant:
			if entry is Dictionary:
				hydrology_rivers.append((entry as Dictionary).duplicate(true))
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
				if culture_layer_grid.size() > 0:
					if x >= 0 and x < grid_width and y >= 0 and y < grid_height:
						var index: int = y * grid_width + x
						if index >= 0 and index < culture_layer_grid.size():
							culture_layer_grid[index] = int(tile_dict.get("culture_layer", -1))
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
	_update_2d_minimap()

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
	if _shader_terrain_active():
		# Approach B: the whole-map blend shader draws the base terrain on the behind-quad; MapView only
		# adds grid lines on top here. The CPU cache is bypassed (the shader is a single cheap GPU draw).
		_update_terrain_shader_quad(radius, origin, viewport_size)
		_draw_hex_grid_overlay(radius, origin, col_start, col_end, row_start, row_end)
	else:
		_hide_terrain_shader_quad()
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
	_draw_hydrology(radius, origin)
	_draw_crisis_annotations(radius, origin)

	# Selected + hovered hex outlines (drawn under the markers).
	_draw_tile_selection_highlight(radius, origin)

	# Selected player band: highlight what it's working (forage tiles / hunted herds) and
	# its assignable reach (work-range ring). Drawn before the
	# unit/herd markers so those sit on top of the tile tints.
	_draw_band_work_highlights(radius, origin)

	_draw_supply_links(radius, origin)
	_draw_primary_bands(radius, origin)

	_compute_secondary_slots()
	for herd in herds:
		_draw_herd(herd, radius, origin)
	for site in food_sites:
		_draw_food_site(site, radius, origin)
	for wsite in discovered_sites:
		_draw_discovered_site(wsite, radius, origin)
	_draw_secondary_overflow(radius, origin)

	_draw_harvest_markers(radius, origin)
	_draw_scout_markers(radius, origin)

	for order in routes:
		_draw_route(order, radius, origin)

	_draw_targeting(radius, origin)

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
					_draw_hex_textured_direct(center, terrain_id, radius, _fow_texture_tint_for_state(vstate))
			else:
				var final_color: Color = _tile_color(data_x, y)
				var polygon_points := _hex_points(center, radius)
				draw_polygon(polygon_points, PackedColorArray([final_color, final_color, final_color, final_color, final_color, final_color]))

	# Draw grid lines on top of all terrain (batched, shared with the shader path).
	_draw_hex_grid_overlay(radius, origin, col_start, col_end, row_start, row_end)


func _draw_hex_textured_direct(center: Vector2, terrain_id: int, radius: float, tint: Color = Color.WHITE) -> void:
	## Draw a single hex with texture (direct rendering version). `tint` modulates
	## the texture (used for Fog of War: mist for Discovered, white for Active).
	var tex: ImageTexture = _hex_texture_cache.get(terrain_id)
	if tex == null:
		var color: Color = _terrain_color_for_id(terrain_id) * tint
		var polygon_points := _hex_points(center, radius)
		draw_polygon(polygon_points, PackedColorArray([color, color, color, color, color, color]))
		return

	var polygon_points := _hex_points(center, radius)
	var uvs := PackedVector2Array()
	for point in polygon_points:
		var uv := Vector2(
			(point.x - center.x) / radius * 0.5 + 0.5,
			(point.y - center.y) / radius * 0.5 + 0.5
		)
		uvs.append(uv)
	var colors := PackedColorArray([tint, tint, tint, tint, tint, tint])
	draw_polygon(polygon_points, colors, uvs, tex)


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
	_rebuild_terrain_shader_maps()  # refresh the blend-shader vis-map for the new FoW state
	_invalidate_map_cache()  # FoW changes require fresh cache render
	queue_redraw()
	_emit_overlay_legend()
	_update_2d_minimap()  # Rebuild minimap with/without FoW (also sets _explored_bounds_world)
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
		_toggle_terrain_textures()
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

## PRIMARY marker pass: draw player-band tokens as center card-stacks, one per
## occupied tile. Co-located bands fan up-right (back cards darkened/shrunk); the active band
## (selected, else first) is the opaque, full-brightness top card. There is NO per-token ring —
## the active band reads by brightness alone; selection is the hex-shape outline. Beyond
## BAND_STACK_MAX_CARDS a `×N` count badge notes the hidden bands.
func _draw_primary_bands(radius: float, origin: Vector2) -> void:
	# Group units by tile, preserving snapshot order (deterministic stack order).
	var by_tile: Dictionary = {}   # Vector2i -> Array[Dictionary]
	var order: Array = []          # tiles in first-seen order
	for unit in units:
		var pos: Array = Array(unit.get("pos", []))
		if pos.size() != 2:
			continue
		var tile := Vector2i(int(pos[0]), int(pos[1]))
		if not by_tile.has(tile):
			by_tile[tile] = []
			order.append(tile)
		by_tile[tile].append(unit)
	for tile in order:
		_draw_band_stack(by_tile[tile], radius, origin)

func _draw_band_stack(group: Array, radius: float, origin: Vector2) -> void:
	var count := group.size()
	if count == 0:
		return
	var first: Dictionary = group[0]
	var pos: Array = Array(first.get("pos", []))
	var group_tile := Vector2i(int(pos[0]), int(pos[1]))
	var center: Vector2 = _hex_center_wrapped(group_tile.x, group_tile.y, radius, origin)
	# Active band = the selected one on this tile (selected_unit_id is the cycle target),
	# else the first. selected_unit_id already tracks the cycled/roster-picked band.
	var active_idx := 0
	for i in range(count):
		if int((group[i] as Dictionary).get("entity", -1)) == selected_unit_id:
			active_idx = i
			break
	var token_radius := radius * BAND_TOKEN_RADIUS_FACTOR
	# Back cards = every non-active band (decorative depth), active drawn last on top.
	var back_bands: Array = []
	for i in range(count):
		if i != active_idx:
			back_bands.append(group[i])
	var back_to_draw := mini(count, BAND_STACK_MAX_CARDS) - 1
	var back_radius := token_radius * BAND_STACK_BEHIND_SCALE   # shrink back cards for depth
	for j in range(back_to_draw):
		var depth := back_to_draw - j   # furthest (largest offset) drawn first
		var offset := BAND_STACK_CARD_STEP * radius * float(depth)
		_draw_band_token(back_bands[j], center + offset, back_radius, true)
	# Active top card at base position.
	var active: Dictionary = group[active_idx]
	_draw_band_token(active, center, token_radius, false)
	# Faction nameplate banner under the active (primary) card only. Far-zoom LOD-gated with the
	# same threshold that suppresses secondary icons/chips. Returns its rect so the count pill
	# can cap its right end.
	# Expeditions carry their faction on the flag-disc ring, not a settlement nameplate, so skip
	# the banner for them (and thus the banner-anchored count pill falls back to the offset).
	var active_is_expedition := bool(active.get("is_expedition", false))
	var show_banner := radius >= ICON_MIN_DETAIL_RADIUS and not active_is_expedition
	var banner_rect := Rect2()
	if show_banner:
		banner_rect = _draw_band_banner(center, token_radius, _band_faction_color(active))
	# Active band reads by brightness alone now (full-color top card over darkened back cards);
	# the hex selection outline still marks the selected tile. No per-token ring.
	# Decorations on the active band only (expeditions show provisions in their drawer, not a dot).
	if _is_player_unit(active) and not active_is_expedition:
		_draw_band_status(active, center, token_radius)
	_draw_band_task_arrow(active, center, radius, origin)
	# Count badge for hidden bands beyond the visible cap (suppressed at far zoom). Folded onto
	# the right end of the banner (nameplate-with-count look); falls back to the old bottom-right
	# offset only if the banner is LOD-suppressed (which shares the same zoom gate, so in practice
	# it always caps the banner).
	if count > BAND_STACK_MAX_CARDS and radius >= ICON_MIN_DETAIL_RADIUS:
		var pill_center := center + BAND_COUNT_BADGE_OFFSET * radius
		if show_banner:
			pill_center = Vector2(banner_rect.position.x + banner_rect.size.x, banner_rect.position.y + banner_rect.size.y * 0.5)
		_draw_count_pill(pill_center, "×%d" % count)

func _draw_band_token(unit: Dictionary, center: Vector2, token_radius: float, dim: bool) -> void:
	if bool(unit.get("is_expedition", false)):
		# A detached scouting party keeps its distinct hollow flag disc + awaiting-orders pulse
		# (not a settlement glyph). Faction reads off the ring, so no nameplate banner is drawn
		# (guarded in _draw_band_stack). Expeditions are lone on their tile, so `dim` is unused.
		_draw_expedition_body(unit, center, token_radius, _band_faction_color(unit))
		return
	var stage_icon := String(unit.get("settlement_stage_icon", ""))
	if stage_icon == "":
		# Fallback: pre-stage / missing snapshot — a small neutral, NON-circular placeholder
		# square (never a faction disc). Ownership is still carried by the banner below.
		var marker_color := BAND_FALLBACK_MARKER_COLOR
		var outline := BAND_TOKEN_OUTLINE_COLOR
		if dim:
			marker_color *= BAND_STACK_BEHIND_TINT
			outline *= BAND_STACK_BEHIND_TINT
		var side := token_radius * BAND_FALLBACK_MARKER_SIZE_FACTOR
		var square := Rect2(center.x - side * 0.5, center.y - side * 0.5, side, side)
		draw_rect(square, marker_color)
		draw_rect(square, outline, false, BAND_TOKEN_OUTLINE_WIDTH)
		return
	# Stage glyph token: just the shadowed glyph — ownership is carried by the banner, not a ring.
	var glyph_color := BAND_STAGE_GLYPH_COLOR
	if dim:
		glyph_color *= BAND_STACK_BEHIND_TINT
	var glyph_size := int(maxf(SECONDARY_ICON_MIN_SIZE, token_radius * BAND_STAGE_GLYPH_SIZE_FACTOR))
	_draw_marker_glyph(center, stage_icon, glyph_size, glyph_color)

## Faction color lookup for a band token, with a neutral fallback for unknown factions.
func _band_faction_color(unit: Dictionary) -> Color:
	return faction_colors.get(unit.get("faction", ""), BAND_FACTION_FALLBACK_COLOR)

## Faction-colored nameplate banner drawn under the PRIMARY band token (caller draws it for the
## active top card only — never the dimmed back cards). Ownership reads off the fill color, so no
## ring/disc is needed. The bar is sized to later host an optional faction/band NAME LABEL drawn
## on top of it (this bar is the substrate); keep it wide/structured enough for that. Returns the
## bar Rect2 so the caller can anchor the `×N` count pill to its right end.
func _draw_band_banner(center: Vector2, token_radius: float, faction_color: Color) -> Rect2:
	var width := token_radius * BAND_BANNER_WIDTH_FACTOR
	var height := token_radius * BAND_BANNER_HEIGHT_FACTOR
	var top := center.y + token_radius + token_radius * BAND_BANNER_GAP_FACTOR
	var rect := Rect2(center.x - width * 0.5, top, width, height)
	if _band_banner_box == null:
		# Constant chrome (border) set once; per-call fields updated below.
		_band_banner_box = StyleBoxFlat.new()
		_band_banner_box.border_color = BAND_BANNER_OUTLINE_COLOR
		_band_banner_box.set_border_width_all(int(BAND_BANNER_OUTLINE_WIDTH))
	_band_banner_box.bg_color = faction_color
	_band_banner_box.set_corner_radius_all(int(maxf(0.0, height * BAND_BANNER_CORNER_RADIUS_FACTOR)))
	draw_style_box(_band_banner_box, rect)
	return rect

## Travel/task destination arrow for a band, extracted so the stack draws it for the
## active card only. Skips the arrow when the band is already at its destination or the
## line would span the wrap seam.
func _draw_band_task_arrow(unit: Dictionary, center: Vector2, radius: float, origin: Vector2) -> void:
	var pos: Array = Array(unit.get("pos", []))
	if pos.size() != 2:
		return
	var dest_x: int = int(unit.get("dest_x", -1))
	var dest_y: int = int(unit.get("dest_y", -1))
	if dest_x < 0 or dest_y < 0:
		return
	if int(pos[0]) == dest_x and int(pos[1]) == dest_y:
		return
	var dest_center: Vector2 = _hex_center_wrapped(dest_x, dest_y, radius, origin)
	if abs(center.x - dest_center.x) > last_map_size.x * 0.4:
		return
	var arrow_color: Color = _travel_arrow_color(String(unit.get("travel_task_kind", "")))
	draw_line(center, dest_center, arrow_color, BAND_TASK_ARROW_WIDTH)
	_draw_arrowhead(center, dest_center, arrow_color)

## Draw an expedition's map body (docs/plan_exploration_and_sites.md §2 / §2b): a hollow,
## faction-tinted disc — visually distinct from a resident band's solid dot — carrying a mission
## glyph (scout = ⚑ flag, hunt = 🏹 bow). Phase decorations: a scout `awaiting` party pulses an
## amber ring (needs a command); a hunt `delivering` party shows a green food pip (carrying a haul
## home). The shared label / travel arrow / selection ring stay in `_draw_unit`.
func _draw_expedition_body(unit: Dictionary, center: Vector2, marker_radius: float, color: Color) -> void:
	var is_hunt := String(unit.get("expedition_mission", "")) == EXPEDITION_HUNT_MISSION
	var glyph := EXPEDITION_HUNT_GLYPH if is_hunt else EXPEDITION_GLYPH
	# Dark backing disc keeps the glyph legible over any terrain (mirrors the site/herd markers).
	draw_circle(center, marker_radius, Color(0.04, 0.06, 0.07, EXPEDITION_DISC_ALPHA))
	# Hollow faction ring — no solid fill, so it never reads as a resident band's dot.
	draw_arc(center, marker_radius * EXPEDITION_RING_FACTOR, 0, TAU, 24, color, EXPEDITION_RING_WIDTH)
	# Mission glyph at the center.
	var font: Font = ThemeDB.fallback_font
	if font != null:
		var glyph_size: int = int(maxf(12.0, marker_radius * EXPEDITION_GLYPH_SIZE_FACTOR * 2.0))
		var text_size: Vector2 = font.get_string_size(glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, glyph_size)
		var pos := Vector2(center.x - text_size.x * 0.5, center.y + glyph_size * 0.34)
		draw_string(font, pos, glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, glyph_size, EXPEDITION_GLYPH_COLOR)

	# Hunt phase decoration: hauling a haul home (delivering/returning) → a solid green food pip;
	# gathering at the herd (hunting) → a small red "working" cue ring. Mutually exclusive phases.
	if is_hunt:
		var hphase := String(unit.get("expedition_phase", ""))
		if hphase == EXPEDITION_PHASE_DELIVERING or hphase == EXPEDITION_PHASE_RETURNING:
			var pip_center := center + Vector2(marker_radius, marker_radius) * EXPEDITION_DELIVER_PIP_OFFSET
			var pip_radius := marker_radius * EXPEDITION_DELIVER_PIP_FACTOR
			draw_circle(pip_center, pip_radius, HudStyle.HEALTHY)
			draw_arc(pip_center, pip_radius, 0, TAU, 10, Color(0, 0, 0, 0.5), 1.0)
		elif hphase == EXPEDITION_PHASE_HUNTING:
			var cue_center := center + Vector2(marker_radius, marker_radius) * EXPEDITION_GATHER_CUE_OFFSET
			var cue_radius := marker_radius * EXPEDITION_GATHER_CUE_FACTOR
			draw_arc(cue_center, cue_radius, 0, TAU, 12, HudStyle.DANGER, EXPEDITION_GATHER_CUE_WIDTH)

	# Awaiting-orders idle indicator (scout): a pulsing amber ring (needs a command).
	if String(unit.get("expedition_phase", "")) == EXPEDITION_PHASE_AWAITING:
		var pulse: float = 0.5 + 0.5 * sin(_expedition_time * EXPEDITION_AWAITING_PULSE_SPEED)
		var ring_radius: float = marker_radius * (EXPEDITION_AWAITING_RING_FACTOR + EXPEDITION_AWAITING_PULSE_AMPLITUDE * pulse)
		var ring_color := Color(HudStyle.WARN.r, HudStyle.WARN.g, HudStyle.WARN.b, 0.45 + 0.4 * pulse)
		draw_arc(center, ring_radius, 0, TAU, 28, ring_color, EXPEDITION_AWAITING_RING_WIDTH)

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

## One decoration on a player band marker: a food-days dot (green/amber/red by
## the shared BandFoodStatus thresholds) up-and-right of the marker.
func _draw_band_status(unit: Dictionary, center: Vector2, marker_radius: float) -> void:
	var days: float = float(unit.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
	var dot_color := BandFoodStatus.color_for_days(days)
	var dot_radius: float = marker_radius * BAND_FOOD_DOT_RADIUS_FACTOR
	var dot_center := center + Vector2(marker_radius, -marker_radius) * BAND_FOOD_DOT_OFFSET_FACTOR
	draw_circle(dot_center, dot_radius, dot_color)
	draw_arc(dot_center, dot_radius, 0, TAU, 10, Color(0, 0, 0, 0.5), 1.0)

## When a player band is selected, surface what it is working (Early-Game Labor slice 3b):
##  - work-range ring: outline every tile within `work_range` of the band = the assignable
##    forage area. Replicates the sim's true **odd-r hex distance** EXACTLY (offset→axial cube
##    distance via _hex_distance, matching `hex_distance_wrapped`) so highlighted ==
##    actually-assignable. This is a real hexagonal ring (19 tiles at range 2), NOT the old
##    Chebyshev/king-move square whose diagonal corners were 3 hex-steps away yet wrongly lit.
##  - worked forage tiles: strong green fill on each `forage` assignment's target tile.
##  - hunted herds: a red ring on the herd tile + a band→herd link (the herd can sit outside
##    the work-range ring — hunt reach = work_range + leash).
## All cleared automatically when the band is deselected (selected_unit_id < 0 → early out).
func _draw_band_work_highlights(radius: float, origin: Vector2) -> void:
	if selected_unit_id < 0:
		return
	var band := _selected_player_band()
	if band.is_empty():
		return
	var pos: Array = Array(band.get("pos", []))
	if pos.size() != 2:
		return
	var band_col := int(pos[0])
	var band_row := int(pos[1])
	# Render neighbours in the band's wrapped column frame so the ring stays contiguous
	# across the horizontal seam.
	var eff_col := _band_effective_col(band_col, radius, origin)
	var band_center := _hex_center(eff_col, band_row, radius, origin)

	# Scouting no longer draws a disc: `scout_reveal_radius` now carries the band's scout vantage
	# distance (how far forward-observer vantages are posted, `0` with no scouts), and its effect is
	# visible directly in the fog — staffed scouts reveal LOS from vantages that see around obstacles.
	# The client can't reconstruct the true revealed area (it doesn't know the server-side LOS/terrain),
	# so nothing is drawn here.

	# 1. Work-range ring: true odd-r hex-distance outline (matches the sim's hex_distance_wrapped).
	# Iterate a ±work_range col/row bounding box (a superset of the hex disc) and outline only
	# tiles whose hex distance from the band is <= work_range — a hexagonal ring, not a square.
	var work_range := int(band.get("work_range", 0))
	if work_range > 0:
		for drow in range(-work_range, work_range + 1):
			var row := band_row + drow
			if row < 0 or row >= grid_height:
				continue
			for dcol in range(-work_range, work_range + 1):
				if dcol == 0 and drow == 0:
					continue
				var col := eff_col + dcol
				# Both tiles already share the band's effective column frame, so the delta is
				# seam-correct — measure hex distance and skip anything beyond work_range.
				if _hex_distance(eff_col, band_row, col, row) > work_range:
					continue
				# Without horizontal wrap, edge columns fall off the map — don't outline
				# nonexistent tiles (mirrors the grid_height row clamp above).
				if not _wrap_horizontal and (col < 0 or col >= grid_width):
					continue
				_outline_hex(col, row, radius, origin, WORK_RANGE_OUTLINE, WORK_RANGE_OUTLINE_WIDTH)

	# 2. Worked forage tiles + 3. hunted herds, from the band's assignments. Each staffed source is
	# annotated with its per-turn `actual_yield` (LOD-suppressed at far zoom so tiny hexes stay clean).
	var show_yields := radius >= ICON_MIN_DETAIL_RADIUS
	for entry_variant in _labor_assignments_of_marker(band):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var kind := String(entry.get("kind", "")).strip_edges().to_lower()
		if int(entry.get("workers", 0)) <= 0:
			continue
		if kind == LABOR_KIND_FORAGE:
			var tcol := eff_col + _wrapped_col_delta(band_col, int(entry.get("target_x", -1)))
			var trow := int(entry.get("target_y", -1))
			if trow < 0 or trow >= grid_height:
				continue
			_fill_hex(tcol, trow, radius, origin, FORAGE_WORKED_FILL)
			_outline_hex(tcol, trow, radius, origin, FORAGE_WORKED_OUTLINE, FORAGE_WORKED_OUTLINE_WIDTH)
			# Forage patch: label the take. Sustain gathers at regrowth (actual == sustainable → plain
			# green), but a Surplus/Market/Eradicate policy overdraws (actual > sustainable + ε) → ⚠.
			if show_yields and entry.has("actual_yield"):
				var fcenter := _hex_center(tcol, trow, radius, origin)
				var forage_overdraw := float(entry.get("actual_yield", 0.0)) \
					> float(entry.get("sustainable_yield", 0.0)) + YIELD_OVERHUNT_EPSILON
				_draw_yield_label(fcenter, float(entry.get("actual_yield", 0.0)), forage_overdraw, radius)
		elif kind == LABOR_KIND_HUNT:
			var herd := _herd_by_id(String(entry.get("fauna_id", "")))
			var herd_col := int(entry.get("target_x", -1))
			var herd_row := int(entry.get("target_y", -1))
			if not herd.is_empty():
				herd_col = int(herd.get("x", herd_col))
				herd_row = int(herd.get("y", herd_row))
			if herd_col < 0 or herd_row < 0 or herd_row >= grid_height:
				continue
			var hc := _hex_center(eff_col + _wrapped_col_delta(band_col, herd_col), herd_row, radius, origin)
			# Link the band to the herd it is hunting (skip a wrap-spanning artifact).
			if absf(band_center.x - hc.x) <= last_map_size.x * 0.4:
				draw_line(band_center, hc, HUNT_WORKED_LINK_COLOR, HUNT_WORKED_LINK_WIDTH)
			draw_arc(hc, radius * HUNT_WORKED_RING_FACTOR, 0, TAU, 28, HUNT_WORKED_COLOR, HUNT_WORKED_RING_WIDTH)
			# Depletable herd: label the take, flagging overhunting when actual > sustainable + ε.
			if show_yields and entry.has("actual_yield"):
				var overhunt := float(entry.get("actual_yield", 0.0)) \
					> float(entry.get("sustainable_yield", 0.0)) + YIELD_OVERHUNT_EPSILON
				_draw_yield_label(hc, float(entry.get("actual_yield", 0.0)), overhunt, radius)

	# 5. Optimistic PENDING actions for this band (dashed amber): a just-issued assign/move that
	#    the snapshot hasn't confirmed yet. Drawn last so it reads on top of the confirmed styles.
	_draw_band_pending(band, band_col, band_row, eff_col, band_center, radius, origin)

	# 6. Travel destination: a confirmed in-progress move the snapshot reports (`is_traveling`).
	#    Line + reticle toward the wrapped-nearest copy of the target, so it follows the short
	#    (possibly seam-crossing) path the sim actually takes. Works for bands AND expeditions.
	_draw_travel_destination(band, band_col, band_row, eff_col, band_center, radius, origin)

## Draw the dashed-amber pending overlay for a band: pending forage tiles, pending hunted herds
## (dashed ring + dashed link), and a pending move destination (dashed tile + dashed link).
func _draw_band_pending(band: Dictionary, band_col: int, band_row: int, eff_col: int, band_center: Vector2, radius: float, origin: Vector2) -> void:
	var entity := int(band.get("entity", -1))
	var pend_variant: Variant = _labor_pending.get(entity, {})
	if not (pend_variant is Dictionary):
		return
	var pend: Dictionary = pend_variant
	var link_color := LABOR_PENDING_COLOR
	link_color.a = LABOR_PENDING_LINK_ALPHA
	var assigns_variant: Variant = pend.get("assign", {})
	if assigns_variant is Dictionary:
		for key in (assigns_variant as Dictionary):
			var a: Dictionary = (assigns_variant as Dictionary)[key]
			var kind := String(a.get("kind", "")).strip_edges().to_lower()
			if kind == LABOR_KIND_FORAGE:
				var trow := int(a.get("y", -1))
				if trow < 0 or trow >= grid_height:
					continue
				var tcol := eff_col + _wrapped_col_delta(band_col, int(a.get("x", -1)))
				_draw_dashed_hex(tcol, trow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
			elif kind == LABOR_KIND_HUNT:
				var herd := _herd_by_id(String(a.get("herd_id", "")))
				if herd.is_empty():
					continue
				var hrow := int(herd.get("y", -1))
				if hrow < 0 or hrow >= grid_height:
					continue
				var hcol := eff_col + _wrapped_col_delta(band_col, int(herd.get("x", -1)))
				var hc := _hex_center(hcol, hrow, radius, origin)
				_draw_dashed_hex(hcol, hrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
				if absf(band_center.x - hc.x) <= last_map_size.x * 0.4:
					_draw_dashed_line(band_center, hc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)
	var move_variant: Variant = pend.get("move", {})
	if move_variant is Dictionary and not (move_variant as Dictionary).is_empty():
		var mrow := int((move_variant as Dictionary).get("y", -1))
		if mrow >= 0 and mrow < grid_height:
			var mcol := eff_col + _wrapped_col_delta(band_col, int((move_variant as Dictionary).get("x", -1)))
			var mc := _hex_center(mcol, mrow, radius, origin)
			_draw_dashed_hex(mcol, mrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
			if absf(band_center.x - mc.x) <= last_map_size.x * 0.4:
				_draw_dashed_line(band_center, mc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)

## Draw the selected traveling unit's destination: a thin cyan line from its current tile to the
## wrapped-nearest copy of the `travel_target` hex + a target reticle on that hex. Only the target
## coords are read when `is_traveling` (they are `0,0` otherwise). Bringing the target into the
## band's effective column frame via `_wrapped_col_delta` makes the line follow the SHORT wrapped
## path (matching the sim's seam-crossing pathing) rather than shooting the long way across the map.
func _draw_travel_destination(unit: Dictionary, band_col: int, band_row: int, eff_col: int, band_center: Vector2, radius: float, origin: Vector2) -> void:
	if not bool(unit.get("is_traveling", false)):
		return
	var target_x := int(unit.get("travel_target_x", 0))
	var target_y := int(unit.get("travel_target_y", 0))
	if target_y < 0 or target_y >= grid_height:
		return
	# Already on the destination tile — nothing to draw (also guards a `0,0` slip-through).
	if target_x == band_col and target_y == band_row:
		return
	var dest_col := eff_col + _wrapped_col_delta(band_col, target_x)
	var dest_center := _hex_center(dest_col, target_y, radius, origin)
	var line_color := TRAVEL_DEST_COLOR
	line_color.a = TRAVEL_DEST_LINE_ALPHA
	draw_line(band_center, dest_center, line_color, TRAVEL_DEST_LINE_WIDTH)
	# Reticle marks the destination hex; no pulse (this is a steady, confirmed heading, unlike the
	# animated targeting reticle).
	_draw_reticle(dest_center, radius * TRAVEL_DEST_RETICLE_FACTOR, TRAVEL_DEST_COLOR, 1.0)

## Coordinator push (Hud.labor_pending_changed → Main → here): the per-band optimistic pending
## map. Stored + redrawn; the selected band's pending shows in a dashed-amber style.
func set_labor_pending(pending: Dictionary) -> void:
	_labor_pending = pending if pending is Dictionary else {}
	queue_redraw()

## A dashed line a→b (used for pending links). `dash`/`gap` are pixel lengths.
func _draw_dashed_line(a: Vector2, b: Vector2, color: Color, width: float, dash: float, gap: float) -> void:
	var delta := b - a
	var length := delta.length()
	if length <= 0.001:
		return
	var dir := delta / length
	var pos := 0.0
	while pos < length:
		var seg_end: float = minf(pos + dash, length)
		draw_line(a + dir * pos, a + dir * seg_end, color, width)
		pos = seg_end + gap

## A hex outline drawn as dashed edges (pending-tile marker).
func _draw_dashed_hex(col: int, row: int, radius: float, origin: Vector2, color: Color, width: float) -> void:
	var center := _hex_center(col, row, radius, origin)
	var pts := _hex_points(center, radius)
	for i in range(6):
		_draw_dashed_line(pts[i], pts[(i + 1) % 6], color, width, LABOR_PENDING_DASH, LABOR_PENDING_GAP)

## The selected band, if it is one of the player's own; {} otherwise.
func _selected_player_band() -> Dictionary:
	if selected_unit_id < 0:
		return {}
	for unit in units:
		if int(unit.get("entity", -1)) == selected_unit_id and _is_player_unit(unit):
			return unit
	return {}

func _labor_assignments_of_marker(band: Dictionary) -> Array:
	var v: Variant = band.get("labor_assignments", [])
	return v if v is Array else []

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
## resolves to the POSITIVE direct value, matching the sim — NOT `round()`'s half-away-from-zero
## (which flipped the sign at the antipode and pointed the travel line the wrong seam direction).
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

func _travel_arrow_color(task_kind: String) -> Color:
	match task_kind:
		"harvest":
			return Color(0.3, 0.8, 0.3, 0.85)  # Green
		"hunt":
			return Color(0.8, 0.3, 0.3, 0.85)  # Red
		"scout":
			return Color(0.3, 0.6, 0.9, 0.85)  # Blue
		_:
			return Color(0.7, 0.7, 0.7, 0.85)  # Gray

func _draw_label(pos: Vector2, text: String, max_width: float, font_size: int, color: Color) -> void:
	var font: Font = ThemeDB.fallback_font
	if font != null:
		draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, max_width, font_size, color)

# ---------------------------------------------------------------------------
# SECONDARY markers (herds / food sites / wondrous sites) — fixed edge-slot icons.
# ---------------------------------------------------------------------------

## Assign each SECONDARY marker a fixed edge slot on its hex, once per frame. Priority
## order wonder → food → herd, sequential fill, so a tile's icons never jump between
## frames. Beyond SECONDARY_VISIBLE_CAP the extras collapse into a `+N` overflow chip
## (drawn in the next slot). Visibility gating matches each category's own rule
## (herds/food Active-only; wonders any explored tile). Skipped entirely at far zoom.
func _compute_secondary_slots() -> void:
	_secondary_slot_lookup.clear()
	_secondary_overflow.clear()
	if last_hex_radius < ICON_MIN_DETAIL_RADIUS:
		return
	var per_tile: Dictionary = {}   # Vector2i -> Array[String] of entry keys, priority order
	for wsite in discovered_sites:
		var wx := int((wsite as Dictionary).get("x", -1))
		var wy := int((wsite as Dictionary).get("y", -1))
		if wx < 0 or wy < 0:
			continue
		if _visibility_state_at(wx, wy) == "unexplored":
			continue
		if String((wsite as Dictionary).get("glyph", "")) == "":
			continue
		_append_secondary(per_tile, Vector2i(wx, wy), _wonder_key(wsite))
	for site in food_sites:
		var fx := int((site as Dictionary).get("x", -1))
		var fy := int((site as Dictionary).get("y", -1))
		if fx < 0 or fy < 0 or not _is_tile_visible(fx, fy):
			continue
		_append_secondary(per_tile, Vector2i(fx, fy), _food_key(fx, fy))
	for herd in herds:
		var hx := int((herd as Dictionary).get("x", -1))
		var hy := int((herd as Dictionary).get("y", -1))
		if hx < 0 or hy < 0 or not _is_tile_visible(hx, hy):
			continue
		_append_secondary(per_tile, Vector2i(hx, hy), _herd_key(String((herd as Dictionary).get("id", ""))))
	for tile in per_tile:
		var keys: Array = per_tile[tile]
		for i in range(keys.size()):
			_secondary_slot_lookup[keys[i]] = i if i < SECONDARY_VISIBLE_CAP else -1
		if keys.size() > SECONDARY_VISIBLE_CAP:
			_secondary_overflow[tile] = keys.size() - SECONDARY_VISIBLE_CAP

func _append_secondary(per_tile: Dictionary, tile: Vector2i, key: String) -> void:
	var list: Array = per_tile.get(tile, [])
	list.append(key)
	per_tile[tile] = list

func _wonder_key(wsite: Dictionary) -> String:
	var fallback := "%d,%d" % [int(wsite.get("x", -1)), int(wsite.get("y", -1))]
	return "wonder:%s" % String(wsite.get("site_id", fallback))

func _food_key(x: int, y: int) -> String:
	return "food:%d,%d" % [x, y]

func _herd_key(herd_id: String) -> String:
	return "herd:%s" % herd_id

func _secondary_icon_size(radius: float) -> int:
	return int(maxf(SECONDARY_ICON_MIN_SIZE, radius * SECONDARY_ICON_SIZE_FACTOR))

func _secondary_slot_center(tile_center: Vector2, slot: int, radius: float) -> Vector2:
	return tile_center + SECONDARY_SLOT_OFFSETS[slot] * radius

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

## A small drop-shadow per-source yield label above a worked tile's center (reuses `_draw_marker_glyph`
## for legibility over terrain). Food-income green normally; WARN amber + a `⚠` suffix when `overhunt`.
func _draw_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float) -> void:
	var text := _format_yield_signed(value)
	var color := HudStyle.HEALTHY
	if overhunt:
		text += " " + YIELD_OVERHUNT_FLAG
		color = HudStyle.WARN
	var font_size := clampi(int(radius * YIELD_LABEL_SIZE_FACTOR), YIELD_LABEL_MIN_FONT, YIELD_LABEL_MAX_FONT)
	var label_center := tile_center + Vector2(0.0, -radius * YIELD_LABEL_OFFSET_FACTOR)
	_draw_marker_glyph(label_center, text, font_size, color)

## Signed, fixed-decimal food-rate string for the on-tile yield labels ("+0.48" / "-0.30"). Mirrors
## Hud's `_format_signed` (separate script); actual yields are ≥0 but the sign keeps it explicit.
func _format_yield_signed(value: float) -> String:
	var magnitude := String.num(absf(value), YIELD_LABEL_DECIMALS).pad_decimals(YIELD_LABEL_DECIMALS)
	return ("+" if value >= 0.0 else "-") + magnitude

## A small dark rounded pill with centered text — shared by the primary `×N` count
## badge and the secondary `+N` overflow chip (draw_rect body + two end-cap circles).
func _draw_count_pill(center: Vector2, text: String) -> void:
	var font: Font = ThemeDB.fallback_font
	if font == null or text == "":
		return
	var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, MARKER_BADGE_FONT_SIZE)
	var half_w: float = text_size.x * 0.5
	var half_h: float = text_size.y * 0.5 * MARKER_BADGE_HEIGHT_FACTOR
	draw_rect(Rect2(center.x - half_w, center.y - half_h, text_size.x, half_h * 2.0), MARKER_BADGE_BG)
	draw_circle(Vector2(center.x - half_w, center.y), half_h, MARKER_BADGE_BG)
	draw_circle(Vector2(center.x + half_w, center.y), half_h, MARKER_BADGE_BG)
	draw_string(font, Vector2(center.x - text_size.x * 0.5, center.y + text_size.y * 0.32), text, HORIZONTAL_ALIGNMENT_LEFT, -1, MARKER_BADGE_FONT_SIZE, MARKER_BADGE_FG)

## Per-tile `+N` overflow chip pass (secondaries beyond SECONDARY_VISIBLE_CAP).
func _draw_secondary_overflow(radius: float, origin: Vector2) -> void:
	if SECONDARY_VISIBLE_CAP >= SECONDARY_SLOT_OFFSETS.size():
		return
	for tile in _secondary_overflow:
		var tile_center: Vector2 = _hex_center_wrapped(tile.x, tile.y, radius, origin)
		var chip_center := _secondary_slot_center(tile_center, SECONDARY_VISIBLE_CAP, radius)
		_draw_count_pill(chip_center, "+%d" % int(_secondary_overflow[tile]))

func _draw_herd(herd: Dictionary, radius: float, origin: Vector2) -> void:
	var herd_id := String(herd.get("id", ""))
	var x: int = int(herd.get("x", -1))
	var y: int = int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _is_tile_visible(x, y):
		return
	var slot: int = _secondary_slot_lookup.get(_herd_key(herd_id), -1)
	if slot < 0:
		return   # far-zoom LOD or overflowed into the +N chip
	# Herd trail stays centered on the hex path (a route, not a marker), but only
	# when the herd icon itself draws — no orphaned trail for an LOD-suppressed or
	# overflowed herd (its slot is gone).
	_draw_herd_trail(herd_id, radius, origin)
	var tile_center: Vector2 = _hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	var herd_icon := FoodIcons.for_herd(String(herd.get("label", herd.get("id", "Herd"))))
	_draw_marker_glyph(icon_center, herd_icon, _secondary_icon_size(radius), SECONDARY_ICON_COLOR)

	# Migration arrow — thinner, and only on the hovered/selected herd tile to cut clutter.
	var tile := Vector2i(x, y)
	if tile == _hovered_tile or tile == selected_tile:
		var next_x := int(herd.get("next_x", -1))
		var next_y := int(herd.get("next_y", -1))
		if next_x >= 0 and next_y >= 0:
			var next_center := _hex_center_wrapped(next_x, next_y, radius, origin)
			var line_too_long: bool = abs(tile_center.x - next_center.x) > last_map_size.x * 0.4
			if not line_too_long:
				draw_line(tile_center, next_center, HERD_MIGRATION_ARROW_COLOR, HERD_MIGRATION_ARROW_WIDTH)
				_draw_arrowhead(tile_center, next_center, HERD_MIGRATION_ARROW_COLOR)

func _draw_food_site(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _is_tile_visible(x, y):
		return
	var slot: int = _secondary_slot_lookup.get(_food_key(x, y), -1)
	if slot < 0:
		return
	var tile_center: Vector2 = _hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	var module_key := String(site.get("module", ""))
	var kind := String(site.get("kind", ""))
	var is_hunt := kind == "game_trail"
	var icon := FoodIcons.for_site(module_key, is_hunt)
	if _food_harvest_active(x, y):
		draw_arc(icon_center, radius * FOOD_HARVEST_RING_FACTOR, 0, TAU, 20, Color(HudStyle.SIGNAL, 0.9), FOOD_HARVEST_RING_WIDTH)
	_draw_marker_glyph(icon_center, icon, _secondary_icon_size(radius), SECONDARY_ICON_COLOR)

func _draw_discovered_site(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	# A discovered site is permanent geographic knowledge, not current-state info — unlike a
	# herd (moves) or food site (Active-only). Persist its marker on any known/remembered tile
	# (Discovered or Active), not only Active, so it stays visible once found even under fog.
	if _visibility_state_at(x, y) == "unexplored":
		return
	var slot: int = _secondary_slot_lookup.get(_wonder_key(site), -1)
	if slot < 0:
		return
	var glyph := String(site.get("glyph", ""))
	if glyph == "":
		return
	var tile_center: Vector2 = _hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	_draw_marker_glyph(icon_center, glyph, _secondary_icon_size(radius), SECONDARY_ICON_COLOR)

func _draw_harvest_markers(radius: float, origin: Vector2) -> void:
	if harvest_sites.is_empty():
		return
	for key in harvest_sites.keys():
		var entries_variant: Variant = harvest_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _hex_center_wrapped(key.x, key.y, radius, origin)
		var module_key := String((entries[0] as Dictionary).get("module", ""))
		var style: Dictionary = FOOD_SITE_STYLE_DEFAULT
		var base_site: Variant = food_site_lookup.get(key, null)
		if base_site is Dictionary:
			var kind := String((base_site as Dictionary).get("kind", ""))
			style = FOOD_SITE_STYLES.get(kind, FOOD_SITE_STYLE_DEFAULT)
		var color: Color = style.get("color", FOOD_SITE_STYLE_DEFAULT["color"])
		var glow_color := color
		glow_color.a = 0.25
		draw_circle(center, radius * 0.65, glow_color)
		var stroke_color := color
		stroke_color.a = 0.95
		draw_arc(center, radius * 0.55, 0, TAU, 32, stroke_color, 3.0)
		if entries.size() > 1:
			var label := "x%d" % entries.size()
			_draw_label(center + Vector2(-radius * 0.25, radius * 0.05), label, radius * 0.6, int(radius * 0.4), Color(0, 0, 0, 0.85))
		if not (base_site is Dictionary) and _selected_tile_matches_food(key.x, key.y, module_key):
			var highlight_color := Color(1.0, 1.0, 1.0, 0.9)
			draw_arc(center, radius * 0.45, 0, TAU, 32, highlight_color, 2.5)

func _draw_scout_markers(radius: float, origin: Vector2) -> void:
	if scout_sites.is_empty():
		return
	for key in scout_sites.keys():
		var entries_variant: Variant = scout_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _hex_center_wrapped(key.x, key.y, radius, origin)
		var base_color := Color(0.8, 0.92, 1.0, 0.4)
		draw_circle(center, radius * 0.4, base_color)
		var stroke_color := Color(0.9, 0.97, 1.0, 0.95)
		draw_arc(center, radius * 0.5, 0, TAU, 24, stroke_color, 2.0)

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
			# expedition) in _draw_travel_destination.
			"travel_target_x": int(entry.get("travel_target_x", 0)),
			"travel_target_y": int(entry.get("travel_target_y", 0)),
			"days_of_food": float(entry.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS)),
			# Band food ledger (food/turn) — total income across worked sources vs total consumption.
			# Carried onto the marker so the allocation panel's ledger footer reads them off the
			# selected-unit copy (the per-source actual/sustainable yields ride inside labor_assignments).
			"food_income": float(entry.get("food_income", 0.0)),
			"food_consumption": float(entry.get("food_consumption", 0.0)),
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
			# Hard party-size cap (from the expedition config); the resident-band outfit stepper
			# clamps its max to min(idle_workers, this).
			"max_expedition_party_size": int(entry.get("max_expedition_party_size", 0)),
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

## Select a band/herd chosen from the HUD Occupants roster (no hex click). `kind` is
## "unit" (id = entity_id int) or "herd" (id = herd_id String). Sets
## `selected_unit_id`/`selected_herd_id` (and syncs `cycle_index`) so the picked occupant
## becomes the active/top stack card and the hex selection outline reflects it — there is
## no per-token ring; selection is the hex outline.
func select_occupant(kind: String, id) -> void:
	if kind == "unit":
		selected_unit_id = int(id)
		selected_herd_id = ""
		# Surface the roster-picked band as the top stack card, and seed cycling from it.
		cycle_index = _cycle_index_for_unit(selected_unit_id)
	elif kind == "herd":
		selected_herd_id = String(id)
		selected_unit_id = -1
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

func _unit_at_point(point: Vector2) -> Dictionary:
	for unit in units:
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		var center := _hex_center_wrapped(int(position[0]), int(position[1]), last_hex_radius, last_origin)
		if center.distance_to(point) <= last_hex_radius * 0.55:
			return unit
	return {}

func _herd_at_point(point: Vector2) -> Dictionary:
	for herd in herds:
		var x := int(herd.get("x", -1))
		var y := int(herd.get("y", -1))
		if x < 0 or y < 0:
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

func _units_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
	for unit in units:
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		if int(position[0]) == col and int(position[1]) == row:
			matches.append((unit as Dictionary).duplicate(true))
	return matches

func _herds_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
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
	return GRID_COLOR.lerp(overlay_color, overlay_value)

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
	## computed from the per-tile `_cached_terrain_ids` set in `display_snapshot`.
	## Empty before the first snapshot (no per-tile terrain cached yet) — callers
	## fall back to the full palette in that case.
	var seen: Dictionary = {}
	for raw_id in _cached_terrain_ids:
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
	for raw_id in _cached_terrain_ids:
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

func _draw_hydrology(radius: float, origin: Vector2) -> void:
	if hydrology_rivers.is_empty():
		return
	var river_color := (Color(0.95, 0.25, 0.25, 0.95) if highlight_rivers else Color(0.12, 0.5, 0.85, 0.85))
	var line_width := (4.0 if highlight_rivers else 3.0)
	for river in hydrology_rivers:
		if not (river is Dictionary):
			continue
		var points := Array(river.get("points", []))
		if points.size() < 2:
			continue
		# When FoW is enabled, only draw visible segments
		if _fow_enabled:
			var current_segment: PackedVector2Array = PackedVector2Array()
			for pt in points:
				if not (pt is Dictionary):
					continue
				var x := int(pt.get("x", 0))
				var y := int(pt.get("y", 0))
				var is_visible := _is_tile_visible(x, y)
				if is_visible:
					current_segment.append(_hex_center(x, y, radius, origin))
				else:
					# End current segment and start new one
					if current_segment.size() >= 2:
						draw_polyline(current_segment, river_color, line_width, false)
					current_segment = PackedVector2Array()
			# Draw final segment if any
			if current_segment.size() >= 2:
				draw_polyline(current_segment, river_color, line_width, false)
		else:
			# FoW disabled - draw entire river
			var poly: PackedVector2Array = PackedVector2Array()
			for pt in points:
				if not (pt is Dictionary):
					continue
				var x := int(pt.get("x", 0))
				var y := int(pt.get("y", 0))
				poly.append(_hex_center(x, y, radius, origin))
			if poly.size() >= 2:
				draw_polyline(poly, river_color, line_width, false)

func set_highlight_rivers(enabled: bool) -> void:
	highlight_rivers = enabled
	queue_redraw()

func toggle_terrain_mode() -> void:
	set_overlay_channel("")

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
		# Hunt-expedition targeting: glow every huntable herd (the valid targets) + reticle the
		# hovered hex, so it reads "click on a herd".
		for herd in herds:
			if not bool(herd.get("huntable", false)):
				continue
			var hx := int(herd.get("x", -1))
			var hy := int(herd.get("y", -1))
			if hx < 0 or hy < 0:
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

func _targeting_distance(col: int, row: int) -> int:
	var ox := int(_targeting.get("origin_x", -1))
	var oy := int(_targeting.get("origin_y", -1))
	if ox < 0 or oy < 0:
		return -1
	var a := _offset_to_axial(col, row)
	var b := _offset_to_axial(ox, oy)
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
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()

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
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()
	# Reaching here means the factor actually changed (the no-op / clamped-equal
	# cases early-returned above), so the readout only updates on a real change.
	emit_signal("zoom_changed", zoom_factor)

## Public zoom API — the on-screen zoom rail routes through the same `_apply_zoom`
## path the trackpad/wheel uses, so there is exactly one map-zoom code path.
## `direction` is +1 (in) / -1 (out); the pivot is the map center so button-zoom
## doesn't drift the view.
func zoom_step(direction: int) -> void:
	_apply_zoom(float(direction) * ZOOM_BUTTON_STEP, _viewport_center_pivot())

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
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()
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

# --- Terrain Texture System for 2D View (textures loaded via TerrainTextureManager autoload) ---

func _init_terrain_rendering() -> void:
	## Initialize 2D terrain rendering from TerrainTextureManager. The per-hex texture cache (blend-OFF
	## renderer) and the Approach-B blend shader both set up ONCE here; the layer Images are captured in
	## the manager (no GPU readback on rebuild). The id-map/vis-map splatmaps rebuild per snapshot.
	var mgr := TerrainTextureManager
	if mgr.terrain_textures != null and mgr.terrain_textures.get_layers() > 0:
		_build_terrain_blend_class_map()
		_build_hex_texture_cache()  # per-hex textures = the blend-OFF (use_edge_blending=false) renderer
		_setup_terrain_blend_shader()

func _build_hex_texture_cache() -> void:
	## Pre-render hex-masked textures from the terrain atlas
	var mgr := TerrainTextureManager
	if mgr.terrain_textures == null:
		return

	_hex_texture_cache.clear()
	var layer_count: int = mgr.terrain_textures.get_layers()

	for terrain_id in range(layer_count):
		var hex_tex := _render_hex_texture(terrain_id)
		if hex_tex != null:
			_hex_texture_cache[terrain_id] = hex_tex

	print("[MapView] Built hex texture cache: %d textures" % _hex_texture_cache.size())

func _render_hex_texture(terrain_id: int) -> ImageTexture:
	## Render a hex-masked texture for the given terrain ID (optimized)
	var mgr := TerrainTextureManager
	if mgr.terrain_textures == null or terrain_id < 0 or terrain_id >= mgr.terrain_textures.get_layers():
		return null

	var size := _hex_texture_size
	# Use the fixed get_terrain_image() which properly extracts from Texture2DArray
	var source_img: Image = mgr.get_terrain_image(terrain_id)
	if source_img == null:
		return null

	# Scale source image to output size
	var scaled_source := source_img.duplicate()
	scaled_source.resize(size, size)
	if scaled_source.get_format() != Image.FORMAT_RGBA8:
		scaled_source.convert(Image.FORMAT_RGBA8)

	# Ensure hex alpha mask is built (done once, reused for all textures)
	if _hex_alpha_mask.is_empty():
		_build_hex_alpha_mask(size)

	# Get raw pixel data and apply hex mask directly (much faster than get_pixel/set_pixel)
	var data: PackedByteArray = scaled_source.get_data()

	# Apply hex mask: set alpha to 0 for pixels outside the hex
	# RGBA8 format: 4 bytes per pixel, alpha is at offset 3
	for i in range(_hex_alpha_mask.size()):
		if _hex_alpha_mask[i] == 0:
			data[i * 4 + 3] = 0  # Set alpha to 0

	var output := Image.create_from_data(size, size, false, Image.FORMAT_RGBA8, data)
	return ImageTexture.create_from_image(output)


func _build_hex_alpha_mask(size: int) -> void:
	## Pre-compute hex mask once (reused for all terrain textures)
	_hex_alpha_mask.resize(size * size)
	var center := Vector2(size * 0.5, size * 0.5)
	var hex_radius := size * 0.51  # Slightly larger to ensure edge pixels are included

	for y in range(size):
		for x in range(size):
			var idx := y * size + x
			_hex_alpha_mask[idx] = 1 if _point_in_hex(Vector2(x, y), center, hex_radius) else 0

func _point_in_hex(point: Vector2, center: Vector2, radius: float) -> bool:
	# Check if a point is inside a pointy-top hexagon
	var dx := absf(point.x - center.x)
	var dy := absf(point.y - center.y)

	# Bounding box check
	if dy > radius:
		return false
	if dx > radius * SQRT3 * 0.5:
		return false

	# Edge check for hex shape
	return (radius * SQRT3 * 0.5 - dx) * 2.0 >= (dy - radius * 0.5) * SQRT3

func _draw_hex_textured(center: Vector2, terrain_id: int, radius: float) -> void:
	# Draw a textured hex at the given center position
	var color: Color = _terrain_color_for_id(terrain_id)
	var polygon_points := _hex_points(center, radius)

	var tex: ImageTexture = _hex_texture_cache.get(terrain_id)
	if tex == null:
		# No texture - just draw solid color
		draw_polygon(polygon_points, PackedColorArray([color, color, color, color, color, color]))
		return

	# Calculate UVs for the hex polygon
	# The texture is a square, so we map hex points to UV space
	var uvs := PackedVector2Array()
	for point in polygon_points:
		# Convert point relative to center into 0-1 UV range
		var uv := Vector2(
			(point.x - center.x) / radius * 0.5 + 0.5,
			(point.y - center.y) / radius * 0.5 + 0.5
		)
		uvs.append(uv)

	# Draw hex polygon with texture (clips texture to exact hex shape)
	var colors := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
	draw_polygon(polygon_points, colors, uvs, tex)

func get_terrain_textures_enabled() -> bool:
	var mgr := TerrainTextureManager
	return mgr.use_terrain_textures and mgr.terrain_textures != null

func enable_terrain_textures(enabled: bool) -> void:
	## Toggle terrain texture rendering for 2D view
	TerrainTextureManager.use_terrain_textures = enabled
	_invalidate_map_cache()  # cache bakes textured vs solid tiles; force a re-render
	queue_redraw()

## Toggle terrain textures on/off (bound to the T key). Flips the underlying
## intent flag directly rather than get_terrain_textures_enabled(), which also
## factors in atlas presence — using the getter would leave the toggle stuck
## "on" (and pop textures in later) whenever the atlas isn't loaded yet.
## No visible effect until an atlas is available, since rendering needs one.
func _toggle_terrain_textures() -> void:
	enable_terrain_textures(not TerrainTextureManager.use_terrain_textures)

func _build_terrain_blend_class_map() -> void:
	## Cache terrain_id -> blend_class ("flat"|"water"|"rugged") from config. Only flat↔flat seams
	## blend; water/rugged always render a hard edge. Single source of truth: terrain_config.json.
	_terrain_blend_class.clear()
	var terrains: Array = TerrainTextureManager.terrain_config.get("terrains", [])
	for entry: Variant in terrains:
		if entry is Dictionary:
			var tid: int = int(entry.get("id", -1))
			if tid >= 0:
				_terrain_blend_class[tid] = String(entry.get("blend_class", "rugged"))

func _terrain_is_flat(terrain_id: int) -> bool:
	## Flat biomes are the only ones eligible to blend across a seam (unknown ids → not flat).
	return String(_terrain_blend_class.get(terrain_id, "rugged")) == "flat"

func _blend_class_code(terrain_id: int) -> int:
	## Numeric blend class for the id-map G channel: 0 = water, 1 = flat, 2 = rugged.
	var c := String(_terrain_blend_class.get(terrain_id, "rugged"))
	if c == "flat":
		return 1
	if c == "water":
		return 0
	return 2

func _canopy_code(terrain_id: int) -> int:
	## Canopy code for the id-map B channel: 0 = no canopy, else canopy-array layer + 1.
	var layer := TerrainTextureManager.canopy_layer_for(terrain_id)
	return clampi(layer + 1, 0, 255)

func _peak_code(terrain_id: int) -> int:
	## Peak code for the id-map A channel: 0 = no peaks, else peak-array layer + 1.
	var layer := TerrainTextureManager.peak_layer_for(terrain_id)
	return clampi(layer + 1, 0, 255)

func _setup_terrain_blend_shader() -> void:
	## Create the Approach-B whole-map blend quad + its ShaderMaterial ONCE. The quad renders BEHIND
	## MapView's own draws (grid/markers) via show_behind_parent, so the material only shades terrain.
	## Non-fatal if the shader is missing — MapView then falls back to the per-hex textured path.
	var shader: Shader = load("res://assets/terrain/terrain_blend.gdshader")
	if shader == null:
		push_warning("[MapView] terrain_blend.gdshader missing — blend shader disabled")
		return
	_terrain_blend_material = ShaderMaterial.new()
	_terrain_blend_material.shader = shader
	_terrain_blend_material.set_shader_parameter("biome_array", TerrainTextureManager.terrain_textures)
	# Per-layer mean luminance (1×N, fetched by layer index): the zero-point of each base texture's
	# pseudo-height for the flat↔flat HEIGHT BLENDING. Built once with the base array, so it's set once here.
	_terrain_blend_material.set_shader_parameter("layer_luma_map", TerrainTextureManager.layer_luma_texture)
	# Per-water-terrain SHORE PROFILE (1×N RGBA float, fetched by layer index): R = sand_scale (the beach's
	# inland reach; 0 = a CLIFF), G = foam_scale (the main wave's reaches, never its peak), B = wisp_scale (the
	# offshore disturbance's centre, half-width and strength). Applied to the shore pass on the WATER side's
	# terrain — a deep-ocean cliff, a shelf beach and an inland-sea lake wear different coasts — and blended
	# across the water neighbours by shared-edge proximity, so those coasts transition rather than switch.
	# Neutral (1, 1, 1) for every terrain without a `shore_profile` block. Bound once (the manager updates
	# the texture in place on a rebuild, so the binding survives).
	_terrain_blend_material.set_shader_parameter("layer_shore_map", TerrainTextureManager.layer_shore_texture)
	# Canopy: a SECOND Texture2DArray in the same canvas shader. Disabled (and the sampler harmlessly
	# bound to the base array) when no canopy asset exists.
	var canopy_arr: Texture2DArray = TerrainTextureManager.canopy_textures
	_terrain_blend_material.set_shader_parameter("canopy_enabled", canopy_arr != null)
	_terrain_blend_material.set_shader_parameter("canopy_tex", canopy_arr if canopy_arr != null else TerrainTextureManager.terrain_textures)
	# Peaks: a THIRD Texture2DArray in the same canvas shader (mountain relief). Disabled (and the sampler
	# harmlessly bound to the base array) when no peak asset exists.
	var peak_arr: Texture2DArray = TerrainTextureManager.peak_textures
	_terrain_blend_material.set_shader_parameter("peaks_enabled", peak_arr != null and peak_arr.get_layers() > 0)
	_terrain_blend_material.set_shader_parameter("peak_tex", peak_arr if peak_arr != null else TerrainTextureManager.terrain_textures)
	var QuadScript: GDScript = preload("res://src/scripts/TerrainBlendQuad.gd")
	_terrain_blend_quad = QuadScript.new()
	_terrain_blend_quad.name = "TerrainBlendQuad"
	_terrain_blend_quad.material = _terrain_blend_material
	_terrain_blend_quad.show_behind_parent = true
	_terrain_blend_quad.visible = false
	add_child(_terrain_blend_quad)
	move_child(_terrain_blend_quad, 0)  # keep it first so it draws behind
	_terrain_blend_ready = true
	print("[MapView] Terrain blend shader ready (Approach B)")

func _has_terrain_textures() -> bool:
	return TerrainTextureManager.use_terrain_textures and TerrainTextureManager.terrain_textures != null

func _shader_terrain_active() -> bool:
	## The Approach-B shader renders the base terrain when textures are on, no overlay is selected, and
	## edge blending is enabled. Otherwise the per-hex texture path (blend OFF) or overlay/solid path runs.
	return _terrain_blend_ready and TerrainTextureManager.use_edge_blending \
		and active_overlay_key == "" and _has_terrain_textures()

func _update_terrain_shader_quad(radius: float, origin: Vector2, viewport_size: Vector2) -> void:
	## Push the exact hex-layout + blend uniforms (so terrain aligns with grid/markers), size the quad
	## to the usable rect (bounds the shader to the area beside the Inspector strip), and show it.
	if _terrain_blend_material == null or _terrain_blend_quad == null:
		return
	if _terrain_id_map_tex == null:
		_rebuild_terrain_shader_maps()
	var config: Dictionary = TerrainTextureManager.terrain_config
	var blend_width: float = clampf(float(config.get("blend_width", EDGE_BLEND_DEFAULT_WIDTH)), 0.02, 1.0)
	# Feather softness + the detail-following nudge (see EDGE_BLEND_DEFAULT_SOFT / _HEIGHT_INFLUENCE).
	var blend_soft: float = clampf(
		float(config.get("blend_soft", EDGE_BLEND_DEFAULT_SOFT)), 0.0, EDGE_BLEND_MAX_SOFT
	)
	# Hard-clamped to EDGE_BLEND_MAX_HEIGHT_INFLUENCE: the height term is only ever a NUDGE. Letting it
	# out-vote the distance weight is what shredded prairie hexes (see EDGE_BLEND_DEFAULT_HEIGHT_INFLUENCE).
	var blend_height_influence: float = clampf(
		float(config.get("blend_height_influence", EDGE_BLEND_DEFAULT_HEIGHT_INFLUENCE)),
		0.0,
		EDGE_BLEND_MAX_HEIGHT_INFLUENCE
	)
	# Seam-wobble cell is a FRACTION of the hex radius (× radius → px, like blend_width) so the boundary
	# meanders at the same scale relative to a hex at every zoom (see EDGE_BLEND_DEFAULT_NOISE_SCALE).
	var blend_noise_scale: float = clampf(
		float(config.get("blend_noise_scale", EDGE_BLEND_DEFAULT_NOISE_SCALE)), 0.02, 4.0
	)
	var blend_noise_amount: float = clampf(
		float(config.get("blend_noise_amount", EDGE_BLEND_DEFAULT_NOISE_AMOUNT)), 0.0, 2.0
	)
	var feature_noise_cell: float = maxf(
		float(config.get("feature_noise_cell", EDGE_BLEND_DEFAULT_FEATURE_NOISE_CELL)), 1.0
	)  # shoreline reach/wisp + canopy treeline + peak footline
	# Widens the LAND seam gate from same-class to both-sides-land (see EDGE_BLEND_DEFAULT_RUGGED_LAND).
	var blend_rugged_land: bool = bool(
		config.get("blend_rugged_land", EDGE_BLEND_DEFAULT_RUGGED_LAND)
	)
	# WATER↔WATER overrides (wider/softer/wobblier than land — see WATER_BLEND_DEFAULT_*). Same clamps as
	# the land levers, so water can never exceed the caps the land path is held to either.
	var water_blend: Dictionary = config.get("water_blend", {})
	var water_width: float = clampf(
		float(water_blend.get("blend_width", WATER_BLEND_DEFAULT_WIDTH)), 0.02, 1.0
	)
	var water_soft: float = clampf(
		float(water_blend.get("blend_soft", WATER_BLEND_DEFAULT_SOFT)), 0.0, EDGE_BLEND_MAX_SOFT
	)
	var water_noise_amount: float = clampf(
		float(water_blend.get("blend_noise_amount", WATER_BLEND_DEFAULT_NOISE_AMOUNT)), 0.0, 2.0
	)
	var m := _terrain_blend_material
	m.set_shader_parameter("grid_w", grid_width)
	m.set_shader_parameter("grid_h", grid_height)
	m.set_shader_parameter("hex_radius", radius)
	m.set_shader_parameter("hex_origin", origin)
	m.set_shader_parameter("wrap_h", _wrap_horizontal)
	m.set_shader_parameter("blend_band", blend_width * radius)  # transition half-band width in px
	m.set_shader_parameter("blend_soft", blend_soft)                          # feather half-width
	m.set_shader_parameter("blend_height_influence", blend_height_influence)  # detail-following NUDGE
	m.set_shader_parameter("blend_noise_cell", blend_noise_scale * radius)  # seam-wobble cell in px
	m.set_shader_parameter("blend_noise_amount", blend_noise_amount)        # seam-wobble amplitude
	m.set_shader_parameter("blend_rugged_land", blend_rugged_land)          # rugged base floors blend too
	# The water↔water triple the shader swaps in when the hex's blend_class is water (see the same-class gate).
	m.set_shader_parameter("water_blend_band", water_width * radius)
	m.set_shader_parameter("water_blend_soft", water_soft)
	m.set_shader_parameter("water_blend_noise_amount", water_noise_amount)
	m.set_shader_parameter("noise_cell", feature_noise_cell)   # shore/canopy/peak grain — raw px, decoupled
	# Base biome texture is sampled in continuous world space (kills the per-hex repeat grid); one tile
	# spans ~1/base_scale hex-rows. See BASE_DEFAULT_TEXTURE_SCALE / CLAUDE.md → Edge Blending.
	var base_scale: float = maxf(float(config.get("base_texture_scale", BASE_DEFAULT_TEXTURE_SCALE)), 0.01)
	m.set_shader_parameter("base_scale", base_scale)
	m.set_shader_parameter("blend_enabled", radius >= EDGE_BLEND_MIN_RADIUS)  # LOD: base-only at far zoom
	# Shoreline: the three reaches of the ONE continuous land→sand→surf→water profile, measured from the
	# coastline (the signed coast coordinate u in the shader), inland and seaward.
	var shore: Dictionary = config.get("shore", {})
	var sand_frac: float = clampf(
		float(shore.get("sand_width", SHORE_DEFAULT_SAND_WIDTH)), 0.0, 2.0)
	var foam_inland_frac: float = clampf(
		float(shore.get("foam_inland_width", SHORE_DEFAULT_FOAM_INLAND_WIDTH)), 0.0, 2.0)
	var foam_frac: float = clampf(float(shore.get("foam_width", SHORE_DEFAULT_FOAM_WIDTH)), 0.0, 2.0)
	var wisp_center_frac: float = clampf(
		float(shore.get("wisp_center_width", SHORE_DEFAULT_WISP_CENTER_WIDTH)), 0.0, 2.0)
	var wisp_half_frac: float = clampf(
		float(shore.get("wisp_half_width", SHORE_DEFAULT_WISP_HALF_WIDTH)), 0.0, 2.0)
	# The waterline base cross-fade (the wet edge that removed the base's own step at u = 0) and the surf's
	# peak opacity, which only became a lever once that step was gone. See the SHORE_DEFAULT_* consts.
	var waterline_frac: float = clampf(
		float(shore.get("waterline_width", SHORE_DEFAULT_WATERLINE_WIDTH)), 0.0, 1.0)
	var foam_opacity: float = clampf(
		float(shore.get("foam_opacity", SHORE_DEFAULT_FOAM_OPACITY)), 0.0, 1.0)
	m.set_shader_parameter("waterline_band", waterline_frac * radius)  # base cross-fade half-reach (px)
	m.set_shader_parameter("foam_opacity", foam_opacity)               # surf + wisp peak opacity
	m.set_shader_parameter("sand_band", sand_frac * radius)            # sand INLAND of the waterline (px)
	m.set_shader_parameter("foam_inland_band", foam_inland_frac * radius)  # surf washing UP the beach (px)
	m.set_shader_parameter("foam_band", foam_frac * radius)            # surf SEAWARD of the waterline (px)
	m.set_shader_parameter("wisp_center_band", wisp_center_frac * radius)  # 2nd surf line's centre (px)
	m.set_shader_parameter("wisp_half_band", wisp_half_frac * radius)      # 2nd surf line's half-width (px)
	m.set_shader_parameter("foam_color", _shore_color(shore.get("foam_color", null), SHORE_DEFAULT_FOAM_COLOR))
	m.set_shader_parameter("beach_color", _shore_color(shore.get("beach_color", null), SHORE_DEFAULT_BEACH_COLOR))
	var canopy: Dictionary = config.get("canopy", {})
	var overhang_frac: float = clampf(float(canopy.get("overhang_width", CANOPY_DEFAULT_OVERHANG_WIDTH)), 0.0, 2.0)
	var softness_frac: float = clampf(float(canopy.get("softness_width", CANOPY_DEFAULT_SOFTNESS_WIDTH)), 0.02, 2.0)
	var canopy_scale: float = maxf(float(canopy.get("texture_scale", CANOPY_DEFAULT_TEXTURE_SCALE)), 0.05)
	var canopy_min_radius: float = maxf(float(canopy.get("canopy_min_radius", CANOPY_DEFAULT_MIN_RADIUS)), 0.0)
	m.set_shader_parameter("canopy_overhang", overhang_frac * radius) # crown overhang past the treeline (px)
	m.set_shader_parameter("canopy_softness", softness_frac * radius) # inner treeline ramp half-width (px)
	m.set_shader_parameter("canopy_scale", canopy_scale)
	# Canopy LOD is DECOUPLED from the blend LOD (blend_enabled): the canopy pass keeps running far below
	# EDGE_BLEND_MIN_RADIUS so forests stay a distinct darker-green mass at far zoom (see CANOPY_DEFAULT_MIN_RADIUS).
	m.set_shader_parameter("canopy_lod_enabled", radius >= canopy_min_radius)
	# Peak overlay (highland/volcanic relief): overhang/softness/shadow are px = fraction × radius (like
	# canopy); prominence/strength are unit scalars; light_dir points TOWARD the light. LOD is decoupled
	# from the flat↔flat blend gate (own peak_min_radius), so the mountain mass persists at far zoom.
	var peaks: Dictionary = config.get("peaks", {})
	var peak_overhang_frac: float = clampf(float(peaks.get("overhang_width", PEAK_DEFAULT_OVERHANG_WIDTH)), 0.0, 2.0)
	var peak_softness_frac: float = clampf(float(peaks.get("softness_width", PEAK_DEFAULT_SOFTNESS_WIDTH)), 0.02, 2.0)
	var peak_scale: float = maxf(float(peaks.get("texture_scale", PEAK_DEFAULT_TEXTURE_SCALE)), 0.05)
	var peak_min_radius: float = maxf(float(peaks.get("peak_min_radius", PEAK_DEFAULT_MIN_RADIUS)), 0.0)
	var peak_shadow_frac: float = clampf(float(peaks.get("shadow_length", PEAK_DEFAULT_SHADOW_LENGTH)), 0.0, 4.0)
	var peak_shadow_strength: float = clampf(float(peaks.get("shadow_strength", PEAK_DEFAULT_SHADOW_STRENGTH)), 0.0, 1.0)
	var peak_min_prominence: float = clampf(float(peaks.get("min_prominence", PEAK_DEFAULT_MIN_PROMINENCE)), 0.0, 1.0)
	var peak_light_dir: Vector2 = Vector2(
		float(peaks.get("light_dir_x", PEAK_DEFAULT_LIGHT_DIR.x)),
		float(peaks.get("light_dir_y", PEAK_DEFAULT_LIGHT_DIR.y)))
	if peak_light_dir.length() < 0.0001:
		peak_light_dir = PEAK_DEFAULT_LIGHT_DIR
	peak_light_dir = peak_light_dir.normalized()
	m.set_shader_parameter("peaks_lod_enabled", radius >= peak_min_radius)
	m.set_shader_parameter("peak_overhang", peak_overhang_frac * radius)  # relief overhang past the footline (px)
	m.set_shader_parameter("peak_softness", peak_softness_frac * radius)  # inner footline ramp half-width (px)
	m.set_shader_parameter("peak_scale", peak_scale)
	m.set_shader_parameter("peak_shadow_len", peak_shadow_frac * radius)  # cast-shadow reach toward the light (px)
	m.set_shader_parameter("peak_shadow_strength", peak_shadow_strength)
	m.set_shader_parameter("peak_min_prominence", peak_min_prominence)
	m.set_shader_parameter("peak_light_dir", peak_light_dir)
	m.set_shader_parameter("fow_enabled", _fow_enabled)
	m.set_shader_parameter("bg_color", TERRAIN_BG_COLOR)
	m.set_shader_parameter("fog_color", _fow_fog_fill_color)
	m.set_shader_parameter("mist_color", Vector3(_fow_mist_color.r, _fow_mist_color.g, _fow_mist_color.b))
	m.set_shader_parameter("mist_blend", _fow_mist_blend)
	# FoW boundary softening: radius-relative (like blend_band) so the mist gradient is zoom-invariant.
	m.set_shader_parameter("fow_soft", _fow_softness * radius)
	m.set_shader_parameter("fow_noise_amount", _fow_noise_amount)
	_terrain_blend_quad.visible = true
	_terrain_blend_quad.set_rect_size(viewport_size)
	_terrain_blend_quad.queue_redraw()

func _hide_terrain_shader_quad() -> void:
	if _terrain_blend_quad != null and _terrain_blend_quad.visible:
		_terrain_blend_quad.visible = false

func _shore_color(raw, fallback: Vector3) -> Vector3:
	## Parse a config [r,g,b] (0–255) shoreline color into a normalized Vector3 shader uniform, falling
	## back to the named default when the key is absent/malformed.
	if raw is Array and raw.size() >= 3:
		return Vector3(float(raw[0]), float(raw[1]), float(raw[2])) / 255.0
	return fallback

func _rebuild_terrain_shader_maps() -> void:
	## (Re)build the id-map (RGBA8: R=terrain id, G=blend_class code, B=canopy code, A=peak code) +
	## vis-map (R8: FoW state) + elev-map (R8: per-hex relative height for peak prominence/shadow)
	## splatmaps, one texel per hex, from the current terrain + FoW + elevation. Called each snapshot.
	## NEAREST-sampled in-shader.
	if grid_width <= 0 or grid_height <= 0 or _cached_terrain_ids.is_empty():
		return
	var w := grid_width
	var h := grid_height
	var id_bytes := PackedByteArray()
	id_bytes.resize(w * h * 4)
	var vis_bytes := PackedByteArray()
	vis_bytes.resize(w * h)
	var elev_bytes := PackedByteArray()
	elev_bytes.resize(w * h)
	# Hoist the per-hex-invariant raster fetches + sea-level math out of the double loop:
	# both the FoW visibility channel and the elevation channel (plus its sea-level rescale
	# constants) are the same for every hex, so fetch/compute them once here instead of
	# re-running _visibility_state_at / relative_height_at per hex (each of which re-did an
	# _overlay_raw_array dict lookup). The per-hex byte encoding below reproduces those two
	# helpers exactly, including the empty-raster fallbacks.
	var vis_raster := _visibility_array()  # raw visibility channel (see _visibility_state_at)
	var elev_raster := _overlay_raw_array("elevation")  # raw elevation channel (see relative_height_at)
	var elev_has_data := not elev_raster.is_empty()  # matches relative_height_at's -1 (missing) guard
	# Sea-level rescale constants (see relative_height_at): above-sea span normalized into 0..1.
	var elev_sea_level := clampf(_elevation_sea_level, 0.0, 0.999)
	var elev_span := 1.0 - elev_sea_level
	for y in range(h):
		for x in range(w):
			var idx := y * w + x
			var tid := _terrain_id_at(x, y)
			id_bytes[idx * 4] = clampi(tid, 0, 255) if tid >= 0 else 0
			id_bytes[idx * 4 + 1] = _blend_class_code(tid)
			id_bytes[idx * 4 + 2] = _canopy_code(tid)
			id_bytes[idx * 4 + 3] = _peak_code(tid)
			# Mirror _visibility_state_at's active/discovered/unexplored classification (guarded by
			# _fow_enabled) directly into the 255/128/0 byte, reading the hoisted raster.
			var v := 255
			if _fow_enabled:
				var vis := _value_at(vis_raster, x, y)
				if vis > FOW_VISIBLE_THRESHOLD:
					v = 255
				elif vis > FOW_EXPLORED_THRESHOLD:
					v = 128
				else:
					v = 0
			vis_bytes[idx] = v
			# Per-hex relative height (0..100 → 0..255) for peak prominence + shadow scaling; the
			# PEAK_ELEV_FALLBACK keeps relief rendering when a snapshot carries no elevation raster.
			# Mirrors relative_height_at() against the hoisted raster/constants.
			if elev_has_data:
				var above_sea := clampf((_value_at(elev_raster, x, y) - elev_sea_level) / elev_span, 0.0, 1.0)
				var rh := int(round(above_sea * 100.0))
				elev_bytes[idx] = clampi(int(round(float(rh) * 2.55)), 0, 255)
			else:
				elev_bytes[idx] = PEAK_ELEV_FALLBACK
	var id_img := Image.create_from_data(w, h, false, Image.FORMAT_RGBA8, id_bytes)
	_terrain_id_map_tex = ImageTexture.create_from_image(id_img)
	var vis_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, vis_bytes)
	_terrain_vis_map_tex = ImageTexture.create_from_image(vis_img)
	var elev_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, elev_bytes)
	_terrain_elev_map_tex = ImageTexture.create_from_image(elev_img)
	if _terrain_blend_material != null:
		_terrain_blend_material.set_shader_parameter("id_map", _terrain_id_map_tex)
		_terrain_blend_material.set_shader_parameter("vis_map", _terrain_vis_map_tex)
		_terrain_blend_material.set_shader_parameter("elev_map", _terrain_elev_map_tex)

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
## Must be called before _ready() or _setup_2d_minimap() for embedded mode to work.
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
	if _minimap_2d != null and _minimap_2d.has_method("queue_indicator_redraw"):
		_minimap_2d.queue_indicator_redraw()

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

func _setup_2d_minimap() -> void:
	_minimap_2d = MinimapPanelScript.new()
	add_child(_minimap_2d)

	# Prefer embedded mode if HUD reference is available
	var embedded := false
	if _hud_layer != null and _hud_layer.has_method("get_minimap_container"):
		var container: Control = _hud_layer.get_minimap_container()
		if container != null:
			_minimap_2d.setup_embedded(container)
			embedded = true

	if not embedded:
		# Fallback to floating mode (legacy behavior)
		_minimap_2d.setup(self, MinimapPanelScript.MINIMAP_CANVAS_LAYER)

	_minimap_2d.pan_requested.connect(_on_minimap_2d_pan_requested)
	_minimap_2d.connect_indicator_draw(_draw_minimap_viewport_indicator)

func _update_2d_minimap() -> void:
	if grid_width == 0 or grid_height == 0:
		return

	# Lazy initialization: set up minimap on first call (after Main.gd has a chance to set HUD reference)
	if _minimap_2d == null:
		_setup_2d_minimap()

	_minimap_2d.set_visible(true)

	# Check if we need to regenerate the minimap image
	var current_size := Vector2i(grid_width, grid_height)
	var size_changed := _minimap_2d_last_grid_size != current_size
	var data_changed := _minimap_2d_last_data_version != _minimap_2d_data_version
	var fow_changed := _minimap_2d_last_fow_enabled != _fow_enabled
	var needs_rebuild := _minimap_2d_image == null or size_changed or data_changed or fow_changed

	if needs_rebuild:
		_minimap_2d_last_grid_size = current_size
		_minimap_2d_last_data_version = _minimap_2d_data_version
		_minimap_2d_last_fow_enabled = _fow_enabled
		_rebuild_minimap_2d_image()

	# Update viewport indicator
	_minimap_2d.queue_indicator_redraw()

func _rebuild_minimap_2d_image() -> void:
	if grid_width == 0 or grid_height == 0:
		return

	# Cache terrain colors lookup for faster access
	var colors := _get_terrain_colors()
	var fallback_color := Color(0.2, 0.2, 0.2, 1.0)
	var fog_color := _fow_fog_fill_color  # Dark color for unexplored
	var mist_color := _fow_mist_color  # Light gray-blue mist for explored-but-not-visible

	# Get visibility data for FoW (if enabled)
	var visibility_data: PackedFloat32Array = PackedFloat32Array()
	if _fow_enabled:
		visibility_data = _visibility_array()

	# The minimap always renders the FULL grid so its shape/aspect ratio stays
	# constant whether FoW is on or off; unexplored tiles are painted as fog in
	# the pixel loop below (standard 4X behaviour — no unseen terrain is revealed).
	# Explored bounds are still computed when FoW is on so that panning stays
	# clamped to discovered space (see _clamp_pan_offset).
	var img_width := grid_width
	var img_height := grid_height

	# Pre-allocate byte array for RGB8 image data (3 bytes per pixel)
	var pixel_count := img_width * img_height
	var data := PackedByteArray()
	data.resize(pixel_count * 3)

	# Track the bounding box of explored tiles while painting (FoW only), so pan
	# clamping can use it without a second full pass over the visibility array.
	# Gate fog on _fow_enabled alone (not on visibility_data being populated): when
	# FoW is on but the visibility channel hasn't streamed yet, the per-tile lookup
	# below falls back to vis == 0.0, so every tile paints as fog rather than leaking
	# the unexplored map as full terrain. Explored bounds simply stay empty.
	var min_col := grid_width
	var max_col := -1
	var min_row := grid_height
	var max_row := -1

	# Fill byte array with terrain colors
	var byte_index := 0
	for grid_row in range(img_height):
		for grid_col in range(img_width):
			var grid_index := grid_row * grid_width + grid_col

			var terrain_id := int(terrain_overlay[grid_index]) if grid_index < terrain_overlay.size() else -1
			var color: Color = colors.get(terrain_id, fallback_color)

			# Apply Fog of War visibility
			if _fow_enabled:
				var vis: float = visibility_data[grid_index] if grid_index < visibility_data.size() else 0.0
				if vis <= 0.0:
					# Unexplored - show dark fog
					color = fog_color
				else:
					# Explored (discovered or active) - grow the explored bounds
					min_col = mini(min_col, grid_col)
					max_col = maxi(max_col, grid_col)
					min_row = mini(min_row, grid_row)
					max_row = maxi(max_row, grid_row)
					if vis <= FOW_VISIBLE_THRESHOLD:
						# Explored but not currently visible - show terrain with light mist overlay
						# Desaturate slightly and blend with mist to show "remembered" state
						color = color.lerp(mist_color, _fow_mist_blend)
					# else: vis > FOW_VISIBLE_THRESHOLD - fully visible, use terrain color as-is

			# Convert Color (0-1 floats) to RGB bytes (0-255)
			data[byte_index] = int(color.r * 255.0)
			data[byte_index + 1] = int(color.g * 255.0)
			data[byte_index + 2] = int(color.b * 255.0)
			byte_index += 3

	# Update world bounds for pan clamping (at unit radius, scaled in _clamp_pan_offset).
	# Cleared when FoW is off (full map) or nothing is explored yet.
	if _fow_enabled and max_col >= 0:
		var explored := Rect2i(min_col, min_row, max_col - min_col + 1, max_row - min_row + 1)
		_explored_bounds_world = _compute_explored_bounds_world(explored, 1.0)
	else:
		_explored_bounds_world = Rect2()

	# Create image from byte array
	_minimap_2d_image = Image.create_from_data(img_width, img_height, false, Image.FORMAT_RGB8, data)

	# Create texture from image and update panel
	var tex := ImageTexture.create_from_image(_minimap_2d_image)
	_minimap_2d.set_texture(tex)
	_minimap_2d.set_grid_size(img_width, img_height)

## Draw the viewport indicator rectangle on the 2D minimap.
##
## This shows which portion of the map is currently visible in the main view.
## The coordinate transformation uses the same axial hex math as _point_to_offset:
##
## Screen-to-Hex Coordinate Conversion (pointy-top hexes):
##   1. Subtract origin and divide by hex radius to get relative position
##   2. Convert to axial coordinates (q, r) using pointy-top hex formulas:
##      q = (sqrt(3)/3 * x - 1/3 * y)
##      r = (2/3 * y)
##   3. Round to nearest hex using cube coordinate rounding
##   4. Convert axial (q, r) to offset (col, row) coordinates
##
## The resulting hex coordinates are then normalized to [0,1] range and
## mapped to pixel positions within the minimap texture display area.
func _draw_minimap_viewport_indicator() -> void:
	if _minimap_2d == null or grid_width == 0 or grid_height == 0:
		return
	if last_hex_radius <= 0:
		return

	var viewport_size := _get_adjusted_viewport_size()
	if viewport_size.x <= 0 or viewport_size.y <= 0:
		return

	var radius: float = max(last_hex_radius, 0.0001)

	# Use the visible column/row range stored during the last render
	# This ensures the indicator matches exactly what's being drawn
	var tl_col_f: float = _last_visible_col_start
	var tl_row_f: float = _last_visible_row_start
	var br_col_f: float = _last_visible_col_end
	var br_row_f: float = _last_visible_row_end

	# Normalize hex coordinates to [0,1] range for minimap positioning.
	# The minimap image spans the full grid (FoW or not), so normalize against it.
	var view_left: float
	var view_right: float
	var view_top: float
	var view_bottom: float

	if _wrap_horizontal:
		# When wrapping, don't clamp X - allow values outside [0,1] to indicate wrap
		view_left = tl_col_f / float(grid_width)
		view_right = br_col_f / float(grid_width)
		view_top = clampf(tl_row_f / float(grid_height), 0.0, 1.0)
		view_bottom = clampf(br_row_f / float(grid_height), 0.0, 1.0)
	else:
		# Full grid normalization with clamping
		view_left = clampf(tl_col_f / float(grid_width), 0.0, 1.0)
		view_right = clampf(br_col_f / float(grid_width), 0.0, 1.0)
		view_top = clampf(tl_row_f / float(grid_height), 0.0, 1.0)
		view_bottom = clampf(br_row_f / float(grid_height), 0.0, 1.0)

	# Map normalized coords to pixel positions within minimap texture display area
	var texture_display_rect: Rect2 = _minimap_2d.get_texture_display_rect()
	var indicator_color := Color(1.0, 1.0, 1.0, 0.8)

	# Calculate viewport width in normalized coords
	var viewport_width_norm := view_right - view_left

	# When wrapping is enabled and viewport spans the wrap boundary, may need split rectangles
	# But if viewport shows >= entire map width, draw full-width indicator instead
	if _wrap_horizontal and (view_left < 0.0 or view_right > 1.0):
		if viewport_width_norm >= 1.0:
			# Viewport shows entire map width or more - draw full-width indicator
			var rect := Rect2(
				texture_display_rect.position.x,
				texture_display_rect.position.y + view_top * texture_display_rect.size.y,
				texture_display_rect.size.x,
				(view_bottom - view_top) * texture_display_rect.size.y
			)
			_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)
		else:
			# Wrap the normalized coordinates to [0,1] range
			var wrapped_left := fposmod(view_left, 1.0)
			var wrapped_right := fposmod(view_right, 1.0)

			# If viewport spans wrap, wrapped_right < wrapped_left
			if wrapped_right < wrapped_left:
				# Draw left portion (from wrapped_left to right edge)
				var rect_left := Rect2(
					texture_display_rect.position.x + wrapped_left * texture_display_rect.size.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					(1.0 - wrapped_left) * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect_left, indicator_color, false, 2.0)

				# Draw right portion (from left edge to wrapped_right)
				var rect_right := Rect2(
					texture_display_rect.position.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					wrapped_right * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect_right, indicator_color, false, 2.0)
			else:
				# Viewport doesn't span wrap, just draw single rectangle at wrapped position
				var rect := Rect2(
					texture_display_rect.position.x + wrapped_left * texture_display_rect.size.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					(wrapped_right - wrapped_left) * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)
	else:
		# Standard non-wrapping case
		var rect := Rect2(
			texture_display_rect.position.x + view_left * texture_display_rect.size.x,
			texture_display_rect.position.y + view_top * texture_display_rect.size.y,
			(view_right - view_left) * texture_display_rect.size.x,
			(view_bottom - view_top) * texture_display_rect.size.y
		)
		_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)

## Handle minimap click/drag to pan the main view.
##
## Converts the normalized minimap position (0-1) to hex grid coordinates,
## then calculates the pan_offset needed to center that hex in the viewport.
##
## normalized_pos: Position within minimap texture, (0,0)=top-left, (1,1)=bottom-right
func _on_minimap_2d_pan_requested(normalized_pos: Vector2) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if last_hex_radius <= 0:
		return

	# Convert normalized [0,1] position to hex grid coordinates (col, row).
	# The minimap image spans the full grid, so denormalize against it directly;
	# _clamp_pan_offset() still confines the resulting pan to explored space.
	# normalized_pos is clamped to [0,1], so x/y == 1.0 must map to the LAST
	# column/row; clamp the source index here so the wrap branch's posmod() below
	# doesn't turn a right-edge click (col == grid_width) into column 0.
	var target_col := mini(int(normalized_pos.x * float(grid_width)), grid_width - 1)
	var target_row := mini(int(normalized_pos.y * float(grid_height)), grid_height - 1)
	focus_on_tile(target_col, target_row)

## Center the main view on a hex, reusing the minimap's centering + wrap-nearest
## machinery. Public so the HUD (e.g. clicking a band alert) can pan to a band's
## tile. Silently no-ops before the first layout pass.
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
	_minimap_2d.queue_indicator_redraw()

## Centre the view on a tile AND select it (as if the hex were clicked), so a jump
## from the turn-orb attention popover lands on a *selected* tile — the Tile card +
## Occupants roster populate, not just a recentre. Select first, then centre.
func focus_and_select_tile(col: int, row: int) -> void:
	handle_hex_click(col, row, MOUSE_BUTTON_LEFT)
	focus_on_tile(col, row)

# --- End 2D Minimap System ---
