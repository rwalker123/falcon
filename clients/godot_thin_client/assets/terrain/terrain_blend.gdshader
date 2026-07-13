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
// WATER↔WATER overrides of the three levers above (config "water_blend"). The land levers assume the two
// textures carry detail for the height term to interlock on; water is smooth and low-variance, so at land
// reach/amplitude the seam just draws a soft-edged HEXAGON. Water therefore gets a wider reach, a softer
// feather and a bigger wobble (ocean depth grades gradually anyway). Selected per-fragment by own_class.
// The wobble CELL and the height nudge are deliberately NOT overridden (shared with land).
uniform float water_blend_band;
uniform float water_blend_soft;
uniform float water_blend_noise_amount;
uniform float noise_cell;     // FEATURE value-noise cell size in px (config feature_noise_cell) — shared by the
                              // shoreline reach/wisp, the canopy treeline and the peak footline. Deliberately
                              // DECOUPLED from blend_noise_cell so retuning the interlock grain can't move any
                              // coastline, treeline or footline.
uniform bool blend_enabled;   // false below the LOD radius → base only (no shimmer at far zoom)
uniform bool fow_enabled;
uniform vec4 bg_color;        // out-of-map fill (matches the direct path background)
uniform vec4 fog_color;       // unexplored fill
uniform vec3 mist_color;      // discovered multiply tint (matches _fow_texture_tint_for_state)
uniform float mist_blend;
// Shoreline (land↔water coasts). EVERYTHING LIVES ON THE WATER SIDE: sand shallows hug the land edge,
// the surf breaks over their seaward lip, then open water. The LAND hex keeps its own texture (the
// land-side rim is an opt-in damp rim, default OFF). Why: when both sides painted their own colour at
// full strength AT the shared edge, the land went solid tan and the water solid white and they met along
// the hex boundary — a hard tan↔white line that TRACED THE HEXAGON — and the land-side beach also
// overpainted the land texture radius-deep. Same signed-distance-to-shared-edge machinery as the flat
// interlock; LOD-suppressed via blend_enabled.
uniform float sand_band;       // sand-shallows reach on the WATER side, in px (sand_width * radius)
uniform float foam_band;       // surf reach BEYOND the sand, on the WATER side, in px (foam_width * radius)
uniform float land_beach_band; // optional damp-rim reach on the LAND side, in px (land_beach_width * radius);
                               // 0 (the default) = the land texture is left completely untouched
uniform vec3 foam_color;      // surf line color (light desaturated cyan / near-white)
uniform vec3 beach_color;     // sand color — the water-side shallows (and the optional land damp rim)

const float SQRT3 = 1.7320508075688772;
// blend_class codes, as packed into the id-map's G channel by MapView._blend_class_code. The SAME-CLASS
// eligibility rule below is expressed in these, never in bare literals.
const int CLASS_WATER = 0;
const int CLASS_FLAT = 1;
const int CLASS_RUGGED = 2;  // never blends (discrete-object textures must not bleed) — see the gate
// Shore look tuning (fixed feel; the widths/colors are the config levers).
const float SHORE_REACH_NOISE_MIN = 0.35; // min fraction of the shore band the noise leaves → wavy fingers
const float SHORE_REACH_MIN_PX = 1.0;     // floor on any noise-shrunk reach, so no fade ever divides by ~0
const float SHORE_SAND_OPACITY = 0.85;    // max sand wash over the water texture — deliberately < 1 so the
                                          // shallows keep the water's own detail and never read as flat paint
const float SHORE_SAND_PLATEAU = 0.45;    // fraction of the sand band that stays at FULL sand before it fades
                                          // out at its seaward lip. Without a plateau the surf (which rises
                                          // across the same band) whitened the sand almost everywhere and the
                                          // coast read as one pale wash instead of sand-then-surf.
const float SHORE_FOAM_OPACITY = 0.9;     // max surf opacity at the break line
const float SHORE_WISP_CENTER = 1.35;     // inner foam wisp center, as a multiple of the surf's OUTER edge
const float SHORE_WISP_HALF = 0.35;       // inner foam wisp half-thickness (× foam_band)
const float SHORE_WISP_STRENGTH = 0.45;   // inner foam wisp max opacity
const float CANOPY_TREELINE_NOISE = 0.6;  // how hard world-noise waves the treeline in/out (× overhang)
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
		bool own_blendable = own_class == CLASS_FLAT || own_class == CLASS_WATER;
		if (blend_enabled && own_blendable) {
			// Per-class seam levers: water gets the wider/softer/wobblier set (see the uniforms).
			bool water_seam = own_class == CLASS_WATER;
			float seam_band = water_seam ? water_blend_band : blend_band;
			float seam_soft = water_seam ? water_blend_soft : blend_soft;
			float seam_noise_amount = water_seam ? water_blend_noise_amount : blend_noise_amount;
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
				if (nclass != own_class || nlayer == own_layer) { continue; } // same class & different id only
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

		// --- shoreline: sand shallows + surf, BOTH on the water side, at land↔water edges ---
		// Universal: fires for ANY edge where exactly one side is water. Inland flat↔flat, water↔water and
		// rugged↔* edges are untouched (both sides same wetness → skipped). LOD-gated like the blend.
		// Seaward read: land (its own texture) → sand shallows → surf → open water. Every fade is a
		// smoothstep (no linear ramp's kink, and — crucially — no term saturates against the land texture
		// at the shared edge, which is what used to draw a hard tan↔white hexagon outline there).
		if (blend_enabled) {
			bool own_water = own_class == CLASS_WATER;
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
				int nclass = int(texture(id_map, nuv).g * 255.0 + 0.5);
				bool nb_water = nclass == CLASS_WATER;
				if (own_water == nb_water) { continue; } // exactly one side must be water
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				best_d = max(best_d, dot(v_world - own_c, dir_v) - half_dist); // <=0 inside own hex
			}
			if (best_d > -1e8) {
				float dist_in = max(-best_d, 0.0);            // 0 at the coast edge, grows away from it
				// The SAME world-noise wobble scales every reach, so the sand's outer lip, the surf line and
				// the wisp all meander together (organic fingers) instead of drawing concentric clean stripes.
				float n = vnoise(v_map / noise_cell);
				float wobble = mix(SHORE_REACH_NOISE_MIN, 1.0, n);
				if (own_water) {
					// 1. SAND SHALLOWS — full against the land edge across the plateau, then fading out at
					// their seaward lip (sand_reach).
					float sand_reach = max(sand_band * wobble, SHORE_REACH_MIN_PX);
					float sand_plateau = sand_reach * SHORE_SAND_PLATEAU;
					float sand = smoothstep(sand_reach, sand_plateau, dist_in) * SHORE_SAND_OPACITY;
					result = mix(result, beach_color, sand);
					// 2. SURF — breaks OVER the sand's seaward lip, never against the land: it rises across
					// the sand's fade-out (crossfading sand → foam), peaks at the lip, and fades into open
					// water. ZERO at the coast edge is the whole point — a foam term that peaked at the edge
					// is what put a white line right up against the land texture, tracing the hexagon.
					float foam_reach = max(foam_band * wobble, SHORE_REACH_MIN_PX);
					float foam_outer = sand_reach + foam_reach;
					float surf = smoothstep(sand_plateau, sand_reach, dist_in)
						* smoothstep(foam_outer, sand_reach, dist_in);
					result = mix(result, foam_color, surf * SHORE_FOAM_OPACITY);
					// 3. faint second wisp: a thinner surf line further out over open water
					float wisp_center = foam_outer * SHORE_WISP_CENTER;
					float wisp_half = max(foam_reach * SHORE_WISP_HALF, SHORE_REACH_MIN_PX);
					float wisp = smoothstep(wisp_half, 0.0, abs(dist_in - wisp_center));
					float wn = vnoise(v_map / noise_cell + vec2(17.0, 5.0));
					result = mix(result, foam_color, wisp * SHORE_WISP_STRENGTH * wn);
				} else if (land_beach_band > 0.0) {
					// OPT-IN damp rim on the land side (default width 0 → skipped entirely, land texture
					// untouched). Kept as a lever in case a bone-dry land↔sand meeting reads too abrupt.
					float rim_reach = max(land_beach_band * wobble, SHORE_REACH_MIN_PX);
					float rim = smoothstep(rim_reach, 0.0, dist_in) * SHORE_SAND_OPACITY;
					result = mix(result, beach_color, rim);
				}
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
