shader_type canvas_item;
render_mode blend_mix;

// Approach B — per-pixel biome blend for the 2D hex map (see clients/.../CLAUDE.md → Edge Blending).
// One whole-map quad runs this fragment shader. Each fragment inverts the pointy-top odd-r hex layout
// (MUST match MapView._hex_center / _axial_center exactly) to find its hex, samples that hex's biome
// from the Texture2DArray, and — only when the hex AND a neighbour across a shared edge are both
// blend_class "flat" and different biomes — dithers the neighbour's texture in near the seam. The mix
// is symmetric (0.5 at the exact edge on both sides) and driven by WORLD-space value noise (varies per
// hex, no baked tiling). water/rugged INLAND edges never blend (hard). Separately, land↔water edges get
// a SHORELINE wash (foam on the water side, sand beach on the land side) — see the shoreline block. See
// the id-map (splatmap) below.

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
uniform float blend_band;     // interlock half-band width in px (blend_width * radius)
uniform float noise_cell;     // world-space value-noise cell size in px (blend_noise_cell)
uniform bool blend_enabled;   // false below the LOD radius → base only (no shimmer at far zoom)
uniform bool fow_enabled;
uniform vec4 bg_color;        // out-of-map fill (matches the direct path background)
uniform vec4 fog_color;       // unexplored fill
uniform vec3 mist_color;      // discovered multiply tint (matches _fow_texture_tint_for_state)
uniform float mist_blend;
// Shoreline (land↔water coasts): a surf/foam wash on the WATER side + a sand beach on the LAND side.
// Universal for now — every land↔water edge gets both (flat-land and rugged-land alike). Same
// signed-distance-to-shared-edge machinery as the flat interlock; LOD-suppressed via blend_enabled.
uniform float foam_band;      // foam reach on the water side, in px (foam_width * radius)
uniform float beach_band;     // beach reach on the land side, in px (beach_width * radius)
uniform vec3 foam_color;      // surf line color (light desaturated cyan / near-white)
uniform vec3 beach_color;     // sand beach color (warm tan)

const float SQRT3 = 1.7320508075688772;
const int BLEND_CLASS_WATER = 0;          // id-map G channel codes (see the id_map uniform): 0 water / 1 flat / 2 rugged
// Shore look tuning (fixed feel; the widths/colors are the config levers).
const float SHORE_REACH_NOISE_MIN = 0.35; // min fraction of the shore band the noise leaves → wavy fingers
const float SHORE_WISP_CENTER = 1.35;     // inner foam wisp center, as a multiple of foam_band (out past the main band)
const float SHORE_WISP_HALF = 0.35;       // inner foam wisp half-thickness (× foam_band)
const float SHORE_WISP_STRENGTH = 0.45;   // inner foam wisp max opacity
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
const vec3 RIVER_LUMA = vec3(0.2126, 0.7152, 0.0722);    // Rec.709 luma, for the class harmonization
// Minor (light water over gravel) and Major (deep water) are deliberately DIFFERENT art, but untreated
// they meet as turquoise-next-to-near-black and read as two waterways spliced, not one river growing.
// Both layers are pulled toward a shared hue and a shared depth pivot; what survives is the LUMA ORDER
// (Minor lighter, Major deeper), which is the thing that should say "bigger".
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
// Hex CORNER geometry, for the riverInflow mask (B/A of the river map): corner `i` is the vertex at angle
// 60*i + 30 with +y DOWN — exactly MapView._hex_points' order (0 lower-right, 1 bottom, 2 lower-left,
// 3 upper-left, 4 top, 5 upper-right), which is the wire contract the sim packs the mask against. So a
// corner's position is just own_c + hex_radius * (cos, sin) of that angle: no neighbour fetch, no table.
const float RIVER_CORNER_ANGLE_STEP = PI / 3.0;    // 60° between adjacent corners
const float RIVER_CORNER_ANGLE_OFFSET = PI / 6.0;  // 30° — flat-top-up hex, first corner at lower-right

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

// Canopy density across the treeline. s = signed distance to the canopy↔non-canopy boundary,
// POSITIVE inside the forest, 0 at the edge, negative in the neighbour. Deep inside → 1; ~0.5 at the
// exact edge; ramps to 0 canopy_overhang px OUTSIDE the forest (crowns overhang, then fade).
float canopy_density(float s, float overhang, float softness) {
	if (s >= 0.0) { return 0.5 + 0.5 * smoothstep(0.0, softness, s); }
	return 0.5 * smoothstep(-overhang, 0.0, s);
}

// Pull a river water sample onto the SHARED river look: compress its luma toward RIVER_DEPTH_PIVOT
// (river_depth_compress) and its chroma toward RIVER_SHARED_HUE (river_tint_strength), preserving the
// relative order (Minor stays lighter than Major). This is what makes following a river from headwater to
// mouth read as one waterway GROWING rather than a bright stream spliced onto a near-black one.
vec3 river_harmonize(vec3 c) {
	float l = dot(c, RIVER_LUMA);
	float lc = mix(l, RIVER_DEPTH_PIVOT, river_depth_compress);   // depth-compressed target luma
	vec3 graded = c * (lc / max(l, RIVER_LUMA_EPSILON));          // same pixel, re-exposed to that luma
	vec3 hue = RIVER_SHARED_HUE * (lc / dot(RIVER_SHARED_HUE, RIVER_LUMA)); // shared hue AT that luma
	return clamp(mix(graded, hue, river_tint_strength), vec3(0.0), vec3(1.0));
}

// --- the river "organic" machinery, SHARED by the edge (Minor/Major) and navigable-channel passes ---
// All three sample WORLD space (v_map), never hex-local space: two hexes flanking a shared boundary must
// get identical values there, or the water would step at the boundary. That is what makes the edge river's
// two half-bands meet with no seam AND the navigable channel continuous across adjacent navigable hexes.
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
// The blend/shore/canopy/peak passes only ever loop over all 6 and are order-agnostic, but the RIVER
// pass indexes the per-tile riverEdges bitmask BY DIRECTION (class = (mask >> (2*dir)) & 3), so this
// table's order IS the wire contract with the sim. Do not reorder.
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

		if (blend_enabled && own_class == 1) {
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
				if (nclass != 1 || nlayer == own_layer) { continue; } // flat & different only
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				// signed distance to the shared edge (the perpendicular bisector); <=0 inside own hex.
				float d = dot(v_world - own_c, dir_v) - half_dist;
				float p = clamp(0.5 + d / (2.0 * blend_band), 0.0, 1.0); // 0.5 AT the edge → symmetric
				if (p > best_p) {
					best_p = p;
					best_layer = nlayer;
				}
			}
			if (best_layer >= 0 && best_p > vnoise(v_map / noise_cell)) {
				// Sample the neighbour biome at the SAME continuous world position as the base (only the
				// array layer differs), so the flat↔flat dither interlocks two world-sampled biomes at one
				// world point — the cross-edge blend stays continuous with the new base sampling.
				result = texture(biome_array, vec3(base_uv, float(best_layer))).rgb;
			}
		}

		// --- shoreline: foam wash on the water side, sand beach on the land side, at land↔water edges ---
		// Universal: fires for ANY edge where exactly one side is water (own_class 0). Inland flat↔flat
		// and rugged↔* edges are untouched (both sides non-water → skipped). LOD-gated like the blend.
		// EXCEPT a NAVIGABLE river: it is a bank hex (blend_class "flat") with a channel painted through it,
		// so a coast treatment there is a lie in both directions — a beach on the bank at the river's mouth,
		// and a surf line drawn ACROSS that mouth (the river visibly walled off from the sea it drains into).
		// A river meeting the sea is not a coast, so any edge with a navigable hex on EITHER side is skipped.
		bool own_navigable = own_layer == river_navigable_terrain_id;
		if (blend_enabled && !own_navigable) {
			bool own_water = own_class == BLEND_CLASS_WATER;
			float best_d = -1e9; // nearest qualifying edge = max signed distance (closest to the seam)
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
				vec4 nsh = texture(id_map, nuv);
				if (int(nsh.r * 255.0 + 0.5) == river_navigable_terrain_id) { continue; } // river mouth ≠ coast
				int nclass = int(nsh.g * 255.0 + 0.5);
				bool nb_water = nclass == BLEND_CLASS_WATER;
				if (own_water == nb_water) { continue; } // exactly one side must be water
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				best_d = max(best_d, dot(v_world - own_c, dir_v) - half_dist); // <=0 inside own hex
			}
			if (best_d > -1e8) {
				float dist_in = max(-best_d, 0.0);            // 0 at the coast edge, grows away from it
				float band = own_water ? foam_band : beach_band;
				float n = vnoise(v_map / noise_cell);
				float reach = max(band * mix(SHORE_REACH_NOISE_MIN, 1.0, n), 1.0); // wavy inward extent
				float amount = clamp(1.0 - dist_in / reach, 0.0, 1.0); // strongest at the edge, fades away
				result = mix(result, own_water ? foam_color : beach_color, amount);
				if (own_water) {
					// optional faint second wisp: a thinner surf line further out over open water
					float wisp = clamp(1.0 - abs(dist_in - foam_band * SHORE_WISP_CENTER)
						/ max(foam_band * SHORE_WISP_HALF, 1.0), 0.0, 1.0);
					float wn = vnoise(v_map / noise_cell + vec2(17.0, 5.0));
					result = mix(result, foam_color, wisp * SHORE_WISP_STRENGTH * wn);
				}
			}
		}

		// --- river pass: Minor/Major rivers run along hex EDGES (not through centers) ---
		// The own hex's 12-bit riverEdges mask (2 bits per odd-r direction) says which of its 6 sides carry
		// water and of what class. For each such side we measure the distance to the SHARED EDGE SEGMENT
		// (mid ± perp * hex_radius * RIVER_HALF_SIDE — clamped to the segment, or the band would smear
		// across the whole hex along the infinite bisector), perturb it with the same world value-noise the
		// treeline/shore ride (so the river WANDERS instead of reading as a honeycomb), and keep the edge
		// with the MAX coverage. That max-coverage (== min-distance) pick is what rounds a 120° corner join
		// for free — no spline math. Each hex paints only the half-band on ITS OWN side; both hexes flanking
		// an edge carry it in their masks, so the two halves meet symmetrically with no seam and NO
		// cross-hex sampling. Sits AFTER the shore pass (so a navigable river's banks are already drawn) and
		// BEFORE the canopy/peak passes (trees overhang a river; mountains sit above it) and before FoW (so
		// a river in a Discovered tile dims with the mist rather than punching through it).
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
		// A NavigableRiver (terrain 37) is a water TERRAIN in the sim (blocking, boats), but rendering it as
		// one made it a hex-shaped puddle with a beach and surf — indistinguishable from a lake, and nothing
		// like a river. So its BASE texture is the silty bank ground (and its blend_class is "flat", so the
		// bank blends softly into neighbouring flat land and takes no shore pass — see the guard above), and
		// the water is painted ON TOP here, as a channel far narrower than the hex.
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
			// HEAD TAPER — the width the trunk's arms START at, at the hex CENTRE. On the FIRST hex of a
			// chain (the only hex with a nonzero riverInflow) the trunk used to spring to full navigable
			// width the instant it left the centre: a hairline Minor arrived at a vertex and a few px later
			// was a great river. A jump-cut, not a river. So on that hex the arms start at the half-width of
			// the WIDEST tributary feeding in (max over the 6 inflow corners — two tributaries can land on
			// the same trunk head, and the sim already stores the widest class per corner) and SWELL to the
			// channel's full width by the time they reach the hex EDGE (see the per-arm mix below).
			// A hex with inflow == 0 (mid-chain, or a river navigable from its first step) keeps the
			// CONSTANT full width: head_base stays river_navigable_half_width and the mix is a no-op.
			// min() with the channel width guards a config where a class is somehow wider than the trunk —
			// a head must never START wider than it ends.
			float inflow_base = 0.0;
			for (int c = 0; c < 6; c++) {
				int icls = (inflow >> (2 * c)) & 3;
				if (icls < RIVER_CLASS_MINOR || icls > RIVER_CLASS_MAX) { continue; } // 0 = none, 3 = reserved
				inflow_base = max(inflow_base,
					(icls == RIVER_CLASS_MINOR) ? river_minor_half_width : river_major_half_width);
			}
			float head_base = (inflow_base > 0.0)
				? min(inflow_base, river_navigable_half_width)
				: river_navigable_half_width;
			float cov_trunk = -1e9;         // coverage = half_width - distance; > 0 inside the water
			float cov_spur = -1e9;          // best INFLOW spur, tracked apart so the two can crossfade
			float spur_layer = 0.0;         // river-array layer of the winning spur (class - 1)
			vec2 best_tangent = vec2(1.0, 0.0);
			int arms = 0;
			for (int dir = 0; dir < 6; dir++) {
				// The exit bit IS the arm rule — no neighbour fetch, and none possible: the mask is
				// asymmetric at the mouth, so the neighbour's opinion is not a substitute for the own hex's.
				// Needing no neighbour texel, an arm also draws correctly at the map border / wrap seam (the
				// geometry only wants the neighbour's CENTRE, and hex_center is pure math on the raw,
				// unwrapped column — so the arm points the right way even across the seam).
				if (((channel >> dir) & 1) == 0) { continue; }
				arms++;
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
			if (arms == 0) {
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
				if (d > pk_best_d) {
					pk_best_d = d;
					nb_peak_code = ncode;
					nb_peak_elev = texture(elev_map, nuv).r;   // same uv we read its id/peak-code from
				}
			}
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
				float peak_elev = own_is_peak ? texture(elev_map, id_uv).r : nb_peak_elev;  // 0..1
				vec2 puv = v_map / (2.0 * hex_radius) * peak_scale;       // continuous world UV (like canopy cuv)
				float prom = mix(peak_min_prominence, 1.0, peak_elev);

				// --- CAST SHADOW: look back toward the light; if a peak occludes, darken this ground ---
				// peak_light_dir points TOWARD the light (top-left). A fragment is shadowed if a peak sits
				// between it and the light, i.e. at puv + light_dir * t. Multi-tap for a soft ramp.
				float shadow = 0.0;
				float slen_uv = (peak_shadow_len * (0.4 + 0.6 * peak_elev)) / (2.0 * hex_radius) * peak_scale;
				const int TAPS = 4;
				for (int k = 1; k <= TAPS; k++) {
					float t = float(k) / float(TAPS);
					vec2 sp = puv + peak_light_dir * (slen_uv * t);
					float a = texture(peak_tex, vec3(sp, float(peak_code - 1))).a;
					shadow = max(shadow, a * (1.0 - t));   // nearer occluder = darker
				}
				result *= mix(1.0, 1.0 - peak_shadow_strength, clamp(shadow, 0.0, 1.0));

				// --- PEAK COMPOSITE (over the shadowed ground) ---
				float tn = vnoise(v_map / noise_cell + vec2(7.0, 5.0));
				float ss = s + (tn - 0.5) * 2.0 * peak_overhang * CANOPY_TREELINE_NOISE;  // bumpy footline
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
			float vis = texture(vis_map, id_uv).r;
			if (vis < 0.25) {
				result = fog_color.rgb;
			} else if (vis < 0.75) {
				result *= mix(vec3(1.0), mist_color, mist_blend); // discovered: remembered/cloudy
			}
		}
		COLOR = vec4(result, 1.0);
	}
}
