shader_type spatial;
render_mode cull_disabled, depth_draw_opaque, specular_schlick_ggx;

// Base biome color texture (fallback for LOD)
uniform sampler2D biome_texture : source_color, filter_linear_mipmap;

// Terrain texture array (37 terrain types)
uniform sampler2DArray terrain_textures : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D terrain_index_texture : filter_nearest;
uniform sampler2D terrain_neighbor_texture : filter_nearest; // RGB8: neighbors 0-2 in pixel 1, 3-5 in pixel 2
uniform bool use_terrain_textures = false;
uniform bool use_edge_blending = false;
uniform float texture_scale : hint_range(1.0, 16.0) = 4.0;
uniform float blend_width : hint_range(0.0, 0.5) = 0.15;
uniform float lod_near : hint_range(0.0, 500.0) = 50.0;
uniform float lod_far : hint_range(0.0, 500.0) = 200.0;

// Hex grid parameters (must match GDScript)
uniform vec2 hex_size = vec2(1.0, 0.866025); // width, height (sqrt(3)/2)
uniform vec2 grid_size = vec2(64.0, 64.0); // number of hexes in grid

// Overlay system
uniform sampler2D overlay_texture : source_color, filter_linear_mipmap;
uniform bool overlay_enabled = false;
uniform vec3 overlay_color : source_color = vec3(0.15, 0.45, 1.0);
uniform float overlay_mix : hint_range(0.0, 1.0) = 0.7;

// Lighting
uniform float ambient_strength : hint_range(0.0, 1.0) = 0.35;
uniform bool debug_mode = false;

varying vec2 relief_uv;
varying vec3 world_pos;

void vertex() {
	relief_uv = UV;
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

// Constants for hex math
const float PI = 3.14159265359;
const float SQRT3 = 1.7320508075688772;

// Calculate local UV within a hex cell (0-1 range, centered)
// Returns: xy = local position (-0.5 to 0.5), z = distance from center (0 at center, 1 at edge)
vec3 get_hex_local_coords(vec2 uv, vec2 grid_size) {
	// Convert UV to hex grid coordinates
	vec2 hex_dims = vec2(1.0, SQRT3 * 0.5); // pointy-top hex
	vec2 scaled = uv * grid_size;

	// Determine which hex column/row
	float col = floor(scaled.x / (hex_dims.x * 0.75));
	float row_offset = mod(col, 2.0) * 0.5;
	float row = floor((scaled.y / hex_dims.y) - row_offset);

	// Calculate hex center
	vec2 hex_center;
	hex_center.x = col * hex_dims.x * 0.75 + hex_dims.x * 0.5;
	hex_center.y = (row + row_offset + 0.5) * hex_dims.y;

	// Local position relative to hex center
	vec2 local = scaled - hex_center;

	// Normalize to hex radius
	float hex_radius = hex_dims.x * 0.5;
	local /= hex_radius;

	// Calculate distance to hex edge using hex distance function
	vec2 abs_local = abs(local);
	float hex_dist = max(abs_local.x * 0.5 + abs_local.y * (SQRT3 / 2.0), abs_local.x);

	return vec3(local, hex_dist);
}

// Get the neighbor index (0-5) based on angle from hex center
int get_neighbor_index(vec2 local_pos) {
	float angle = atan(local_pos.y, local_pos.x);
	// Shift so neighbor 0 is at the right (East)
	angle = mod(angle + PI / 6.0, 2.0 * PI);
	return int(floor(angle / (PI / 3.0)));
}

// Sample neighbor terrain ID from the neighbor texture
// The neighbor texture is 2x width: each hex has 2 adjacent RGB pixels
// Pixel 0: RGB = neighbors 0,1,2
// Pixel 1: RGB = neighbors 3,4,5
int get_neighbor_terrain_id(vec2 uv, int neighbor_idx) {
	// The neighbor texture is 2x the width of terrain_index_texture
	// So we need to sample at uv.x * 2.0 for the first pixel
	vec2 neighbor_uv = vec2(uv.x * 2.0, uv.y);
	vec2 tex_size = vec2(textureSize(terrain_neighbor_texture, 0));
	float pixel_width = 1.0 / tex_size.x;

	if (neighbor_idx < 3) {
		// Sample first pixel (neighbors 0-2)
		vec3 neighbors = texture(terrain_neighbor_texture, neighbor_uv).rgb;
		if (neighbor_idx == 0) return int(neighbors.r * 255.0 + 0.5);
		if (neighbor_idx == 1) return int(neighbors.g * 255.0 + 0.5);
		return int(neighbors.b * 255.0 + 0.5);
	} else {
		// Sample second pixel (neighbors 3-5)
		vec3 neighbors2 = texture(terrain_neighbor_texture, neighbor_uv + vec2(pixel_width, 0.0)).rgb;
		if (neighbor_idx == 3) return int(neighbors2.r * 255.0 + 0.5);
		if (neighbor_idx == 4) return int(neighbors2.g * 255.0 + 0.5);
		return int(neighbors2.b * 255.0 + 0.5);
	}
}

vec3 sample_terrain_by_id(int terrain_id, vec2 tile_uv) {
	return texture(terrain_textures, vec3(tile_uv, float(terrain_id))).rgb;
}

vec3 sample_terrain_texture(vec2 uv, vec2 tile_uv) {
	// Get terrain ID from index texture (0-36 stored as 0.0-1.0)
	float terrain_val = texture(terrain_index_texture, uv).r;
	int terrain_id = int(terrain_val * 255.0 + 0.5);

	// Sample terrain texture array with tiled UVs
	return sample_terrain_by_id(terrain_id, tile_uv);
}

vec3 sample_terrain_blended(vec2 uv, vec2 tile_uv, vec2 grid_size) {
	// Get main terrain
	float terrain_val = texture(terrain_index_texture, uv).r;
	int terrain_id = int(terrain_val * 255.0 + 0.5);
	vec3 main_color = sample_terrain_by_id(terrain_id, tile_uv);

	if (!use_edge_blending) {
		return main_color;
	}

	// Get hex local coordinates
	vec3 hex_local = get_hex_local_coords(uv, grid_size);
	vec2 local_pos = hex_local.xy;
	float edge_dist = hex_local.z;

	// Check if we're near an edge (edge_dist approaches 1.0 at hex boundary)
	float blend_start = 1.0 - blend_width;
	if (edge_dist > blend_start) {
		// Determine which neighbor to blend with
		int neighbor_idx = get_neighbor_index(local_pos);
		int neighbor_terrain = get_neighbor_terrain_id(uv, neighbor_idx);

		// Only blend if neighbor has different terrain
		if (neighbor_terrain != terrain_id && neighbor_terrain >= 0 && neighbor_terrain < 37) {
			vec3 neighbor_color = sample_terrain_by_id(neighbor_terrain, tile_uv);

			// Smooth blend from center to edge
			float blend_factor = smoothstep(blend_start, 1.0, edge_dist);
			main_color = mix(main_color, neighbor_color, blend_factor * 0.5);
		}
	}

	return main_color;
}

void fragment() {
	vec3 normal = NORMAL;
	vec3 light_dir = normalize(vec3(0.4, 0.8, 0.35));

	// Get base biome color (simple fallback)
	vec3 biome_color = texture(biome_texture, relief_uv).rgb;

	// Calculate albedo based on terrain texture mode
	vec3 albedo;
	if (use_terrain_textures) {
		// Calculate tiled UV coordinates for terrain texture
		vec2 tile_uv = fract(relief_uv * texture_scale);

		// Sample terrain texture with optional edge blending
		vec3 terrain_color;
		if (use_edge_blending) {
			terrain_color = sample_terrain_blended(relief_uv, tile_uv, grid_size);
		} else {
			terrain_color = sample_terrain_texture(relief_uv, tile_uv);
		}

		// LOD blending based on camera distance
		float camera_dist = length(world_pos - CAMERA_POSITION_WORLD);
		float lod_factor = clamp((camera_dist - lod_near) / max(lod_far - lod_near, 0.001), 0.0, 1.0);

		// Blend between detailed texture and simple biome color
		albedo = mix(terrain_color, biome_color, lod_factor);
	} else {
		albedo = biome_color;
	}

	// Debug mode override
	if (debug_mode) {
		float h = COLOR.r;
		albedo = vec3(h);
	}

	// Lighting
	float diffuse = clamp(dot(normal, light_dir), 0.0, 1.0);
	vec3 lit = albedo * (ambient_strength + diffuse * (1.0 - ambient_strength));

	// Overlay blending
	float overlay_value = overlay_enabled ? texture(overlay_texture, relief_uv).r : 0.0;
	vec3 final_color = mix(lit, overlay_color, overlay_value * overlay_mix);

	ALBEDO = final_color;
	ROUGHNESS = 0.9;
	SPECULAR = 0.05;
}
