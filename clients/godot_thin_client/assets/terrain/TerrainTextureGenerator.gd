@tool
extends SceneTree
## Run from command line:
## godot --path . --headless --script assets/terrain/TerrainTextureGenerator.gd

const TERRAIN_COLORS := {
	0: Color8(11, 30, 61),      # Deep Ocean
	1: Color8(20, 64, 94),      # Continental Shelf
	2: Color8(28, 88, 114),     # Inland Sea
	3: Color8(21, 122, 115),    # Coral Shelf
	4: Color8(47, 127, 137),    # Hydrothermal Vent Field
	5: Color8(184, 176, 138),   # Tidal Flat
	6: Color8(155, 195, 123),   # River Delta
	7: Color8(79, 124, 56),     # Mangrove Swamp
	8: Color8(92, 140, 99),     # Freshwater Marsh
	9: Color8(136, 182, 90),    # Floodplain
	10: Color8(201, 176, 120),  # Alluvial Plain
	11: Color8(211, 165, 77),   # Prairie Steppe
	12: Color8(91, 127, 67),    # Mixed Woodland
	13: Color8(59, 79, 49),     # Boreal Taiga
	14: Color8(100, 85, 106),   # Peatland/Heath
	15: Color8(231, 195, 106),  # Hot Desert Erg
	16: Color8(138, 95, 60),    # Rocky Reg Desert
	17: Color8(164, 135, 85),   # Semi-Arid Scrub
	18: Color8(224, 220, 210),  # Salt Flat
	19: Color8(58, 162, 162),   # Oasis Basin
	20: Color8(166, 199, 207),  # Tundra
	21: Color8(127, 183, 161),  # Periglacial Steppe
	22: Color8(209, 228, 236),  # Glacier
	23: Color8(192, 202, 214),  # Seasonal Snowfield
	24: Color8(111, 155, 75),   # Rolling Hills
	25: Color8(150, 126, 92),   # High Plateau
	26: Color8(122, 127, 136),  # Alpine Mountain
	27: Color8(74, 106, 85),    # Karst Highland
	28: Color8(182, 101, 68),   # Canyon Badlands
	29: Color8(140, 52, 45),    # Active Volcano Slope
	30: Color8(64, 51, 61),     # Basaltic Lava Field
	31: Color8(122, 110, 104),  # Ash Plain
	32: Color8(76, 137, 145),   # Fumarole Basin
	33: Color8(91, 70, 57),     # Impact Crater Field
	34: Color8(46, 79, 92),     # Karst Cavern Mouth
	35: Color8(79, 75, 51),     # Sinkhole Field
	36: Color8(47, 143, 178),   # Aquifer Ceiling
}

const TERRAIN_NAMES := {
	0: "deep_ocean",
	1: "continental_shelf",
	2: "inland_sea",
	3: "coral_shelf",
	4: "hydrothermal_vent_field",
	5: "tidal_flat",
	6: "river_delta",
	7: "mangrove_swamp",
	8: "freshwater_marsh",
	9: "floodplain",
	10: "alluvial_plain",
	11: "prairie_steppe",
	12: "mixed_woodland",
	13: "boreal_taiga",
	14: "peat_heath",
	15: "hot_desert_erg",
	16: "rocky_reg",
	17: "semi_arid_scrub",
	18: "salt_flat",
	19: "oasis_basin",
	20: "tundra",
	21: "periglacial_steppe",
	22: "glacier",
	23: "seasonal_snowfield",
	24: "rolling_hills",
	25: "high_plateau",
	26: "alpine_mountain",
	27: "karst_highland",
	28: "canyon_badlands",
	29: "active_volcano_slope",
	30: "basaltic_lava_field",
	31: "ash_plain",
	32: "fumarole_basin",
	33: "impact_crater_field",
	34: "karst_cavern_mouth",
	35: "sinkhole_field",
	36: "aquifer_ceiling",
}

const SIZE := 512
const NOISE_STRENGTH := 0.08

func _init() -> void:
	print("Generating placeholder terrain textures...")
	var output_dir := "res://assets/terrain/textures/base/"

	# Ensure directory exists
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(output_dir))

	for terrain_id in TERRAIN_COLORS.keys():
		var base_color: Color = TERRAIN_COLORS[terrain_id]
		var tname: String = TERRAIN_NAMES[terrain_id]
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var filepath := output_dir + filename

		var image := _generate_noise_texture(base_color, terrain_id)
		image = _make_seamless(image)

		var error := image.save_png(ProjectSettings.globalize_path(filepath))
		if error == OK:
			print("  Created: ", filename)
		else:
			push_error("Failed to save: %s" % filename)

	print("Done! Generated %d textures." % TERRAIN_COLORS.size())
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
