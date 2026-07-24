class_name TerrainRenderer
extends RefCounted

## Owns MapView's TERRAIN raster/shader subsystem: the Approach-B per-pixel biome-blend shader
## (the whole-map `TerrainBlendQuad` child + its ShaderMaterial + the six per-hex splatmap textures
## it is fed) and the blend-OFF per-hex texture cache (hex-masked ImageTextures cut from the terrain
## atlas). Extracted from MapView (composition — MapView holds one and delegates). Owns only terrain
## raster state; every draw command plus the shared geometry/colour/visibility/river primitives stay
## on MapView and are reached through the `_view` back-ref.
##
## What deliberately did NOT come along: `_draw_terrain_direct` (the frame's base-pass loop, which
## branches between this renderer's textured hex and MapView's solid `_tile_color` fill) and the
## `_cache_*` SubViewport, which caches the whole non-shader base render and is invalidated by nine
## non-terrain concerns. Both stay on MapView.
##
## Behaviour — and every rendered pixel — is identical to the old inlined terrain code: the move was
## verified by byte-diffing all 286 reproducible `map_preview` + `blend_probe` frames before and
## after, with zero differing frames.

# The blend LOD and the FoW thresholds are MapView's, aliased rather than duplicated so there stays
# exactly one definition of each. EDGE_BLEND_MIN_RADIUS is deliberately the ICON detail radius: below
# it the shader renders base-only, the same far-zoom cutoff at which the secondary icons drop out.
const EDGE_BLEND_MIN_RADIUS := MapView.ICON_MIN_DETAIL_RADIUS
const FOW_EXPLORED_THRESHOLD := MapView.FOW_EXPLORED_THRESHOLD
const FOW_VISIBLE_THRESHOLD := MapView.FOW_VISIBLE_THRESHOLD

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
# Rivers (Minor/Major) ride hex EDGES, not centers: each tile carries a 12-bit `river_edges` mask (2 bits
# per odd-r direction). The shader's river pass paints a water band along each carrying edge. Widths /
# softness / meander are fractions of the hex radius (× radius → px, like blend_width / canopy_overhang);
# texture_scale is the world-UV multiplier; flow_speed scrolls the water downstream. Fallbacks mirror
# terrain_config's "rivers" block. (Navigable rivers are NOT here — they are an ordinary water terrain.)
const RIVER_DEFAULT_MINOR_WIDTH := 0.05
const RIVER_DEFAULT_MAJOR_WIDTH := 0.09
# NAVIGABLE is the exception to the "rivers ride edges" rule: it is a water TERRAIN, so its channel runs
# hex CENTRE → edge midpoints, not along an edge. It must read as the BIGGEST water on the map — but as a
# great RIVER, not a flood: only somewhat wider than Major (0.09). Far wider and the hex stops being a bank
# with a channel through it and becomes a puddle again, which is the read this whole pass exists to kill.
const RIVER_DEFAULT_NAVIGABLE_WIDTH := 0.14
# BANK SKIRT — a navigable hex now renders its UNDERLYING biome (the valley the river cut), not a
# whole-hex bank. The silty bank is only a slim skirt hugging the channel: this is its half-width BEYOND
# the channel (hex-radius fraction), so across the hex you read water (< navigable half-width) → thin bank
# gravel (out to navigable + this) → the underlying terrain. A slim ~10% skirt per side reads as a river
# in a valley, not a hex of gravel.
const RIVER_DEFAULT_NAVIGABLE_BANK_WIDTH := 0.10
# HEAD TAPER — on the FIRST hex of a navigable chain (the hex whose river_channel names at most ONE exit,
# i.e. it has a downstream link but no upstream one — NOT "the hex with a nonzero river_inflow", which
# since the drainage network is true of any navigable hex a tributary joins, mid-chain ones included) the
# trunk starts at the widest inflowing tributary's half-width and swells to the full navigable width by
# the hex EDGE, instead of springing to a great river at the centre. This is the exponent applied to the
# swell's smoothstep: 1.0 = plain smoothstep, < 1 swells early (wide sooner), > 1 holds the tributary's width
# longer and swells late. It is an EXPONENT, never a width, so it does not disturb the exact
# navigable-width match at the hex edge (pow(1, k) == 1 for any k) — which is what keeps the head hex
# meeting the next, constant-width, navigable hex with no step.
const RIVER_DEFAULT_HEAD_TAPER_CURVE := 1.0
# Sane bounds for that exponent: below the min the swell is a step at the centre (no taper read at all),
# above the max the trunk stays a hairline for most of the hex and then flares.
const RIVER_HEAD_TAPER_CURVE_MIN := 0.2
const RIVER_HEAD_TAPER_CURVE_MAX := 5.0
# River-array layer of the navigable channel water (the array is keyed by class - 1: 0 Minor, 1 Major).
const RIVER_NAVIGABLE_LAYER := 2
# The terrain the navigable pass keys on, resolved from terrain_config BY NAME (never by a literal id).
const TERRAIN_NAME_NAVIGABLE_RIVER := "navigable_river"
const RIVER_DEFAULT_SOFTNESS_WIDTH := 0.05
# The meander is CAPPED by design, not under-tuned: the river is edge-LOCKED (the water must be drawn on
# the edge a crossing cost will apply to), so a warp big enough to erase the lattice read would detach the
# band from its own edge — and past ~0.24 it tears the river into disconnected pools anyway. What actually
# kills the honeycomb is THIN bands (above) + width variation ALONG the river (below).
const RIVER_DEFAULT_MEANDER_WIDTH := 0.22
# Low-frequency swell/pinch of the half-width along the river's length, as a fraction (0.4 = ±40%).
const RIVER_DEFAULT_WIDTH_VARIATION := 0.4
# Higher-frequency ragged-bank wobble of the half-width, as a fraction of the hex radius.
const RIVER_DEFAULT_BANK_NOISE_WIDTH := 0.045
# Distance (fraction of the hex radius) over which Minor crossfades into Major at a class change.
const RIVER_DEFAULT_CLASS_BLEND_WIDTH := 1.0
# How hard the two class textures (light gravel-shallow vs near-black deep) are pulled onto a shared hue /
# depth so a river reads as ONE waterway growing rather than two spliced together.
const RIVER_DEFAULT_TINT_STRENGTH := 0.5
const RIVER_DEFAULT_DEPTH_COMPRESS := 0.5
const RIVER_DEFAULT_TEXTURE_SCALE := 1.8
# River LOD gate, DECOUPLED from the flat↔flat blend gate (EDGE_BLEND_MIN_RADIUS) exactly like the canopy
# and peak floors: a river is a landmark you navigate BY, so it must survive zooming out. The mipmapped
# river array keeps the thin band stable (no shimmer) down here.
const RIVER_DEFAULT_MIN_RADIUS := 3.0
const RIVER_DEFAULT_FLOW_SPEED := 0.03
# The river-map splatmap is RGBA8 and carries BOTH 12-bit river masks, one per pair of channels:
# R/B = low 8 bits, G/A = the high 4 (RG = river_edges, BA = river_inflow). Two 12-bit masks do not fit in
# one RG8 texel, and the id-map's four channels are all taken, so the two share this one texture rather
# than each getting its own.
const RIVER_MASK_LOW_BYTE := 0xFF
const RIVER_MASK_HIGH_SHIFT := 8
const RIVER_MAP_CHANNELS := 4  # bytes per river-map texel (RGBA8)
# Both masks pack one river CLASS per slot: class = (mask >> (2 * slot)) & 0b11 — the slot being an odd-r
# DIRECTION for river_edges (which side of the hex the river runs along) and a hex CORNER for river_inflow
# (which vertex a tributary hands over to the navigable trunk at).
const RIVER_MASK_CLASS_BITS := 2
const RIVER_MASK_CLASS_MAX := 0b11
# The river-CHANNEL mask (`river_channel`) is a third, differently-shaped primitive: 1 BIT per odd-r
# direction (exits(dir) = (mask >> dir) & 1), naming the sides a NAVIGABLE hex's channel flows out through.
# It is the SIM's word on the trunk's connectivity, and the renderer must take it: inferring an arm from
# the neighbouring terrain (navigable/water/delta) cross-linked side-by-side chain hexes into a WEB of
# triangles — a chain is a PATH, and only the tracer knows which two neighbours are on it. 6 bits do not
# fit the RGBA8 river-map (all 32 bits are taken by the two 12-bit masks), so it rides its own R8 texture.
const RIVER_CHANNEL_MASK := 0b111111  # the 6 exit bits, one per odd-r direction (the R8 texel's payload)
# (All three river masks are indexed in the SIM's odd-r direction/corner order — see the shader's
# neighbor_offset(), which is the wire contract. MapView itself no longer walks hex neighbours for rivers:
# the connectivity now comes from the sim's river_channel mask, not from the terrain around a hex.)

var _view: MapView = null

# Terrain texture system for 2D view (textures loaded via TerrainTextureManager autoload)
var _hex_texture_cache: Dictionary = {}  # terrain_id -> ImageTexture (hex-masked)
var _hex_texture_size: int = 128  # Size of cached hex textures
var _terrain_grid_width: int = 0
var _terrain_grid_height: int = 0
var _cached_terrain_ids: PackedInt32Array = PackedInt32Array()
var _hex_alpha_mask: PackedByteArray = PackedByteArray()  # Pre-computed hex mask for texture rendering
var _terrain_blend_class: Dictionary = {}  # terrain_id -> "flat"|"water"|"rugged" (edge-blend eligibility)
var _terrain_id_by_name: Dictionary = {}   # terrain config `name` -> id (feeds the shader's navigable/delta ids)
# Approach B — per-pixel biome-blend shader (terrain_blend.gdshader). A whole-map quad child renders
# the blended terrain behind MapView's own draws; MapView feeds it the biome array + a per-hex id-map
# (splatmap) + the exact hex-layout uniforms. Supersedes A's baked-overlay dither when use_edge_blending.
var _terrain_blend_quad: Node2D = null
var _terrain_blend_material: ShaderMaterial = null
var _terrain_blend_ready: bool = false
var _terrain_id_map_tex: ImageTexture = null   # RGBA8: R=terrain id, G=blend_class code (0 water/1 flat/2 rugged), B=canopy code (0=none else layer+1), A=peak code (0=none else layer+1)
var _terrain_vis_map_tex: ImageTexture = null  # R8: 0 unexplored / 0.5 discovered / 1 active
var _terrain_elev_map_tex: ImageTexture = null # R8: per-hex relative height (0..255 = 0..100), for peak prominence + shadow scaling
var _terrain_river_map_tex: ImageTexture = null # RGBA8: the 12-bit river-EDGE mask (R = low 8 bits, G = high 4)
                                                # + the 12-bit river-INFLOW mask (B = low 8, A = high 4)
var _terrain_river_channel_map_tex: ImageTexture = null # R8: the 6-bit river-CHANNEL exit mask (1 bit per
                                                # odd-r direction) — its own texture; the RGBA8 above is full
var _terrain_navigable_underlying_map_tex: ImageTexture = null # R8: the per-hex UNDERLYING terrain id (the
                                                # valley biome on a navigable hex). Shader reads it on navigable
                                                # hexes only, so non-navigable texels are don't-care.

func _init(view: MapView) -> void:
	_view = view

## The per-snapshot terrain raster MapView ingests in `display_snapshot`: the flat terrain-id array
## plus the grid it is indexed by. Kept here (rather than read back off `_view`) because the shader
## splatmaps are rebuilt from exactly this triple.
func set_grid_terrain(terrain_ids: PackedInt32Array, width: int, height: int) -> void:
	_cached_terrain_ids = terrain_ids
	_terrain_grid_width = width
	_terrain_grid_height = height

## The cached per-tile terrain ids, for MapView's present-biome / biome-count passes.
func cached_terrain_ids() -> PackedInt32Array:
	return _cached_terrain_ids

## The hex-masked texture for a terrain id, or null when the atlas has no layer for it.
## `CachedMapRenderer` draws the cache into its SubViewport through this.
func hex_texture_for(terrain_id: int) -> ImageTexture:
	return _hex_texture_cache.get(terrain_id)


# --- Terrain Texture System for 2D View (textures loaded via TerrainTextureManager autoload) ---

func setup() -> void:
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
	if dx > radius * _view.SQRT3 * 0.5:
		return false

	# Edge check for hex shape
	return (radius * _view.SQRT3 * 0.5 - dx) * 2.0 >= (dy - radius * 0.5) * _view.SQRT3


func get_terrain_textures_enabled() -> bool:
	var mgr := TerrainTextureManager
	return mgr.use_terrain_textures and mgr.terrain_textures != null

func enable_terrain_textures(enabled: bool) -> void:
	## Toggle terrain texture rendering for 2D view
	TerrainTextureManager.use_terrain_textures = enabled
	_view._invalidate_map_cache()  # cache bakes textured vs solid tiles; force a re-render
	_view.queue_redraw()

## Toggle terrain textures on/off (bound to the T key). Flips the underlying
## intent flag directly rather than get_terrain_textures_enabled(), which also
## factors in atlas presence — using the getter would leave the toggle stuck
## "on" (and pop textures in later) whenever the atlas isn't loaded yet.
## No visible effect until an atlas is available, since rendering needs one.
func toggle_terrain_textures() -> void:
	enable_terrain_textures(not TerrainTextureManager.use_terrain_textures)

func _build_terrain_blend_class_map() -> void:
	## Cache terrain_id -> blend_class ("flat"|"water"|"rugged") from config. Only flat↔flat seams
	## blend; water/rugged always render a hard edge. Single source of truth: terrain_config.json.
	## Also caches name -> terrain_id, so the shader's navigable/delta ids come from the config by NAME
	## rather than being hardcoded twice (here and in GLSL).
	_terrain_blend_class.clear()
	_terrain_id_by_name.clear()
	var terrains: Array = TerrainTextureManager.terrain_config.get("terrains", [])
	for entry: Variant in terrains:
		if entry is Dictionary:
			var tid: int = int(entry.get("id", -1))
			if tid >= 0:
				_terrain_blend_class[tid] = String(entry.get("blend_class", "rugged"))
				_terrain_id_by_name[String(entry.get("name", ""))] = tid

func _terrain_id_for_name(name: String) -> int:
	## Terrain id for a config `name` (-1 when absent — the shader then never matches it).
	return int(_terrain_id_by_name.get(name, -1))

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
	# Per-terrain BLEND PROFILE (1×N RGBA float, fetched by layer index) — the flat↔flat seam's twin of the
	# shore profile: R = width_scale (the ecotone's REACH), G = noise_scale (the boundary wobble's AMPLITUDE),
	# B = noise_cell_scale (its WAVELENGTH). A terrain whose texture is far from its neighbours' in BOTH tone
	# and hue (the NavigableRiver bank against prairie one side, floodplain the other) needs a wider, wobblier
	# seam than the global levers give, or it reads as a hexagon with a blurred edge. Combined across an edge
	# with max(), which is commutative → both hexes agree → the seam stays continuous. Neutral (1, 1, 1) for
	# every terrain with no `blend_profile` block, so every other seam is bit-identical. Bound once (the manager
	# updates the texture in place on a rebuild, so the binding survives).
	_terrain_blend_material.set_shader_parameter("layer_blend_map", TerrainTextureManager.layer_blend_texture)
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
	# Rivers: a FOURTH Texture2DArray in the same canvas shader (flowing water for the hex-EDGE Minor/Major
	# rivers, layer = class - 1). Disabled (sampler harmlessly bound to the base array) with no river asset.
	var river_arr: Texture2DArray = TerrainTextureManager.river_textures
	_terrain_blend_material.set_shader_parameter("rivers_enabled", river_arr != null and river_arr.get_layers() > 0)
	_terrain_blend_material.set_shader_parameter("river_tex", river_arr if river_arr != null else TerrainTextureManager.terrain_textures)
	# The navigable channel is river-array layer 2 (the third file in textures/rivers/). Without that layer
	# there is no channel water to paint, so the pass is disabled and a navigable hex renders as bare bank.
	var has_navigable_layer: bool = river_arr != null and river_arr.get_layers() > RIVER_NAVIGABLE_LAYER
	_terrain_blend_material.set_shader_parameter("river_navigable_enabled", has_navigable_layer)
	_terrain_blend_material.set_shader_parameter("river_navigable_terrain_id", _terrain_id_for_name(TERRAIN_NAME_NAVIGABLE_RIVER))
	var QuadScript: GDScript = preload("res://src/scripts/TerrainBlendQuad.gd")
	_terrain_blend_quad = QuadScript.new()
	_terrain_blend_quad.name = "TerrainBlendQuad"
	_terrain_blend_quad.material = _terrain_blend_material
	_terrain_blend_quad.show_behind_parent = true
	_terrain_blend_quad.visible = false
	_view.add_child(_terrain_blend_quad)
	_view.move_child(_terrain_blend_quad, 0)  # keep it first so it draws behind
	_terrain_blend_ready = true
	print("[MapView] Terrain blend shader ready (Approach B)")

func _has_terrain_textures() -> bool:
	return TerrainTextureManager.use_terrain_textures and TerrainTextureManager.terrain_textures != null

func shader_active() -> bool:
	## The Approach-B shader renders the base terrain when textures are on, no overlay is selected, and
	## edge blending is enabled. Otherwise the per-hex texture path (blend OFF) or overlay/solid path runs.
	return _terrain_blend_ready and TerrainTextureManager.use_edge_blending \
		and _view.active_overlay_key == "" and _has_terrain_textures()

func update_shader_quad(radius: float, origin: Vector2, viewport_size: Vector2) -> void:
	## Push the exact hex-layout + blend uniforms (so terrain aligns with grid/markers), size the quad
	## to the usable rect (bounds the shader to the area beside the Inspector strip), and show it.
	if _terrain_blend_material == null or _terrain_blend_quad == null:
		return
	if _terrain_id_map_tex == null:
		rebuild_shader_maps()
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
	m.set_shader_parameter("grid_w", _view.grid_width)
	m.set_shader_parameter("grid_h", _view.grid_height)
	m.set_shader_parameter("hex_radius", radius)
	m.set_shader_parameter("hex_origin", origin)
	m.set_shader_parameter("wrap_h", _view._wrap_horizontal)
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
	# Rivers (Minor/Major, on hex EDGES): widths/softness/meander are hex-radius fractions → px, exactly
	# like blend_width / canopy_overhang. LOD is decoupled from the flat↔flat blend gate (own
	# river_min_radius, well below EDGE_BLEND_MIN_RADIUS) so rivers survive zooming out.
	var rivers: Dictionary = config.get("rivers", {})
	var minor_frac: float = clampf(float(rivers.get("minor_width", RIVER_DEFAULT_MINOR_WIDTH)), 0.01, 1.0)
	var major_frac: float = clampf(float(rivers.get("major_width", RIVER_DEFAULT_MAJOR_WIDTH)), 0.01, 1.0)
	var navigable_frac: float = clampf(float(rivers.get("navigable_width", RIVER_DEFAULT_NAVIGABLE_WIDTH)), 0.01, 1.0)
	var navigable_bank_frac: float = clampf(float(rivers.get("navigable_bank_width", RIVER_DEFAULT_NAVIGABLE_BANK_WIDTH)), 0.0, 1.0)
	var head_taper_curve: float = clampf(
		float(rivers.get("head_taper_curve", RIVER_DEFAULT_HEAD_TAPER_CURVE)),
		RIVER_HEAD_TAPER_CURVE_MIN, RIVER_HEAD_TAPER_CURVE_MAX)
	var river_soft_frac: float = clampf(float(rivers.get("softness_width", RIVER_DEFAULT_SOFTNESS_WIDTH)), 0.005, 1.0)
	var meander_frac: float = clampf(float(rivers.get("meander_width", RIVER_DEFAULT_MEANDER_WIDTH)), 0.0, 1.0)
	var width_variation: float = clampf(float(rivers.get("width_variation", RIVER_DEFAULT_WIDTH_VARIATION)), 0.0, 1.0)
	var bank_noise_frac: float = clampf(float(rivers.get("bank_noise_width", RIVER_DEFAULT_BANK_NOISE_WIDTH)), 0.0, 1.0)
	var class_blend_frac: float = clampf(float(rivers.get("class_blend_width", RIVER_DEFAULT_CLASS_BLEND_WIDTH)), 0.01, 2.0)
	var tint_strength: float = clampf(float(rivers.get("tint_strength", RIVER_DEFAULT_TINT_STRENGTH)), 0.0, 1.0)
	var depth_compress: float = clampf(float(rivers.get("depth_compress", RIVER_DEFAULT_DEPTH_COMPRESS)), 0.0, 1.0)
	var river_scale: float = maxf(float(rivers.get("texture_scale", RIVER_DEFAULT_TEXTURE_SCALE)), 0.05)
	var river_min_radius: float = maxf(float(rivers.get("river_min_radius", RIVER_DEFAULT_MIN_RADIUS)), 0.0)
	var river_flow_speed: float = float(rivers.get("flow_speed", RIVER_DEFAULT_FLOW_SPEED))
	m.set_shader_parameter("rivers_lod_enabled", radius >= river_min_radius)
	m.set_shader_parameter("river_minor_half_width", minor_frac * radius)  # Minor band half-width (px)
	m.set_shader_parameter("river_major_half_width", major_frac * radius)  # Major band half-width (px)
	m.set_shader_parameter("river_navigable_half_width", navigable_frac * radius)  # channel half-width (px)
	m.set_shader_parameter("river_navigable_bank_half_width", navigable_bank_frac * radius)  # bank skirt beyond the channel (px)
	m.set_shader_parameter("river_head_taper_curve", head_taper_curve)     # trunk-head swell shape (unitless)
	m.set_shader_parameter("river_softness", river_soft_frac * radius)     # bank ramp half-width (px)
	m.set_shader_parameter("river_meander", meander_frac * radius)         # noise wander of the band (px)
	m.set_shader_parameter("river_width_variation", width_variation)       # swell/pinch along the river (frac)
	m.set_shader_parameter("river_bank_noise", bank_noise_frac * radius)   # ragged-bank wobble (px)
	m.set_shader_parameter("river_class_blend", class_blend_frac * radius) # Minor→Major crossfade span (px)
	m.set_shader_parameter("river_tint_strength", tint_strength)
	m.set_shader_parameter("river_depth_compress", depth_compress)
	m.set_shader_parameter("river_scale", river_scale)
	m.set_shader_parameter("river_flow_speed", river_flow_speed)
	m.set_shader_parameter("river_highlight", _view.highlight_rivers)
	m.set_shader_parameter("fow_enabled", _view._fow_enabled)
	m.set_shader_parameter("bg_color", _view.TERRAIN_BG_COLOR)
	m.set_shader_parameter("fog_color", _view._fow_fog_fill_color)
	m.set_shader_parameter("mist_color", Vector3(_view._fow_mist_color.r, _view._fow_mist_color.g, _view._fow_mist_color.b))
	m.set_shader_parameter("mist_blend", _view._fow_mist_blend)
	# FoW boundary softening: radius-relative (like blend_band) so the mist gradient is zoom-invariant.
	m.set_shader_parameter("fow_soft", _view._fow_softness * radius)
	m.set_shader_parameter("fow_noise_amount", _view._fow_noise_amount)
	_terrain_blend_quad.visible = true
	_terrain_blend_quad.set_rect_size(viewport_size)
	_terrain_blend_quad.queue_redraw()

func hide_shader_quad() -> void:
	if _terrain_blend_quad != null and _terrain_blend_quad.visible:
		_terrain_blend_quad.visible = false

func _shore_color(raw, fallback: Vector3) -> Vector3:
	## Parse a config [r,g,b] (0–255) shoreline color into a normalized Vector3 shader uniform, falling
	## back to the named default when the key is absent/malformed.
	if raw is Array and raw.size() >= 3:
		return Vector3(float(raw[0]), float(raw[1]), float(raw[2])) / 255.0
	return fallback

func rebuild_shader_maps() -> void:
	## (Re)build the id-map (RGBA8: R=terrain id, G=blend_class code, B=canopy code, A=peak code) +
	## vis-map (R8: FoW state) + elev-map (R8: per-hex relative height for peak prominence/shadow) +
	## river-map (RGBA8: the 12-bit river-EDGE mask in R/G, the 12-bit river-INFLOW mask in B/A, each as
	## low 8 bits / high 4) + river-channel-map (R8: the 6-bit channel-EXIT mask) splatmaps, one texel per
	## hex, from the current terrain + FoW + elevation + river edges/inflow/channel. Called each snapshot.
	## NEAREST-sampled in-shader. All four id-map channels are taken, hence the rivers' own texture — and
	## all four of THAT one's are taken too (2 x 12 bits), hence the channel mask's own R8 on top.
	if _view.grid_width <= 0 or _view.grid_height <= 0 or _cached_terrain_ids.is_empty():
		return
	var w := _view.grid_width
	var h := _view.grid_height
	var id_bytes := PackedByteArray()
	id_bytes.resize(w * h * 4)
	var vis_bytes := PackedByteArray()
	vis_bytes.resize(w * h)
	var elev_bytes := PackedByteArray()
	elev_bytes.resize(w * h)
	var river_bytes := PackedByteArray()
	river_bytes.resize(w * h * RIVER_MAP_CHANNELS)
	var river_channel_bytes := PackedByteArray()   # R8: one texel per hex, the 6 exit bits
	river_channel_bytes.resize(w * h)
	var navigable_underlying_bytes := PackedByteArray()  # R8: per-hex underlying (valley) terrain id
	navigable_underlying_bytes.resize(w * h)
	# Hoist the per-hex-invariant raster fetches + sea-level math out of the double loop:
	# both the FoW visibility channel and the elevation channel (plus its sea-level rescale
	# constants) are the same for every hex, so fetch/compute them once here instead of
	# re-running _visibility_state_at / relative_height_at per hex (each of which re-did an
	# _overlay_raw_array dict lookup). The per-hex byte encoding below reproduces those two
	# helpers exactly, including the empty-raster fallbacks.
	var vis_raster := _view._visibility_array()  # raw visibility channel (see _visibility_state_at)
	var elev_raster := _view._overlay_raw_array("elevation")  # raw elevation channel (see relative_height_at)
	var elev_has_data := not elev_raster.is_empty()  # matches relative_height_at's -1 (missing) guard
	# Sea-level rescale constants (see relative_height_at): above-sea span normalized into 0..1.
	var elev_sea_level := clampf(_view._elevation_sea_level, 0.0, 0.999)
	var elev_span := 1.0 - elev_sea_level
	# Collected in the loop below (free — we already visit every hex) so the orphan-channel diagnostic
	# costs one pass over the handful of navigable hexes instead of a second pass over the whole grid.
	var navigable_hexes: Array[Vector2i] = []
	var navigable_id := _terrain_id_for_name(TERRAIN_NAME_NAVIGABLE_RIVER)
	for y in range(h):
		for x in range(w):
			var idx := y * w + x
			var tid := _view._terrain_id_at(x, y)
			if tid == navigable_id:
				navigable_hexes.append(Vector2i(x, y))
			id_bytes[idx * 4] = clampi(tid, 0, 255) if tid >= 0 else 0
			id_bytes[idx * 4 + 1] = _blend_class_code(tid)
			id_bytes[idx * 4 + 2] = _canopy_code(tid)
			id_bytes[idx * 4 + 3] = _peak_code(tid)
			# Mirror _visibility_state_at's active/discovered/unexplored classification (guarded by
			# _fow_enabled) directly into the 255/128/0 byte, reading the hoisted raster.
			var v := 255
			if _view._fow_enabled:
				var vis := _view._value_at(vis_raster, x, y)
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
				var above_sea := clampf((_view._value_at(elev_raster, x, y) - elev_sea_level) / elev_span, 0.0, 1.0)
				var rh := int(round(above_sea * 100.0))
				elev_bytes[idx] = clampi(int(round(float(rh) * 2.55)), 0, 255)
			else:
				elev_bytes[idx] = PEAK_ELEV_FALLBACK
			# Each 12-bit river mask straddles two channels — every id-map channel is already taken, and the
			# two masks (edges by SIDE, inflow by CORNER) are 24 bits together: R/G = edges, B/A = inflow.
			var hex_key := Vector2i(x, y)
			var river_mask: int = int(_view.tile_river_edges.get(hex_key, 0))
			var inflow_mask: int = int(_view.tile_river_inflow.get(hex_key, 0))
			river_bytes[idx * RIVER_MAP_CHANNELS] = river_mask & RIVER_MASK_LOW_BYTE
			river_bytes[idx * RIVER_MAP_CHANNELS + 1] = (river_mask >> RIVER_MASK_HIGH_SHIFT) & RIVER_MASK_LOW_BYTE
			river_bytes[idx * RIVER_MAP_CHANNELS + 2] = inflow_mask & RIVER_MASK_LOW_BYTE
			river_bytes[idx * RIVER_MAP_CHANNELS + 3] = (inflow_mask >> RIVER_MASK_HIGH_SHIFT) & RIVER_MASK_LOW_BYTE
			# The channel-exit mask is only 6 bits, but there is no room left in the RGBA8 above, so it gets
			# its own R8 texel. It is what the shader arms the trunk's arms from — see RIVER_CHANNEL_MASK.
			river_channel_bytes[idx] = int(_view.tile_river_channel.get(hex_key, 0)) & RIVER_CHANNEL_MASK
			# The underlying (valley) biome id the shader swaps in for a navigable hex's base. Falls back to
			# this hex's own terrain id when the tile carried none — non-navigable texels are don't-care anyway.
			var underlying_id: int = int(_view.tile_underlying_terrain.get(hex_key, tid))
			navigable_underlying_bytes[idx] = clampi(underlying_id, 0, 255) if underlying_id >= 0 else 0
	var id_img := Image.create_from_data(w, h, false, Image.FORMAT_RGBA8, id_bytes)
	_terrain_id_map_tex = ImageTexture.create_from_image(id_img)
	var vis_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, vis_bytes)
	_terrain_vis_map_tex = ImageTexture.create_from_image(vis_img)
	var elev_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, elev_bytes)
	_terrain_elev_map_tex = ImageTexture.create_from_image(elev_img)
	var river_img := Image.create_from_data(w, h, false, Image.FORMAT_RGBA8, river_bytes)
	_terrain_river_map_tex = ImageTexture.create_from_image(river_img)
	var river_channel_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, river_channel_bytes)
	_terrain_river_channel_map_tex = ImageTexture.create_from_image(river_channel_img)
	var navigable_underlying_img := Image.create_from_data(w, h, false, Image.FORMAT_R8, navigable_underlying_bytes)
	_terrain_navigable_underlying_map_tex = ImageTexture.create_from_image(navigable_underlying_img)
	if _terrain_blend_material != null:
		_terrain_blend_material.set_shader_parameter("id_map", _terrain_id_map_tex)
		_terrain_blend_material.set_shader_parameter("vis_map", _terrain_vis_map_tex)
		_terrain_blend_material.set_shader_parameter("elev_map", _terrain_elev_map_tex)
		_terrain_blend_material.set_shader_parameter("river_map", _terrain_river_map_tex)
		_terrain_blend_material.set_shader_parameter("river_channel_map", _terrain_river_channel_map_tex)
		_terrain_blend_material.set_shader_parameter("navigable_underlying_map", _terrain_navigable_underlying_map_tex)
	_warn_orphan_navigable_rivers(navigable_hexes)

func _warn_orphan_navigable_rivers(navigable_hexes: Array[Vector2i]) -> void:
	## Diagnostic mirror of the shader's navigable ARM rule (terrain_blend.gdshader, navigable pass): a
	## navigable hex's channel must LEAVE it through at least one side, and the sim says which sides those
	## are (`river_channel`) — the renderer no longer guesses from the neighbouring terrain, because that
	## guess wove a web (see RIVER_CHANNEL_MASK). So the orphan test is now purely on the masks: a hex with
	## no channel exit AND no inflow carries no water at all — a river the water neither enters nor leaves.
	## (A hex with an inflow but no exit is drainless but still gets its tributary's spur, and one with an
	## exit is by definition on the chain.) The shader stays graceful — it draws a centre blob rather than a
	## hex of bare bank — so without this the anomaly would be silent. Keep the two rules in step.
	if navigable_hexes.is_empty():
		return
	var orphans: Array[String] = []
	for hex: Vector2i in navigable_hexes:
		if int(_view.tile_river_channel.get(hex, 0)) != 0 or int(_view.tile_river_inflow.get(hex, 0)) != 0:
			continue
		orphans.append("(%d, %d)" % [hex.x, hex.y])
	if not orphans.is_empty():
		push_warning("[MapView] %d navigable-river hex(es) carry no channel — no river_channel exit and no river_inflow, so the water neither enters nor leaves; rendering a centre blob: %s"
			% [orphans.size(), ", ".join(orphans)])

func draw_hex_textured_direct(center: Vector2, terrain_id: int, radius: float, tint: Color = Color.WHITE) -> void:
	## Draw a single hex with texture (direct rendering version). `tint` modulates
	## the texture (used for Fog of War: mist for Discovered, white for Active).
	var tex: ImageTexture = _hex_texture_cache.get(terrain_id)
	if tex == null:
		var color: Color = _view._terrain_color_for_id(terrain_id) * tint
		var polygon_points := _view._hex_points(center, radius)
		_view.draw_polygon(polygon_points, PackedColorArray([color, color, color, color, color, color]))
		return

	var polygon_points := _view._hex_points(center, radius)
	var uvs := PackedVector2Array()
	for point in polygon_points:
		var uv := Vector2(
			(point.x - center.x) / radius * 0.5 + 0.5,
			(point.y - center.y) / radius * 0.5 + 0.5
		)
		uvs.append(uv)
	var colors := PackedColorArray([tint, tint, tint, tint, tint, tint])
	_view.draw_polygon(polygon_points, colors, uvs, tex)
