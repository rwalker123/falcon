shader_type canvas_item;
render_mode blend_mix;

// Approach B — per-pixel biome blend for the 2D hex map (see clients/.../CLAUDE.md → Edge Blending).
// One whole-map quad runs this fragment shader. Each fragment inverts the pointy-top odd-r hex layout
// (MUST match MapView._hex_center / _axial_center exactly) to find its hex, samples that hex's biome
// from the Texture2DArray, and — only when the hex AND a neighbour across a shared edge share a BLENDABLE
// blend_class (SAME class, different biome: flat↔flat or water↔water) — MIXES the neighbour's texture in
// near the seam: a symmetric signed-distance splat weight (a tie at the exact edge), wobbled by WORLD-space
// value noise (varies per hex, no baked tiling), optionally nudged by the two textures' luminance detail,
// and feathered through a smoothstep into a continuous mix factor — never a 1-bit pick (see the const
// block). rugged edges never blend, and neither does any CLASS CHANGE — land↔water stays hard because that
// seam is the SHORELINE, which gets its own wash instead (sand shallows then surf, BOTH on the water side;
// the land keeps its texture) — see the shoreline block. See the id-map (splatmap) below.

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
// Per-base-layer MEAN LUMINANCE, a 1×N single-channel texture fetched by layer index (N = terrain count,
// so the layer count is never baked into this shader as a fixed array size). Height blending uses each
// texture's per-pixel luminance as a pseudo-height (we have no height maps); subtracting the layer's mean
// ZERO-CENTRES it, without which a bright biome would always out-height a dark one and the seam would
// collapse onto one side instead of interlocking. Built by TerrainTextureManager.
uniform sampler2D layer_luma_map : filter_nearest;
// PER-TERRAIN SHORE PROFILE, a 1×N RG texture fetched by layer index (same table shape as layer_luma_map):
// R = reach_scale (multiplies ALL FOUR shore reaches), G = wisp_scale (multiplies the offshore wisp's
// half-width AND its strength; 0 = no wisp). The shipped global profile is ocean-tuned; a small inland_sea
// needs a lighter coast. Terrains with no `shore_profile` block read the neutral (1, 1), which is a
// bit-exact no-op — oceans render exactly as before. Built by TerrainTextureManager.
uniform sampler2D layer_shore_map : filter_nearest;
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
// RUGGED-LAND eligibility (config blend_rugged_land, default FALSE = the shipped same-class rule). When TRUE
// the LAND seam gate widens from same-class to "both sides are land": a rugged biome's BASE FLOOR then blends
// with any land neighbour (flat↔rugged and rugged↔rugged), instead of ending in a razor-straight hexagon.
// land↔water is untouched (still hard — that seam is the shoreline) and water↔water still takes the depth
// field. OFF by default because rugged base textures are high-contrast/structured and the height term can
// shred them — see EDGE_BLEND_DEFAULT_RUGGED_LAND in MapView.gd.
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
// THE SHAPE: the SAND lives ONLY in the LAND hex (full at the waterline, smoothstep-fading inland into
// the biome art — it must never bury it, hence the short reach). The SURF peaks AT the waterline and
// washes BOTH ways: a short distance INLAND over the sand (foam_inland_band — the crossfade that blends
// the waves into the beach) and a longer way SEAWARD into open water (foam_band). The foam peak is also
// what conceals the base's own step at u = 0 (sand-tinted land on one side, open water on the other), so
// its weight must reach ~1 there — never scale the peak down.
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
uniform vec3 foam_color;      // surf line color (light desaturated cyan / near-white)
uniform vec3 beach_color;     // sand color — the beach, on the LAND side of the waterline only

const float SQRT3 = 1.7320508075688772;
// blend_class codes, as packed into the id-map's G channel by MapView._blend_class_code. The SAME-CLASS
// eligibility rule below is expressed in these, never in bare literals.
const int CLASS_WATER = 0;
const int CLASS_FLAT = 1;
const int CLASS_RUGGED = 2;  // never blends (discrete-object textures must not bleed) — see the gate
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
const float CANOPY_TREELINE_NOISE = 0.6;  // how hard world-noise waves the treeline in/out (× overhang)
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

// Shore profile of a terrain: (reach_scale, wisp_scale), fetched by layer index. See layer_shore_map.
vec2 shore_profile(int layer) {
	return texelFetch(layer_shore_map, ivec2(layer, 0), 0).rg;
}

// Canopy density across the treeline. s = signed distance to the canopy↔non-canopy boundary,
// POSITIVE inside the forest, 0 at the edge, negative in the neighbour. Deep inside → 1; ~0.5 at the
// exact edge; ramps to 0 canopy_overhang px OUTSIDE the forest (crowns overhang, then fade).
float canopy_density(float s, float overhang, float softness) {
	if (s >= 0.0) { return 0.5 + 0.5 * smoothstep(0.0, softness, s); }
	return 0.5 * smoothstep(-overhang, 0.0, s);
}

// odd-r neighbour offsets (MUST match MapView._get_neighbor_offset_{x,y}_2d).
ivec2 neighbor_offset(int row, int dir) {
	int odd = row & 1;
	if (dir == 0) { return ivec2(1, 0); }                 // E
	if (dir == 1) { return ivec2(odd == 1 ? 1 : 0, -1); } // NE
	if (dir == 2) { return ivec2(odd == 1 ? 0 : -1, -1); }// NW
	if (dir == 3) { return ivec2(-1, 0); }                // W
	if (dir == 4) { return ivec2(odd == 1 ? 0 : -1, 1); } // SW
	return ivec2(odd == 1 ? 1 : 0, 1);                    // SE
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

		// SAME-CLASS eligibility: a hex blends only with a neighbour of its OWN blend_class and a different
		// terrain id. flat↔flat (grass↔soil ecotones) and water↔water (deep ocean grading into shelf) both
		// qualify; land↔water is a CLASS CHANGE, so it stays hard — that seam is the SHORELINE, owned by the
		// foam/beach pass below, and softening it would wash the coastline out. rugged (class 2) never blends.
		// blend_rugged_land widens the LAND half of that rule to "both sides are land" (see the uniform).
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
			float seam_band = blend_band;
			float seam_soft = blend_soft;
			float seam_noise_amount = blend_noise_amount;
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
				// Eligibility: same class by default; with blend_rugged_land, ANY land neighbour (own is land
				// here, so this is flat↔rugged / rugged↔rugged). Water is excluded either way — that seam is
				// the shoreline. Symmetric on both sides of the edge, so the seam stays continuous.
				bool nb_eligible = blend_rugged_land && own_land
					? nclass != CLASS_WATER
					: nclass == own_class;
				if (!nb_eligible || nlayer == own_layer) { continue; }
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				// signed distance to the shared edge (the perpendicular bisector); <=0 inside own hex.
				float d = dot(v_world - own_c, dir_v) - half_dist;
				float p = clamp(0.5 + d / (2.0 * seam_band), 0.0, 1.0); // 0.5 AT the edge → symmetric
				if (p > best_p) {
					best_p = p;
					best_layer = nlayer;
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
				float n = vnoise(v_map / blend_noise_cell);
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

		// --- shoreline: ONE continuous coastal profile straddling every land↔water edge ---
		// Universal: fires for ANY edge where exactly one side is water. Inland flat↔flat, water↔water and
		// rugged↔* edges are untouched (both sides same wetness → skipped). LOD-gated like the blend.
		// Seaward read: land → sand → surf → open water, with NO hard step anywhere: every weight below is a
		// smoothstep of the SIGNED coast coordinate u, one function evaluated on both sides of the shared
		// edge, so nothing can pop at the hex boundary (see the uniform block for the two bugs this fixes).
		if (blend_enabled) {
			bool own_water = own_class == CLASS_WATER;
			float best_d = -1e9; // nearest qualifying edge = max signed distance (closest to the seam)
			// THE PROFILE IS KEYED ON THE WATER SIDE — a correctness requirement, not a preference. The
			// profile must be the SAME function on both sides of the waterline, or the smoothsteps of u would
			// evaluate with different reaches on the land hex and the water hex and re-introduce exactly the
			// hard line at the hex edge that the signed coast coordinate exists to prevent. The water hex's
			// terrain is the only side both fragments can agree on (the land biome differs from hex to hex
			// along a coast, the body of water does not), and it is also the meaningful one: "is this a lake
			// or an ocean?" is a property of the water. So: own layer when we ARE the water, else the layer of
			// the nearest qualifying water NEIGHBOUR (the same edge best_d is measured to).
			int water_layer = own_layer;
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
				int nclass = int(nidpx.g * 255.0 + 0.5);
				bool nb_water = nclass == CLASS_WATER;
				if (own_water == nb_water) { continue; } // exactly one side must be water
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				float d = dot(v_world - own_c, dir_v) - half_dist; // <=0 inside own hex
				if (d > best_d) {
					best_d = d; // nearest qualifying edge
					if (!own_water) { water_layer = int(nidpx.r * 255.0 + 0.5); }
				}
			}
			if (best_d > -1e8) {
				// (1, 1) for any water terrain without a `shore_profile` — a bit-exact no-op below.
				vec2 profile = shore_profile(water_layer);
				float reach_scale = profile.r;  // scales ALL FOUR reaches
				float wisp_scale = profile.g;   // scales the wisp's half-width AND its strength
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
				// …and the WATER terrain's reach_scale then scales every reach (1.0 = the global, ocean-tuned
				// profile), so a lake can wear a shorter coastline than an ocean.
				// GROUPED AS `reach_scale * wobble` ON PURPOSE: folding the scale into the wobble factor means
				// a neutral 1.0 reduces each expression to the ORIGINAL `band * wobble` bit-for-bit, so every
				// water terrain without a shore_profile (i.e. every ocean) renders EXACTLY as it did before
				// this table existed. Scaling each band separately instead (`(band * reach_scale) * wobble`)
				// is the same value in exact arithmetic but rounds a different intermediate, which moved a
				// handful of ocean pixels by 1/255 — a no-op, but not a bit-identical one.
				float reach_wobble = reach_scale * wobble;
				float sand_reach = max(sand_band * reach_wobble, SHORE_REACH_MIN_PX);        // inland, u < 0
				float wash_reach = max(foam_inland_band * reach_wobble, SHORE_REACH_MIN_PX); // surf up the beach, u < 0
				float foam_reach = max(foam_band * reach_wobble, SHORE_REACH_MIN_PX);        // surf out to sea, u > 0

				// 1. SAND — the LAND side only (the water hex gets no sand at all). FULL from the waterline
				// across the surf's inland wash (the plateau — see SHORE_SAND_PLATEAU_MAX: the surf is painted
				// over the sand, so a sand that decayed from the waterline would be whitewashed exactly where
				// it is strongest and the beach would never appear), THEN smoothstep-fading inland into the
				// biome art over the rest of its reach — a soft inner edge, and short enough that the sand
				// tints the land art rather than burying it.
				float sand_plateau = min(wash_reach, sand_reach * SHORE_SAND_PLATEAU_MAX);
				float sand = ((u <= 0.0) ? smoothstep(sand_reach, sand_plateau, -u) : 0.0) * SHORE_SAND_OPACITY;
				result = mix(result, beach_color, sand);

				// 2. SURF — peaks AT the waterline and washes BOTH ways: INLAND over the sand (the
				// crossfade that blends the waves into the beach, killing the sand↔foam line) and SEAWARD
				// into open water. The peak must stay at ~1: with no sand in the water the base itself steps
				// at u = 0 (sand-tinted land → open water), and the full-strength foam is what hides it.
				float foam_in = smoothstep(-wash_reach, 0.0, u);   // rises across the beach → crossfade
				float foam_out = 1.0 - smoothstep(0.0, foam_reach, u); // dies out to sea
				float surf = foam_in * foam_out;
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
				// The wisp takes BOTH per-terrain scales: reach_scale pulls its whole band in with the rest of
				// the profile, while wisp_scale thins it (half-width) and fades it (strength) on its own — so
				// wisp_scale = 0 cleanly removes the second disturbance, which reads as noise on a small lake,
				// without touching that water's sand or surf. Same neutral-1.0 grouping as the reaches above.
				float wisp_reach_wobble = reach_wobble * wisp_scale;
				float wisp_center = max(wisp_center_band * reach_wobble, SHORE_REACH_MIN_PX);
				float wisp_half = max(wisp_half_band * wisp_reach_wobble, SHORE_REACH_MIN_PX);
				float wisp = wisp_half_band * wisp_scale > 0.0
					? smoothstep(wisp_half, 0.0, abs(u - wisp_center))
					: 0.0;
				float wn = vnoise(v_map / noise_cell + vec2(17.0, 5.0));
				result = mix(result, foam_color, wisp * (SHORE_WISP_STRENGTH * wisp_scale) * wn);
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
				if (own_is_peak == (ncode > 0)) { continue; } // only peak↔non-peak edges are boundaries
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				float d = dot(v_world - own_c, dir_v) - half_dist; // <=0 inside own hex, larger = nearer edge
				float nb_elev = texture(elev_map, nuv).r;      // same uv we read its id/peak-code from
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
				vec2 puv = v_map / (2.0 * hex_radius) * peak_scale;       // continuous world UV (like canopy cuv)
				float prom = mix(peak_min_prominence, 1.0, peak_elev);

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
						float a = texture(peak_tex, vec3(sp, float(peak_code - 1))).a;
						shadow = max(shadow, a * (1.0 - t));   // nearer occluder = darker
					}
				}
				result *= mix(1.0, 1.0 - peak_shadow_strength, clamp(shadow * shadow_env, 0.0, 1.0));

				// --- PEAK COMPOSITE (over the shadowed ground) ---
				float ss = s + pk_wobble;   // bumpy footline — the same wobble the shadow envelope rides
				if (ss > -peak_overhang) {
					float dens = canopy_density(ss, peak_overhang, peak_softness) * prom;  // reuse canopy_density
					if (dens > 0.0) {
						vec4 pk = texture(peak_tex, vec3(puv, float(peak_code - 1)));
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
