extends Node2D
## Dedicated canvas item that draws the whole-map terrain-blend shader (Approach B) as a single rect.
## It lives as a child of MapView with show_behind_parent = true, so the blended terrain renders BEHIND
## MapView's own draws (grid lines, overlays, markers). A separate node is required because a canvas
## item's ShaderMaterial applies to ALL of that item's draw commands — keeping the shader on its own
## node leaves MapView's markers/grid unshaded. MapView owns the ShaderMaterial + uniforms; this node
## only issues the one draw_rect. Shader uniforms are live: MapView re-pushes them and calls this node's
## queue_redraw() each frame (from _update_terrain_shader_quad), so the rect re-renders every frame with the
## fresh uniforms. set_rect_size() additionally queue_redraw()s only when the rect size actually changes.

var rect_size: Vector2 = Vector2.ZERO

func set_rect_size(size: Vector2) -> void:
	if size == rect_size:
		return
	rect_size = size
	queue_redraw()

func _draw() -> void:
	if rect_size.x > 0.0 and rect_size.y > 0.0:
		# White vertex color; the shader ignores COLOR and computes the terrain itself.
		draw_rect(Rect2(Vector2.ZERO, rect_size), Color.WHITE)
