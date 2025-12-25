shader_type spatial;
render_mode specular_schlick_ggx, cull_disabled;

uniform vec4 deep_color : source_color = vec4(0.04, 0.12, 0.24, 0.9);
uniform vec4 coastal_color : source_color = vec4(0.08, 0.25, 0.37, 0.85);
uniform vec4 fresh_color : source_color = vec4(0.11, 0.35, 0.45, 0.8);

void fragment() {
	float type = COLOR.r;
	
	vec4 base_color = deep_color;
	if (type > 0.4) {
		base_color = fresh_color;
	} else if (type > 0.1) {
		base_color = coastal_color;
	}
	
	ALBEDO = base_color.rgb;
	ALPHA = base_color.a;
	ROUGHNESS = 0.1;
	METALLIC = 0.0;
	SPECULAR = 0.5;
}
