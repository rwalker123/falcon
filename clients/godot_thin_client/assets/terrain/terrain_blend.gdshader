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
// Canopy overlay array: whole tree crowns on transparency (RGBA), sampled in WORLD space so crowns are
// continuous across hexes and overhang the boundary. Indexed by canopy-array layer (id-map B - 1).
// Trilinear (mipmap) filtering so far-zoom crowns AVERAGE into a smooth darker-green forest mass instead
// of shimmering/aliasing — the crown array is built with mipmaps (see TerrainTextureManager).
uniform sampler2DArray canopy_tex : filter_linear_mipmap, repeat_enable;
uniform bool canopy_enabled;      // false when no canopy asset exists → skip the canopy pass entirely
uniform bool canopy_lod_enabled;  // canopy LOD gate — DECOUPLED from blend_enabled so forests stay
                                  // readable (a darker-green mass) at far zoom where the blend is off
uniform float canopy_overhang;    // how far crowns overhang past the treeline, in px (overhang_width * radius)
uniform float canopy_softness;    // inner treeline ramp half-width, in px (softness_width * radius)
uniform float canopy_scale;       // world-UV scale multiplier (1.0 = one crown tile per hex, matching the base)
// id-map splatmap: R = terrain id (0..255), G = blend_class code (0 water, 1 flat, 2 rugged),
// B = canopy code (0 = no canopy, else canopy-array layer + 1).
uniform sampler2D id_map : filter_nearest;
// visibility raster: R = 0 unexplored / 0.5 discovered / 1 active (only read when fow_enabled).
uniform sampler2D vis_map : filter_nearest;

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
// Shore look tuning (fixed feel; the widths/colors are the config levers).
const float SHORE_REACH_NOISE_MIN = 0.35; // min fraction of the shore band the noise leaves → wavy fingers
const float SHORE_WISP_CENTER = 1.35;     // inner foam wisp center, as a multiple of foam_band (out past the main band)
const float SHORE_WISP_HALF = 0.35;       // inner foam wisp half-thickness (× foam_band)
const float SHORE_WISP_STRENGTH = 0.45;   // inner foam wisp max opacity
const float CANOPY_TREELINE_NOISE = 0.6;  // how hard world-noise waves the treeline in/out (× overhang)

varying vec2 v_world;

void vertex() {
	v_world = VERTEX; // local == MapView local space, where hex centers are computed
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
		vec2 own_uv = (v_world - own_c) / (2.0 * hex_radius) + 0.5;
		vec3 result = texture(biome_array, vec3(own_uv, float(own_layer))).rgb;

		if (blend_enabled && own_class == 1) {
			float best_p = 0.0;
			int best_layer = -1;
			vec2 best_c = vec2(0.0);
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
					best_c = nb_c;
				}
			}
			if (best_layer >= 0 && best_p > vnoise(v_world / noise_cell)) {
				vec2 nb_uv = (v_world - best_c) / (2.0 * hex_radius) + 0.5;
				result = texture(biome_array, vec3(nb_uv, float(best_layer))).rgb;
			}
		}

		// --- shoreline: foam wash on the water side, sand beach on the land side, at land↔water edges ---
		// Universal: fires for ANY edge where exactly one side is water (own_class 0). Inland flat↔flat
		// and rugged↔* edges are untouched (both sides non-water → skipped). LOD-gated like the blend.
		if (blend_enabled) {
			bool own_water = own_class == 0;
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
				bool nb_water = nclass == 0;
				if (own_water == nb_water) { continue; } // exactly one side must be water
				vec2 nb_c = hex_center(ncol, nrow);
				vec2 dir_v = normalize(nb_c - own_c);
				float half_dist = 0.5 * distance(own_c, nb_c);
				best_d = max(best_d, dot(v_world - own_c, dir_v) - half_dist); // <=0 inside own hex
			}
			if (best_d > -1e8) {
				float dist_in = max(-best_d, 0.0);            // 0 at the coast edge, grows away from it
				float band = own_water ? foam_band : beach_band;
				float n = vnoise(v_world / noise_cell);
				float reach = max(band * mix(SHORE_REACH_NOISE_MIN, 1.0, n), 1.0); // wavy inward extent
				float amount = clamp(1.0 - dist_in / reach, 0.0, 1.0); // strongest at the edge, fades away
				result = mix(result, own_water ? foam_color : beach_color, amount);
				if (own_water) {
					// optional faint second wisp: a thinner surf line further out over open water
					float wisp = clamp(1.0 - abs(dist_in - foam_band * SHORE_WISP_CENTER)
						/ max(foam_band * SHORE_WISP_HALF, 1.0), 0.0, 1.0);
					float wn = vnoise(v_world / noise_cell + vec2(17.0, 5.0));
					result = mix(result, foam_color, wisp * SHORE_WISP_STRENGTH * wn);
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
				float tn = vnoise(v_world / noise_cell + vec2(3.0, 11.0));
				s += (tn - 0.5) * 2.0 * canopy_overhang * CANOPY_TREELINE_NOISE; // bumpy treeline
				float dens = canopy_density(s, canopy_overhang, canopy_softness);
				if (dens > 0.0) {
					vec2 cuv = v_world / (2.0 * hex_radius) * canopy_scale; // world-space, continuous
					vec4 crown = texture(canopy_tex, vec3(cuv, float(forest_code - 1)));
					result = mix(result, crown.rgb, crown.a * dens);
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
