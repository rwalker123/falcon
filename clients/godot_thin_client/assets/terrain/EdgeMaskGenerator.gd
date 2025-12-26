@tool
extends SceneTree
## Generates 6 thin hex edge strip masks for terrain transitions (overlay/fringe technique).
## Run from command line:
## godot --headless --script assets/terrain/EdgeMaskGenerator.gd

const OUTPUT_DIR := "res://assets/terrain/textures/edges/"
const MASK_SIZE := 512
const SQRT3 := 1.7320508075688772
const EDGE_THICKNESS := 0.18  # Only outer 18% of hex has the fringe

func _init() -> void:
	print("Generating thin hex edge strip masks...")

	# Ensure output directory exists
	var dir := DirAccess.open("res://assets/terrain/textures/")
	if dir and not dir.dir_exists("edges"):
		dir.make_dir("edges")

	# Generate 6 edge masks (one for each hex edge direction)
	# Edge 0 = East, 1 = NE, 2 = NW, 3 = West, 4 = SW, 5 = SE
	for edge_idx in range(6):
		var mask := _generate_edge_mask(edge_idx)
		var filename := "edge_mask_%d.png" % edge_idx
		var filepath := OUTPUT_DIR + filename
		var abs_path := ProjectSettings.globalize_path(filepath)

		var err := mask.save_png(abs_path)
		if err != OK:
			push_error("Failed to save %s: %d" % [filename, err])
		else:
			print("  Generated: %s" % filename)

	print("")
	print("Edge masks generated in: %s" % OUTPUT_DIR)
	print("Each mask is a thin strip (%.0f%%) at one edge of the hex." % (EDGE_THICKNESS * 100))
	quit()

func _generate_edge_mask(edge_idx: int) -> Image:
	# Create a hex-shaped mask with a thin opaque strip at one edge
	# that fades quickly to transparent. Only covers outer EDGE_THICKNESS of hex.
	var img := Image.create(MASK_SIZE, MASK_SIZE, false, Image.FORMAT_RGBA8)

	var center := Vector2(MASK_SIZE * 0.5, MASK_SIZE * 0.5)
	var hex_radius := MASK_SIZE * 0.5

	# Edge direction angles (pointy-top hex)
	# Edge 0 (E) = 0°, Edge 1 (NE) = 60°, Edge 2 (NW) = 120°, etc.
	var edge_angle := float(edge_idx) * PI / 3.0
	var edge_dir := Vector2(cos(edge_angle), -sin(edge_angle))  # Toward that edge

	# Also need perpendicular to limit the strip width along the edge
	var edge_perp := Vector2(-edge_dir.y, edge_dir.x)

	for y in range(MASK_SIZE):
		for x in range(MASK_SIZE):
			var pos := Vector2(x, y)
			var local := pos - center

			# Check if inside hex
			if not _point_in_hex(pos, center, hex_radius):
				img.set_pixel(x, y, Color(1, 1, 1, 0))
				continue

			# Normalize local position to hex radius
			var normalized := local / hex_radius

			# Project onto edge direction (-1 = opposite edge, +1 = this edge)
			var proj := normalized.dot(edge_dir)

			# Only draw in the thin outer strip near this edge
			# proj ranges from -1 (far edge) to +1 (this edge)
			# We want alpha only when proj > (1 - EDGE_THICKNESS*2), fading to 0
			var edge_start := 1.0 - EDGE_THICKNESS * 2.0  # ~0.64 for 18%
			var alpha := 0.0

			if proj > edge_start:
				# Fade from 0 at edge_start to 1 at the edge (proj=1)
				alpha = smoothstep(edge_start, 1.0, proj)

				# Also fade out toward the corners of the edge (perpendicular falloff)
				# This makes the strip narrower at the corners
				var perp_dist := absf(normalized.dot(edge_perp))
				var corner_fade := 1.0 - smoothstep(0.4, 0.8, perp_dist)
				alpha *= corner_fade

			img.set_pixel(x, y, Color(1, 1, 1, alpha))

	return img

func _point_in_hex(point: Vector2, center: Vector2, radius: float) -> bool:
	var dx := absf(point.x - center.x)
	var dy := absf(point.y - center.y)

	# Bounding box
	if dy > radius:
		return false
	if dx > radius * SQRT3 * 0.5:
		return false

	# Hex edge check (pointy-top)
	return (radius * SQRT3 * 0.5 - dx) * 2.0 >= (dy - radius * 0.5) * SQRT3

func smoothstep(edge0: float, edge1: float, x: float) -> float:
	var t := clampf((x - edge0) / (edge1 - edge0), 0.0, 1.0)
	return t * t * (3.0 - 2.0 * t)
