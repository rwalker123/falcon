shader_type canvas_item;
render_mode blend_mix;

// Approach B — per-pixel biome blend for the 2D hex map (see clients/.../CLAUDE.md → Edge Blending).
// One whole-map quad runs this fragment shader. Each fragment inverts the pointy-top odd-r hex layout
// (MUST match MapView._hex_center / _axial_center exactly) to find its hex, samples that hex's biome
// from the Texture2DArray, and — when the hex AND a neighbour across a shared edge are BLEND-ELIGIBLE
// (different biome, and either the same blendable blend_class — flat↔flat / water↔water — or, with the
// SHIPPED-TRUE blend_rugged_land gate, any land↔land pair) — MIXES the neighbour's texture in
// near the seam: a symmetric signed-distance splat weight (a tie at the exact edge), wobbled by WORLD-space
// value noise (varies per hex, no baked tiling), optionally nudged by the two textures' luminance detail,
// and feathered through a smoothstep into a continuous mix factor — never a 1-bit pick (see the const
// block). The one CLASS CHANGE that never blends is land↔water: that seam is the SHORELINE, which gets its
// own coastal profile instead (a LAND-ONLY beach, then a surf that peaks at the waterline and washes both
// ways, over a base that cross-fades across it) — see the shoreline block. See the id-map (splatmap) below.
// RIVERS ride on top of all of that, as two further passes (see the river blocks): Minor/Major rivers run
// along hex EDGES (the per-tile river_edges mask, keyed BY DIRECTION), and a NavigableRiver — a water
// TERRAIN whose blend_class is deliberately "flat" — is a BANK hex with a water CHANNEL painted through it.
// Both composite AFTER the shoreline and BEFORE the canopy/peaks (trees overhang a river; mountains sit
// above it) and before the FoW tint (a river in a Discovered tile dims rather than punching through).
// A navigable hex's edges are EXCLUDED from the shore pass — a river meeting the sea is not a coast.

uniform sampler2DArray biome_array : filter_linear, repeat_enable;
// Canopy overlay array: whole tree crowns on transparency (RGBA), sampled in MAP space (v_map, the
// pan/zoom-anchored coordinate) so crowns are continuous across hexes, overhang the boundary, and
// track the grid on pan/zoom. Indexed by canopy-array layer (id-map B - 1).
// Trilinear (mipmap) filtering so far-zoom crowns AVERAGE into a smooth darker-green forest mass instead
// of shimmering/aliasing — the crown array is built with mipmaps (see TerrainTextureManager).
uniform sampler2DArray canopy_tex : filter_linear_mipmap, repeat_enable;
uniform bool canopy_enabled;      // false when no canopy asset exists → skip the canopy pass entirely
uniform bool canopy_lod_enabled;  // canopy LOD gate — DECOUPLED from blend_enabled so forests stay
                                  // readable (a darker-green mass) at far zoom where the blend is off
uniform float canopy_overhang;    // how far crowns overhang past the treeline, in px (overhang_width * radius)
uniform float canopy_softness;    // inner treeline ramp half-width, in px (softness_width * radius)
uniform float canopy_scale;       // world-UV scale multiplier (1.0 = one crown tile per hex, matching the base)
uniform float base_scale;         // base biome-texture world-UV scale: like canopy_scale, one texture tile
                                  // spans (1/base_scale) hex-rows in world space (continuous across hexes,
                                  // so adjacent hexes show DIFFERENT regions of the texture → no per-hex repeat)
// Peak overlay array: faceted mountain relief on transparency (RGBA), the mountain-drama analog of the
// canopy pass — sampled in MAP space (v_map) so peaks are continuous across hexes, overhang the boundary,
// and cast a shadow onto the neighbour. Indexed by peak-array layer (id-map A - 1). Trilinear (mipmap)
// filtering so far-zoom relief AVERAGES into a smooth raised mountain mass (crown array is built with mipmaps).
uniform sampler2DArray peak_tex : filter_linear_mipmap, repeat_enable;
uniform bool peaks_enabled;       // false when no peak asset exists → skip the peak pass entirely
uniform bool peaks_lod_enabled;   // peak LOD gate — DECOUPLED from blend_enabled so relief stays at far zoom
uniform float peak_overhang;      // how far relief overhangs past the footline, in px (overhang_width * radius)
uniform float peak_softness;      // inner footline ramp half-width, in px (softness_width * radius)
uniform float peak_scale;         // world-UV scale multiplier (1.0 = one peak tile per hex, matching the base)
uniform float peak_shadow_len;    // cast-shadow reach toward the light, in px (shadow_length * radius)
uniform float peak_shadow_strength; // max darkening a fully-occluding peak casts on the ground (0..1)
uniform float peak_min_prominence;  // relief opacity floor at the lowest elevation (0..1); scales to 1 at peak elev
uniform vec2 peak_light_dir;      // normalized direction TOWARD the light in canvas space (+y DOWN → top-left is -,-)
// River water array: flowing-water bands (RGBA, opaque) for the Minor/Major hex-EDGE rivers. Indexed by
// river CLASS - 1 (layer 0 = Minor, 1 = Major) — NOT by terrain id: a river is not a biome, it rides an
// edge. Sampled in MAP space (v_map) like the canopy/peaks so the water is continuous across hexes, plus
// a TIME scroll along the edge tangent so it flows. Trilinear (mipmap) because a thin band aliases badly
// at far zoom (the river array is built with mipmaps — see TerrainTextureManager).
uniform sampler2DArray river_tex : filter_linear_mipmap, repeat_enable;
uniform bool rivers_enabled;       // false when no river asset exists → skip the river pass entirely
uniform bool rivers_lod_enabled;   // river LOD gate — DECOUPLED from blend_enabled (like canopy/peaks),
                                   // so rivers persist at far zoom where the flat↔flat blend is off
uniform bool river_navigable_enabled; // false when the river array has no navigable layer → skip that pass
uniform int river_navigable_terrain_id; // terrain id of NavigableRiver (from terrain_config, by NAME)
uniform float river_navigable_half_width; // navigable CHANNEL half-width, in px (navigable_width * radius)
uniform float river_minor_half_width; // Minor half-band half-width, in px (minor_width * radius)
uniform float river_major_half_width; // Major half-band half-width, in px (major_width * radius)
uniform float river_head_taper_curve; // exponent on the head taper's smoothstep: on the FIRST hex of a
                                   // navigable chain the trunk starts at the widest INFLOWING tributary's
                                   // width and swells to the channel's full width by the hex edge. 1.0 =
                                   // plain smoothstep; < 1 swells early (wide sooner), > 1 swells late.
uniform float river_softness;      // bank ramp half-width, in px (softness_width * radius)
uniform float river_meander;       // world-noise perturbation of the band's distance, in px (meander_width * radius)
uniform float river_width_variation; // 0..1: how far the LOW-frequency world noise swells/pinches the
                                   // half-width along the river's length (0.3 = ±30%). The main cure for
                                   // the honeycomb read — a constant-width stroke looks INKED, not flowed.
uniform float river_bank_noise;    // higher-frequency ragged-bank wobble of the half-width, in px
                                   // (bank_noise_width * radius) — the shore pass's noisy-reach idiom
uniform float river_class_blend;   // px over which Minor crossfades into Major at a class change, so one
                                   // river GROWS instead of splicing two waterways together
uniform float river_tint_strength; // 0..1: how hard both class layers are pulled onto the shared river hue
uniform float river_depth_compress; // 0..1: how hard both class layers' luma is pulled toward the shared
                                   // depth pivot — Major stays deeper, but stops reading as near-black
uniform float river_scale;         // world-UV scale multiplier for the water texture (like canopy_scale)
uniform float river_flow_speed;    // downstream UV scroll per second along the edge tangent
uniform bool river_highlight;      // debug toggle (Map tab): tint the rivers hard so they pop
// river-map splatmap, RGBA8 — two per-tile 12-bit masks, each 2 bits per slot (class = (mask >> (2*slot))
// & 3; 0 none / 1 Minor / 2 Major, 3 reserved), each split low-8 / high-4 across a channel pair:
//   R/G = riverEdges, keyed by odd-r DIRECTION — which of the hex's 6 SIDES the river runs along.
//   B/A = riverInflow, keyed by hex CORNER — the VERTEX at which an edge river hands over to a navigable
//         trunk (nonzero on the trunk's first hex only). An edge river runs along a side and therefore
//         ends at a corner, never mid-edge; this is what the channel's inflow SPUR is drawn to.
uniform sampler2D river_map : filter_nearest;
// river-CHANNEL splatmap, R8 — the per-tile 6-bit riverChannel mask (1 bit per odd-r direction:
// exits(dir) = (mask >> dir) & 1), naming the sides a NAVIGABLE hex's channel actually flows out through.
// Its own texture because the RGBA8 river_map's 32 bits are all spoken for (two 12-bit masks).
// THE TRUNK'S CONNECTIVITY IS SIM-AUTHORED, and must stay that way: this pass used to infer an arm for
// every navigable/water/delta NEIGHBOUR, which cross-linked adjacent chains (and a chain bending back on
// itself) into a WEB of triangles. A chain is a PATH — only the sim's tracer knows which two neighbours
// are its upstream and downstream — so arm ONLY the set bits. Do not "simplify" this back to terrain.
uniform sampler2D river_channel_map : filter_nearest;
// Per-base-layer MEAN LUMINANCE, a 1×N single-channel texture fetched by layer index (N = terrain count,
// so the layer count is never baked into this shader as a fixed array size). Height blending uses each
// texture's per-pixel luminance as a pseudo-height (we have no height maps); subtracting the layer's mean
// ZERO-CENTRES it, without which a bright biome would always out-height a dark one and the seam would
// collapse onto one side instead of interlocking. Built by TerrainTextureManager.
uniform sampler2D layer_luma_map : filter_nearest;
// PER-WATER-TERRAIN SHORE PROFILE, a 1×N RGBA float texture fetched by layer index (same table shape as
// layer_luma_map). THREE INDEPENDENT SCALES on the global (ocean-tuned) coastal profile, because a coast is
// not one thing: deep ocean only ever touches HIGH land (a CLIFF — no beach, big surf), the continental
// shelf is the ordinary beach (sand, muted surf), an inland sea is a small lake (short everything, no
// offshore wisp).
//   R = sand_scale — multiplies the inland beach reach (sand_band). 0 = NO BEACH AT ALL (the cliff).
//   G = foam_scale — multiplies the MAIN WAVE's reaches, both ways: the wash up the beach
//                    (foam_inland_band) and the surf's seaward reach (foam_band). It scales REACH only —
//                    the surf's PEAK is the GLOBAL foam_opacity lever (see the SURF block below), not a
//                    per-water one. foam_scale = 0 is not a legal profile.
//   B = wisp_scale — multiplies the secondary offshore disturbance: its centre distance, its half-width AND
//                    its strength. 0 = no second disturbance.
// Terrains with no `shore_profile` block read the neutral (1, 1, 1), which is a bit-exact no-op.
// Built by TerrainTextureManager.
uniform sampler2D layer_shore_map : filter_nearest;
// PER-TERRAIN BLEND PROFILE, the same 1×N RGBA-float-by-layer-index table — the flat↔flat seam's twin of
// layer_shore_map. The three global blend levers are ONE ecotone, tuned for biome pairs that are close in
// tone and share a hue. Against a pair far apart in BOTH (the NavigableRiver BANK — grey, low-contrast
// gravel at mean luma 89 — has prairie/scrub at 112–127 on one side of a river corridor and
// floodplain/alluvial at 55–58 on the other) the very same ecotone reads as a HEXAGON WITH A BLURRED EDGE:
// the ramp is only ~0.35·r wide and the wobble displaces it by a fraction of that, so the boundary still
// traces the hex polyline and the eye locks onto it. These three scales let such a terrain widen and roughen
// the seams IT is on, and nobody else's:
//   R = width_scale      — multiplies blend_band (the ecotone's REACH).
//   G = noise_scale      — multiplies blend_noise_amount (the boundary wobble's AMPLITUDE), so the boundary
//                          leaves the hexagon instead of tracing it.
//   B = noise_cell_scale — multiplies blend_noise_cell (the wobble's WAVELENGTH). Amplitude on a fine cell is
//                          a fringe along a straight line; the lobes must scale with the band to read organic.
// CROSS-EDGE AGREEMENT: an edge takes the **max** of its two terrains' scales. max is COMMUTATIVE, so both
// flanking hexes derive the identical band/amplitude/cell for that edge — the seam weight p is 0.5 on the
// edge from both frames and the mix stays continuous across it, exactly as with the global levers. (This is
// the same discipline that makes the shore profile key on the water side: both sides must agree, or the
// profile itself draws the hard line it exists to remove.) Two unprofiled terrains give max(1,1) = 1 on every
// axis, so every existing biome seam is bit-identical. Built by TerrainTextureManager.
uniform sampler2D layer_blend_map : filter_nearest;
// id-map splatmap: R = terrain id (0..255), G = blend_class code (0 water, 1 flat, 2 rugged),
// B = canopy code (0 = no canopy, else canopy-array layer + 1),
// A = peak code (0 = no peaks, else peak-array layer + 1).
uniform sampler2D id_map : filter_nearest;
// visibility raster: R = 0 unexplored / 0.5 discovered / 1 active (only read when fow_enabled).
uniform sampler2D vis_map : filter_nearest;
// elevation raster: R = per-hex relative height 0..1 (0..100 scaled), drives peak prominence + shadow length.
uniform sampler2D elev_map : filter_nearest;

uniform int grid_w;
uniform int grid_h;
uniform float hex_radius;
uniform vec2 hex_origin;      // MapView last_origin, in this quad's local space (== MapView local space)
uniform bool wrap_h;
uniform float blend_band;     // transition half-band width in px (blend_width * radius) — the seam's REACH
uniform float blend_soft;     // FEATHER SOFTNESS: half-width (in seam-weight units) of the smoothstep that
                              // turns the seam weight into the mix factor. Small ⇒ a crisp, stippled edge
                              // that follows the noise/detail closely; large ⇒ a smooth gradient.
uniform float blend_height_influence; // how strongly texture detail NUDGES the boundary (vs. the distance
                              // weight, which always LEADS). Keep SMALL (0..~0.5): the weights decide, the
                              // heights only bend the 50/50 line toward the darker/lighter side.
uniform float blend_noise_cell; // flat↔flat INTERLOCK-only value-noise cell size in px = the wobble's cell.
                              // Fed as blend_noise_scale * hex_radius (config blend_noise_scale, a
                              // FRACTION of the hex radius) so the grain-per-hex is zoom-invariant — a fixed
                              // px cell made the grain grow/shrink relative to a hex as you zoomed.
uniform float blend_noise_amount; // amplitude of that boundary wobble (added to the seam weight, NOT
                              // thresholded against it — this is not a dither).
// WATER↔WATER levers (config "water_blend"). Water does NOT take the flat↔flat nearest-edge seam path at
// all — it is a DEPTH FIELD over all six neighbours (see WATER_FIELD_PLATEAU_MAX and the water block in
// fragment()), because a hex's water id is a quantized sample of a continuous seafloor, not one side of an
// ecotone. The three levers keep their names but mean:
//   band  — the field's REACH: how far into the own hex a neighbour's depth still bleeds (px).
//   soft  — the PLATEAU as a fraction of that reach: how far back from the shared edge a neighbour already
//           carries FULL weight (this is what dissolves the hexagon; capped by WATER_FIELD_PLATEAU_MAX).
//   noise — amplitude of the world-noise displacement of the depth boundary (in reach units).
// The wobble CELL and the height nudge are deliberately NOT overridden (shared with land; the height term is
// a no-op on smooth low-variance water anyway).
uniform float water_blend_band;
uniform float water_blend_soft;
uniform float water_blend_noise_amount;
uniform float noise_cell;     // FEATURE value-noise cell size in px (config feature_noise_cell) — shared by the
                              // shoreline reach/wisp, the canopy treeline and the peak footline. Deliberately
                              // DECOUPLED from blend_noise_cell so retuning the interlock grain can't move any
                              // coastline, treeline or footline.
uniform bool blend_enabled;   // false below the LOD radius → base only (no shimmer at far zoom)
// RUGGED-LAND eligibility (config blend_rugged_land — it SHIPS TRUE: terrain_config.json "blend_rugged_land"
// and EDGE_BLEND_DEFAULT_RUGGED_LAND in MapView.gd). When TRUE the LAND seam gate widens from same-class to
// "both sides are land": a rugged biome's BASE FLOOR then blends with any land neighbour (flat↔rugged and
// rugged↔rugged), instead of ending in a razor-straight hexagon — which is the whole reason it shipped (a
// peak biome's floor IS the ground under its relief overlay, so a hard-edged floor made rolling hills read as
// CUT OFF at the hex line). land↔water is untouched (still hard — that seam is the shoreline) and water↔water
// still takes the depth field, so a frame with no rugged hex is bit-identical either way.
// The risk the flag is a LEVER for — high-contrast/structured rugged art SHREDDING under the height term —
// is why the sweep exists, not why it is off: blend_probe state R renders EVERY rugged biome as an ISOLATED
// hex in a contrasting field, gate OFF vs ON, and none shredded (interiors stay solid; only the rim
// feathers). Keep the flag so a future rugged texture that DOES shred can be swept and, if need be, gated.
uniform bool blend_rugged_land;
uniform bool fow_enabled;
uniform vec4 bg_color;        // out-of-map fill (matches the direct path background)
uniform vec4 fog_color;       // unexplored fill
uniform vec3 mist_color;      // discovered multiply tint (matches _fow_texture_tint_for_state)
uniform float mist_blend;
// FoW SOFTENING (see the fow block in fragment()). The vis-map is NEAREST-sampled per hex, so an
// active↔discovered adjacency used to be a HARD HEXAGONAL brightness step — straight edges cutting across
// even uniform water, where no terrain seam exists at all. fow_soft is the width (px) of the cross-edge
// smoothing kernel: each neighbour's visibility is weighted by how close the fragment is to THAT shared
// edge, so the visibility scalar is continuous across every hex boundary before it is mapped to a tint.
uniform float fow_soft;         // smoothing reach in px (fow_softness * radius), like blend_band
uniform float fow_noise_amount; // wispiness: world-noise perturbation of the smoothed visibility, applied
                                // ONLY near a visibility boundary (see FOW_NOISE_EDGE_GAIN), so the fog line
                                // reads cloudy instead of a clean arc. 0 disables.
// Shoreline (land↔water coasts) — a coastal profile read seaward as: land → sand → surf → open water,
// with NO hard step anywhere in that chain. It is a function of a single SIGNED coast coordinate u
// (negative inland, 0 exactly at the coastline, positive seaward — see the shore block in fragment()),
// so every weight it defines is one function evaluated on both sides of the shared edge and therefore
// CONTINUOUS ACROSS IT by construction.
// THE SHAPE: the BASE ITSELF cross-fades across the waterline (waterline_band — see below), then the
// SAND lives ONLY in the LAND hex (full at the waterline, smoothstep-fading inland into
// the biome art — it must never bury it, hence the short reach). The SURF peaks AT the waterline and
// washes BOTH ways: a short distance INLAND over the sand (foam_inland_band — the crossfade that blends
// the waves into the beach) and a longer way SEAWARD into open water (foam_band).
// History (three rejected passes, all the same class of bug — a term saturating AT the hex edge, or sand
// where the user does not want it):
//   · Two-sided linear fades: land went solid tan, water solid white, and they met along the boundary —
//     a hard tan↔white line TRACING THE HEXAGON.
//   · Then everything was pushed onto the WATER side (land_beach_width = 0): that killed the sand↔foam
//     line but the sand now stopped DEAD at the hex edge against the raw land texture — a NEW hard
//     sand↔land line. Symmetric bug, other side of the seam.
//   · Then sand on BOTH sides (sand_land_band + sand_sea_band): every line was gone, but the beach now
//     read TWICE AS WIDE — sand in the water hex is not wanted at all. Hence: sand is land-only, and the
//     sand↔foam blend is bought with the surf washing INLAND over it instead of with sand in the sea.
// Same signed-distance-to-shared-edge machinery as the flat interlock; LOD-suppressed via blend_enabled.
uniform float sand_band;         // sand reach INLAND of the coastline, in px (sand_width * radius). LAND ONLY.
uniform float foam_inland_band;  // how far the surf washes UP the beach, in px (foam_inland_width * radius)
uniform float foam_band;         // surf reach SEAWARD of the coastline, in px (foam_width * radius)
// The faint SECOND surf line further out to sea. Its geometry is radius-relative like every other shore
// reach (wisp_center_width / wisp_half_width * radius) and therefore INDEPENDENT of foam_band: it used to be
// a fixed multiple of the surf's reach, which chained the two so the wisp could not be pulled in without
// also shortening the surf (and vice versa). Set wisp_half_band = 0 to turn the wisp off entirely.
uniform float wisp_center_band;  // distance from the coastline to the wisp's CENTRE, in px
uniform float wisp_half_band;    // the wisp's half-thickness, in px
// THE WATERLINE BASE CROSS-FADE — the reason the surf no longer has to be opaque.
// Until this existed the BASE TEXTURE ITSELF stepped at u = 0: on a beach coast the (sand-tinted) land
// texture met open water with nothing in between, and on a CLIFF coast (deep_ocean, sand_scale 0) it was
// raw land meeting raw water. The full-strength foam peak was the ONLY thing concealing that flip — which
// is why every attempt to soften the surf re-exposed a hard land↔water line and had to be reverted. So the
// base now cross-fades across a SHORT reach either side of the waterline (a WET EDGE, not an ecotone: keep
// it well under the sand's reach or land texture reads out to sea / water texture reads up the beach).
// It is the same weighted-mean-over-{own + 6 neighbours} construction as the shore PROFILE field (see
// SHORE_PROFILE_REACH_APOTHEMS), so both the land estimate and the water estimate are pure functions of the
// id-map and the world UV — identical read from the land hex's frame and from the water hex's frame — and
// the mix weight is one smoothstep of u. Continuous across the hex boundary by construction, like every
// other shore term. waterline_band = 0 disables it (bit-exact: the whole block is skipped).
uniform float waterline_band;  // half-reach of the base cross-fade either side of the waterline, in px
                               // (waterline_width * radius)
uniform vec3 foam_color;      // surf line color (light desaturated cyan / near-white)
uniform vec3 beach_color;     // sand color — the beach, on the LAND side of the waterline only
// The surf's PEAK opacity at the waterline. This is a lever ONLY because the waterline base cross-fade above
// removed the step the foam used to be covering up; with waterline_band = 0 the surf is load-bearing and
// this must stay ~1. It scales the offshore wisp too (SHORE_WISP_STRENGTH), so the whole surf mutes as one
// gesture rather than the peak fading while the froth stays bright. 1.0 is a bit-exact no-op.
uniform float foam_opacity;

const float SQRT3 = 1.7320508075688772;
// blend_class codes, as packed into the id-map's G channel by MapView._blend_class_code. The SAME-CLASS
// eligibility rule below is expressed in these, never in bare literals.
const int CLASS_WATER = 0;
const int CLASS_FLAT = 1;
const int CLASS_RUGGED = 2;  // blends with any LAND neighbour while blend_rugged_land (shipped TRUE) is on;
                             // with the gate off it never blends at all — see the uniform + the gate
// Shore look tuning (fixed feel; the widths/colors are the config levers).
const float SHORE_REACH_NOISE_MIN = 0.35; // min fraction of the shore band the noise leaves → wavy fingers
const float SHORE_REACH_MIN_PX = 1.0;     // floor on any noise-shrunk reach, so no fade ever divides by ~0
const float SHORE_SAND_OPACITY = 0.85;    // max sand wash over the base texture — deliberately < 1 so the
                                          // beach keeps the underlying biome detail and never reads as flat paint
// The sand's PLATEAU: how far inland it stays at FULL strength before it starts fading into the biome art.
// It is anchored to the SURF'S INLAND WASH (`foam_inland_band`), and that anchor is the whole point. The surf
// is composited OVER the sand and peaks at ~1 AT the waterline, so wherever the wash is strong the sand is
// whitewashed and contributes nothing. If the sand ALSO starts decaying from the waterline (as a plain
// `1 - smoothstep(0, sand_reach, -u)` does), then by the time the foam has cleared, the sand is already down
// to ~30% opacity and gone entirely a hair further inland — the beach is invisible and the coast reads
// land → surf → water with NO SAND AT ALL (verified against a dark rocky-regolith coast: white foam met bare
// rock). Holding the sand at FULL across the wash means the retreating surf uncovers a full-strength beach —
// that IS the sand↔foam crossfade — and only THEN does the sand fade inland. The cap keeps a fade window even
// if a config sets the wash wider than the sand reach: at least (1 - this) of the sand reach is always fade,
// so the inner edge can never become a hard sand↔land step.
const float SHORE_SAND_PLATEAU_MAX = 0.6; // plateau ≤ this fraction of the sand reach (shipped config sits at
                                          // exactly this cap: wash 0.15·r vs sand 0.25·r)
const float SHORE_WISP_STRENGTH = 0.45;   // second foam wisp max opacity (its GEOMETRY is a config lever —
                                          // see wisp_center_band / wisp_half_band)
// The waterline cross-fade's OUTER FADE, as a fraction of its half-reach: how far past the wet edge the true
// base (the flat↔flat interlock inland, the water depth field seaward) fades back in. It exists so the
// cross-fade can hold FULL weight across its whole reach. The first cut enveloped the cross-fade by
// `1 − smoothstep(0, reach, |u|)`, i.e. multiplied two ramps that both peak at u = 0 — the water content on
// the land side was already down to 8% at HALF the reach, so the visible transition was a quarter of the
// nominal one (~4px) and the base step SURVIVED, which is the whole thing this pass exists to remove
// (measured on the foam-off cliff step check). Holding the mix at full across the reach and fading back to
// the true base OUTSIDE it makes the gradient the reach you actually configured.
const float SHORE_WATERLINE_FADE = 0.5;
// --- THE SHORE-PROFILE FIELD: the profile is a CONTINUOUS WEIGHTED MEAN over the nearby water, never a
// nearest-pick. One land hex can border a deep_ocean hex AND a continental_shelf hex along the SAME
// coastline, and those two carry different profiles (sand_scale 0 vs 1). Picking the nearest water
// neighbour's profile makes the beach SWITCH ON at the bisector between them — a hard line of sand appearing
// out of nowhere along the shore. So every WATER hex in the {own + 6 neighbours} set contributes at once,
// weighted by proximity to THAT shared edge (the water depth field's discipline), and the profile is their
// normalized weighted mean: a cliff coast TRANSITIONS into a beach coast instead of switching.
//
// REACH = this × the own↔neighbour half-distance (the hex APOTHEM, which the loop already computes as
// half_dist). 1.0 is the LARGEST reach for which the mean is EXACTLY continuous across every hex edge, and
// that continuity is the whole ballgame (the shore weights are smoothsteps of the signed coast coordinate u,
// evaluated from BOTH sides of the waterline — if the two sides read different scales, the profile steps at
// the hex line, which is the exact bug u exists to prevent). Why 1.0 works, on the shared edge of hexes A|B:
//   · a water hex C that is a neighbour of BOTH reads the same signed distance from A's frame and from B's
//     frame (the three bisectors meeting at the corner are symmetric under the 120° rotation about it), so
//     the two frames give C the same weight;
//   · a water hex enumerated from only ONE of the frames has signed distance ≤ −apothem there, so its weight
//     is exactly 0 — the frame that cannot see it agrees.
// Raising it above 1.0 breaks the second bullet and re-introduces a step at the hex boundary.
const float SHORE_PROFILE_REACH_APOTHEMS = 1.0;
// Divide-by-zero guard on the weighted mean. The weight sum only approaches 0 deep inside a LAND hex, more
// than an apothem (0.87·r) from every water edge — far beyond the longest shore reach (0.68·r) — so the
// profile there is multiplied into terms that are already 0. Clamping the denominator makes the mean decay to
// (0,0,0) instead of exploding, which is the same zero output, continuously.
const float SHORE_PROFILE_WEIGHT_MIN = 1e-4;
const float CANOPY_TREELINE_NOISE = 0.6;  // how hard world-noise waves the treeline in/out (× overhang)
// River look tuning (fixed feel; the widths/softness/meander/scale are the config levers).
const int RIVER_CLASS_MINOR = 1;          // 2-bit mask class values (0 = no river; 3 is RESERVED, never drawn)
const int RIVER_CLASS_MAX = 2;            // Major — the highest class with a texture layer (layer = class - 1)
const float RIVER_HALF_SIDE = 0.5;        // a regular hexagon's side length == its circumradius, so the shared
                                          // edge is mid ± perp * (hex_radius * 0.5) — the segment, NOT the
                                          // infinite bisector (which would smear the band across the hex)
const vec2 RIVER_NOISE_OFFSET = vec2(23.0, 41.0); // decorrelate the meander noise from the shore/treeline ones
const vec2 RIVER_NOISE_OFFSET_Y = vec2(61.0, 7.0); // second axis of the meander domain warp
// Meander wavelength, in HEX RADII — deliberately NOT the shared `noise_cell`, which is a fixed px size:
// at a few px the river would get a fuzzy bank rather than a wander, and being screen-space its character
// would change with zoom. Keying the cell to hex_radius puts the wander on a ~1.5-hex wavelength that
// looks the same at every zoom. The amplitude (river_meander) must stay WELL under this, or the warp's
// gradient exceeds the band's half-width and the river TEARS into disconnected pools.
const float RIVER_MEANDER_CELL = 0.9;
// Width-variation wavelength, in HEX RADII. Deliberately LONG (a few radii, so one swell spans several
// edges): the swell must read as a property of the REACH, not of the hex it happens to sit on — a cell
// near 1 would re-key the variation to the lattice and reinforce exactly the honeycomb it is fighting.
const float RIVER_WIDTH_NOISE_CELL = 2.6;
const vec2 RIVER_WIDTH_NOISE_OFFSET = vec2(13.0, 71.0);  // decorrelated from the meander / bank / shore noises
// Ragged-bank wavelength, in HEX RADII — a few of these wobbles per edge, so the bank frays.
const float RIVER_BANK_NOISE_CELL = 0.35;
const vec2 RIVER_BANK_NOISE_OFFSET = vec2(89.0, 31.0);
// px floor on the half-width. Two jobs: the width/bank noise may PINCH the band but never sever it, and
// at far zoom (where the fraction-of-radius width goes sub-pixel) it keeps the river a legible hairline
// rather than letting it alias away — a river is a landmark you navigate BY (cf. the decoupled river LOD).
const float RIVER_MIN_HALF_WIDTH = 1.25;
// Minor (light water over gravel) and Major (deep water) are deliberately DIFFERENT art, but untreated
// they meet as turquoise-next-to-near-black and read as two waterways spliced, not one river growing.
// Both layers are pulled toward a shared hue and a shared depth pivot; what survives is the LUMA ORDER
// (Minor lighter, Major deeper), which is the thing that should say "bigger". The luma weights are the
// shared Rec.709 ones — see luma() / LUMA_WEIGHTS in TerrainTextureManager.
const vec3 RIVER_SHARED_HUE = vec3(0.24, 0.52, 0.58);
const float RIVER_DEPTH_PIVOT = 0.38;     // the luma both classes are drawn toward (≈ a mid river green-blue)
const float RIVER_LUMA_EPSILON = 0.001;   // guard: never divide by a black pixel's luma
const vec3 RIVER_HIGHLIGHT_COLOR = vec3(0.95, 0.25, 0.25); // debug tint (matches the retired polyline highlight)
const float RIVER_HIGHLIGHT_MIX = 0.75;   // how hard river_highlight pushes toward that tint
// NAVIGABLE channel (a water TERRAIN, not an edge — see the navigable pass): its water is river-array
// layer 2, painted OVER the hex's own bank floor. Unlike Minor/Major it runs hex CENTER → edge MIDPOINTS.
const float RIVER_NAVIGABLE_LAYER = 2.0;
// Degenerate fallback: a navigable hex with no qualifying arm (the sim should never emit one — MapView
// push_warnings if it does) still gets water, as a blob at the hex centre, rather than a hex of bare bank.
const float RIVER_NAVIGABLE_ORPHAN_BLOB = 0.9;  // blob radius, as a multiple of the channel half-width
// CHAIN HEAD, read off the riverChannel exit mask — the ONLY thing that says a navigable hex is the SOURCE
// of its chain rather than a hex in the middle of it. A head has just its ONE downstream link; a mid-chain
// hex has an upstream link too (2); a confluence has 3. riverInflow can NOT answer this any more: since the
// drainage network landed, a tributary hands over to the channel at ANY navigable hex's vertex, not only the
// chain's first (core_sim/CLAUDE.md → Rivers; docs/plan_rivers_drainage_network.md §A). Gating the head taper
// on inflow would pinch a full-width trunk to the tributary's width at a mid-chain junction — an HOURGLASS.
const int RIVER_CHANNEL_HEAD_MAX_EXITS = 1;
// Hex CORNER geometry, for the riverInflow mask (B/A of the river map): corner `i` is the vertex at angle
// 60*i + 30 with +y DOWN — exactly MapView._hex_points' order (0 lower-right, 1 bottom, 2 lower-left,
// 3 upper-left, 4 top, 5 upper-right), which is the wire contract the sim packs the mask against. So a
// corner's position is just own_c + hex_radius * (cos, sin) of that angle: no neighbour fetch, no table.
const float RIVER_CORNER_ANGLE_STEP = PI / 3.0;    // 60° between adjacent corners
const float RIVER_CORNER_ANGLE_OFFSET = PI / 6.0;  // 30° — flat-top-up hex, first corner at lower-right
// --- PEAK CAST SHADOW: it must die off with DISTANCE FROM THE RELIEF, never with the hex grid ---
// The occlusion term (the multi-tap look back toward the light) is near-CONSTANT across a whole neighbour hex,
// because the peak art is near-opaque almost everywhere it is sampled. On its own it therefore painted a flat
// darkening over every hex merely ADJACENT to relief, terminating on THAT hex's own boundary: dark HEXAGONS in
// the neighbouring biome. So the occlusion is enveloped by the signed distance to the peak↔non-peak boundary —
// the same quantity the overhang already uses, and evaluated identically on both sides of a shared edge (the
// discipline the shoreline's signed coast coordinate `u` follows), so nothing can pop at the hex line.
// The envelope is FULL at the (noise-wobbled) footline and smoothsteps to 0 over a reach that is:
//   * elevation-scaled — a high massif throws a longer shadow (PEAK_SHADOW_ELEV_FLOOR is the reach at elev 0);
//   * DIRECTIONAL — full length where the relief lies TOWARD the light (we are down-light of it, i.e. in its
//     cast shadow) and shrinking to PEAK_SHADOW_UPLIGHT_REACH of it on the LIT side, which leaves a short
//     contact/ambient skirt at the foot instead of a hard angular cutoff. A hard cutoff on dot(light, normal)
//     would step to zero at the footline on the lit side, where the relief art is only ~half opaque and the
//     shadowed ground shows through — i.e. it would trade the hexagon for a dark crescent.
// The envelope is a MAX over the qualifying boundary edges, never a sum: a hex touching relief on two sides
// takes the nearer/darker of the two and cannot double-darken into a seam. (A single-nearest-edge envelope
// would have been discontinuous along the bisector where the nearest edge switches, because the two edges'
// light alignments differ; max-of-continuous is continuous everywhere.)
const float PEAK_SHADOW_ELEV_FLOOR = 0.4;     // shadow reach at elevation 0, as a fraction of peak_shadow_len
const float PEAK_SHADOW_UPLIGHT_REACH = 0.15; // …and on the LIT side of the relief, as a fraction of the reach
const float PEAK_SHADOW_REACH_MIN = 1e-3;     // px floor, so the envelope's smoothstep can never divide by zero
const int PEAK_SHADOW_TAPS = 4;               // occlusion samples along the light ray (nearer occluder = darker)
// --- PEAK ↔ PEAK: two DIFFERENT reliefs meeting must cross-fade, and the TALLER one must do the overhanging ---
// A peak↔non-peak edge is a footline: the relief overhangs it and thins away (canopy_density). But an edge
// between two hexes that BOTH carry relief was not a boundary at all — each hex composited its OWN peak layer
// at full density right up to the shared edge, so the art switched 1-bit ON the hex line: rolling_hills' green
// mounds ended in a razor-straight diagonal and alpine_mountain's spires began (the "hills are still cut off"
// report; the BASE floor under them blends fine — it is simply invisible under near-opaque relief art).
// So a peak↔peak edge now cross-fades the two relief layers, as a CONTINUOUS WEIGHTED MIX (never a pick), with
// the seam's CENTRE — not its shape — driven by elevation: the taller relief overhangs DOWN onto the lower one,
// and the lower one must not climb uphill into it.
//   depth = distance from the shared edge INTO the own hex (0 at the edge, growing inward)
//   asym  = the elevation ordering, smooth and ODD in Δelev: +1 when the neighbour towers over us (its art
//           spills onto us), −1 when we tower over it (ours spills onto them), 0 when we are the same height
//   m     = peak_overhang · asym  — the depth (into US) at which the two layers sit 50/50. Positive = the
//           neighbour's art has pushed the seam INTO our hex; negative = we have pushed it into theirs.
//   w     = 1 − smoothstep(m − peak_softness, m + peak_softness, depth) — the neighbour layer's coverage here.
// CONTINUITY (the discipline the shoreline's signed coast coordinate `u` follows): the neighbour computes the
// same edge with asym negated, hence m negated, and smoothstep is symmetric about its centre — so at the shared
// edge (depth = 0 from BOTH sides) the two hexes assign the SAME coverage to the SAME layer. Nothing can step
// there for any elevation pair. At Δelev = 0 this degrades to m = 0: a symmetric 50/50 cross-fade of half-width
// peak_softness, not a coin flip. The reach and the feather reuse the peak block's existing overhang/softness
// levers, so a relief spilling onto a lower relief looks exactly like one spilling onto flat ground.
const float PEAK_BLEND_FULL_DELTA = 0.25;  // Δ in elev-map units (0..1 relative height) at which the overhang is
                                           // FULLY one-sided; below it the seam eases back toward symmetric.
// The seam centre is displaced by the same world-noise the footline rides, so the cross-fade MEANDERS instead of
// tracing the straight hex line (a soft straight gradient still reads as a straight line — which is the whole
// complaint). It must be applied ANTISYMMETRICALLY — the two hexes must displace the seam the same way in world
// space, i.e. with opposite signs in their own local `m` — or the continuity above breaks. The peak LAYER INDEX
// gives a total order that both sides agree on and that never ties (the layers differ, or there is no seam).
const float PEAK_BLEND_WOBBLE = 0.5;       // seam-centre wobble, as a fraction of the footline's wobble
// Fog-of-war visibility states, as written into the vis-map's R channel by MapView._rebuild_terrain_shader_maps.
// The tint is a CONTINUOUS function of the (cross-edge smoothed) visibility scalar between these anchors, so
// the pure states still look exactly as they did under the old per-state branch: 1 = clear, 0.5 = mist, 0 = fog.
const float FOW_UNEXPLORED = 0.0;
const float FOW_DISCOVERED = 0.5;
const float FOW_ACTIVE = 1.0;
// Envelope for the FoW wispiness noise. |smoothed − own| is 0 in a uniform-visibility interior and peaks at
// a state boundary, where the 6-neighbour average pulls the scalar halfway across ONE state gap
// (FOW_DISCOVERED − FOW_UNEXPLORED = 0.5, halved = 0.25). Dividing by that peak normalizes the envelope to
// 0..1, so the noise only bites AT boundaries and pure Active / Discovered / Unexplored areas are untouched.
const float FOW_NOISE_EDGE_PEAK = 0.25;
// Flat↔flat transition look tuning: the reach is blend_width (→ blend_band px), the feather softness is
// blend_soft, the boundary wobble is blend_noise_scale (a FRACTION OF THE HEX RADIUS → blend_noise_cell px)
// × blend_noise_amount, and blend_height_influence is an OPTIONAL detail-following nudge.
// THE INVARIANT: the seam is ALWAYS a continuous weighted mix (`mix(own, nb, t)`), never a 1-bit pick.
// Two rejected attempts were both 1-bit picks in disguise: (1) the original dither
// (`if (best_p > vnoise(...)) result = neighbour;`) — user verdict "chunky blobs"; (2) height blending with
// a large blend_height_influence and a small overlap depth, where the luminance term dwarfed the distance
// weight and degenerated into winner-takes-all-by-luma — user verdict "shredded, this isn't a blend at all"
// (dark soil punched holes deep into grass-hex interiors, because a dark patch of grass ANYWHERE in the band
// lost outright). Hence: the DISTANCE WEIGHT LEADS, heights only NUDGE (influence stays ≤ ~0.5), and the
// final step is always a smoothstep-weighted mix.
// Perturbation envelope: the noise + height perturbation is scaled by 4·p·(1−p), a parabola peaking at the
// seam (p = 0.5) and vanishing at both ends of the band, so a perturbation can never leak neighbour texture
// into hex interiors nor leave a straight discontinuity along the band's outer edge.
const float BLEND_PERTURB_ENVELOPE_PEAK = 4.0; // normalizes p·(1−p) (max 0.25) to a peak of 1.0 at p = 0.5
const float BLEND_SOFT_MIN = 1e-3;             // floor on blend_soft so the smoothstep never degenerates
                                               // into a divide-by-zero / hard step
// WATER↔WATER is NOT a seam — it is a DEPTH FIELD (see the water block in fragment()).
// The flat↔flat path above blends against the ONE nearest qualifying edge, which is right for an ecotone:
// two biomes meet along a line and interlock across it. Water is not that. A hex's water id is a QUANTIZED
// SAMPLE of a continuous seafloor depth, and the real map's ocean is salt-and-pepper (a live 80×52 snapshot's
// id-map carried 2332 deep_ocean↔continental_shelf hex adjacencies and 16 deep hexes whose SIX water
// neighbours were all a different id). Under a nearest-edge seam blend such a hex can only read as a dark
// HEXAGON: the rim feathers, but the interior keeps the (much darker) deep texture and the silhouette IS the
// hex — which is exactly the "dark patches with hard full-hexagon edges in open water" report, and it is
// TERRAIN, not the FoW tint (FoW off leaves the vis-map unread — the tint cannot fire).
// So for water every qualifying neighbour contributes AT ONCE, weighted by how close the fragment is to THAT
// shared edge — the same 6-neighbour cross-edge weighting the FoW softening uses — and the result is a
// normalized weighted mean of the water textures. The weight reaches 1 exactly AT a shared edge, so at that
// edge the mean is (own + nb)/2 read from BOTH sides: continuous across every boundary by construction.
// water_blend_soft is reinterpreted for this field as the PLATEAU: the fraction of the reach, measured back
// from the shared edge, over which a neighbour already carries FULL weight. A wide plateau is what actually
// dissolves the silhouette (a pure ramp only softens its rim). Capped so a ramp always survives — a plateau
// spanning the whole reach would put a hard step at the reach's outer edge.
const float WATER_FIELD_PLATEAU_MAX = 0.5;

// v_world is the quad-LOCAL (viewport/screen-fixed) coordinate — it does NOT track the map on
// pan/zoom. hex_origin (= MapView last_origin) moves with the pan/zoom, so hex-anchored terms
// (pp, the base/neighbour biome UVs, hex_center/signed-distance math) that subtract hex_origin or a
// hex center are pan/zoom-invariant for a fixed map point. Any map-space term built from RAW v_world
// would slide against the grid — use the derived v_map (= v_world - hex_origin) in fragment() instead.
varying vec2 v_world;

void vertex() {
	v_world = VERTEX; // quad-LOCAL (screen-fixed) coord; hex centers are computed in this same local space
}

float hash21(vec2 p) {
	p = fract(p * vec2(127.1, 311.7));
	p += dot(p, p + 34.23);
	return fract(p.x * p.y);
}

// Smooth value noise in world space (chunky blobs, not per-pixel snow) → clean dither grain.
float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash21(i);
	float b = hash21(i + vec2(1.0, 0.0));
	float c = hash21(i + vec2(0.0, 1.0));
	float d = hash21(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Rec.709 luminance — the pseudo-height of a texel for height blending. MUST match LUMA_WEIGHTS in
// TerrainTextureManager, which measures each layer's mean of exactly this quantity.
float luma(vec3 c) {
	return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// Mean luminance of a base layer (the zero-point of its pseudo-height), fetched by layer index.
float mean_luma(int layer) {
	return texelFetch(layer_luma_map, ivec2(layer, 0), 0).r;
}

// Shore profile of a terrain: (sand_scale, foam_scale, wisp_scale), fetched by layer index. Neutral
// (1, 1, 1) for any terrain without a `shore_profile` block. See layer_shore_map.
vec3 shore_profile(int layer) {
	return texelFetch(layer_shore_map, ivec2(layer, 0), 0).rgb;
}

// Blend profile of a terrain: (width_scale, noise_scale, noise_cell_scale), fetched by layer index. Neutral
// (1, 1, 1) for any terrain without a `blend_profile` block. See layer_blend_map.
vec3 blend_profile(int layer) {
	return texelFetch(layer_blend_map, ivec2(layer, 0), 0).rgb;
}

// The blend profile of an EDGE, from its two terrains': the per-axis MAX. Commutative, so the two hexes
// flanking the edge compute the same three scales and the seam stays continuous across it (see
// layer_blend_map). A seam between two unprofiled terrains is (1, 1, 1) — a bit-exact no-op.
vec3 edge_blend_profile(vec3 own_profile, int nb_layer) {
	return max(own_profile, blend_profile(nb_layer));
}

// Canopy density across the treeline. s = signed distance to the canopy↔non-canopy boundary,
// POSITIVE inside the forest, 0 at the edge, negative in the neighbour. Deep inside → 1; ~0.5 at the
// exact edge; ramps to 0 canopy_overhang px OUTSIDE the forest (crowns overhang, then fade).
float canopy_density(float s, float overhang, float softness) {
	if (s >= 0.0) { return 0.5 + 0.5 * smoothstep(0.0, softness, s); }
	return 0.5 * smoothstep(-overhang, 0.0, s);
}

// Premultiplied-alpha conversions, for AVERAGING two overlay samples that have independent alpha (the
// peak↔peak cross-fade). Averaging straight-alpha RGBA lets a fully TRANSPARENT texel's leftover RGB drag
// the colour wherever the other layer is opaque — it painted bright fringes along the seam. Colour must be
// weighted by coverage × alpha; alpha averages on its own.
const float PREMUL_ALPHA_MIN = 1e-4;   // un-premultiply guard: below this the colour is undefined anyway
vec4 premultiplied(vec4 c) {
	return vec4(c.rgb * c.a, c.a);
}
vec4 unpremultiplied(vec4 c) {
	return vec4(c.rgb / max(c.a, PREMUL_ALPHA_MIN), c.a);
}

// Pull a river water sample onto the SHARED river look: compress its luma toward RIVER_DEPTH_PIVOT
// (river_depth_compress) and its chroma toward RIVER_SHARED_HUE (river_tint_strength), preserving the
// relative order (Minor stays lighter than Major). This is what makes following a river from headwater to
// mouth read as one waterway GROWING rather than a bright stream spliced onto a near-black one.
vec3 river_harmonize(vec3 c) {
	float l = luma(c);
	float lc = mix(l, RIVER_DEPTH_PIVOT, river_depth_compress);   // depth-compressed target luma
	vec3 graded = c * (lc / max(l, RIVER_LUMA_EPSILON));          // same pixel, re-exposed to that luma
	vec3 hue = RIVER_SHARED_HUE * (lc / luma(RIVER_SHARED_HUE));  // shared hue AT that luma
	return clamp(mix(graded, hue, river_tint_strength), vec3(0.0), vec3(1.0));
}

// --- the river "organic" machinery, SHARED by the edge (Minor/Major) and navigable-channel passes ---
// All three sample WORLD space (v_map), never hex-local space: two hexes flanking a shared boundary must
// get identical values there, or the water would step at the boundary. That is what makes the edge river's
// two half-bands meet with no seam AND the navigable channel continuous across adjacent navigable hexes.
// It is the same discipline every other cross-edge term here follows (the shore's signed coast coordinate
// u, the peak seam's wobble): load-bearing, not stylistic.
//
// MEANDER — a DOMAIN WARP of the point we measure FROM, not a scalar bias on the distance. Nudging the
// (unsigned) distance would only bulge/pinch a still perfectly straight centerline; warping the point bends
// the centerline itself. CAPPED by design (see RIVER_MEANDER_CELL).
vec2 river_meander_warp(vec2 p_world, vec2 p_map) {
	vec2 mp = p_map / (hex_radius * RIVER_MEANDER_CELL);
	vec2 warp = vec2(vnoise(mp + RIVER_NOISE_OFFSET) - 0.5,
		vnoise(mp + RIVER_NOISE_OFFSET_Y) - 0.5) * 2.0 * river_meander;
	return p_world + warp;
}

// WIDTH VARIATION — the river swells and pinches ALONG its length. A constant-width stroke is what makes an
// edge river read as INKED HEX BORDERS (and a navigable channel as a canal); a varying one reads as flowed
// water. Low-frequency (cells of a few hex radii) so one swell spans several hexes and the variation is a
// property of the REACH, not of the hex it sits on.
float river_width_mod(vec2 p_map) {
	float wn = vnoise(p_map / (hex_radius * RIVER_WIDTH_NOISE_CELL) + RIVER_WIDTH_NOISE_OFFSET);
	return mix(1.0 - river_width_variation, 1.0 + river_width_variation, wn);
}

// RAGGED BANKS — a higher-frequency wobble of the half-width (px), the same idiom the shore pass's noisy
// `reach` rides. Frays the bank without moving the centerline.
float river_bank_wobble(vec2 p_map) {
	return river_bank_noise * (vnoise(p_map / (hex_radius * RIVER_BANK_NOISE_CELL) + RIVER_BANK_NOISE_OFFSET) - 0.5);
}

// odd-r neighbour offsets, in the SIM's direction order (core_sim grid_utils HEX_NEIGHBOR_OFFSETS:
// clockwise from E — 0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE; +y is DOWN in canvas space, so SE steps row+1).
// THIS ORDER IS A WIRE CONTRACT. The blend / water-depth-field / shore-profile / waterline / canopy / peak
// passes only ever loop over all 6 and are ORDER-AGNOSTIC (they key off the neighbour's id-map texel, never
// off `dir`), so they are indifferent to it — but the RIVER passes index the per-tile river_edges and
// river_channel bitmasks BY DIRECTION (class = (mask >> (2*dir)) & 3; exits = (mask >> dir) & 1), so the
// table's order IS the contract with the sim's packer. Reordering it (e.g. back to the mirrored
// 0=E, 1=NE, 2=NW… this file once carried) silently renders every river on the WRONG SIDE of its hex.
// Do not reorder.
ivec2 neighbor_offset(int row, int dir) {
	int odd = row & 1;
	if (dir == 0) { return ivec2(1, 0); }                  // E
	if (dir == 1) { return ivec2(odd == 1 ? 1 : 0, 1); }   // SE
	if (dir == 2) { return ivec2(odd == 1 ? 0 : -1, 1); }  // SW
	if (dir == 3) { return ivec2(-1, 0); }                 // W
	if (dir == 4) { return ivec2(odd == 1 ? 0 : -1, -1); } // NW
	return ivec2(odd == 1 ? 1 : 0, -1);                    // NE
}

// offset (col,row) → hex center in local space (MUST match MapView._axial_center + _offset_to_axial).
vec2 hex_center(int col, int row) {
	float q = float(col) - floor(float(row) / 2.0);
	float r = float(row);
	return hex_origin + vec2(hex_radius * (SQRT3 * q + SQRT3 * 0.5 * r), hex_radius * 1.5 * r);
}

int wrap_col(int col) {
	return col - grid_w * int(floor(float(col) / float(grid_w)));
}

void fragment() {
	// --- invert the hex layout: world → fractional axial → cube-round → offset (col,row) ---
	vec2 pp = (v_world - hex_origin) / hex_radius;
	// Pan/zoom-anchored MAP-space coordinate: v_world is the quad-LOCAL (screen-fixed) coord, so any
	// map-space term built from RAW v_world slides against the grid on pan/zoom. Subtracting hex_origin
	// anchors it to the map (invariant for a fixed map point). Use v_map — NOT v_world — for every
	// map-space term below (canopy UV, dither/shore/treeline noise) so they track the grid.
	vec2 v_map = pp * hex_radius; // == v_world - hex_origin
	float rf = pp.y / 1.5;
	float qf = pp.x / SQRT3 - 0.5 * rf;
	float xf = qf;
	float zf = rf;
	float yf = -qf - rf;
	float rx = floor(xf + 0.5);
	float ry = floor(yf + 0.5);
	float rz = floor(zf + 0.5);
	float dx = abs(rx - xf);
	float dy = abs(ry - yf);
	float dz = abs(rz - zf);
	if (dx > dy && dx > dz) {
		rx = -ry - rz;
	} else if (dy > dz) {
		ry = -rx - rz;
	} else {
		rz = -rx - ry;
	}
	int row = int(rz);
	int col = int(rx) + int(floor(float(row) / 2.0));

	bool in_map = row >= 0 && row < grid_h;
	int col_id = col;
	if (wrap_h) {
		col_id = wrap_col(col);
	} else {
		in_map = in_map && col >= 0 && col < grid_w;
	}

	// Peak UV + its EXPLICIT screen-space gradients, computed HERE — in fully uniform control flow, before any
	// branch. The peak pass's texture fetches all sit inside per-edge/per-fragment `if`s (divergent flow), where
	// implicit-LOD `texture()` has UNDEFINED derivatives: on a 2×2 quad that straddles a relief↔relief seam the
	// lanes take different branches, the driver picks a garbage mip, and the fetch comes back at the wrong
	// resolution. That drew a 1-PIXEL DARK COLUMN exactly along the hex edge — a razor line, straight back where
	// we started, hiding inside an otherwise correct cross-fade. Sampling with textureGrad and these gradients
	// makes the LOD well-defined for every lane. (Same continuous map-space UV as the canopy's cuv.)
	vec2 puv = v_map / (2.0 * hex_radius) * peak_scale;
	vec2 puv_dx = dFdx(puv);
	vec2 puv_dy = dFdy(puv);

	if (!in_map) {
		COLOR = bg_color;
	} else {
		vec2 grid_dim = vec2(float(grid_w), float(grid_h));
		vec2 id_uv = (vec2(float(col_id), float(row)) + 0.5) / grid_dim;
		vec4 idpx = texture(id_map, id_uv);
		int own_layer = int(idpx.r * 255.0 + 0.5);
		int own_class = int(idpx.g * 255.0 + 0.5);

		vec2 own_c = hex_center(col, row);
		// Base biome UV — CONTINUOUS world space (like the canopy pass), NOT per-hex-normalized. A single
		// texture spans ~1/base_scale hex-rows, so adjacent hexes show DIFFERENT regions of it and the
		// per-hex identical repeat (the diagonal-seam grid on detailed textures) disappears. Uses v_map
		// (pan/zoom-anchored) so terrain tracks the grid on pan/zoom. repeat_enable tiles it.
		vec2 base_uv = v_map / (2.0 * hex_radius) * base_scale;
		vec3 result = texture(biome_array, vec3(base_uv, float(own_layer))).rgb;

		// ELIGIBILITY: the base rule is SAME-CLASS — a hex blends only with a neighbour of its OWN blend_class
		// and a different terrain id. flat↔flat (grass↔soil ecotones) and water↔water (deep ocean grading into
		// shelf) both qualify; land↔water is a CLASS CHANGE, so it stays hard — that seam is the SHORELINE,
		// owned by the foam/beach pass below, and softening it would wash the coastline out.
		// blend_rugged_land (SHIPPED TRUE — see the uniform) widens the LAND half of that rule to "both sides
		// are land", so rugged (class 2) blends with any land neighbour; with it off, rugged never blends.
		bool own_land = own_class != CLASS_WATER;
		bool own_blendable = own_class == CLASS_FLAT || own_class == CLASS_WATER
			|| (blend_rugged_land && own_class == CLASS_RUGGED);
		if (blend_enabled && own_class == CLASS_WATER) {
			// --- WATER: a continuous DEPTH FIELD, not a seam (see WATER_FIELD_PLATEAU_MAX) ---
			// EVERY qualifying water neighbour contributes at once, weighted by proximity to THAT shared edge;
			// the normalized weighted mean is the fragment's water. This dissolves the hex silhouette a lone
			// deep_ocean hex would otherwise keep (the dark-hexagon report) instead of merely feathering its rim.
			float reach = max(water_blend_band, BLEND_SOFT_MIN);
			// Plateau: distance back from the shared edge over which a neighbour already has FULL weight.
			float plateau = clamp(water_blend_soft, 0.0, WATER_FIELD_PLATEAU_MAX) * reach;
			// ONE world-noise sample, shared by all six weights: it displaces the fragment's distance to every
			// shared edge by the same amount, so the depth boundary MEANDERS organically. Sampled in map space,
			// so a given world point reads the same value from both sides of an edge — continuity survives it.
			float wobble = (vnoise(v_map / blend_noise_cell) - 0.5) * water_blend_noise_amount * reach;
			vec3 wsum_rgb = result;   // own water carries weight 1 by construction
			float wsum = 1.0;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				bool nb_in = true;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					nb_in = false;
				}
				if (!nb_in) { continue; }
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				vec4 npx = texture(id_map, nuv);
				int nlayer = int(npx.r * 255.0 + 0.5);
				int nclass = int(npx.g * 255.0 + 0.5);
				if (nclass != CLASS_WATER || nlayer == own_layer) { continue; } // water & a different depth only
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				// signed distance to the shared edge (the perpendicular bisector); <= 0 inside own hex.
				float d = dot(v_world - own_c, dir_v) - half_dist + wobble;
				// 0 at the reach limit → 1 from the plateau line inward to (and across) the shared edge.
				float w = smoothstep(-reach, -plateau, d);
				wsum_rgb += texture(biome_array, vec3(base_uv, float(nlayer))).rgb * w;
				wsum += w;
			}
			result = wsum_rgb / wsum;
		} else if (blend_enabled && own_blendable) {
			// --- FLAT↔FLAT: the nearest-edge ECOTONE interlock (unchanged; water no longer takes this path) ---
			float seam_soft = blend_soft;
			// This hex's BLEND PROFILE (width / noise-amplitude / noise-cell scales; neutral (1,1,1) unless the
			// terrain carries a `blend_profile` block). Each EDGE takes the per-axis max of the two terrains',
			// which is commutative — so the neighbour across that edge derives the same band, and the seam is
			// continuous exactly as it is under the plain global levers. See layer_blend_map.
			vec3 own_bp = blend_profile(own_layer);
			vec3 best_bp = own_bp;
			float best_p = 0.0;
			int best_layer = -1;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				bool nb_in = true;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					nb_in = false;
				}
				if (!nb_in) { continue; }
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				vec4 npx = texture(id_map, nuv);
				int nlayer = int(npx.r * 255.0 + 0.5);
				int nclass = int(npx.g * 255.0 + 0.5);
				// Eligibility: with blend_rugged_land (SHIPPED TRUE), ANY land neighbour (own is land here, so
				// this is flat↔flat / flat↔rugged / rugged↔rugged); with the gate off, same class only. Water
				// is excluded either way — that seam is the shoreline. Symmetric on both sides of the edge,
				// so the seam stays continuous.
				bool nb_eligible = blend_rugged_land && own_land
					? nclass != CLASS_WATER
					: nclass == own_class;
				if (!nb_eligible || nlayer == own_layer) { continue; }
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				// signed distance to the shared edge (the perpendicular bisector); <=0 inside own hex.
				float d = dot(v_world - own_c, dir_v) - half_dist;
				// THIS edge's band: the global reach scaled by the edge's profile (max of the two terrains',
				// so the hex on the other side scales it identically and p is 0.5 there from both frames).
				vec3 bp = edge_blend_profile(own_bp, nlayer);
				float seam_band = max(blend_band * bp.r, BLEND_SOFT_MIN);
				float p = clamp(0.5 + d / (2.0 * seam_band), 0.0, 1.0); // 0.5 AT the edge → symmetric
				if (p > best_p) {
					best_p = p;
					best_layer = nlayer;
					best_bp = bp;
				}
			}
			if (best_layer >= 0) {
				// FEATHERED, WOBBLED, DETAIL-NUDGED MIX — always continuous, never a pick.
				// best_p is the SPLAT WEIGHT (0 deep in own hex, 0.5 exactly at the shared edge, 1 deep in
				// the neighbour). It LEADS. Two perturbations bend it, both enveloped by 4·p·(1−p) so they
				// peak at the seam and vanish at the band's ends (no leakage into hex interiors):
				//   · world value noise → an organic, meandering boundary instead of the straight hex line;
				//   · the two textures' zero-centred luminances (their pseudo-height, we have no height maps)
				//     → the boundary bends toward the darker/lighter side so it follows the textures' own
				//     tufts and grains. This is a NUDGE: blend_height_influence stays small, so it can never
				//     out-vote the distance weight (which is exactly what shredded the hexes at 4.0).
				// The perturbed weight is then feathered through a smoothstep of half-width blend_soft and
				// used as a plain mix factor: every pixel in the band is a genuine blend of the two biomes.
				// The neighbour is sampled at the SAME continuous world position as the base (only the array
				// layer differs), so both competitors are the same world point of two biomes.
				vec3 own_rgb = result;
				vec3 nb_rgb = texture(biome_array, vec3(base_uv, float(best_layer))).rgb;

				float perturb_env = BLEND_PERTURB_ENVELOPE_PEAK * best_p * (1.0 - best_p);
				// The wobble rides the WINNING edge's profile: its amplitude scaled by noise_scale (bp.g) and
				// its cell — the wavelength — by noise_cell_scale (bp.b). Both are max()-derived, so the hex
				// across that edge samples the identical noise field at the identical amplitude; the world-space
				// sample already reads the same value from both sides at a shared point.
				float seam_noise_amount = blend_noise_amount * best_bp.g;
				float seam_noise_cell = max(blend_noise_cell * best_bp.b, BLEND_SOFT_MIN);
				float n = vnoise(v_map / seam_noise_cell);
				float wobble = (n - 0.5) * seam_noise_amount;
				// positive when OWN is the taller/brighter texture here → push the boundary toward the
				// neighbour (own keeps the pixel), i.e. SUBTRACT from the neighbour's weight.
				float h = (luma(own_rgb) - mean_luma(own_layer)) - (luma(nb_rgb) - mean_luma(best_layer));
				float pw = clamp(best_p + (wobble - h * blend_height_influence) * perturb_env, 0.0, 1.0);

				float soft = max(seam_soft, BLEND_SOFT_MIN);
				float t = smoothstep(0.5 - soft, 0.5 + soft, pw);
				result = mix(own_rgb, nb_rgb, t);
			}
		}

		// A NAVIGABLE river hex, by terrain id. It is a water TERRAIN in the sim, but its blend_class is
		// deliberately "flat" — it renders as a BANK with a channel painted on top (see the navigable pass) —
		// so the shore pass, which keys purely on blend_class, would otherwise treat every one of its edges
		// against real water as a COAST. Hence the skip below; see there for why it cannot be a shore_profile.
		bool own_navigable = own_layer == river_navigable_terrain_id;

		// --- shoreline: ONE continuous coastal profile straddling every land↔water edge ---
		// Universal: fires for ANY edge where exactly one side is water. Inland flat↔flat, water↔water and
		// rugged↔* edges are untouched (both sides same wetness → skipped). LOD-gated like the blend.
		// Seaward read: land → sand → surf → open water, with NO hard step anywhere: every weight below is a
		// smoothstep of the SIGNED coast coordinate u, one function evaluated on both sides of the shared
		// edge, so nothing can pop at the hex boundary (see the uniform block for the two bugs this fixes).
		if (blend_enabled) {
			bool own_water = own_class == CLASS_WATER;
			float best_d = -1e9; // nearest qualifying edge = max signed distance (closest to the seam)
			// THE PROFILE IS KEYED ON THE WATER, AS A CONTINUOUS FIELD — both halves are correctness
			// requirements, not preferences.
			// (a) KEYED ON THE WATER: the profile must be the SAME function on both sides of the waterline, or
			//     the smoothsteps of u would evaluate with different reaches on the land hex and the water hex
			//     and re-introduce exactly the hard line at the hex edge that the signed coast coordinate
			//     exists to prevent. The water is the only side both fragments can agree on (the land biome
			//     differs from hex to hex along a coast, the body of water does not), and it is the meaningful
			//     one: "cliff, beach or lake?" is a property of the water.
			// (b) A FIELD, NOT A PICK: a land hex can border deep_ocean AND continental_shelf along ONE
			//     coastline, and their profiles differ (no beach vs a full beach). Taking the NEAREST water
			//     neighbour's profile switches at the bisector between them — the beach would appear along a
			//     hard line. So every water hex in {own + 6 neighbours} contributes at once, weighted by
			//     proximity to THAT shared edge, and we take the normalized weighted mean (the water depth
			//     field's discipline). See SHORE_PROFILE_REACH_APOTHEMS for why that mean is EXACTLY
			//     continuous across every hex edge — including the land↔water edge, where it must be.
			// Own water carries weight 1 by construction; land contributes nothing (it has no profile).
			vec3 prof_sum = own_water ? shore_profile(own_layer) : vec3(0.0);
			float prof_w = own_water ? 1.0 : 0.0;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					continue;
				}
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				vec4 nidpx = texture(id_map, nuv);
				int nlayer = int(nidpx.r * 255.0 + 0.5);
				int nclass = int(nidpx.g * 255.0 + 0.5);
				bool nb_water = nclass == CLASS_WATER;
				bool nb_navigable = nlayer == river_navigable_terrain_id;
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c); // the hex APOTHEM
				float d = dot(v_world - own_c, dir_v) - half_dist; // <=0 inside own hex, 0 AT the shared edge
				if (nb_water) {
					// Profile weight: 0 at (and beyond) our own centre, 1 AT the shared edge. Unwobbled on
					// purpose — a noise displacement here would break the exact cross-edge agreement.
					float prof_reach = half_dist * SHORE_PROFILE_REACH_APOTHEMS;
					float w = smoothstep(-prof_reach, 0.0, d);
					prof_sum += shore_profile(nlayer) * w;
					prof_w += w;
				}
				if (own_water == nb_water) { continue; } // the COAST edge: exactly one side must be water
				// …EXCEPT where a NAVIGABLE river is on either side of it. A RIVER MEETING THE SEA IS NOT A
				// COAST: a navigable hex is a bank with a channel through it, so a shore treatment there is a lie
				// in both directions — a beach on the bank at the river's mouth, and a surf line drawn ACROSS
				// that mouth, visibly walling the river off from the sea it drains into (it read as a lake).
				// This has to be an EDGE-LEVEL exclusion, not a shore_profile entry: the profile field is keyed
				// on the WATER side (only a CLASS_WATER hex contributes one — see (a) above), and a navigable
				// hex is land-class by construction, so it can never supply a profile. The profile that WOULD
				// apply at the mouth is the SEA's, and that sea's coast elsewhere must keep it.
				// Dropping the edge here removes the whole chain at once (profile, waterline cross-fade, sand,
				// surf, wisp all live under `best_d > -1e8`), and symmetrically from BOTH hexes' frames — so no
				// half-drawn coast can survive on one side of it.
				if (own_navigable || nb_navigable) { continue; }
				if (d > best_d) { best_d = d; }           // nearest qualifying edge → the coast coordinate
			}
			if (best_d > -1e8) {
				// (1, 1, 1) for any water terrain without a `shore_profile` — a bit-exact no-op below (the
				// mean of a set of identical neutral profiles is exactly 1.0 in float, so every ocean renders
				// as it did before this table existed).
				vec3 profile = prof_sum / max(prof_w, SHORE_PROFILE_WEIGHT_MIN);
				float sand_scale = profile.r;  // scales the beach's inland reach (0 = a CLIFF: no beach)
				float foam_scale = profile.g;  // scales the MAIN WAVE's reaches (both ways) — never its peak
				float wisp_scale = profile.b;  // scales the offshore wisp's centre, half-width AND strength
				float dist_in = max(-best_d, 0.0);            // 0 at the coast edge, grows away from it
				// THE SIGNED COAST COORDINATE. dist_in → 0 at the shared edge on BOTH sides, so negating it
				// on the land side gives one coordinate that runs continuously through the coastline:
				//   u < 0 = inland · u = 0 exactly AT the waterline · u > 0 = seaward.
				// Every weight below is a smoothstep of u alone, so its value at u = 0 is the same whether
				// the fragment belongs to the land hex or the water hex — nothing can step at the boundary.
				float u = own_water ? dist_in : -dist_in;
				// The SAME world-noise wobble scales every reach, so the sand's inland edge, its seaward lip,
				// the surf line and the wisp all meander together (organic fingers) instead of drawing
				// concentric clean stripes. Sampled in map space, so it is the same value on both sides of
				// the edge at a given world point — the profile's continuity survives the wobble.
				float n = vnoise(v_map / noise_cell);
				float wobble = mix(SHORE_REACH_NOISE_MIN, 1.0, n);
				// …and the water's THREE profile scales then scale their own parts of the profile (1.0 = the
				// global, ocean-tuned reach), so a cliff coast, a beach coast and a lake coast can differ.
				// EACH SCALE IS GROUPED WITH THE WOBBLE ON PURPOSE (`scale * wobble`, then × the band): a
				// neutral 1.0 reduces each expression to the ORIGINAL `band * wobble` bit-for-bit, so every
				// water terrain without a shore_profile renders EXACTLY as it did before this table existed.
				// Scaling each band separately instead (`(band * scale) * wobble`) is the same value in exact
				// arithmetic but rounds a different intermediate, which moved a handful of ocean pixels by
				// 1/255 — a no-op, but not a bit-identical one.
				float sand_wobble = sand_scale * wobble;
				float foam_wobble = foam_scale * wobble;
				float wisp_wobble = wisp_scale * wobble;
				// The beach's reach, BEFORE the px floor — the floor is what the fade-out below is measured
				// against, so it has to be visible here.
				float sand_reach_raw = sand_band * sand_wobble;                              // inland, u < 0
				float sand_reach = max(sand_reach_raw, SHORE_REACH_MIN_PX);
				float wash_reach = max(foam_inland_band * foam_wobble, SHORE_REACH_MIN_PX);  // surf up the beach, u < 0
				float foam_reach = max(foam_band * foam_wobble, SHORE_REACH_MIN_PX);         // surf out to sea, u > 0

				// 0. THE WATERLINE BASE CROSS-FADE — kill the base's OWN step at u = 0, so the surf above it
				// no longer has to be an opaque cover-up (see the waterline_band uniform). A short WET EDGE:
				// the land base and the water base are cross-faded across ±wl_reach of the coastline.
				// Both estimates are the weighted mean over {own + 6 neighbours} of that wetness, weighted by
				// proximity to THAT shared edge — the shore-PROFILE field's construction, and continuous across
				// every hex edge for exactly the reason documented at SHORE_PROFILE_REACH_APOTHEMS. Because
				// they are functions of the id-map and the world UV alone (never of `result`, which carries the
				// own hex's flat-interlock/depth-field history), the land hex and the water hex compute the SAME
				// pair at a given world point — so `mixed` is frame-independent and the cross-fade cannot step
				// at the boundary. `result` (the true base, with its interlock/depth field intact) is then
				// blended toward `mixed` by an envelope that holds FULL across the wet edge and fades back to
				// the true base over SHORE_WATERLINE_FADE beyond it: at u = 0 both sides land on `mixed`
				// exactly, and past the wet edge the flat↔flat blend and the water depth field are returned
				// untouched. (Enveloping with a ramp that also peaks at u = 0 collapses the gradient to a
				// quarter of its reach and the step survives — see SHORE_WATERLINE_FADE.)
				float wl_reach = max(waterline_band * wobble, SHORE_REACH_MIN_PX);
				float wl_outer = wl_reach * (1.0 + SHORE_WATERLINE_FADE);
				if (waterline_band > 0.0 && abs(u) < wl_outer) {
					vec3 own_base = texture(biome_array, vec3(base_uv, float(own_layer))).rgb;
					vec3 land_sum = own_water ? vec3(0.0) : own_base;   // own carries weight 1 by construction
					float land_w = own_water ? 0.0 : 1.0;
					vec3 water_sum = own_water ? own_base : vec3(0.0);
					float water_w = own_water ? 1.0 : 0.0;
					for (int dir = 0; dir < 6; dir++) {
						ivec2 off = neighbor_offset(row, dir);
						int ncol = col + off.x;
						int nrow = row + off.y;
						if (nrow < 0 || nrow >= grid_h) { continue; }
						int ncol_id = ncol;
						if (wrap_h) {
							ncol_id = wrap_col(ncol);
						} else if (ncol < 0 || ncol >= grid_w) {
							continue;
						}
						vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
						vec4 nidpx = texture(id_map, nuv);
						int nlayer = int(nidpx.r * 255.0 + 0.5);
						bool nb_water = int(nidpx.g * 255.0 + 0.5) == CLASS_WATER;
						vec2 nb_c = hex_center(ncol, nrow);
						vec2 dir_v = normalize(nb_c - own_c);
						float half_dist = 0.5 * distance(own_c, nb_c);
						float d = dot(v_world - own_c, dir_v) - half_dist;
						// The SAME unwobbled weight as the profile field — a noise displacement here would
						// break the exact cross-edge agreement the whole cross-fade rests on.
						float w = smoothstep(-half_dist * SHORE_PROFILE_REACH_APOTHEMS, 0.0, d);
						vec3 nb_base = texture(biome_array, vec3(base_uv, float(nlayer))).rgb;
						if (nb_water) {
							water_sum += nb_base * w;
							water_w += w;
						} else {
							land_sum += nb_base * w;
							land_w += w;
						}
					}
					vec3 land_base = land_sum / max(land_w, SHORE_PROFILE_WEIGHT_MIN);
					vec3 water_base = water_sum / max(water_w, SHORE_PROFILE_WEIGHT_MIN);
					// THE wet edge: one smoothstep of u, 0.5 exactly AT the waterline, spread over ±wl_reach.
					vec3 mixed = mix(land_base, water_base, smoothstep(-wl_reach, wl_reach, u));
					// …held at FULL across that reach, then handing back to the true base over the outer fade.
					result = mix(result, mixed, 1.0 - smoothstep(wl_reach, wl_outer, abs(u)));
				}

				// 1. SAND — the LAND side only (the water hex gets no sand at all). FULL from the waterline
				// across the surf's inland wash (the plateau — see SHORE_SAND_PLATEAU_MAX: the surf is painted
				// over the sand, so a sand that decayed from the waterline would be whitewashed exactly where
				// it is strongest and the beach would never appear), THEN smoothstep-fading inland into the
				// biome art over the rest of its reach — a soft inner edge, and short enough that the sand
				// tints the land art rather than burying it.
				// THE BEACH FADES OUT WITH ITS OWN REACH (sand_fade). SHORE_REACH_MIN_PX floors the reach so
				// no fade divides by ~0 — but on a CLIFF coast (sand_scale → 0) that floor would keep a
				// 1px-wide, FULL-STRENGTH tan hairline alive along the waterline, and worse, the beach would
				// POP into existence at full opacity the instant sand_scale left 0 as a cliff profile blends
				// into a beach one. Fading the opacity out with the reach over the last floor's-worth makes
				// the beach GROW IN continuously from nothing. Exactly 1.0 (a bit-exact no-op) for any beach
				// wider than the floor, i.e. for every real coast.
				float sand_fade = min(sand_reach_raw / SHORE_REACH_MIN_PX, 1.0);
				float sand_plateau = min(wash_reach, sand_reach * SHORE_SAND_PLATEAU_MAX);
				// ASCENDING-EDGE FORM, and that is not cosmetic: `sand_plateau` is capped at
				// SHORE_SAND_PLATEAU_MAX (0.6) of `sand_reach`, which is itself floored at
				// SHORE_REACH_MIN_PX, so edge0 > edge1 on EVERY path — and smoothstep is UNDEFINED
				// (GLSL + SPIR-V) when edge0 >= edge1. `1 - smoothstep(plateau, reach, -u)` is
				// algebraically identical (smoothstep's descending form) and still a pure function of
				// `u` alone, so the cross-edge continuity invariant is preserved.
				float sand = ((u <= 0.0) ? 1.0 - smoothstep(sand_plateau, sand_reach, -u) : 0.0)
					* (SHORE_SAND_OPACITY * sand_fade);
				result = mix(result, beach_color, sand);

				// 2. SURF — peaks AT the waterline and washes BOTH ways: INLAND over the sand (the
				// crossfade that blends the waves into the beach, killing the sand↔foam line) and SEAWARD
				// into open water. foam_scale still shortens the wave's REACH only; its PEAK is the global
				// foam_opacity lever, which exists ONLY because the waterline cross-fade above removed the base
				// step the peak used to conceal. With waterline_band = 0 the peak is load-bearing again and
				// foam_opacity must stay ~1 — see the uniform block.
				float foam_in = smoothstep(-wash_reach, 0.0, u);   // rises across the beach → crossfade
				float foam_out = 1.0 - smoothstep(0.0, foam_reach, u); // dies out to sea
				float surf = foam_in * foam_out * foam_opacity;
				result = mix(result, foam_color, surf);

				// 3. faint second wisp: a thinner surf line further out over open water. Its centre and
				// half-thickness are their OWN radius-relative levers (wisp_center_band / wisp_half_band),
				// not multiples of the surf's reach — so the surf can be shortened without dragging the wisp
				// in with it, and the wisp can be pulled in or thinned on its own. Config is responsible for
				// keeping the band seaward of the surf (centre − half > foam reach) so the two read as two
				// lines; if they overlap they simply merge into one wider surf. Scaled by the SAME wobble as
				// every other reach, so the wisp meanders with the surf instead of drawing a perfect ring.
				// wisp_half_band = 0 turns the wisp off (the REACH_MIN_PX floor would otherwise keep a 1px
				// ring alive).
				// The wisp is the water's OWN lever: wisp_scale pulls its centre in, thins it (half-width) and
				// fades it (strength) together, so it recedes toward the shore and dies out as one gesture —
				// and wisp_scale = 0 removes the second disturbance entirely (it reads as noise on a small
				// lake) without touching that water's sand or surf. Same neutral-1.0 grouping as the reaches.
				float wisp_center = max(wisp_center_band * wisp_wobble, SHORE_REACH_MIN_PX);
				float wisp_half = max(wisp_half_band * wisp_wobble, SHORE_REACH_MIN_PX);
				// Ascending-edge form for the same reason as the sand above: `wisp_half` is floored at
				// SHORE_REACH_MIN_PX, so a descending `smoothstep(wisp_half, 0.0, …)` would have
				// edge0 >= edge1 on every path — undefined behaviour. Algebraically identical.
				float wisp = wisp_half_band * wisp_scale > 0.0
					? 1.0 - smoothstep(0.0, wisp_half, abs(u - wisp_center))
					: 0.0;
				// foam_opacity scales the wisp too, so the whole surf mutes as ONE gesture — muting the peak
				// while the offshore froth stayed bright would just move the eye-catching bright ring out to sea.
				float wn = vnoise(v_map / noise_cell + vec2(17.0, 5.0));
				result = mix(result, foam_color, wisp * (SHORE_WISP_STRENGTH * wisp_scale * foam_opacity) * wn);
			}
		}

		// --- river pass: Minor/Major rivers run along hex EDGES (not through centers) ---
		// The own hex's 12-bit riverEdges mask (2 bits per odd-r direction) says which of its 6 sides carry
		// water and of what class. For each such side we measure the distance to the SHARED EDGE SEGMENT
		// (mid ± perp * hex_radius * RIVER_HALF_SIDE — clamped to the segment, or the band would smear
		// across the whole hex along the infinite bisector), perturb it with world value-noise (so the river
		// WANDERS instead of reading as a honeycomb), and keep the edge with the MAX coverage. That
		// max-coverage (== min-distance) pick is what rounds a 120° corner join for free — no spline math.
		// Each hex paints only the half-band on ITS OWN side; both hexes flanking an edge carry it in their
		// masks, so the two halves meet symmetrically with no seam and NO cross-hex sampling. Sits AFTER the
		// shore pass (whose coastal profile a navigable hex's edges are excluded from) and BEFORE the
		// canopy/peak passes (trees overhang a river; mountains sit above it) and before FoW (so a river in a
		// Discovered tile dims with the mist rather than punching through it).
		if (rivers_enabled && rivers_lod_enabled) {
			vec4 rvpx = texture(river_map, id_uv);
			int mask = int(rvpx.r * 255.0 + 0.5) | (int(rvpx.g * 255.0 + 0.5) << 8);
			if (mask != 0) {
				// The shared organic machinery (meander warp / width variation / ragged banks) — sampled in
				// WORLD space, so the two hexes flanking an edge see identical values there and their two
				// half-bands still meet with no seam. Same three calls the navigable pass below makes.
				vec2 rp = river_meander_warp(v_world, v_map);
				float width_mod = river_width_mod(v_map);
				float width_px = river_bank_wobble(v_map);
				// Track the best coverage per CLASS (not one winner), so a class change crossfades over
				// river_class_blend px instead of switching layers hard — one river growing, not a splice.
				float cov_minor = -1e9; // coverage = half_width - distance; > 0 inside the water
				float cov_major = -1e9;
				vec2 best_tangent = vec2(1.0, 0.0);
				float best_cov = -1e9;
				for (int dir = 0; dir < 6; dir++) {
					int cls = (mask >> (2 * dir)) & 3;
					if (cls < RIVER_CLASS_MINOR || cls > RIVER_CLASS_MAX) { continue; } // 0 = none, 3 = reserved
					// Geometry only — no id_map fetch — so an edge river still draws at the map border and
					// across the wrap seam (an off-map neighbour's CENTER is still well defined).
					ivec2 off = neighbor_offset(row, dir);
					vec2 nb_c = hex_center(col + off.x, row + off.y);
					vec2 axis = nb_c - own_c;
					vec2 mid = (own_c + nb_c) * 0.5;
					vec2 perp = normalize(vec2(-axis.y, axis.x));
					vec2 seg = perp * (hex_radius * RIVER_HALF_SIDE);
					vec2 a = mid - seg;
					vec2 ab = 2.0 * seg;
					float t = clamp(dot(rp - a, ab) / dot(ab, ab), 0.0, 1.0); // clamp → SEGMENT, not bisector
					float d = distance(rp, a + ab * t);
					float base_w = (cls == RIVER_CLASS_MINOR) ? river_minor_half_width : river_major_half_width;
					float half_w = max(base_w * width_mod + width_px, RIVER_MIN_HALF_WIDTH);
					float cov = half_w - d;
					if (cls == RIVER_CLASS_MINOR) {
						cov_minor = max(cov_minor, cov);
					} else {
						cov_major = max(cov_major, cov);
					}
					if (cov > best_cov) {
						best_cov = cov;
						best_tangent = normalize(ab);
					}
				}
				if (best_cov > -river_softness) {
					float alpha = smoothstep(-river_softness, river_softness, best_cov);
					// Continuous map-space UV (exactly like the canopy), plus a downstream scroll along the
					// winning edge's tangent so the water flows.
					vec2 ruv = v_map / (2.0 * hex_radius) * river_scale
						+ best_tangent * (TIME * river_flow_speed);
					// CLASS CROSSFADE: which layer wins is a SMOOTH function of how much closer the nearest
					// Major edge is than the nearest Minor one, ramped over river_class_blend px. Where a
					// hex carries both classes (the class change) the two layers dissolve into each other
					// over that distance; a pure-Minor / pure-Major hex is unaffected (the loser stays at
					// -1e9). Hexes flanking the same edge see the same coverages → still no seam.
					float class_mix = smoothstep(-river_class_blend, river_class_blend, cov_major - cov_minor);
					vec3 water = mix(texture(river_tex, vec3(ruv, 0.0)).rgb,
						texture(river_tex, vec3(ruv, 1.0)).rgb, class_mix);
					water = river_harmonize(water);
					if (river_highlight) {
						water = mix(water, RIVER_HIGHLIGHT_COLOR, RIVER_HIGHLIGHT_MIX);
					}
					result = mix(result, water, alpha);
				}
			}
		}

		// --- navigable-river pass: a BANK hex with a wide water CHANNEL running through it ---
		// A NavigableRiver is a water TERRAIN in the sim (blocking, boats), but rendering it as one made it a
		// hex-shaped puddle with a beach and surf — indistinguishable from a lake, and nothing like a river. So
		// its BASE texture is the silty bank ground and its blend_class is "flat": the bank takes the ordinary
		// land interlock into its neighbours (and, with blend_rugged_land, into rugged ones too), takes NO
		// shore pass (see the `own_navigable || nb_navigable` skip above), and the water is painted ON TOP
		// here, as a channel far narrower than the hex.
		//
		// Two kinds of stroke, unioned by the same MAX-COVERAGE (== min-distance) pick the Minor/Major pass
		// uses — which is what fuses them into one connected body with rounded joins, for free:
		//
		// 1. TRUNK ARMS, at the channel's own (navigable) width: hex CENTRE → the MIDPOINT of each side the
		//    river actually flows out through — and the SIM says which those are, in the per-tile
		//    riverChannel mask (river_channel_map, 1 bit per odd-r direction). It is NOT inferred from the
		//    neighbouring terrain. Arming every navigable/water/delta neighbour, as this pass used to, is
		//    what drew the WEB: a river is a PATH — each hex links only to its upstream and downstream
		//    neighbour — so wherever two chain hexes merely sat side by side (parallel reaches, a chain
		//    bending back on itself), the terrain rule cross-linked them and the corridor filled with a mesh
		//    of triangles. Only the tracer knows the path, so only the tracer can name the exits.
		//    The mask is symmetric across a shared side EXCEPT at the MOUTH, where the final hex also exits
		//    into the sea / inland sea / RiverDelta it drains into (open water carries no channel of its
		//    own, so that bit is not mirrored back) — which is what keeps the river from dead-ending a hex
		//    short of the sea. So read only the OWN hex's bits; never assume the neighbour agrees.
		//    NOTE it is also NOT armed by this hex's own river_edges bits. An edge river runs ALONG a side;
		//    it does not flow through the side's midpoint, and a trunk hex can flank two or three of them
		//    (the tributary ran along several of its sides on the way in) — arming on that mask drew three
		//    fat centre→midpoint arms at the TRUNK's width and the hex filled with water.
		// 2. INFLOW SPURS, at the TRIBUTARY's own Minor/Major width: hex CENTRE → the CORNER the riverInflow
		//    mask names. An edge river ends at a VERTEX, so that vertex is where the water enters the trunk,
		//    and it enters as itself — a Minor stays a hairline until the channel swallows it. The mask
		//    permits several (two tributaries can terminate on different corners of the same trunk head), so
		//    all 6 corners are checked. It needs no neighbour fetch, so it spurs at the border/wrap seam too.
		// Runs AFTER the Minor/Major pass so a Major feeding a navigable trunk composites into it, and
		// before canopy/peaks/FoW like the rest of the water.
		if (river_navigable_enabled && rivers_lod_enabled && own_navigable) {
			vec4 rvpx = texture(river_map, id_uv);
			int inflow = int(rvpx.b * 255.0 + 0.5) | (int(rvpx.a * 255.0 + 0.5) << 8);
			// The sim-authored channel-exit mask: 1 bit per odd-r direction, and the ONLY thing that arms a
			// trunk arm (see the pass header — the terrain-inferred rule is what wove the web).
			int channel = int(texture(river_channel_map, id_uv).r * 255.0 + 0.5);
			// The SAME organic machinery as the edge pass — meander warp, low-frequency swell along the
			// reach, ragged banks — all sampled in WORLD space, which is what keeps the channel CONTINUOUS
			// across adjacent navigable hexes: both hexes warp the same point and read the same width at
			// their shared boundary, so the two half-channels line up exactly. No seam, no pinch, no gap.
			// The spurs ride the same three, so a tributary's band arrives at the vertex already warped
			// exactly as the edge pass warped it on the far side — the two meet without a notch.
			vec2 rp = river_meander_warp(v_world, v_map);
			float width_mod = river_width_mod(v_map);
			float width_px = river_bank_wobble(v_map);
			float half_w = max(river_navigable_half_width * width_mod + width_px, RIVER_MIN_HALF_WIDTH);
			// How many sides the channel flows out through — the chain's TOPOLOGY at this hex, read off the
			// very mask the trunk-arm loop below arms itself from (no new uniform, no new texture: the data is
			// already on the GPU). A chain HEAD has only its downstream link (1 exit); a MID-CHAIN hex has its
			// upstream one too (2); a confluence has 3. It is also the arm count the degenerate-blob fallback
			// needs, so it is counted ONCE here rather than tallied inside the arm loop.
			int exits = 0;
			for (int dir = 0; dir < 6; dir++) {
				exits += (channel >> dir) & 1;
			}
			// HEAD TAPER — the width the trunk's arms START at, at the hex CENTRE. On the FIRST hex of a
			// chain (the only hex with <= RIVER_CHANNEL_HEAD_MAX_EXITS channel exits) the trunk used to spring
			// to full navigable width the instant it left the centre: a hairline Minor arrived at a vertex and
			// a few px later was a great river. A jump-cut, not a river. So on that hex the arms start at the
			// half-width of the WIDEST tributary feeding in (max over the 6 inflow corners — two tributaries
			// can land on the same head, and the sim already stores the widest class per corner) and SWELL to
			// the channel's full width by the time they reach the hex EDGE (see the per-arm mix below).
			// THE GATE IS THE CHANNEL EXIT COUNT, *NOT* `inflow != 0`. Since the drainage network landed,
			// riverInflow means "a tributary hands over at this vertex" — true of ANY navigable hex, not just
			// the chain's first (core_sim/CLAUDE.md → Rivers; docs/plan_rivers_drainage_network.md §A). Keying
			// the taper off it would pinch a full-width trunk down to the tributary's width at a mid-chain
			// junction's CENTRE and swell it back out on both sides: a visible HOURGLASS in mid-channel.
			// A hex with >= 2 exits is MID-CHAIN and keeps the CONSTANT full width (head_base stays
			// river_navigable_half_width and the mix is a no-op); only a genuine source hex tapers.
			// The SPUR below stays UNCONDITIONAL — it carries the tributary from the hex centre out to its
			// vertex, and a mid-chain junction needs it MORE, not less: without it the tributary's band would
			// dead-end at the vertex (distance r from the centre), short of the trunk's arms, which only reach
			// the edge MIDPOINTS.
			// min() with the channel width guards a config where a class is somehow wider than the trunk —
			// a head must never START wider than it ends.
			float inflow_base = 0.0;
			for (int c = 0; c < 6; c++) {
				int icls = (inflow >> (2 * c)) & 3;
				if (icls < RIVER_CLASS_MINOR || icls > RIVER_CLASS_MAX) { continue; } // 0 = none, 3 = reserved
				inflow_base = max(inflow_base,
					(icls == RIVER_CLASS_MINOR) ? river_minor_half_width : river_major_half_width);
			}
			bool is_chain_head = exits <= RIVER_CHANNEL_HEAD_MAX_EXITS;
			float head_base = (is_chain_head && inflow_base > 0.0)
				? min(inflow_base, river_navigable_half_width)
				: river_navigable_half_width;
			float cov_trunk = -1e9;         // coverage = half_width - distance; > 0 inside the water
			float cov_spur = -1e9;          // best INFLOW spur, tracked apart so the two can crossfade
			float spur_layer = 0.0;         // river-array layer of the winning spur (class - 1)
			vec2 best_tangent = vec2(1.0, 0.0);
			for (int dir = 0; dir < 6; dir++) {
				// The exit bit IS the arm rule — no neighbour fetch, and none possible: the mask is
				// asymmetric at the mouth, so the neighbour's opinion is not a substitute for the own hex's.
				// Needing no neighbour texel, an arm also draws correctly at the map border / wrap seam (the
				// geometry only wants the neighbour's CENTRE, and hex_center is pure math on the raw,
				// unwrapped column — so the arm points the right way even across the seam).
				if (((channel >> dir) & 1) == 0) { continue; }
				ivec2 off = neighbor_offset(row, dir);
				vec2 nb_c = hex_center(col + off.x, row + off.y);
				// The arm SEGMENT: own centre → the shared edge's midpoint. Clamped to the segment (like the
				// edge pass clamps to the edge), so the channel stops at the hex boundary instead of
				// smearing along an infinite ray — the neighbour paints its own half.
				vec2 ab = (own_c + nb_c) * 0.5 - own_c;
				float t = clamp(dot(rp - own_c, ab) / dot(ab, ab), 0.0, 1.0);
				// The taper's progress along the arm, 0 at the centre → 1 at the edge midpoint. Taken from
				// the UNWARPED point, unlike `t` above (which measures distance to the meandering
				// centerline and so must use the warped one), and for one load-bearing reason: EVERY
				// fragment on the shared edge projects to EXACTLY 1.0 on the arm's axis (the projection of
				// that whole edge line onto the arm direction is the apothem, whatever the lateral offset),
				// so the taper lands on exactly river_navigable_half_width at the boundary — which is the
				// constant width the downstream hex draws its own half-channel at. The warped point's
				// projection would wander by the meander amplitude and leave a visible STEP at the head
				// hex's downstream edge. Width is a scalar field of world position here, same as
				// river_width_mod / river_bank_wobble (both also sampled unwarped) — so this is the house
				// idiom, not an exception to it.
				float t_head = clamp(dot(v_world - own_c, ab) / dot(ab, ab), 0.0, 1.0);
				float swell = pow(smoothstep(0.0, 1.0, t_head), river_head_taper_curve); // 1.0 at t_head = 1
				float arm_base = mix(head_base, river_navigable_half_width, swell);
				// The organic machinery rides ON TOP of the tapered base width (world-sampled, so the
				// downstream hex reads the same width_mod/width_px at the shared boundary → still no seam).
				float arm_w = max(arm_base * width_mod + width_px, RIVER_MIN_HALF_WIDTH);
				float cov = arm_w - distance(rp, own_c + ab * t);
				if (cov > cov_trunk) {
					cov_trunk = cov;
					best_tangent = normalize(ab);
				}
			}
			// INFLOW SPURS — centre → the named CORNER, at the arriving tributary's width (the same
			// river_minor/major_half_width the edge pass uses, so it keeps its identity as it enters).
			for (int c = 0; c < 6; c++) {
				int cls = (inflow >> (2 * c)) & 3;
				if (cls < RIVER_CLASS_MINOR || cls > RIVER_CLASS_MAX) { continue; } // 0 = none, 3 = reserved
				float ang = RIVER_CORNER_ANGLE_STEP * float(c) + RIVER_CORNER_ANGLE_OFFSET;
				vec2 ab = vec2(cos(ang), sin(ang)) * hex_radius;  // centre → corner c
				float t = clamp(dot(rp - own_c, ab) / dot(ab, ab), 0.0, 1.0);
				float base_w = (cls == RIVER_CLASS_MINOR) ? river_minor_half_width : river_major_half_width;
				float spur_w = max(base_w * width_mod + width_px, RIVER_MIN_HALF_WIDTH);
				float cov = spur_w - distance(rp, own_c + ab * t);
				if (cov > cov_spur) {
					cov_spur = cov;
					spur_layer = float(cls - RIVER_CLASS_MINOR);   // layer = class - 1 (Minor 0, Major 1)
					if (cov > cov_trunk) { best_tangent = normalize(ab); }
				}
			}
			float best_cov = max(cov_trunk, cov_spur);
			if (exits == 0) {
				// Degenerate — a navigable hex the river never leaves (a tributary may still feed it: an
				// inflow spur is not an arm). The sim should never emit one (MapView push_warnings if it
				// does); draw a blob at the centre rather than a hex of bare bank with no water in it.
				best_cov = max(best_cov, half_w * RIVER_NAVIGABLE_ORPHAN_BLOB - distance(rp, own_c));
			}
			if (best_cov > -river_softness) {
				float alpha = smoothstep(-river_softness, river_softness, best_cov);
				// Same continuous map-space UV + downstream TIME scroll as the edge classes, so a Major and
				// the trunk it feeds flow as one body of water.
				vec2 ruv = v_map / (2.0 * hex_radius) * river_scale
					+ best_tangent * (TIME * river_flow_speed);
				// The spur wears the TRIBUTARY's class art and dissolves into the channel over
				// river_class_blend px — the same crossfade the edge pass runs at a Minor→Major change, for
				// the same reason: the join must read as one river growing, not two waterways spliced. With
				// no spur, cov_spur stays at -1e9 → class_mix == 1 → pure channel, exactly as before.
				float class_mix = smoothstep(-river_class_blend, river_class_blend, cov_trunk - cov_spur);
				vec3 water = mix(texture(river_tex, vec3(ruv, spur_layer)).rgb,
					texture(river_tex, vec3(ruv, RIVER_NAVIGABLE_LAYER)).rgb, class_mix);
				// Harmonized like the edge classes (shared hue / depth pivot, luma order preserved), so the
				// trunk reads as the SAME river grown bigger rather than a third, unrelated waterway.
				water = river_harmonize(water);
				if (river_highlight) {
					water = mix(water, RIVER_HIGHLIGHT_COLOR, RIVER_HIGHLIGHT_MIX);
				}
				result = mix(result, water, alpha);
			}
		}

		// --- canopy overlay: whole tree crowns over canopy biomes, overhanging + thinning at the edge ---
		// Composites the RGBA canopy texture over the (grass) forest floor. Density D uses the same
		// signed-distance-to-shared-edge machinery vs the canopy↔non-canopy boundary, so crowns OVERHANG
		// onto the neighbour (up to canopy_overhang) and THIN OUT — never a razor cut at the hex line.
		if (canopy_enabled && canopy_lod_enabled) {
			int own_canopy = int(idpx.b * 255.0 + 0.5);   // 0 = none, else canopy layer + 1
			bool own_is_canopy = own_canopy > 0;
			float best_d = -1e9;                            // nearest boundary edge = max signed distance
			int nb_canopy_code = 0;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					continue;
				}
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				int ncode = int(texture(id_map, nuv).b * 255.0 + 0.5);
				if (own_is_canopy == (ncode > 0)) { continue; } // only canopy↔non-canopy edges are boundaries
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				float d = dot(v_world - own_c, dir_v) - half_dist; // <=0 inside own hex, larger = nearer edge
				if (d > best_d) {
					best_d = d;
					nb_canopy_code = ncode;
				}
			}
			int forest_code = own_is_canopy ? own_canopy : nb_canopy_code;
			// signed distance into the forest (+inside, 0 at treeline, -outside); interior forest → +big.
			float s;
			if (own_is_canopy) {
				s = (best_d > -1e8) ? -best_d : 1e9;  // no non-canopy neighbour → deep interior
			} else {
				s = (best_d > -1e8) ? best_d : -1e9;  // no canopy neighbour → no overhang
			}
			// widen the guard by the noise amplitude so the bumpy fringe isn't clipped
			if (forest_code > 0 && s > -canopy_overhang * (1.0 + CANOPY_TREELINE_NOISE)) {
				float tn = vnoise(v_map / noise_cell + vec2(3.0, 11.0));
				s += (tn - 0.5) * 2.0 * canopy_overhang * CANOPY_TREELINE_NOISE; // bumpy treeline
				float dens = canopy_density(s, canopy_overhang, canopy_softness);
				if (dens > 0.0) {
					vec2 cuv = v_map / (2.0 * hex_radius) * canopy_scale; // map-space, continuous + pan/zoom-anchored
					vec4 crown = texture(canopy_tex, vec3(cuv, float(forest_code - 1)));
					result = mix(result, crown.rgb, crown.a * dens);
				}
			}
		}

		// --- peak overlay: faceted mountain relief over highland/volcanic biomes, overhanging + casting a
		// shadow onto the neighbour (the mountain-drama analog of the canopy pass). Same signed-distance-to-
		// shared-edge machinery vs the peak↔non-peak boundary gives s (+inside relief) and peak_code (own,
		// else the nearest peak-neighbour's, so the overhang/shadow region reads the right layer). Elevation
		// (elev_map) drives per-hex prominence + shadow length. Composited after canopy, before FoW.
		if (peaks_enabled && peaks_lod_enabled) {
			int own_peak = int(idpx.a * 255.0 + 0.5);      // 0 = none, else peak layer + 1
			bool own_is_peak = own_peak > 0;
			float pk_best_d = -1e9;                         // nearest boundary edge = max signed distance
			int nb_peak_code = 0;
			float nb_peak_elev = 0.0;                       // elevation of that nearest peak-neighbour hex
			float own_elev = texture(elev_map, id_uv).r;    // 0..1; the peak's own height when own_is_peak
			// peak↔PEAK cross-blend accumulator (see the PEAK_BLEND_* consts): the neighbouring relief layers
			// contribute AT ONCE, each weighted by its own edge's coverage, and the result is their weighted
			// MEAN — never a pick, and never a nearest-edge switch that could seam along a bisector where two
			// different reliefs meet the same hex (the discipline the water depth field uses).
			vec4 pk_mix = vec4(0.0);                        // Σ wᵢ · relief_rgbaᵢ
			float pk_w_sum = 0.0;                           // Σ wᵢ
			float pk_elev_mix = 0.0;                        // Σ wᵢ · elevᵢ — so prominence follows the art shown
			// The footline wobble, shared by the shadow ENVELOPE and the peak composite below so the shadow's
			// onset meanders with the art's footline instead of tracing the hex edge. Map-space noise, so a
			// world point reads the same value from both sides of an edge — continuity survives it.
			float tn = vnoise(v_map / noise_cell + vec2(7.0, 5.0));
			float pk_wobble = (tn - 0.5) * 2.0 * peak_overhang * CANOPY_TREELINE_NOISE;
			// Cast-shadow envelope: 1 at the footline, 0 beyond the (elevation-scaled, DIRECTIONAL) reach.
			// See the PEAK_SHADOW_* consts — this is what keeps the shadow off the hex grid.
			float shadow_env = 0.0;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					continue;
				}
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				int ncode = int(texture(id_map, nuv).a * 255.0 + 0.5);
				bool nb_is_peak = ncode > 0;
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				float d = dot(v_world - own_c, dir_v) - half_dist; // <=0 inside own hex, larger = nearer edge
				float nb_elev = texture(elev_map, nuv).r;      // same uv we read its id/peak-code from
				if (own_is_peak && nb_is_peak) {
					// --- RELIEF ↔ RELIEF: cross-fade the two layers, taller one overhanging (PEAK_BLEND_*) ---
					if (ncode == own_peak) { continue; }      // same relief layer: the art is already continuous
					float delta = nb_elev - own_elev;         // > 0 ⇒ the neighbour towers over us
					float asym = 2.0 * smoothstep(
						-PEAK_BLEND_FULL_DELTA, PEAK_BLEND_FULL_DELTA, delta) - 1.0;   // odd, −1…+1
					// The wobble must read the SAME way in world space from both hexes, i.e. with OPPOSITE signs
					// in their own local frames. The peak layer index is a total order both sides agree on.
					float wob_sign = own_peak < ncode ? 1.0 : -1.0;
					// The 50/50 line, as a depth INTO our hex (negative = we have pushed it into theirs). The
					// centre is offset by (overhang − softness) rather than the full overhang so that the far
					// edge of the cross-fade — centre + softness — lands at exactly `peak_overhang`: a taller
					// relief reaches into a lower one by the SAME distance it reaches onto flat ground, and no
					// further. (Offsetting by the full overhang stacked the feather on top of it and pushed the
					// alpine art a whole hex radius into the hills, swallowing them.)
					float seam = max(peak_overhang - peak_softness, 0.0) * asym
						+ wob_sign * pk_wobble * PEAK_BLEND_WOBBLE;
					float depth = -d;                         // distance from this edge INTO our hex (0 at it)
					float w = 1.0 - smoothstep(seam - peak_softness, seam + peak_softness, depth);
					if (w > 0.0) {
						pk_mix += w * premultiplied(
							textureGrad(peak_tex, vec3(puv, float(ncode - 1)), puv_dx, puv_dy));
						pk_w_sum += w;
						pk_elev_mix += w * nb_elev;
					}
					continue;                                 // a relief↔relief edge is not a footline
				}
				if (own_is_peak == nb_is_peak) { continue; }  // ground↔ground: no boundary, nothing to do
				if (d > pk_best_d) {
					pk_best_d = d;
					nb_peak_code = ncode;
					nb_peak_elev = nb_elev;
				}
				// --- this edge's contribution to the cast-shadow envelope (see the PEAK_SHADOW_* consts) ---
				// Recast the edge in RELIEF terms, identically from either side of it: q = signed distance INTO
				// the relief (so the shared edge reads q = 0 from both hexes), m = the unit normal pointing INTO
				// the relief (dir_v points at the peak neighbour when we are the ground; when we ARE the peak,
				// dir_v points at the ground neighbour, so the relief is the other way).
				float q = own_is_peak ? -d : d;
				vec2 m = own_is_peak ? -dir_v : dir_v;
				float pk_elev = own_is_peak ? own_elev : nb_elev;
				// +1 when the relief lies straight TOWARD the light (we are down-light of it → deepest shadow),
				// -1 when it lies away from it (we are on its LIT side → only the short contact skirt).
				float align = dot(peak_light_dir, m);
				float lit_side = align * 0.5 + 0.5;           // remap the dot from [-1,1] to [0,1]
				float reach = peak_shadow_len
					* mix(PEAK_SHADOW_ELEV_FLOOR, 1.0, pk_elev)
					* mix(PEAK_SHADOW_UPLIGHT_REACH, 1.0, lit_side);
				// Distance BEYOND the wobbled footline (0 anywhere under the relief, growing outward from it).
				float out_dist = max(0.0, -(q + pk_wobble));
				float env = 1.0 - smoothstep(0.0, max(reach, PEAK_SHADOW_REACH_MIN), out_dist);
				shadow_env = max(shadow_env, env);            // MAX, never a sum → no double-darkening seam
			}
			// A hex deep inside a massif has no peak↔non-peak edge at all, so the loop above never ran: its
			// ground is under the relief and fully occluded.
			if (own_is_peak && pk_best_d < -1e8) { shadow_env = 1.0; }
			int peak_code = own_is_peak ? own_peak : nb_peak_code;
			// signed distance into the relief (+inside, 0 at footline, -outside); interior massif → +big.
			float s;
			if (own_is_peak) {
				s = (pk_best_d > -1e8) ? -pk_best_d : 1e9;   // no non-peak neighbour → deep interior
			} else {
				s = (pk_best_d > -1e8) ? pk_best_d : -1e9;   // no peak neighbour → no overhang
			}
			if (peak_code > 0) {
				// Use the PEAK's elevation for prominence + shadow: own hex when it's the peak, else the
				// captured peak-neighbour's — so the overhang/shadow on the ground side of the seam reads the
				// peak's height, not the low neighbour-ground hex's (which discontinued across the seam).
				float peak_elev = own_is_peak ? own_elev : nb_peak_elev;  // 0..1

				// --- CAST SHADOW: look back toward the light; if a peak occludes, darken this ground ---
				// peak_light_dir points TOWARD the light (top-left). A fragment is shadowed if a peak sits
				// between it and the light, i.e. at puv + light_dir * t. Multi-tap for a soft ramp.
				// The peak art is near-opaque wherever it is sampled, so this term alone is ~CONSTANT across a
				// whole neighbour hex — it is shadow_env (the signed-distance envelope built above) that makes
				// the result die off with distance from the RELIEF rather than with the hex grid.
				float shadow = 0.0;
				if (shadow_env > 0.0) {
					float slen_uv = peak_shadow_len * mix(PEAK_SHADOW_ELEV_FLOOR, 1.0, peak_elev)
						/ (2.0 * hex_radius) * peak_scale;
					for (int k = 1; k <= PEAK_SHADOW_TAPS; k++) {
						float t = float(k) / float(PEAK_SHADOW_TAPS);
						vec2 sp = puv + peak_light_dir * (slen_uv * t);
						float a = textureGrad(peak_tex, vec3(sp, float(peak_code - 1)), puv_dx, puv_dy).a;
						shadow = max(shadow, a * (1.0 - t));   // nearer occluder = darker
					}
				}
				result *= mix(1.0, 1.0 - peak_shadow_strength, clamp(shadow * shadow_env, 0.0, 1.0));

				// --- PEAK COMPOSITE (over the shadowed ground) ---
				float ss = s + pk_wobble;   // bumpy footline — the same wobble the shadow envelope rides
				if (ss > -peak_overhang) {
					// WHICH relief art this fragment shows. Normally our own layer (or, on the ground side of a
					// footline, the overhanging neighbour's). Where a relief↔relief seam is in reach, it is the
					// WEIGHTED MEAN of ours and the neighbouring reliefs' (see the PEAK_BLEND_* consts): our own
					// weight is what the neighbours have not claimed, so at a seam the taller relief's art takes
					// over CONTINUOUSLY instead of switching on the hex line. The denominator is max(1, Σw) — a
					// continuous function of Σw, so a fragment in reach of two seams cannot pop either.
					vec4 pk = textureGrad(peak_tex, vec3(puv, float(peak_code - 1)), puv_dx, puv_dy);
					float art_elev = peak_elev;
					if (pk_w_sum > 0.0) {
						float own_w = max(0.0, 1.0 - pk_w_sum);
						float denom = own_w + pk_w_sum;
						// PREMULTIPLIED — relief art is RGBA with large transparent regions, and a straight-alpha
						// mean lets a transparent texel's (keyed-out) RGB pollute the colour wherever the other
						// layer is opaque: it drew thin bright dotted fringes ALONG the seam, i.e. re-introduced a
						// hex-tracing line by another route. Weighting colour by coverage×alpha is the only mean
						// that is right for two overlays with independent alpha.
						pk = unpremultiplied((own_w * premultiplied(pk) + pk_mix) / denom);
						art_elev = (own_w * peak_elev + pk_elev_mix) / denom;
					}
					// Prominence follows the art that is actually SHOWING, so a tall neighbour's spires overhang
					// us at THEIR prominence rather than being faded down to ours.
					float dens = canopy_density(ss, peak_overhang, peak_softness)
						* mix(peak_min_prominence, 1.0, art_elev);   // reuse canopy_density
					if (dens > 0.0) {
						result = mix(result, pk.rgb, pk.a * dens);
					}
				}
			}
		}

		if (fow_enabled) {
			// --- FoW: SMOOTH the per-hex visibility across hex boundaries, then map it CONTINUOUSLY ---
			// The vis-map is NEAREST-sampled per hex, so reading it raw made every active↔discovered
			// adjacency a hard HEXAGONAL brightness step — straight edges cutting across even uniform water
			// where no terrain seam exists. Fix, in two halves:
			// (a) blur the scalar with the same 6-neighbour signed-distance machinery the blend/shore use:
			//     weight each neighbour's visibility by how close the fragment is to THAT shared edge. At a
			//     shared edge the neighbour's weight → 1, so vis → (own + nb)/2 from BOTH sides — equal, so
			//     continuous across the boundary. Deep inside a hex all six weights → 0 and vis → own.
			float own_vis = texture(vis_map, id_uv).r;
			float vsum = own_vis;
			float wsum = 1.0;
			for (int dir = 0; dir < 6; dir++) {
				ivec2 off = neighbor_offset(row, dir);
				int ncol = col + off.x;
				int nrow = row + off.y;
				if (nrow < 0 || nrow >= grid_h) { continue; }
				int ncol_id = ncol;
				if (wrap_h) {
					ncol_id = wrap_col(ncol);
				} else if (ncol < 0 || ncol >= grid_w) {
					continue;
				}
				vec2 nuv = (vec2(float(ncol_id), float(nrow)) + 0.5) / grid_dim;
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				float d = dot(v_world - own_c, dir_v) - half_dist;  // <= 0 inside own hex, 0 at the edge
				float w = smoothstep(-max(fow_soft, BLEND_SOFT_MIN), 0.0, d);
				vsum += texture(vis_map, nuv).r * w;
				wsum += w;
			}
			float vis = vsum / wsum;
			// Wispiness: perturb the smoothed scalar with world noise so the fog line is cloudy rather than a
			// clean arc — enveloped by how much the smoothing moved the scalar (see FOW_NOISE_EDGE_PEAK), so
			// it bites ONLY at boundaries and pure Active/Discovered/Unexplored areas stay untouched.
			if (fow_noise_amount > 0.0) {
				float edge = clamp(abs(vis - own_vis) / FOW_NOISE_EDGE_PEAK, 0.0, 1.0);
				float fn = vnoise(v_map / noise_cell + vec2(23.0, 13.0));
				vis = clamp(vis + (fn - 0.5) * fow_noise_amount * edge, 0.0, 1.0);
			}
			// (b) map vis to the tint CONTINUOUSLY (the old per-state branch was itself a step function).
			// At the pure states this reproduces exactly today's look: vis 1 → clear, 0.5 → full mist
			// multiply, 0 → fog fill; in between it crossfades.
			float fog_amt = 1.0 - smoothstep(FOW_UNEXPLORED, FOW_DISCOVERED, vis);
			float mist_amt = (1.0 - smoothstep(FOW_DISCOVERED, FOW_ACTIVE, vis)) * mist_blend;
			result = mix(result, result * mist_color, mist_amt);  // discovered: remembered/cloudy (multiply)
			result = mix(result, fog_color.rgb, fog_amt);         // unexplored: fog fill
		}
		COLOR = vec4(result, 1.0);
	}
}
