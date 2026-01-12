@tool
extends SceneTree
## Run from command line:
## godot --path . --headless --script assets/terrain/TerrainTextureGenerator.gd

const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")
const SIZE := 512
const NOISE_STRENGTH := 0.08

func _init() -> void:
	print("Generating placeholder terrain textures...")
	var output_dir := "res://assets/terrain/textures/base/"

	# Ensure directory exists
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(output_dir))

	# Load terrain definitions from single source of truth
	var terrain_colors: Dictionary = TerrainDefinitions.get_colors_dict()
	var terrain_names: Dictionary = TerrainDefinitions.get_names_dict()

	for terrain_id: int in terrain_colors.keys():
		var base_color: Color = terrain_colors[terrain_id]
		var tname: String = terrain_names[terrain_id]
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var filepath := output_dir + filename

		var image := _generate_noise_texture(base_color, terrain_id)
		image = _make_seamless(image)

		var error := image.save_png(ProjectSettings.globalize_path(filepath))
		if error == OK:
			print("  Created: ", filename)
		else:
			push_error("Failed to save: %s" % filename)

	print("Done! Generated %d textures." % terrain_colors.size())
	print("These are placeholder textures - replace with AI-generated textures for production.")
	quit()

func _generate_noise_texture(base_color: Color, seed_value: int) -> Image:
	var image := Image.create(SIZE, SIZE, false, Image.FORMAT_RGB8)

	# Create noise texture
	var noise := FastNoiseLite.new()
	noise.seed = seed_value
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX
	noise.frequency = 0.01
	noise.fractal_octaves = 4

	for y in range(SIZE):
		for x in range(SIZE):
			var noise_val := noise.get_noise_2d(float(x), float(y))
			var variation := noise_val * NOISE_STRENGTH

			var r := clampf(base_color.r + variation, 0.0, 1.0)
			var g := clampf(base_color.g + variation, 0.0, 1.0)
			var b := clampf(base_color.b + variation, 0.0, 1.0)

			image.set_pixel(x, y, Color(r, g, b))

	return image

func _make_seamless(image: Image) -> Image:
	var result: Image = image.duplicate() as Image
	var blend_width: int = SIZE / 4

	# Horizontal seamless blending
	for y in range(SIZE):
		for x in range(blend_width):
			var t := float(x) / float(blend_width)
			var opposite_x: int = SIZE - blend_width + x

			var c1 := image.get_pixel(x, y)
			var c2 := image.get_pixel(opposite_x, y)
			var blended := c1.lerp(c2, 1.0 - t)
			result.set_pixel(x, y, blended)

	# Vertical seamless blending
	for x in range(SIZE):
		for y in range(blend_width):
			var t := float(y) / float(blend_width)
			var opposite_y: int = SIZE - blend_width + y

			var c1 := result.get_pixel(x, y)
			var c2 := result.get_pixel(x, opposite_y)
			var blended := c1.lerp(c2, 1.0 - t)
			result.set_pixel(x, y, blended)

	return result
