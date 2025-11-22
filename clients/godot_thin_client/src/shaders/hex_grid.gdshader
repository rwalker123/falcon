shader_type spatial;
render_mode unshaded, depth_draw_always, cull_disabled;

uniform vec4 line_color : source_color = vec4(0.08, 0.2, 0.45, 0.8);

void fragment() {
    ALBEDO = line_color.rgb;
    ALPHA = line_color.a;
}
