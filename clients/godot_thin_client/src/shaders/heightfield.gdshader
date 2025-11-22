shader_type spatial;
render_mode cull_disabled, depth_draw_opaque, specular_schlick_ggx;

uniform sampler2D biome_texture : source_color, filter_linear_mipmap;
uniform sampler2D overlay_texture : source_color, filter_linear_mipmap;
uniform bool overlay_enabled = false;
uniform vec3 overlay_color : source_color = vec3(0.15, 0.45, 1.0);
uniform float overlay_mix : hint_range(0.0, 1.0) = 0.7;
uniform float ambient_strength : hint_range(0.0, 1.0) = 0.35;
uniform bool debug_mode = false;

varying vec2 relief_uv;

void vertex() {
	relief_uv = UV;
}

void fragment() {
	vec3 normal = NORMAL;
	vec3 light_dir = normalize(vec3(0.4, 0.8, 0.35));
	vec3 albedo = texture(biome_texture, relief_uv).rgb;
	if (debug_mode) {
		float h = COLOR.r;
		albedo = vec3(h);
	}
	float diffuse = clamp(dot(normal, light_dir), 0.0, 1.0);
	vec3 lit = albedo * (ambient_strength + diffuse * (1.0 - ambient_strength));
	float overlay_value = overlay_enabled ? texture(overlay_texture, relief_uv).r : 0.0;
	vec3 overlay_tint = mix(lit, overlay_color, overlay_value * overlay_mix);
	ALBEDO = overlay_tint;
	ROUGHNESS = 0.9;
	SPECULAR = 0.05;
}
