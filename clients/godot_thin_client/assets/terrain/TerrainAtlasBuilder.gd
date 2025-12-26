@tool
extends SceneTree
## Run from command line:
## godot --path . --headless --script assets/terrain/TerrainAtlasBuilder.gd

const BASE_TEXTURES_PATH := "res://assets/terrain/textures/base/"
const OUTPUT_PATH := "res://assets/terrain/textures/terrain_atlas.res"
const TERRAIN_COUNT := 37

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

func _init() -> void:
	print("Building Texture2DArray from terrain textures...")

	# Load all images
	var images: Array[Image] = []
	var first_size: Vector2i = Vector2i.ZERO
	var missing_count := 0

	for terrain_id in range(TERRAIN_COUNT):
		var tname: String = TERRAIN_NAMES.get(terrain_id, "unknown")
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var filepath := BASE_TEXTURES_PATH + filename
		var abs_path := ProjectSettings.globalize_path(filepath)

		if not FileAccess.file_exists(abs_path):
			push_error("Missing texture: %s" % filepath)
			missing_count += 1
			# Create placeholder image
			var placeholder := Image.create(512, 512, false, Image.FORMAT_RGBA8)
			placeholder.fill(Color.MAGENTA)
			images.append(placeholder)
			continue

		var img := Image.load_from_file(abs_path)
		if img == null:
			push_error("Failed to load texture: %s" % filepath)
			missing_count += 1
			var placeholder := Image.create(512, 512, false, Image.FORMAT_RGBA8)
			placeholder.fill(Color.MAGENTA)
			images.append(placeholder)
			continue

		# Validate size
		if first_size == Vector2i.ZERO:
			first_size = Vector2i(img.get_width(), img.get_height())
		elif Vector2i(img.get_width(), img.get_height()) != first_size:
			push_warning("Texture size mismatch for %s: expected %s, got %s" % [
				filename, first_size, Vector2i(img.get_width(), img.get_height())
			])
			img.resize(first_size.x, first_size.y)

		# Ensure consistent format
		if img.get_format() != Image.FORMAT_RGBA8:
			img.convert(Image.FORMAT_RGBA8)

		images.append(img)
		print("  Loaded: %s" % filename)

	if images.size() != TERRAIN_COUNT:
		push_error("Expected %d textures, got %d" % [TERRAIN_COUNT, images.size()])
		quit(1)
		return

	# Create Texture2DArray
	var array_tex := Texture2DArray.new()
	var err := array_tex.create_from_images(images)
	if err != OK:
		push_error("Failed to create Texture2DArray: %d" % err)
		quit(1)
		return

	# Save the resource
	var save_path := ProjectSettings.globalize_path(OUTPUT_PATH)
	var save_err := ResourceSaver.save(array_tex, OUTPUT_PATH)
	if save_err != OK:
		push_error("Failed to save Texture2DArray: %d" % save_err)
		quit(1)
		return

	print("")
	print("Successfully created Texture2DArray:")
	print("  Layers: %d" % array_tex.get_layers())
	print("  Size: %dx%d" % [first_size.x, first_size.y])
	print("  Path: %s" % OUTPUT_PATH)
	if missing_count > 0:
		print("  WARNING: %d textures were missing (using magenta placeholders)" % missing_count)
	print("")
	print("To use terrain textures, ensure terrain_config.json has:")
	print('  "use_terrain_textures": true')
	quit()
