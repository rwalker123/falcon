extends Node2D
class_name MapView

const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

signal hex_selected(col: int, row: int, terrain_id: int)
signal tile_selected(info: Dictionary)
signal overlay_legend_changed(legend: Dictionary)
signal unit_selected(unit: Dictionary)
signal herd_selected(herd: Dictionary)
signal herd_follow_shortcut(herd_id: String)
signal herd_scout_shortcut(herd_id: String, col: int, row: int)
signal tile_hovered(info: Dictionary)
signal selection_cleared()
signal next_turn_requested(steps: int)
signal unit_scout_requested(x: int, y: int, band_entity_bits: int)
signal unit_found_camp_requested(x: int, y: int)
signal herd_follow_requested(herd_id: String)
signal forage_requested(x: int, y: int, module_key: String)

const LOGISTICS_COLOR := Color(0.15, 0.45, 1.0, 1.0)
const SENTIMENT_COLOR := Color(1.0, 0.35, 0.25, 1.0)
const CORRUPTION_COLOR := Color(0.92, 0.58, 0.18, 1.0)
const FOG_COLOR := Color(0.6, 0.78, 0.95, 1.0)
const CULTURE_COLOR := Color(0.72, 0.36, 0.88, 1.0)
const MILITARY_COLOR := Color(0.36, 0.7, 0.43, 1.0)
const CRISIS_COLOR := Color(0.92, 0.24, 0.46, 1.0)
const ELEVATION_LOW_COLOR := Color(0.16, 0.32, 0.78, 1.0)
const ELEVATION_MID_COLOR := Color(0.97, 0.82, 0.32, 1.0)
const ELEVATION_HIGH_COLOR := Color(0.78, 0.14, 0.18, 1.0)
const GRID_COLOR := Color(0.06, 0.08, 0.12, 1.0)
const GRID_LINE_COLOR := Color(0.1, 0.12, 0.18, 0.45)
const SQRT3 := 1.7320508075688772
const SIN_60 := 0.8660254037844386
const MIN_ZOOM_FACTOR := 1.0
const MAX_ZOOM_FACTOR := 4.0
const MOUSE_ZOOM_STEP := 0.2
const KEYBOARD_ZOOM_SPEED := 0.8
const KEYBOARD_PAN_SPEED := 600.0
const PLAYER_FACTION_ID := 0

const OVERLAY_COLORS := {
	"logistics": LOGISTICS_COLOR,
	"sentiment": SENTIMENT_COLOR,
	"corruption": CORRUPTION_COLOR,
	"fog": FOG_COLOR,
	"culture": CULTURE_COLOR,
	"military": MILITARY_COLOR,
	"crisis": CRISIS_COLOR,
	"elevation": ELEVATION_HIGH_COLOR,
	"moisture": Color(0.2, 0.65, 0.95, 1.0),
	"province": Color(0.52, 0.64, 0.78, 1.0),
}

const TERRAIN_TAG_KEYS := [
	1 << 0,  # Water
	1 << 1,  # Freshwater
	1 << 2,  # Coastal
	1 << 3,  # Wetland
	1 << 4,  # Fertile
	1 << 5,  # Arid
	1 << 6,  # Polar
	1 << 7,  # Highland
	1 << 8,  # Volcanic
	1 << 9,  # Hazardous
	1 << 10, # Subsurface
	1 << 11, # Hydrothermal
]

const TERRAIN_TAG_COLORS := {
	TERRAIN_TAG_KEYS[0]: Color8(28, 102, 189),   # Water
	TERRAIN_TAG_KEYS[1]: Color8(72, 174, 206),   # Freshwater
	TERRAIN_TAG_KEYS[2]: Color8(64, 176, 150),   # Coastal
	TERRAIN_TAG_KEYS[3]: Color8(70, 140, 96),    # Wetland
	TERRAIN_TAG_KEYS[4]: Color8(192, 198, 96),   # Fertile
	TERRAIN_TAG_KEYS[5]: Color8(210, 166, 84),   # Arid
	TERRAIN_TAG_KEYS[6]: Color8(214, 232, 246),  # Polar
	TERRAIN_TAG_KEYS[7]: Color8(136, 128, 184),  # Highland
	TERRAIN_TAG_KEYS[8]: Color8(216, 102, 72),   # Volcanic
	TERRAIN_TAG_KEYS[9]: Color8(198, 62, 132),   # Hazardous
	TERRAIN_TAG_KEYS[10]: Color8(124, 118, 150), # Subsurface
	TERRAIN_TAG_KEYS[11]: Color8(244, 156, 68),  # Hydrothermal
}

const TERRAIN_TAG_BLEND_WEIGHTS := {
	TERRAIN_TAG_KEYS[0]: 0.92,
	TERRAIN_TAG_KEYS[1]: 0.8,
	TERRAIN_TAG_KEYS[2]: 0.7,
	TERRAIN_TAG_KEYS[3]: 0.66,
	TERRAIN_TAG_KEYS[4]: 0.65,
	TERRAIN_TAG_KEYS[5]: 0.6,
	TERRAIN_TAG_KEYS[6]: 0.7,
	TERRAIN_TAG_KEYS[7]: 0.68,
	TERRAIN_TAG_KEYS[8]: 0.75,
	TERRAIN_TAG_KEYS[9]: 0.45,
	TERRAIN_TAG_KEYS[10]: 0.4,
	TERRAIN_TAG_KEYS[11]: 0.55,
}

const CRISIS_SEVERITY_COLORS := {
	"critical": Color(0.96, 0.28, 0.38, 0.95),
	"warn": Color(0.97, 0.75, 0.28, 0.92),
	"safe": Color(0.5, 0.82, 0.72, 0.85)
}

# Terrain colors and labels loaded from TerrainDefinitions (single source of truth)
var _terrain_colors: Dictionary
var _terrain_labels: Dictionary

func _get_terrain_colors() -> Dictionary:
	if _terrain_colors.is_empty():
		_terrain_colors = TerrainDefinitions.get_colors_dict()
	return _terrain_colors

func _get_terrain_labels() -> Dictionary:
	if _terrain_labels.is_empty():
		for terrain: Dictionary in TerrainDefinitions.get_terrains():
			var tid: int = int(terrain.get("id", -1))
			_terrain_labels[tid] = terrain.get("label", "Unknown")
	return _terrain_labels

const FOOD_MODULE_COLORS := {
	"coastal_littoral": Color(0.98, 0.76, 0.48, 0.9),
	"riverine_delta": Color(0.45, 0.78, 0.92, 0.9),
	"savanna_grassland": Color(0.92, 0.8, 0.52, 0.9),
	"temperate_forest": Color(0.64, 0.86, 0.58, 0.9),
	"boreal_arctic": Color(0.8, 0.88, 0.98, 0.9),
	"montane_highland": Color(0.78, 0.7, 0.9, 0.9),
	"wetland_swamp": Color(0.56, 0.76, 0.64, 0.9),
	"semi_arid_scrub": Color(0.95, 0.68, 0.44, 0.9),
	"coastal_upwelling": Color(0.6, 0.85, 0.98, 0.9),
	"mixed_woodland": Color(0.64, 0.82, 0.72, 0.9)
}

const FOOD_SITE_STYLE_DEFAULT := {
	"color": Color(0.95, 0.82, 0.5, 0.9),
	"shape": "diamond"
}

const FOOD_SITE_STYLES := {
	"littoral": {"color": Color(0.95, 0.74, 0.32, 0.9), "shape": "diamond"},
	"river_garden": {"color": Color(0.4, 0.75, 0.9, 0.9), "shape": "droplet"},
	"savanna_track": {"color": Color(0.92, 0.78, 0.4, 0.9), "shape": "triangle"},
	"forest_forage": {"color": Color(0.52, 0.78, 0.56, 0.9), "shape": "square"},
	"arctic_fishing": {"color": Color(0.78, 0.88, 0.98, 0.9), "shape": "circle"},
	"highland_grove": {"color": Color(0.78, 0.7, 0.9, 0.9), "shape": "diamond"},
	"wetland_harvest": {"color": Color(0.42, 0.66, 0.52, 0.9), "shape": "square"},
	"scrub_roots": {"color": Color(0.9, 0.6, 0.38, 0.9), "shape": "triangle"},
	"upwelling_drying": {"color": Color(0.58, 0.84, 0.94, 0.9), "shape": "droplet"},
	"woodland_cache": {"color": Color(0.6, 0.78, 0.66, 0.9), "shape": "circle"},
	"game_trail": {"color": Color(0.85, 0.5, 0.35, 0.95), "shape": "circle"}
}

const FOOD_MODULE_LABELS := {
	"coastal_littoral": "Coastal Littoral",
	"riverine_delta": "Riverine / Delta",
	"savanna_grassland": "Savanna Grassland",
	"temperate_forest": "Temperate Forest",
	"boreal_arctic": "Boreal / Arctic",
	"montane_highland": "Montane Highland",
	"wetland_swamp": "Wetland / Swamp",
	"semi_arid_scrub": "Semi-Arid Scrub",
	"coastal_upwelling": "Coastal Upwelling",
	"mixed_woodland": "Mixed Woodland",
}

const HeightfieldPreviewScene := preload("res://src/ui/HeightfieldPreview.tscn")

var grid_width: int = 0
var grid_height: int = 0
var overlay_channels: Dictionary = {}
var overlay_raw_channels: Dictionary = {}
var overlay_channel_labels: Dictionary = {}
var overlay_channel_descriptions: Dictionary = {}
var overlay_placeholder_flags: Dictionary = {}
var overlay_channel_order: PackedStringArray = PackedStringArray()
var culture_layer_map: Dictionary = {}
var active_overlay_key: String = ""
var terrain_overlay: PackedInt32Array = PackedInt32Array()
var terrain_palette: Dictionary = {}
var terrain_tags_overlay: PackedInt32Array = PackedInt32Array()
var terrain_tag_labels: Dictionary = {}
var units: Array = []
var routes: Array = []
var herds: Array = []
var herd_trails: Dictionary = {}
var food_sites: Array = []
var food_site_lookup: Dictionary = {}
var harvest_sites: Dictionary = {}
var scout_sites: Dictionary = {}
var tile_lookup: Dictionary = {}
var trade_links_overlay: Array = []
var trade_overlay_enabled: bool = false
var selected_trade_entity: int = -1
var crisis_annotations: Array = []
var hydrology_rivers: Array = []
var highlight_rivers: bool = false
var start_marker: Vector2i = Vector2i(-1, -1)

# Terrain texture system for 2D view (textures loaded via TerrainTextureManager autoload)
var _hex_texture_cache: Dictionary = {}  # terrain_id -> ImageTexture (hex-masked)
var _hex_texture_size: int = 128  # Size of cached hex textures
var _show_grid_lines: bool = true
var _terrain_grid_width: int = 0
var _terrain_grid_height: int = 0
var _cached_terrain_ids: PackedInt32Array = PackedInt32Array()
var _edge_mask_textures: Array[ImageTexture] = []  # 6 edge masks for overlay blending
var _edge_overlay_cache: Dictionary = {}  # (terrain_id, edge_idx) -> ImageTexture
var _terrain_priority: Dictionary = {}  # terrain_id -> priority (higher wins)
var culture_layer_grid: PackedInt32Array = PackedInt32Array()
var highlighted_culture_layer_ids: PackedInt32Array = PackedInt32Array()
var highlighted_culture_layer_set: Dictionary = {}
var highlighted_culture_context: String = ""

var selected_tile: Vector2i = Vector2i(-1, -1)

var last_hex_radius: float = 48.0
var last_origin: Vector2 = Vector2.ZERO
var last_map_size: Vector2 = Vector2.ZERO
var last_base_origin: Vector2 = Vector2.ZERO
var base_hex_radius: float = 1.0
var zoom_factor: float = 1.0
var pan_offset: Vector2 = Vector2.ZERO
var base_bounds: Rect2 = Rect2(Vector2.ZERO, Vector2.ONE)
var bounds_dirty: bool = true
var mouse_pan_active: bool = false
var mouse_pan_button: int = -1

var faction_colors: Dictionary = {
	"Aurora": Color(0.55, 0.85, 1.0, 1.0),
	"Obsidian": Color(0.95, 0.62, 0.2, 1.0),
	"Verdant": Color(0.4, 0.9, 0.55, 1.0),
	0: Color(0.55, 0.85, 1.0, 1.0),
	1: Color(0.95, 0.62, 0.2, 1.0),
	2: Color(0.4, 0.9, 0.55, 1.0)
}

var selected_unit_id: int = -1
var selected_herd_id: String = ""
var heightfield_data: Dictionary = {}
var biome_color_buffer: PackedColorArray = PackedColorArray()
var heightfield_preview: Control = null
var _heightfield_boot_shown: bool = true  # Default to 2D view; user can press R for 3D
var _hovered_tile: Vector2i = Vector2i(-1, -1)
var _fow_enabled: bool = false

# 2D Minimap (uses shared MinimapPanel component)
const MinimapPanelScript := preload("res://src/scripts/ui/MinimapPanel.gd")
var _minimap_2d: Node = null  # MinimapPanel instance
var _minimap_2d_image: Image = null
var _minimap_2d_last_grid_size: Vector2i = Vector2i.ZERO

func _ready() -> void:
	set_process_unhandled_input(true)
	set_process(true)
	# Use nearest-neighbor filtering to prevent seams from bilinear interpolation
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	_ensure_input_actions()
	_init_terrain_rendering()
	_setup_2d_minimap()

func display_snapshot(snapshot: Dictionary) -> Dictionary:
	print("[MapView] display_snapshot called. Keys: ", snapshot.keys())
	if snapshot.is_empty():
		return {}
	var previous_width: int = grid_width
	var previous_height: int = grid_height
	var grid: Dictionary = snapshot.get("grid", {})
	var new_width: int = int(grid.get("width", 0))
	var new_height: int = int(grid.get("height", 0))
	var dimensions_changed: bool = previous_width != new_width or previous_height != new_height
	grid_width = new_width
	grid_height = new_height

	var overlays: Dictionary = snapshot.get("overlays", {})
	_ingest_overlay_channels(overlays)
	terrain_overlay = PackedInt32Array(overlays.get("terrain", []))
	_cached_terrain_ids = terrain_overlay
	_terrain_grid_width = grid_width
	_terrain_grid_height = grid_height
	_update_biome_color_buffer()
	var palette_raw: Variant = overlays.get("terrain_palette", {})
	terrain_palette = palette_raw if typeof(palette_raw) == TYPE_DICTIONARY else {}
	terrain_tags_overlay = PackedInt32Array(overlays.get("terrain_tags", []))
	var tag_labels_raw: Variant = overlays.get("terrain_tag_labels", {})
	terrain_tag_labels = tag_labels_raw if typeof(tag_labels_raw) == TYPE_DICTIONARY else {}
	var culture_layers_variant: Variant = snapshot.get("culture_layers", null)
	if culture_layers_variant is Array:
		for layer_variant in culture_layers_variant:
			if layer_variant is Dictionary:
				var layer: Dictionary = layer_variant
				var id: int = int(layer.get("id", -1))
				if id >= 0:
					culture_layer_map[id] = layer.duplicate(true)
	var removed_layers_variant: Variant = snapshot.get("culture_layer_removed", null)
	if removed_layers_variant is Array:
		for raw_id in removed_layers_variant:
			var id := int(raw_id)
			if culture_layer_map.has(id):
				culture_layer_map.erase(id)
	crisis_annotations = []
	var crisis_annotations_variant: Variant = overlays.get("crisis_annotations", [])
	if crisis_annotations_variant is Array:
		for entry in crisis_annotations_variant:
			if entry is Dictionary:
				crisis_annotations.append((entry as Dictionary).duplicate(true))
	hydrology_rivers = []
	var rivers_variant: Variant = overlays.get("hydrology_rivers", [])
	if rivers_variant is Array:
		for entry in rivers_variant:
			if entry is Dictionary:
				hydrology_rivers.append((entry as Dictionary).duplicate(true))
	var start_marker_variant: Variant = overlays.get("start_marker", null)
	if start_marker_variant is Dictionary:
		var marker_dict: Dictionary = start_marker_variant
		start_marker = Vector2i(int(marker_dict.get("x", -1)), int(marker_dict.get("y", -1)))
	else:
		start_marker = Vector2i(-1, -1)
	var heightfield_variant: Variant = overlays.get("heightfield", {})
	if heightfield_variant is Dictionary:
		heightfield_data = (heightfield_variant as Dictionary).duplicate(true)
	else:
		heightfield_data = {}
	routes = Array(snapshot.get("orders", []))
	food_sites = []
	food_site_lookup.clear()
	harvest_sites.clear()
	scout_sites.clear()
	var food_variant: Variant = snapshot.get("food_modules", [])
	if food_variant is Array:
		for entry in food_variant:
			if not (entry is Dictionary):
				continue
			var site: Dictionary = (entry as Dictionary).duplicate(true)
			food_sites.append(site)
			var x_site: int = int(site.get("x", -1))
			var y_site: int = int(site.get("y", -1))
			if x_site >= 0 and y_site >= 0:
				food_site_lookup[Vector2i(x_site, y_site)] = site
	var population_variant: Variant = snapshot.get("populations", [])
	if population_variant is Array:
		for entry in population_variant:
			if not (entry is Dictionary):
				continue
			var cohort: Dictionary = entry
			var harvest_variant: Variant = cohort.get("harvest", {})
			if harvest_variant is Dictionary:
				var harvest: Dictionary = (harvest_variant as Dictionary).duplicate(true)
				var hx := int(harvest.get("target_x", -1))
				var hy := int(harvest.get("target_y", -1))
				if hx >= 0 and hy >= 0:
					var key := Vector2i(hx, hy)
					harvest["module_label"] = _food_module_label(String(harvest.get("module", "")))
					var existing: Array = harvest_sites.get(key, [])
					existing.append(harvest)
					harvest_sites[key] = existing
			var scout_variant: Variant = cohort.get("scout", {})
			if scout_variant is Dictionary:
				var scout: Dictionary = (scout_variant as Dictionary).duplicate(true)
				var sx := int(scout.get("target_x", -1))
				var sy := int(scout.get("target_y", -1))
				if sx >= 0 and sy >= 0:
					var scout_key := Vector2i(sx, sy)
					var scout_existing: Array = scout_sites.get(scout_key, [])
					scout_existing.append(scout)
					scout_sites[scout_key] = scout_existing

	tile_lookup.clear()
	if grid_width > 0 and grid_height > 0:
		var total: int = grid_width * grid_height
		culture_layer_grid = PackedInt32Array()
		culture_layer_grid.resize(total)
		culture_layer_grid.fill(-1)
	else:
		culture_layer_grid = PackedInt32Array()
	var tile_entries_variant: Variant = snapshot.get("tiles", [])
	if tile_entries_variant is Array:
		for entry in tile_entries_variant:
			if entry is Dictionary:
				var tile_dict: Dictionary = entry
				var entity_id: int = int(tile_dict.get("entity", -1))
				if entity_id < 0:
					continue
				var x: int = int(tile_dict.get("x", 0))
				var y: int = int(tile_dict.get("y", 0))
				tile_lookup[entity_id] = Vector2i(x, y)
				if culture_layer_grid.size() > 0:
					if x >= 0 and x < grid_width and y >= 0 and y < grid_height:
						var index: int = y * grid_width + x
						if index >= 0 and index < culture_layer_grid.size():
							culture_layer_grid[index] = int(tile_dict.get("culture_layer", -1))
	_install_province_overlay()
	_rebuild_unit_markers(snapshot)
	_rebuild_herd_markers(snapshot)
	# Removed snapshot ingest logging (noise in normal runs).

	if snapshot.has("trade_links"):
		var trade_variant: Variant = snapshot.get("trade_links")
		if trade_variant is Array:
			update_trade_overlay(trade_variant, trade_overlay_enabled)

	if dimensions_changed:
		zoom_factor = 1.0
		pan_offset = Vector2.ZERO
		mouse_pan_active = false
		mouse_pan_button = -1
	bounds_dirty = dimensions_changed

	_update_layout_metrics()
	_clamp_pan_offset()
	queue_redraw()
	_emit_overlay_legend()
	_update_2d_minimap()

	if _is_heightfield_visible():
		_push_heightfield_preview()
	else:
		_maybe_auto_show_heightfield()

	return {
		"unit_count": units.size(),
		"avg_logistics": _average_overlay("logistics"),
		"avg_sentiment": _average_overlay("sentiment"),
		"avg_corruption": _average_overlay("corruption"),
		"avg_fog": _average_overlay("fog"),
		"avg_culture": _average_overlay("culture"),
		"avg_military": _average_overlay("military"),
		"avg_crisis": _average_overlay("crisis"),
		"dimensions_changed": dimensions_changed,
		"active_overlay": active_overlay_key
	}

func _ingest_overlay_channels(overlays: Variant) -> void:
	var preserve_tag_overlay: bool = (active_overlay_key == "terrain_tags")
	overlay_channels.clear()
	overlay_raw_channels.clear()
	overlay_channel_labels.clear()
	overlay_channel_descriptions.clear()
	overlay_placeholder_flags.clear()
	overlay_channel_order = PackedStringArray()

	var overlay_dict: Dictionary = overlays if overlays is Dictionary else {}
	if overlay_dict.has("channels"):
		var channel_variant: Variant = overlay_dict["channels"]
		if channel_variant is Dictionary:
			var channel_dict: Dictionary = channel_variant
			for raw_key in channel_dict.keys():
				var key := String(raw_key)
				var info_variant: Variant = channel_dict[raw_key]
				if not (info_variant is Dictionary):
					continue
				var channel_info: Dictionary = info_variant
				overlay_channels[key] = PackedFloat32Array(channel_info.get("normalized", PackedFloat32Array()))
				overlay_raw_channels[key] = PackedFloat32Array(channel_info.get("raw", PackedFloat32Array()))
				overlay_channel_labels[key] = String(channel_info.get("label", key.capitalize()))
				overlay_channel_descriptions[key] = String(channel_info.get("description", ""))
				overlay_placeholder_flags[key] = bool(channel_info.get("placeholder", false))

	var placeholder_variant: Variant = overlay_dict.get("placeholder_channels", PackedStringArray())
	if placeholder_variant is PackedStringArray:
		var placeholder_array: PackedStringArray = placeholder_variant
		for raw_placeholder_key in placeholder_array:
			var placeholder_key := String(raw_placeholder_key)
			overlay_placeholder_flags[placeholder_key] = true

	var order_variant: Variant = overlay_dict.get("channel_order", PackedStringArray())
	overlay_channel_order = PackedStringArray()
	if order_variant is PackedStringArray:
		var order_array: PackedStringArray = order_variant
		for raw_channel_key in order_array:
			overlay_channel_order.append(String(raw_channel_key))
	if overlay_channel_order.size() == 0:
		var keys: Array = overlay_channels.keys()
		keys.sort()
		for key in keys:
			overlay_channel_order.append(String(key))

	var tag_channel_available: bool = false
	if overlays is Dictionary:
		tag_channel_available = overlays.has("terrain_tags")

	_ensure_default_overlay_channel()

	if overlay_channels.is_empty():
		active_overlay_key = ""
		return

	if preserve_tag_overlay and tag_channel_available:
		active_overlay_key = "terrain_tags"
	else:
		active_overlay_key = ""
func _draw() -> void:
	if grid_width == 0 or grid_height == 0:
		return

	_update_layout_metrics()
	_clamp_pan_offset()

	var radius: float = last_hex_radius
	var origin: Vector2 = last_origin

	# Draw background to hide any sub-pixel gaps between hexes
	var map_bounds := _compute_bounds(radius)
	map_bounds.position += origin
	draw_rect(map_bounds, Color(0.3, 0.35, 0.25, 1.0))  # Neutral earthy color

	# Determine if using textured rendering (only in base overlay mode, disabled when FoW is active)
	var mgr := TerrainTextureManager
	var use_textures := mgr.use_terrain_textures and mgr.terrain_textures != null and active_overlay_key == "" and not _fow_enabled

	# Pass 1: Draw all hex textures/colors
	for y in range(grid_height):
		for x in range(grid_width):
			var center: Vector2 = _hex_center(x, y, radius, origin)
			var is_hovered := _hovered_tile == Vector2i(x, y)

			if use_textures and not is_hovered:
				# Draw textured hex
				var terrain_id := _terrain_id_at(x, y)
				_draw_hex_textured(center, terrain_id, radius)
			else:
				# Draw solid color hex
				var final_color: Color = _tile_color(x, y)
				if is_hovered:
					final_color = final_color.darkened(0.18)
				var polygon_points := _hex_points(center, radius)
				draw_polygon(polygon_points, PackedColorArray([final_color, final_color, final_color, final_color, final_color, final_color]))

	# Pass 2: Edge blending (between hex textures and grid lines)
	if use_textures and mgr.use_edge_blending:
		_draw_terrain_edge_blending(radius, origin)

	# Pass 3: Grid lines (on top of edge blending)
	if _show_grid_lines:
		for y in range(grid_height):
			for x in range(grid_width):
				var center: Vector2 = _hex_center(x, y, radius, origin)
				draw_polyline(_hex_points(center, radius, true), GRID_LINE_COLOR, 2.0, true)

	_draw_trade_overlay(radius, origin)
	_draw_hydrology(radius, origin)
	_draw_crisis_annotations(radius, origin)
	_draw_start_marker(radius, origin)
	_draw_hydrology(radius, origin)

	for unit in units:
		_draw_unit(unit, radius, origin)

	for herd in herds:
		_draw_herd(herd, radius, origin)
	for site in food_sites:
		_draw_food_site(site, radius, origin)
		_draw_food_highlight(site, radius, origin)

	_draw_harvest_markers(radius, origin)
	_draw_scout_markers(radius, origin)

	for order in routes:
		_draw_route(order, radius, origin)

	_draw_start_marker(radius, origin)

func update_trade_overlay(trade_links: Array, enabled: bool = trade_overlay_enabled) -> void:
	trade_links_overlay = []
	if trade_links is Array:
		for entry in trade_links:
			if entry is Dictionary:
				trade_links_overlay.append((entry as Dictionary).duplicate(true))
	trade_overlay_enabled = enabled
	queue_redraw()

func set_trade_overlay_enabled(enabled: bool) -> void:
	trade_overlay_enabled = enabled
	queue_redraw()

func set_trade_overlay_selection(entity_id: int) -> void:
	selected_trade_entity = entity_id
	if trade_overlay_enabled:
		queue_redraw()

func set_culture_layer_highlight(layer_ids: PackedInt32Array, context_label: String = "") -> void:
	highlighted_culture_layer_ids = PackedInt32Array(layer_ids)
	if highlighted_culture_layer_ids.is_empty():
		highlighted_culture_context = ""
	else:
		highlighted_culture_context = context_label
	highlighted_culture_layer_set.clear()
	for id_value in highlighted_culture_layer_ids:
		highlighted_culture_layer_set[int(id_value)] = true
	queue_redraw()
	_emit_overlay_legend()

func set_overlay_channel(key: String) -> void:
	if key == "terrain_tags":
		if active_overlay_key == key:
			return
		active_overlay_key = key
		queue_redraw()
		_emit_overlay_legend()
		return
	if key == "":
		active_overlay_key = ""
		queue_redraw()
		_emit_overlay_legend()
		if _is_heightfield_visible():
			_push_heightfield_preview()
		return
	if not overlay_channels.has(key):
		return
	if active_overlay_key == key:
		return
	active_overlay_key = key
	queue_redraw()
	_emit_overlay_legend()
	if _is_heightfield_visible():
		_push_heightfield_preview()

func set_fow_enabled(enabled: bool) -> void:
	if _fow_enabled == enabled:
		return
	_fow_enabled = enabled
	# When enabling FoW, ensure we're in terrain view (no overlay)
	if _fow_enabled and active_overlay_key != "":
		active_overlay_key = ""
	queue_redraw()
	_emit_overlay_legend()
	if _is_heightfield_visible():
		_push_heightfield_preview()

func is_fow_enabled() -> bool:
	return _fow_enabled

func _is_tile_visible(x: int, y: int) -> bool:
	# Returns true if tile should show entities (Active visibility)
	# When FoW is disabled, all tiles are visible
	if not _fow_enabled:
		return true
	var vis: float = _value_at_overlay("visibility", x, y)
	return vis > 0.7  # Active tiles only

func _draw_crisis_annotations(radius: float, origin: Vector2) -> void:
	if active_overlay_key != "crisis":
		return
	if crisis_annotations.is_empty():
		return
	for entry_variant in crisis_annotations:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var severity := String(entry.get("severity", "safe"))
		var color: Color = CRISIS_SEVERITY_COLORS.get(severity, CRISIS_COLOR)
		var stroke_color: Color = color
		stroke_color.a = max(color.a, 0.9)
		var fill_color: Color = color
		fill_color.a = min(color.a, 0.45)
		var coords: Array[Vector2] = []
		var path_variant: Variant = entry.get("path", PackedInt32Array())
		if path_variant is PackedInt32Array:
			var packed: PackedInt32Array = path_variant
			var length: int = packed.size()
			if length < 2:
				continue
			for idx in range(0, length, 2):
				if idx + 1 >= length:
					break
				var col := int(packed[idx])
				var row := int(packed[idx + 1])
				coords.append(_hex_center(col, row, radius, origin))
		elif path_variant is Array:
			var arr: Array = path_variant
			if arr.is_empty():
				continue
			for step in arr:
				if step is Array and step.size() >= 2:
					var col := int(step[0])
					var row := int(step[1])
					coords.append(_hex_center(col, row, radius, origin))
		if coords.is_empty():
			continue
		var stroke_width: float = clamp(radius * 0.18, 2.0, 8.0)
		if coords.size() == 1:
			var center: Vector2 = coords[0]
			var halo_color: Color = fill_color
			halo_color.a = max(fill_color.a, 0.35)
			draw_circle(center, radius * 0.55, halo_color)
			var core_color: Color = stroke_color
			core_color.a = max(stroke_color.a, 0.85)
			draw_circle(center, radius * 0.32, core_color)
		else:
			var polyline := PackedVector2Array()
			for point in coords:
				polyline.append(point)
			draw_polyline(polyline, stroke_color, stroke_width, true)
			var head: Vector2 = coords[coords.size() - 1]
			var tail: Vector2 = coords[0]
			var head_radius: float = clamp(radius * 0.28, 4.0, 12.0)
			var tail_radius: float = clamp(radius * 0.2, 3.0, 10.0)
			draw_circle(head, head_radius, stroke_color)
			var tail_color: Color = fill_color
			tail_color.a = max(fill_color.a, 0.55)
			draw_circle(tail, tail_radius, tail_color)
		var label: String = String(entry.get("label", ""))
		if label != "":
			var anchor: Vector2 = coords[coords.size() - 1]
			var font_size: int = int(round(clamp(radius * 0.5, 14.0, 26.0)))
			_draw_label(anchor + Vector2(radius * 0.3, -radius * 0.22), label, -1.0, font_size, Color(0.95, 0.96, 0.98, 0.95))

func _draw_trade_overlay(radius: float, origin: Vector2) -> void:
	if not trade_overlay_enabled:
		return
	if trade_links_overlay.is_empty():
		return
	if tile_lookup.is_empty():
		return

	for entry in trade_links_overlay:
		if not (entry is Dictionary):
			continue
		var link: Dictionary = entry
		var from_tile: int = int(link.get("from_tile", -1))
		var to_tile: int = int(link.get("to_tile", -1))
		if not tile_lookup.has(from_tile) or not tile_lookup.has(to_tile):
			continue
		var from_pos: Vector2i = tile_lookup[from_tile]
		var to_pos: Vector2i = tile_lookup[to_tile]
		var start: Vector2 = _hex_center(from_pos.x, from_pos.y, radius, origin)
		var end: Vector2 = _hex_center(to_pos.x, to_pos.y, radius, origin)
		var knowledge_variant: Variant = link.get("knowledge", {})
		var openness: float = 0.0
		var leak_timer: int = 0
		if knowledge_variant is Dictionary:
			var knowledge_dict: Dictionary = knowledge_variant
			openness = float(knowledge_dict.get("openness", 0.0))
			leak_timer = int(knowledge_dict.get("leak_timer", 0))
		var throughput: float = float(link.get("throughput", 0.0))
		var intensity: float = clamp(abs(throughput) * 0.25, 0.0, 2.5)
		var opacity: float = clamp(0.25 + openness * 0.6, 0.3, 0.95)
		var base_color: Color = Color(0.95, 0.74, 0.22, opacity)
		var width: float = 2.0 + intensity
		var entity_id: int = int(link.get("entity", -1))
		if entity_id == selected_trade_entity:
			base_color = Color(0.3, 0.95, 0.7, 0.95)
			width += 2.0

		draw_line(start, end, base_color, width)

		if leak_timer <= 1:
			var midpoint: Vector2 = start.lerp(end, 0.5)
			draw_circle(midpoint, 4.5, Color(1.0, 0.35, 0.28, 0.85))

func _unhandled_input(event: InputEvent) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if event.is_action_pressed("map_toggle_relief"):
		_toggle_heightfield_preview()
		_mark_input_handled()
		return
	if event.is_action_pressed("map_switch_strategic_view"):
		if _is_heightfield_visible():
			var preview := _ensure_heightfield_preview()
			if preview.has_method("hide_preview"):
				preview.call("hide_preview")
			else:
				preview.hide()
			_mark_input_handled()
		return
	if event is InputEventKey and event.pressed and event.keycode == KEY_C:
		_fit_map_to_view()
		_mark_input_handled()
		return
	if event is InputEventKey and event.pressed and event.keycode == KEY_G:
		_show_grid_lines = not _show_grid_lines
		queue_redraw()
		_mark_input_handled()
		return
	if event is InputEventMouseButton:
		var mouse_event: InputEventMouseButton = event
		if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
			_apply_zoom(MOUSE_ZOOM_STEP, get_local_mouse_position())
			_mark_input_handled()
			return
		elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
			_apply_zoom(-MOUSE_ZOOM_STEP, get_local_mouse_position())
			_mark_input_handled()
			return
		elif (mouse_event.button_index == MOUSE_BUTTON_MIDDLE or mouse_event.button_index == MOUSE_BUTTON_RIGHT):
			if mouse_event.pressed:
				_begin_mouse_pan(mouse_event.button_index)
			else:
				_end_mouse_pan(mouse_event.button_index)
			_mark_input_handled()
			return
		elif mouse_event.button_index == MOUSE_BUTTON_LEFT and mouse_event.pressed:
			var local_position: Vector2 = get_local_mouse_position()
			_update_layout_metrics()
			var offset := _point_to_offset(local_position)
			var col: int = offset.x
			var row: int = offset.y
			handle_hex_click(col, row, mouse_event.button_index)
			var herd_hit: Dictionary = _herd_at_point(local_position)
			if mouse_event.double_click and not herd_hit.is_empty():
				var shortcut_id := String(herd_hit.get("id", ""))
				var herd_col := int(herd_hit.get("x", col))
				var herd_row := int(herd_hit.get("y", row))
				if shortcut_id != "":
					if mouse_event.shift_pressed:
						emit_signal("herd_scout_shortcut", shortcut_id, herd_col, herd_row)
					else:
						emit_signal("herd_follow_shortcut", shortcut_id)
			_mark_input_handled()
			return
	elif event is InputEventMouseMotion:
		var motion: InputEventMouseMotion = event
		if mouse_pan_active:
			_apply_pan(motion.relative)
			_mark_input_handled()
		else:
			var local_pos: Vector2 = get_local_mouse_position()
			_update_layout_metrics()
			var offset := _point_to_offset(local_pos)
			if offset != _hovered_tile:
				_hovered_tile = offset
				if offset.x < 0 or offset.y < 0:
					emit_signal("tile_hovered", {})
				elif _fow_enabled and not _is_tile_visible(offset.x, offset.y):
					# Unexplored tiles: no tooltip when FoW is enabled
					var vis: float = _value_at_overlay("visibility", offset.x, offset.y)
					if vis <= 0.3:
						emit_signal("tile_hovered", {})
					else:
						# Discovered tiles: show basic terrain info only
						var info := _tile_info_at(offset.x, offset.y)
						info.erase("food_module")
						info.erase("food_module_label")
						info.erase("food_kind")
						info.erase("units")
						info.erase("herds")
						info.erase("unit_count")
						info.erase("herd_count")
						emit_signal("tile_hovered", info)
				else:
					var info := _tile_info_at(offset.x, offset.y)
					emit_signal("tile_hovered", info)
				queue_redraw()
	elif event is InputEventPanGesture:
		var gesture: InputEventPanGesture = event
		if gesture.alt_pressed:
			return
		_apply_pan(-gesture.delta)
		_mark_input_handled()
	elif event is InputEventMagnifyGesture:
		var magnify: InputEventMagnifyGesture = event
		var amount: float = (magnify.factor - 1.0) * KEYBOARD_ZOOM_SPEED
		if not is_zero_approx(amount):
			_apply_zoom(amount, get_local_mouse_position())
			_mark_input_handled()

func _draw_unit(unit: Dictionary, radius: float, origin: Vector2) -> void:
	var position: Array = Array(unit.get("pos", [0, 0]))
	if position.size() != 2:
		return
	var center: Vector2 = _hex_center(int(position[0]), int(position[1]), radius, origin)
	var marker_radius: float = radius * 0.45
	var color: Color = faction_colors.get(unit.get("faction", ""), Color(0.9, 0.9, 0.9, 1.0))
	draw_circle(center, marker_radius, color)
	draw_arc(center, marker_radius, 0, TAU, 12, Color(0, 0, 0, 0.4), 2.5)

	var label: String = str(unit.get("id", ""))
	if label != "":
		_draw_label(center + Vector2(-marker_radius, marker_radius * 0.1), label, marker_radius * 2.0, 16, Color(0.05, 0.05, 0.05, 0.85))

	# Draw task arrow if unit has active assignment
	var dest_x: int = int(unit.get("dest_x", -1))
	var dest_y: int = int(unit.get("dest_y", -1))
	if dest_x >= 0 and dest_y >= 0:
		var dest_center: Vector2 = _hex_center(dest_x, dest_y, radius, origin)
		var task_kind: String = String(unit.get("travel_task_kind", ""))
		var arrow_color: Color = _travel_arrow_color(task_kind)
		# Only draw arrow if unit is not already at destination
		var pos_x: int = int(position[0])
		var pos_y: int = int(position[1])
		if pos_x != dest_x or pos_y != dest_y:
			draw_line(center, dest_center, arrow_color, 2.5)
			_draw_arrowhead(center, dest_center, arrow_color)

	if int(unit.get("entity", -1)) == selected_unit_id:
		var highlight_color := Color(1.0, 1.0, 1.0, 0.9)
		draw_arc(center, marker_radius + 4.0, 0, TAU, 24, highlight_color, 3.0)

func _travel_arrow_color(task_kind: String) -> Color:
	match task_kind:
		"harvest":
			return Color(0.3, 0.8, 0.3, 0.85)  # Green
		"hunt":
			return Color(0.8, 0.3, 0.3, 0.85)  # Red
		"scout":
			return Color(0.3, 0.6, 0.9, 0.85)  # Blue
		_:
			return Color(0.7, 0.7, 0.7, 0.85)  # Gray

func _draw_label(pos: Vector2, text: String, max_width: float, font_size: int, color: Color) -> void:
	var font: Font = ThemeDB.fallback_font
	if font != null:
		draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, max_width, font_size, color)

func _draw_herd(herd: Dictionary, radius: float, origin: Vector2) -> void:
	var herd_id := String(herd.get("id", ""))
	var x: int = int(herd.get("x", -1))
	var y: int = int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _is_tile_visible(x, y):
		return
	var center: Vector2 = _hex_center(x, y, radius, origin)
	_draw_herd_trail(herd_id, radius, origin)
	var marker_radius: float = radius * 0.35
	var base_color := Color(0.95, 0.76, 0.35, 0.95)
	var points := PackedVector2Array([
		center + Vector2(0, -marker_radius),
		center + Vector2(marker_radius * 0.85, 0),
		center + Vector2(0, marker_radius),
		center + Vector2(-marker_radius * 0.85, 0)
	])
	draw_polygon(points, PackedColorArray([base_color, base_color, base_color, base_color]))
	draw_polyline(points, Color(0, 0, 0, 0.4), 2.0, true)

	var label: String = String(herd.get("label", herd.get("id", "Herd")))
	if label != "":
		_draw_label(center + Vector2(-marker_radius, marker_radius + 4.0), label, marker_radius * 2.0, 14, Color(0.1, 0.1, 0.1, 0.85))

	var next_x := int(herd.get("next_x", -1))
	var next_y := int(herd.get("next_y", -1))
	if next_x >= 0 and next_y >= 0:
		var next_center := _hex_center(next_x, next_y, radius, origin)
		draw_line(center, next_center, Color(0.98, 0.58, 0.18, 0.85), 3.0)
		_draw_arrowhead(center, next_center, Color(0.98, 0.58, 0.18, 0.85))

	if herd_id == selected_herd_id:
		draw_arc(center, marker_radius + 3.0, 0, TAU, 24, Color(1.0, 1.0, 1.0, 0.9), 2.5)

func _draw_food_site(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _is_tile_visible(x, y):
		return
	var center: Vector2 = _hex_center(x, y, radius, origin)
	var module_key := String(site.get("module", ""))
	var kind := String(site.get("kind", ""))
	var style: Dictionary = FOOD_SITE_STYLES.get(kind, FOOD_SITE_STYLE_DEFAULT)
	var color: Color = style.get("color", FOOD_SITE_STYLE_DEFAULT["color"])
	var weight: float = float(site.get("seasonal_weight", 1.0))
	var marker_radius: float = radius * 0.2 + weight * 2.0
	var shape := String(style.get("shape", FOOD_SITE_STYLE_DEFAULT["shape"]))
	match shape:
		"circle":
			draw_circle(center, marker_radius * 0.9, color)
		"triangle":
			var tri := PackedVector2Array([
				center + Vector2(0, -marker_radius),
				center + Vector2(marker_radius, marker_radius),
				center + Vector2(-marker_radius, marker_radius)
			])
			draw_polygon(tri, PackedColorArray([color, color, color]))
			draw_polyline(tri, Color(0, 0, 0, 0.4), 1.75, true)
		"droplet":
			draw_circle(center, marker_radius * 0.65, color)
			var tip := PackedVector2Array([
				center + Vector2(0, -marker_radius),
				center + Vector2(marker_radius * 0.35, -marker_radius * 0.2),
				center + Vector2(-marker_radius * 0.35, -marker_radius * 0.2)
			])
			draw_polygon(tip, PackedColorArray([color, color, color]))
			draw_polyline(tip, Color(0, 0, 0, 0.35), 1.25, true)
		"square":
			var square := PackedVector2Array([
				center + Vector2(-marker_radius, -marker_radius),
				center + Vector2(marker_radius, -marker_radius),
				center + Vector2(marker_radius, marker_radius),
				center + Vector2(-marker_radius, marker_radius)
			])
			draw_polygon(square, PackedColorArray([color, color, color, color]))
			draw_polyline(square, Color(0, 0, 0, 0.4), 1.5, true)
		_:
			var points := PackedVector2Array([
				center + Vector2(0, -marker_radius),
				center + Vector2(marker_radius, 0),
				center + Vector2(0, marker_radius),
				center + Vector2(-marker_radius, 0)
			])
			draw_polygon(points, PackedColorArray([color, color, color, color]))
			draw_polyline(points, Color(0, 0, 0, 0.4), 1.75, true)
	if _food_harvest_active(int(site.get("x", -1)), int(site.get("y", -1))):
		var halo_color := color
		halo_color.a = 0.25
		draw_circle(center, marker_radius * 1.8, halo_color)
		var stroke_color := color
		stroke_color.a = 0.95
		draw_arc(center, marker_radius * 1.4, 0, TAU, 32, stroke_color, 2.0)

func _draw_food_highlight(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _is_tile_visible(x, y):
		return
	var module_key := String(site.get("module", ""))
	if module_key == "":
		return
	if _selected_tile_matches_food(x, y, module_key):
		var center := _hex_center(x, y, radius, origin)
		var highlight_color := Color(1.0, 1.0, 1.0, 0.85)
		draw_arc(center, radius * 0.5, 0, TAU, 32, highlight_color, 2.0)

func _draw_harvest_markers(radius: float, origin: Vector2) -> void:
	if harvest_sites.is_empty():
		return
	for key in harvest_sites.keys():
		var entries_variant: Variant = harvest_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _hex_center(key.x, key.y, radius, origin)
		var module_key := String((entries[0] as Dictionary).get("module", ""))
		var style: Dictionary = FOOD_SITE_STYLE_DEFAULT
		var base_site: Variant = food_site_lookup.get(key, null)
		if base_site is Dictionary:
			var kind := String((base_site as Dictionary).get("kind", ""))
			style = FOOD_SITE_STYLES.get(kind, FOOD_SITE_STYLE_DEFAULT)
		var color: Color = style.get("color", FOOD_SITE_STYLE_DEFAULT["color"])
		var glow_color := color
		glow_color.a = 0.25
		draw_circle(center, radius * 0.65, glow_color)
		var stroke_color := color
		stroke_color.a = 0.95
		draw_arc(center, radius * 0.55, 0, TAU, 32, stroke_color, 3.0)
		if entries.size() > 1:
			var label := "x%d" % entries.size()
			_draw_label(center + Vector2(-radius * 0.25, radius * 0.05), label, radius * 0.6, int(radius * 0.4), Color(0, 0, 0, 0.85))
		if not (base_site is Dictionary) and _selected_tile_matches_food(key.x, key.y, module_key):
			var highlight_color := Color(1.0, 1.0, 1.0, 0.9)
			draw_arc(center, radius * 0.45, 0, TAU, 32, highlight_color, 2.5)

func _draw_scout_markers(radius: float, origin: Vector2) -> void:
	if scout_sites.is_empty():
		return
	for key in scout_sites.keys():
		var entries_variant: Variant = scout_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _hex_center(key.x, key.y, radius, origin)
		var base_color := Color(0.8, 0.92, 1.0, 0.4)
		draw_circle(center, radius * 0.4, base_color)
		var stroke_color := Color(0.9, 0.97, 1.0, 0.95)
		draw_arc(center, radius * 0.5, 0, TAU, 24, stroke_color, 2.0)

func _draw_route(order: Dictionary, radius: float, origin: Vector2) -> void:
	var path: Array = order.get("path", [])
	if path.is_empty():
		return
	var color: Color = faction_colors.get(order.get("faction", ""), Color(0.95, 0.9, 0.6, 0.8))
	var points: Array[Vector2] = []
	for waypoint in path:
		if waypoint.size() != 2:
			continue
		points.append(_hex_center(int(waypoint[0]), int(waypoint[1]), radius, origin))
	if points.size() < 2:
		return
	for i in range(points.size() - 1):
		draw_line(points[i], points[i + 1], color, 3.0)

func _overlay_array(key: String) -> PackedFloat32Array:
	var variant: Variant = overlay_channels.get(key, null)
	if variant is PackedFloat32Array:
		return variant
	return PackedFloat32Array()

func _overlay_raw_array(key: String) -> PackedFloat32Array:
	var variant: Variant = overlay_raw_channels.get(key, null)
	if variant is PackedFloat32Array:
		return variant
	return PackedFloat32Array()

func _average_overlay(key: String) -> float:
	return _average(_overlay_raw_array(key))

func _value_at_overlay(key: String, x: int, y: int) -> float:
	return _value_at(_overlay_array(key), x, y)

func _value_at(data: PackedFloat32Array, x: int, y: int) -> float:
	if data.is_empty() or grid_width == 0:
		return 0.0
	var index: int = y * grid_width + x
	if index < 0 or index >= data.size():
		return 0.0
	return clamp(float(data[index]), 0.0, 1.0)

func _terrain_id_at(x: int, y: int) -> int:
	if terrain_overlay.is_empty() or grid_width == 0:
		return -1
	var index: int = y * grid_width + x
	if index < 0 or index >= terrain_overlay.size():
		return -1
	return int(terrain_overlay[index])

func _rebuild_unit_markers(snapshot: Dictionary) -> void:
	units = []
	var population_variant: Variant = snapshot.get("populations", [])
	if not (population_variant is Array):
		return
	var counter := 1
	var label_cache: Dictionary = {}
	for entry_variant in population_variant:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant

		# Use current position if available, otherwise fall back to home tile lookup
		var current_x: int = int(entry.get("current_x", -1))
		var current_y: int = int(entry.get("current_y", -1))
		var is_traveling: bool = bool(entry.get("is_traveling", false))

		if current_x < 0 or current_y < 0:
			# Fall back to home tile lookup
			var home_id: int = int(entry.get("home", -1))
			if home_id < 0 or not tile_lookup.has(home_id):
				continue
			var coords: Vector2i = tile_lookup[home_id]
			current_x = coords.x
			current_y = coords.y

		var label: String = String(entry.get("label", ""))
		if label == "":
			label = "Band %d" % counter
		while label_cache.has(label):
			counter += 1
			label = "Band %d" % counter
		label_cache[label] = true
		var marker := {
			"entity": int(entry.get("entity", -1)),
			"faction": entry.get("faction", PLAYER_FACTION_ID),
			"pos": [current_x, current_y],
			"size": int(entry.get("size", 0)),
			"id": label,
			"is_traveling": is_traveling
		}

		# Add destination info for units with active assignments
		var harvest_variant: Variant = entry.get("harvest", {})
		if harvest_variant is Dictionary:
			var harvest: Dictionary = harvest_variant as Dictionary
			marker["harvest"] = harvest.duplicate(true)
			marker["dest_x"] = int(harvest.get("target_x", -1))
			marker["dest_y"] = int(harvest.get("target_y", -1))
			marker["travel_task_kind"] = String(harvest.get("kind", "harvest"))
		var scout_variant: Variant = entry.get("scout", {})
		if scout_variant is Dictionary:
			var scout: Dictionary = scout_variant as Dictionary
			marker["scout"] = scout.duplicate(true)
			if not marker.has("dest_x") or int(marker.get("dest_x", -1)) < 0:
				marker["dest_x"] = int(scout.get("target_x", -1))
				marker["dest_y"] = int(scout.get("target_y", -1))
				marker["travel_task_kind"] = "scout"
		var stockpile_variant: Variant = entry.get("accessible_stockpile", {})
		if stockpile_variant is Dictionary:
			marker["accessible_stockpile"] = (stockpile_variant as Dictionary).duplicate(true)
		units.append(marker)
		counter += 1

func _rebuild_herd_markers(snapshot: Dictionary) -> void:
	herds = []
	var herd_variant: Variant = snapshot.get("herds", [])
	if not (herd_variant is Array):
		herd_trails.clear()
		return
	var active_ids := {}
	for entry in herd_variant:
		if entry is Dictionary:
			var herd_dict: Dictionary = (entry as Dictionary).duplicate(true)
			herds.append(herd_dict)
			var herd_id := String(herd_dict.get("id", ""))
			if herd_id != "":
				active_ids[herd_id] = true
				_update_herd_trail(herd_id, herd_dict)
	var stale_ids := herd_trails.keys()
	for herd_id in stale_ids:
		if not active_ids.has(herd_id):
			herd_trails.erase(herd_id)

func _handle_entity_selection(col: int, row: int) -> void:
	# Check for units on this tile
	var units_here := _units_on_tile(col, row)
	if not units_here.is_empty():
		var unit: Dictionary = units_here[0]
		selected_unit_id = int(unit.get("entity", -1))
		selected_herd_id = ""
		var unit_payload: Dictionary = (unit as Dictionary).duplicate(true)
		var pos := Array(unit_payload.get("pos", []))
		var unit_col := col
		var unit_row := row
		if pos.size() == 2:
			unit_col = int(pos[0])
			unit_row = int(pos[1])
		unit_payload["tile_info"] = _tile_info_at(unit_col, unit_row)
		emit_signal("unit_selected", unit_payload)
		queue_redraw()
		if _is_heightfield_visible():
			heightfield_preview.call("update_selection", unit_payload.get("tile_info", {}), unit_payload, {})
		return

	# Check for herds on this tile
	var herds_here := _herds_on_tile(col, row)
	if not herds_here.is_empty():
		var herd: Dictionary = herds_here[0]
		selected_unit_id = -1
		selected_herd_id = String(herd.get("id", ""))
		var herd_payload: Dictionary = (herd as Dictionary).duplicate(true)
		var herd_col: int = int(herd_payload.get("x", col))
		var herd_row: int = int(herd_payload.get("y", row))
		herd_payload["tile_info"] = _tile_info_at(herd_col, herd_row)
		emit_signal("herd_selected", herd_payload)
		queue_redraw()
		if _is_heightfield_visible():
			heightfield_preview.call("update_selection", herd_payload.get("tile_info", {}), {}, herd_payload)
		return
	if selected_unit_id != -1 or selected_herd_id != "":
		selected_unit_id = -1
		selected_herd_id = ""
		emit_signal("selection_cleared")
		selected_tile = Vector2i(-1, -1)
		queue_redraw()
		if _is_heightfield_visible():
			heightfield_preview.call("update_selection", {}, {}, {})

func _update_herd_trail(herd_id: String, herd: Dictionary) -> void:
	if herd_id == "":
		return
	var x := int(herd.get("x", -1))
	var y := int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	var current := Vector2i(x, y)
	var trail: Array = herd_trails.get(herd_id, [])
	if trail.is_empty() or trail[trail.size() - 1] != current:
		trail.append(current)
	var max_len := int(herd.get("route_length", trail.size()))
	if max_len > 0:
		while trail.size() > max_len:
			trail.remove_at(0)
	herd_trails[herd_id] = trail

func _draw_herd_trail(herd_id: String, radius: float, origin: Vector2) -> void:
	if herd_id == "":
		return
	if not herd_trails.has(herd_id):
		return
	var trail: Array = herd_trails[herd_id]
	if trail.size() < 2:
		return
	var points := PackedVector2Array()
	for tile in trail:
		if tile is Vector2i:
			points.append(_hex_center(tile.x, tile.y, radius, origin))
	if points.size() >= 2:
		draw_polyline(points, Color(0.97, 0.69, 0.25, 0.6), 2.0)

func _draw_arrowhead(start: Vector2, end: Vector2, color: Color, size: float = 8.0) -> void:
	var direction := end - start
	if direction.length() <= 0.1:
		return
	var norm := direction.normalized()
	var ortho := Vector2(-norm.y, norm.x)
	var tip := end
	var base_point := tip - norm * size
	var left := base_point + ortho * (size * 0.5)
	var right := base_point - ortho * (size * 0.5)
	var pts := PackedVector2Array([tip, left, right])
	draw_polygon(pts, PackedColorArray([color, color, color]))

func _emit_tile_selection(col: int, row: int) -> void:
	if col < 0 or row < 0 or col >= grid_width or row >= grid_height:
		return
	selected_tile = Vector2i(col, row)
	var info := _tile_info_at(col, row)
	emit_signal("tile_selected", info)
	queue_redraw()
	if _is_heightfield_visible():
		heightfield_preview.call("update_selection", info, {}, {})

func _unit_at_point(point: Vector2) -> Dictionary:
	for unit in units:
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		var center := _hex_center(int(position[0]), int(position[1]), last_hex_radius, last_origin)
		if center.distance_to(point) <= last_hex_radius * 0.55:
			return unit
	return {}

func _herd_at_point(point: Vector2) -> Dictionary:
	for herd in herds:
		var x := int(herd.get("x", -1))
		var y := int(herd.get("y", -1))
		if x < 0 or y < 0:
			continue
		var center := _hex_center(x, y, last_hex_radius, last_origin)
		if center.distance_to(point) <= last_hex_radius * 0.45:
			return herd
	return {}

func _tile_info_at(col: int, row: int) -> Dictionary:
	var info: Dictionary = {
		"x": col,
		"y": row,
	}
	if col < 0 or row < 0 or col >= grid_width or row >= grid_height:
		return info
	var terrain_id := _terrain_id_at(col, row)
	info["terrain_id"] = terrain_id
	info["terrain_label"] = String(_get_terrain_labels().get(terrain_id, "Terrain %d" % terrain_id))
	var mask := _tag_mask_at(col, row)
	info["tags_mask"] = mask
	var tag_labels := _tag_names_for_mask(mask)
	info["tag_labels"] = tag_labels
	var tags_text := "none"
	if not tag_labels.is_empty():
		tags_text = ", ".join(tag_labels)
	info["tags_text"] = tags_text
	var module_entry := _food_module_entry_at(col, row)
	var module_key := ""
	var module_weight := 0.0
	if not module_entry.is_empty():
		module_key = String(module_entry.get("module", ""))
		module_weight = float(module_entry.get("seasonal_weight", 0.0))
		var kind := String(module_entry.get("kind", ""))
		if kind != "":
			info["food_kind"] = kind
	info["food_module"] = module_key
	info["food_module_label"] = _food_module_label(module_key)
	info["food_module_weight"] = module_weight
	var units_here := _units_on_tile(col, row)
	var herds_here := _herds_on_tile(col, row)
	info["units"] = units_here
	info["herds"] = herds_here
	info["unit_count"] = units_here.size()
	info["herd_count"] = herds_here.size()
	var harvest_here: Variant = harvest_sites.get(Vector2i(col, row), null)
	if harvest_here is Array and not harvest_here.is_empty():
		var harvest_array: Array = []
		for entry in harvest_here:
			if entry is Dictionary:
				harvest_array.append((entry as Dictionary).duplicate(true))
		info["harvest_tasks"] = harvest_array
		info["harvest_active"] = harvest_array.size()
	var scout_here: Variant = scout_sites.get(Vector2i(col, row), null)
	if scout_here is Array and not scout_here.is_empty():
		var scout_array: Array = []
		for entry in scout_here:
			if entry is Dictionary:
				scout_array.append((entry as Dictionary).duplicate(true))
		info["scout_tasks"] = scout_array
		info["scout_active"] = scout_array.size()
	var nearest_unit := _nearest_unit_sample(col, row)
	if not nearest_unit.is_empty():
		info["nearest_unit_distance"] = nearest_unit.get("distance", -1)
		info["nearest_unit_label"] = nearest_unit.get("label", "")
		info["nearest_unit_id"] = nearest_unit.get("id", "")
	return info

func _units_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
	for unit in units:
		var position: Array = Array(unit.get("pos", []))
		if position.size() != 2:
			continue
		if int(position[0]) == col and int(position[1]) == row:
			matches.append((unit as Dictionary).duplicate(true))
	return matches

func _herds_on_tile(col: int, row: int) -> Array:
	var matches: Array = []
	for herd in herds:
		var x := int(herd.get("x", -1))
		var y := int(herd.get("y", -1))
		if x == col and y == row:
			matches.append((herd as Dictionary).duplicate(true))
	return matches

func _nearest_unit_sample(col: int, row: int) -> Dictionary:
	if units.is_empty():
		return {}
	var best_distance: int = -1
	var best_unit: Dictionary = {}
	for entry in units:
		if not (entry is Dictionary):
			continue
		var pos_array: Array = Array(entry.get("pos", []))
		if pos_array.size() != 2:
			continue
		var ux := int(pos_array[0])
		var uy := int(pos_array[1])
		var distance: int = abs(col - ux) + abs(row - uy)
		if distance < 0:
			continue
		if best_distance < 0 or distance < best_distance:
			best_distance = distance
			best_unit = entry
	if best_distance < 0 or best_unit.is_empty():
		return {}
	var summary := {
		"distance": best_distance,
		"label": String(best_unit.get("id", best_unit.get("entity", "Band"))),
		"id": best_unit.get("entity", best_unit.get("id", "")),
	}
	return summary

func _food_module_entry_at(col: int, row: int) -> Dictionary:
	var key := Vector2i(col, row)
	if food_site_lookup.has(key):
		return (food_site_lookup[key] as Dictionary).duplicate(true)
	return {}

func _food_harvest_active(col: int, row: int) -> bool:
	return harvest_sites.has(Vector2i(col, row))

func _selected_tile_matches_food(col: int, row: int, module_key: String) -> bool:
	if module_key == "":
		return false
	return selected_tile.x == col and selected_tile.y == row

func _tag_names_for_mask(mask: int) -> PackedStringArray:
	var names := PackedStringArray()
	if mask == 0:
		return names
	for raw_bit in TERRAIN_TAG_KEYS:
		var bit: int = int(raw_bit)
		if (mask & bit) == 0:
			continue
		var label_value: Variant = terrain_tag_labels.get(bit, "")
		var label := String(label_value)
		if label == "":
			label = _tag_label_for_mask(bit)
		names.append(label)
	return names

func _food_module_label(module_key: String) -> String:
	if module_key == "":
		return "None"
	return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

func _culture_layer_at(x: int, y: int) -> int:
	if culture_layer_grid.is_empty() or grid_width == 0:
		return -1
	var index: int = y * grid_width + x
	if index < 0 or index >= culture_layer_grid.size():
		return -1
	return int(culture_layer_grid[index])

func _is_culture_layer_highlighted(layer_id: int) -> bool:
	if highlighted_culture_layer_set.is_empty():
		return true
	return highlighted_culture_layer_set.has(layer_id)

func _elevation_color(value: float) -> Color:
	var t: float = clampf(value, 0.0, 1.0)
	if t <= 0.5:
		return ELEVATION_LOW_COLOR.lerp(ELEVATION_MID_COLOR, t * 2.0)
	return ELEVATION_MID_COLOR.lerp(ELEVATION_HIGH_COLOR, (t - 0.5) * 2.0)

func _desaturate_color(c: Color, factor: float) -> Color:
	# Convert to grayscale luminance and blend back
	var gray: float = c.r * 0.299 + c.g * 0.587 + c.b * 0.114
	return Color(
		lerpf(c.r, gray, factor),
		lerpf(c.g, gray, factor),
		lerpf(c.b, gray, factor),
		c.a
	)

func _tile_color(x: int, y: int) -> Color:
	if active_overlay_key == "":
		var terrain_id := _terrain_id_at(x, y)
		var base_color: Color = GRID_COLOR
		if terrain_id >= 0:
			base_color = _terrain_color_for_id(terrain_id)
		# Apply Fog of War modifiers if enabled
		# Visibility values: Active ≈ 1.0, Discovered ≈ 0.5, Unexplored ≈ 0.0
		if _fow_enabled:
			var vis: float = _value_at_overlay("visibility", x, y)
			if vis > 0.7:  # Active - full terrain color
				return base_color
			elif vis > 0.3:  # Discovered - desaturated terrain
				return _desaturate_color(base_color, 0.65).darkened(0.25)
			else:  # Unexplored - black
				return Color.BLACK
		return base_color
	if active_overlay_key == "terrain_tags":
		var mask := _tag_mask_at(x, y)
		if mask == 0:
			return GRID_COLOR
		var tag_color: Color = _tag_color_for_mask(mask)
		return GRID_COLOR.lerp(tag_color, 0.92)
	var overlay_value: float = _value_at_overlay(active_overlay_key, x, y)
	var overlay_color: Color = OVERLAY_COLORS.get(active_overlay_key, LOGISTICS_COLOR)
	if active_overlay_key == "culture" and not highlighted_culture_layer_set.is_empty():
		var layer_id: int = _culture_layer_at(x, y)
		if not _is_culture_layer_highlighted(layer_id):
			overlay_value *= 0.15
			var muted := GRID_COLOR.lerp(overlay_color, overlay_value)
			return muted.darkened(0.35)
		var highlighted := GRID_COLOR.lerp(overlay_color, overlay_value)
		return highlighted.lightened(0.12)
	if active_overlay_key == "elevation":
		var gradient_color: Color = _elevation_color(overlay_value)
		var blend: float = clampf(overlay_value * 0.85 + 0.15, 0.0, 1.0)
		return GRID_COLOR.lerp(gradient_color, blend)
	return GRID_COLOR.lerp(overlay_color, overlay_value)

func _terrain_color_for_id(terrain_id: int) -> Color:
	var colors := _get_terrain_colors()
	if colors.has(terrain_id):
		return colors[terrain_id]
	return Color(0.2, 0.2, 0.2, 1.0)

func _update_biome_color_buffer() -> void:
	if grid_width <= 0 or grid_height <= 0 or terrain_overlay.is_empty():
		biome_color_buffer = PackedColorArray()
		return
	var total: int = grid_width * grid_height
	biome_color_buffer = PackedColorArray()
	biome_color_buffer.resize(total)
	for idx in range(total):
		var terrain_id := 0
		if idx < terrain_overlay.size():
			terrain_id = int(terrain_overlay[idx])
		biome_color_buffer[idx] = _terrain_color_for_id(terrain_id)

func _ensure_heightfield_preview() -> Control:
	if heightfield_preview == null or not is_instance_valid(heightfield_preview):
		# Create a CanvasLayer to hold the preview overlay
		var layer = CanvasLayer.new()
		layer.layer = 100 # High layer to be on top
		layer.name = "HeightfieldPreviewLayer"
		get_tree().root.add_child(layer)

		heightfield_preview = HeightfieldPreviewScene.instantiate()
		heightfield_preview.hide()
		layer.add_child(heightfield_preview)
		if heightfield_preview.has_signal("strategic_view_requested"):
			heightfield_preview.strategic_view_requested.connect(_on_heightfield_strategic_view_requested)
		
		# Connect relayed HUD signals
		if heightfield_preview.has_signal("next_turn_requested"):
			heightfield_preview.next_turn_requested.connect(func(steps): emit_signal("next_turn_requested", steps))
		if heightfield_preview.has_signal("unit_scout_requested"):
			heightfield_preview.unit_scout_requested.connect(func(x, y, bits): emit_signal("unit_scout_requested", x, y, bits))
		if heightfield_preview.has_signal("unit_found_camp_requested"):
			heightfield_preview.unit_found_camp_requested.connect(func(x, y): emit_signal("unit_found_camp_requested", x, y))
		if heightfield_preview.has_signal("herd_follow_requested"):
			heightfield_preview.herd_follow_requested.connect(func(id): emit_signal("herd_follow_requested", id))
		if heightfield_preview.has_signal("forage_requested"):
			heightfield_preview.forage_requested.connect(func(x, y, mod): emit_signal("forage_requested", x, y, mod))
		if heightfield_preview.has_signal("hex_clicked"):
			heightfield_preview.hex_clicked.connect(handle_hex_click)
		
		# Connect 3D-specific control signals
		if heightfield_preview.has_signal("overlay_changed"):
			heightfield_preview.overlay_changed.connect(_on_heightfield_overlay_changed)
		if heightfield_preview.has_signal("inspector_toggle_requested"):
			heightfield_preview.inspector_toggle_requested.connect(_on_heightfield_inspector_toggle)
		if heightfield_preview.has_signal("legend_toggle_requested"):
			heightfield_preview.legend_toggle_requested.connect(_on_heightfield_legend_toggle)
		if heightfield_preview.has_signal("hex_hovered"):
			heightfield_preview.hex_hovered.connect(_on_heightfield_hex_hovered)
		if heightfield_preview.has_signal("view_state_changed"):
			heightfield_preview.view_state_changed.connect(_on_heightfield_view_state_changed)

		var main_node := get_tree().root.get_node_or_null("Main")
		if main_node != null and heightfield_preview.has_method("apply_hud_state") and main_node.has_method("export_hud_state"):
			var state: Dictionary = main_node.call("export_hud_state")
			heightfield_preview.call("apply_hud_state", state)
	return heightfield_preview

func _maybe_auto_show_heightfield() -> void:
	if _heightfield_boot_shown:
		return
	if heightfield_data.is_empty() or grid_width == 0 or grid_height == 0:
		return
	var preview := _ensure_heightfield_preview()
	if preview.has_method("show_preview"):
		preview.call("show_preview")
	else:
		preview.show()
	_heightfield_boot_shown = true
	if preview.has_method("move_to_front"):
		preview.call("move_to_front")
	if preview.has_method("_resize_to_display"):
		preview.call_deferred("_resize_to_display")
	_push_heightfield_preview()
	if preview.has_method("restore_or_sync_view_state"):
		preview.call("restore_or_sync_view_state", zoom_factor, pan_offset, last_hex_radius)
	elif preview.has_method("sync_view_state"):
		preview.sync_view_state(zoom_factor, pan_offset, last_hex_radius)

func relay_hud_call(method: String, args: Array = []) -> void:
	if heightfield_preview != null and is_instance_valid(heightfield_preview):
		if heightfield_preview.has_method("relay_hud_call"):
			heightfield_preview.call("relay_hud_call", method, args)



func _on_heightfield_overlay_changed(key: String) -> void:
	set_overlay_channel(key)
	_push_heightfield_preview()

func _on_heightfield_inspector_toggle() -> void:
	var main_node := get_tree().root.get_node_or_null("Main")
	if main_node != null and main_node.has_method("_on_toggle_inspector"):
		main_node.call("_on_toggle_inspector")

func _on_heightfield_legend_toggle() -> void:
	var main_node := get_tree().root.get_node_or_null("Main")
	if main_node != null and main_node.has_method("_on_toggle_legend"):
		main_node.call("_on_toggle_legend")

func _on_heightfield_hex_hovered(col: int, row: int) -> void:
	if col < 0 or row < 0:
		if _hovered_tile != Vector2i(-1, -1):
			_hovered_tile = Vector2i(-1, -1)
			queue_redraw()
		emit_signal("tile_hovered", {})
		return
	var incoming := Vector2i(col, row)
	if _hovered_tile != incoming:
		_hovered_tile = incoming
		queue_redraw()
	var info := _tile_info_at(col, row)
	emit_signal("tile_hovered", info)

func _on_heightfield_view_state_changed(zoom_2d: float, pan_2d: Vector2, hex_radius_2d: float) -> void:
	zoom_factor = zoom_2d
	pan_offset = pan_2d
	last_hex_radius = hex_radius_2d
	_update_layout_metrics()
	queue_redraw()

func _push_heightfield_preview() -> void:
	if heightfield_preview == null:
		print("[MapView] _push_heightfield_preview: heightfield_preview is null")
		return
	if not heightfield_preview.visible:
		print("[MapView] _push_heightfield_preview: heightfield_preview is not visible")
		return
	print("[MapView] _push_heightfield_preview: Pushing data...")
	
	var heightfield: Dictionary = heightfield_data
	_update_biome_color_buffer()
	
	var overlay_values: PackedFloat32Array = PackedFloat32Array()
	var overlay_color: Color = Color.WHITE
	if active_overlay_key != "":
		overlay_values = _overlay_array(active_overlay_key) # Changed from _get_overlay_values to _overlay_array
		overlay_color = OVERLAY_COLORS.get(active_overlay_key, LOGISTICS_COLOR)
	
	if not heightfield_preview.has_method("update_snapshot"):
		push_error("[MapView] HeightfieldPreview missing update_snapshot method!")
		return
	
	heightfield_preview.update_snapshot(
		heightfield, 
		biome_color_buffer,
		overlay_values,
		overlay_color,
		active_overlay_key,
		grid_width, 
		grid_height,
		units,
		herds,
		food_sites,
		terrain_tags_overlay,
		terrain_overlay
	)

func _toggle_heightfield_preview() -> void:
	var preview := _ensure_heightfield_preview()
	if preview.visible:
		if preview.has_method("hide_preview"):
			preview.call("hide_preview")
		else:
			preview.hide()
		_update_2d_minimap()  # Show 2D minimap when switching back to 2D view
		return
	if heightfield_data.is_empty():
		push_warning("Relief view not available yet; wait for the next snapshot.")
		return
	if preview.has_method("show_preview"):
		preview.call("show_preview")
	else:
		preview.show()
	if preview.has_method("move_to_front"):
		preview.call("move_to_front")
	if preview.has_method("_resize_to_display"):
		preview.call_deferred("_resize_to_display")
	print("[MapView] _toggle_heightfield_preview: showing window, calling push")
	_push_heightfield_preview()
	_update_2d_minimap()  # Hide 2D minimap when switching to 3D view
	if preview.has_method("restore_or_sync_view_state"):
		preview.call("restore_or_sync_view_state", zoom_factor, pan_offset, last_hex_radius)
	elif preview.has_method("sync_view_state"):
		preview.sync_view_state(zoom_factor, pan_offset, last_hex_radius)

func _is_heightfield_visible() -> bool:
	var vis := heightfield_preview != null and is_instance_valid(heightfield_preview) and heightfield_preview.visible
	# print("[MapView] _is_heightfield_visible: ", vis) # Commented out to avoid spam
	return vis

func _on_heightfield_strategic_view_requested() -> void:
	if heightfield_preview != null and is_instance_valid(heightfield_preview):
		heightfield_preview.hide()

func _tag_mask_at(x: int, y: int) -> int:
	if terrain_tags_overlay.is_empty() or grid_width == 0:
		return 0
	var index: int = y * grid_width + x
	if index < 0 or index >= terrain_tags_overlay.size():
		return 0
	return int(terrain_tags_overlay[index])

func _tag_color_for_mask(mask: int) -> Color:
	var color := GRID_COLOR
	var applied := false
	for raw_bit in TERRAIN_TAG_KEYS:
		var bit: int = int(raw_bit)
		if (mask & bit) == 0:
			continue
		var tag_color: Color = TERRAIN_TAG_COLORS.get(bit, Color.WHITE)
		var weight: float = float(TERRAIN_TAG_BLEND_WEIGHTS.get(bit, 0.6))
		color = color.lerp(tag_color, weight)
		applied = true
	if not applied:
		return GRID_COLOR
	return color

func _tag_label_for_mask(mask: int) -> String:
	if terrain_tag_labels.has(mask):
		return str(terrain_tag_labels[mask])
	for key in terrain_tag_labels.keys():
		if int(key) == mask:
			return str(terrain_tag_labels[key])
	return "Tag %d" % mask

func _compare_tag_rows(a: Dictionary, b: Dictionary) -> bool:
	var a_count: int = int(a.get("count", 0))
	var b_count: int = int(b.get("count", 0))
	if a_count == b_count:
		return int(a.get("mask", 0)) < int(b.get("mask", 0))
	return a_count > b_count

func _tag_coverage_rows() -> Array:
	var rows: Array = []
	if terrain_tags_overlay.is_empty() or grid_width <= 0 or grid_height <= 0:
		return rows
	var total_tiles: int = grid_width * grid_height
	if total_tiles <= 0:
		return rows
	var counts: Dictionary = {}
	var limit: int = min(terrain_tags_overlay.size(), total_tiles)
	for idx in range(limit):
		var mask: int = int(terrain_tags_overlay[idx])
		if mask == 0:
			continue
		for raw_bit in TERRAIN_TAG_KEYS:
			var bit: int = int(raw_bit)
			if (mask & bit) != 0:
				counts[bit] = int(counts.get(bit, 0)) + 1
	for raw_bit in counts.keys():
		var bit_value: int = int(raw_bit)
		var count: int = int(counts[raw_bit])
		var percent: float = 0.0
		if total_tiles > 0:
			percent = (float(count) / float(total_tiles)) * 100.0
		rows.append({
			"mask": bit_value,
			"label": _tag_label_for_mask(bit_value),
			"count": count,
			"percent": percent,
		})
	rows.sort_custom(Callable(self, "_compare_tag_rows"))
	return rows

func _tag_overlay_stats() -> Dictionary:
	var rows: Array = _tag_coverage_rows()
	if rows.is_empty():
		return {"has_values": false}
	return {
		"has_values": true,
		"coverage": rows,
		"tile_total": grid_width * grid_height,
	}

func _build_tag_legend() -> Dictionary:
	var coverage: Array = _tag_coverage_rows()
	var coverage_lookup: Dictionary = {}
	for entry in coverage:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		coverage_lookup[int(entry.get("mask", 0))] = entry
	var rows: Array = []
	for raw_bit in TERRAIN_TAG_KEYS:
		var mask: int = int(raw_bit)
		var label: String = _tag_label_for_mask(mask)
		var entry: Dictionary = coverage_lookup.get(mask, {})
		var percent_val: float = float(entry.get("percent", 0.0))
		var count: int = int(entry.get("count", 0))
		var value_text := ""
		if percent_val > 0.0:
			value_text = "%.1f%%" % percent_val
		var display_label := "%s (%d)" % [label, count] if count > 0 else label
		rows.append({
			"color": TERRAIN_TAG_COLORS.get(mask, Color.WHITE),
			"label": display_label,
			"value_text": value_text,
		})
	return {
		"key": "terrain_tags",
		"title": "Terrain Tags",
		"description": "Tiles blend colors for all active environmental tags.",
		"rows": rows,
		"stats": {
			"tile_total": grid_width * grid_height,
		},
	}

func terrain_palette_entries() -> Array:
	var ids: Array = []
	if terrain_palette.size() > 0:
		ids = Array(terrain_palette.keys())
	else:
		ids = Array(_get_terrain_colors().keys())
	ids.sort()
	var labels := _get_terrain_labels()
	var entries: Array = []
	for raw_id in ids:
		var id := int(raw_id)
		var label := ""
		if terrain_palette.has(id):
			label = str(terrain_palette[id])
		if label == "":
			label = labels.get(id, "Unknown")
		var color := _terrain_color_for_id(id)
		entries.append({
			"id": id,
			"label": label,
			"color": color,
		})
	return entries

func _emit_overlay_legend() -> void:
	emit_signal("overlay_legend_changed", _legend_for_current_view())

func refresh_overlay_legend() -> void:
	_emit_overlay_legend()

func overlay_stats_for_key(key: String) -> Dictionary:
	if key == "terrain_tags":
		return _tag_overlay_stats()
	if not overlay_channels.has(key):
		return {}
	if key == "culture" and not highlighted_culture_layer_set.is_empty():
		var selection := _culture_selection_data()
		if bool(selection.get("valid", false)):
			return selection.get("stats", {})
	var normalized: PackedFloat32Array = _overlay_array(key)
	var raw: PackedFloat32Array = _overlay_raw_array(key)
	return _overlay_stats(normalized, raw)

func _legend_for_current_view() -> Dictionary:
	if active_overlay_key == "":
		return _build_terrain_legend()
	if active_overlay_key == "terrain_tags":
		return _build_tag_legend()
	if not overlay_channels.has(active_overlay_key):
		return {}
	if active_overlay_key == "culture" and not highlighted_culture_layer_set.is_empty():
		var selection := _culture_selection_data()
		if bool(selection.get("valid", false)):
			var normalized: PackedFloat32Array = selection.get("normalized", PackedFloat32Array())
			var raw: PackedFloat32Array = selection.get("raw", PackedFloat32Array())
			var stats: Dictionary = selection.get("stats", {})
			var tile_count: int = int(stats.get("tile_count", stats.get("raw_count", 0)))
			var context_label: String = highlighted_culture_context
			if context_label == "" and tile_count > 0:
				context_label = "Selection (%d tiles)" % tile_count
			return _build_scalar_overlay_legend("culture", normalized, raw, stats, context_label)
	return _build_scalar_overlay_legend(active_overlay_key)

func _build_terrain_legend() -> Dictionary:
	var rows: Array = []
	for entry in terrain_palette_entries():
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		rows.append({
			"color": entry.get("color", Color.WHITE),
			"label": str(entry.get("label", "")),
			"value_text": "#%02d" % int(entry.get("id", 0)),
		})
	return {
		"key": "terrain",
		"title": "Terrain Types",
		"description": "Biome palette applied directly to tiles.",
		"rows": rows,
		"stats": {},
	}

func _build_scalar_overlay_legend(
		key: String,
		normalized_override: Variant = null,
		raw_override: Variant = null,
		stats_override: Dictionary = {},
		context_label: String = ""
	) -> Dictionary:
	var normalized: PackedFloat32Array
	if normalized_override != null and normalized_override is PackedFloat32Array:
		normalized = normalized_override
	else:
		normalized = _overlay_array(key)
	var raw: PackedFloat32Array
	if raw_override != null and raw_override is PackedFloat32Array:
		raw = raw_override
	else:
		raw = _overlay_raw_array(key)
	var stats: Dictionary = stats_override
	if stats_override.is_empty():
		stats = _overlay_stats(normalized, raw)
	var overlay_color: Color = OVERLAY_COLORS.get(key, LOGISTICS_COLOR)
	var label: String = String(overlay_channel_labels.get(key, key.capitalize()))
	var description: String = String(overlay_channel_descriptions.get(key, ""))
	var placeholder: bool = bool(overlay_placeholder_flags.get(key, false))
	var rows: Array = []
	if context_label != "":
		if description != "":
			description = "%s\n%s" % [description, context_label]
		else:
			description = context_label
	var has_values: bool = bool(stats.get("has_values", false))
	var raw_range: float = float(stats.get("raw_range", 0.0))

	if placeholder and not has_values:
		rows.append({
			"color": GRID_COLOR,
			"label": "No data",
			"value_text": "Channel awaiting telemetry",
		})
	elif key == "crisis" and not has_values:
		rows.append({
			"color": GRID_COLOR,
			"label": "No active crises",
			"value_text": "Awaiting crisis incidents",
		})
	elif not has_values:
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, 0.2),
			"label": "No variation",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
	elif raw_range <= 0.0001:
		var tint: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, tint),
			"label": "Uniform",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
	else:
		var low_t: float = clamp(float(stats.get("normalized_min", 0.0)), 0.0, 1.0)
		var mid_t: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
		var high_t: float = clamp(float(stats.get("normalized_max", 0.0)), 0.0, 1.0)
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, low_t),
			"label": "Low",
			"value_text": _format_legend_value(float(stats.get("raw_min", 0.0))),
		})
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, mid_t),
			"label": "Average",
			"value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
		})
		rows.append({
			"color": GRID_COLOR.lerp(overlay_color, high_t),
			"label": "High",
			"value_text": _format_legend_value(float(stats.get("raw_max", 0.0))),
		})

	return {
		"key": key,
		"title": label,
		"description": description,
		"rows": rows,
		"stats": {
			"min": stats.get("raw_min", 0.0),
			"max": stats.get("raw_max", 0.0),
			"avg": stats.get("raw_avg", 0.0),
		},
		"placeholder": placeholder,
	}

func _overlay_stats(normalized: PackedFloat32Array, raw: PackedFloat32Array) -> Dictionary:
	var n_min: float = INF
	var n_max: float = -INF
	var n_sum: float = 0.0
	var n_count: int = 0
	for value in normalized:
		var v: float = float(value)
		if not is_finite(v):
			continue
		n_min = min(n_min, v)
		n_max = max(n_max, v)
		n_sum += v
		n_count += 1
	if n_count == 0:
		n_min = 0.0
		n_max = 0.0

	var r_min: float = INF
	var r_max: float = -INF
	var r_sum: float = 0.0
	var r_count: int = 0
	for value in raw:
		var rv: float = float(value)
		if not is_finite(rv):
			continue
		r_min = min(r_min, rv)
		r_max = max(r_max, rv)
		r_sum += rv
		r_count += 1
	if r_count == 0:
		r_min = 0.0
		r_max = 0.0

	var has_values: bool = n_count > 0 and r_count > 0
	var raw_avg: float = 0.0
	if r_count > 0:
		raw_avg = r_sum / float(r_count)
	var normalized_avg: float = 0.0
	if n_count > 0:
		normalized_avg = n_sum / float(n_count)

	return {
		"normalized_min": n_min,
		"normalized_max": n_max,
		"normalized_avg": normalized_avg,
		"raw_min": r_min,
		"raw_max": r_max,
		"raw_avg": raw_avg,
		"raw_range": r_max - r_min,
		"has_values": has_values,
		"normalized_count": n_count,
		"raw_count": r_count,
	}

func _culture_selection_data() -> Dictionary:
	if highlighted_culture_layer_set.is_empty():
		return {"valid": false}
	if culture_layer_grid.is_empty():
		return {"valid": false}
	var normalized_src: PackedFloat32Array = _overlay_array("culture")
	if normalized_src.is_empty():
		return {"valid": false}
	var raw_src: PackedFloat32Array = _overlay_raw_array("culture")
	var limit: int = min(normalized_src.size(), culture_layer_grid.size())
	if limit <= 0:
		return {"valid": false}
	var selected_norm: Array = []
	var selected_raw: Array = []
	for idx in range(limit):
		var layer_id: int = int(culture_layer_grid[idx])
		if not highlighted_culture_layer_set.has(layer_id):
			continue
		selected_norm.append(normalized_src[idx])
		if raw_src.size() > idx:
			selected_raw.append(raw_src[idx])
		else:
			selected_raw.append(normalized_src[idx])
	if selected_norm.is_empty():
		return {"valid": false}
	var norm_packed := PackedFloat32Array(selected_norm)
	var raw_packed := PackedFloat32Array(selected_raw)
	var stats := _overlay_stats(norm_packed, raw_packed)
	stats["tile_count"] = selected_norm.size()
	return {
		"valid": true,
		"normalized": norm_packed,
		"raw": raw_packed,
		"stats": stats,
	}

func _install_province_overlay() -> void:
	if overlay_channels.has("province"):
		return
	if grid_width <= 0 or grid_height <= 0:
		return
	if culture_layer_map.is_empty() or culture_layer_grid.is_empty():
		return
	var province_raw := PackedFloat32Array()
	var total: int = grid_width * grid_height
	province_raw.resize(total)
	province_raw.fill(-1.0)
	var regional_owner: Dictionary = {}
	for layer_dict in culture_layer_map.values():
		if not (layer_dict is Dictionary):
			continue
		var scope := String(layer_dict.get("scope", ""))
		if scope == "Regional":
			var id: int = int(layer_dict.get("id", -1))
			var owner: int = int(layer_dict.get("owner", -1))
			if id >= 0:
				regional_owner[id] = owner
	if regional_owner.is_empty():
		return
	var layer_to_province: Dictionary = {}
	for idx in range(total):
		var layer_id: int = int(culture_layer_grid[idx])
		if layer_id < 0:
			continue
		if layer_to_province.has(layer_id):
			province_raw[idx] = float(layer_to_province[layer_id])
			continue
		var province_id: int = _resolve_province_for_layer(layer_id, regional_owner)
		layer_to_province[layer_id] = province_id
		province_raw[idx] = float(province_id)
	var province_seq: Dictionary = {}
	var seq: int = 0
	for value in province_raw:
		var pid := int(value)
		if pid < 0:
			continue
		if province_seq.has(pid):
			continue
		province_seq[pid] = seq
		seq += 1
	var province_norm := PackedFloat32Array()
	province_norm.resize(total)
	var denom: float = max(float(seq - 1), 1.0)
	for i in range(total):
		var pid := int(province_raw[i])
		if pid < 0 or seq <= 0:
			province_norm[i] = 0.0
		elif seq == 1:
			province_norm[i] = 0.5
		else:
			var idx_val: int = int(province_seq.get(pid, 0))
			province_norm[i] = float(idx_val) / denom
	_add_overlay_channel(
		"province",
		province_norm,
		province_raw,
		"Provinces",
        "Province/territory partitions"
	)

func _resolve_province_for_layer(layer_id: int, regional_owner: Dictionary) -> int:
	var guard := 0
	var current := layer_id
	while current > 0 and guard < 32:
		if regional_owner.has(current):
			return int(regional_owner[current])
		if not culture_layer_map.has(current):
			break
		var layer: Dictionary = culture_layer_map[current]
		current = int(layer.get("parent", -1))
		guard += 1
	return -1

func _add_overlay_channel(key: String, normalized: PackedFloat32Array, raw: PackedFloat32Array, label: String, description: String = "") -> void:
	overlay_channels[key] = normalized
	overlay_raw_channels[key] = raw
	overlay_channel_labels[key] = label
	overlay_channel_descriptions[key] = description
	overlay_placeholder_flags[key] = false
	if overlay_channel_order.find(key) == -1:
		overlay_channel_order.append(key)

func _ensure_default_overlay_channel() -> void:
	if grid_width <= 0 or grid_height <= 0:
		return
	var total: int = grid_width * grid_height
	var zeros := PackedFloat32Array()
	zeros.resize(total)
	zeros.fill(0.0)
	_add_overlay_channel("", zeros, zeros, "No Overlay", "Base map without overlays")

func _format_legend_value(value: float) -> String:
	return "%0.3f" % value

func set_terrain_mode(_enabled: bool) -> void:
	set_overlay_channel("")

func _draw_hydrology(radius: float, origin: Vector2) -> void:
	if hydrology_rivers.is_empty():
		return
	var river_color := (Color(0.95, 0.25, 0.25, 0.95) if highlight_rivers else Color(0.12, 0.5, 0.85, 0.85))
	var line_width := (4.0 if highlight_rivers else 3.0)
	for river in hydrology_rivers:
		if not (river is Dictionary):
			continue
		var points := Array(river.get("points", []))
		if points.size() < 2:
			continue
		# When FoW is enabled, only draw visible segments
		if _fow_enabled:
			var current_segment: PackedVector2Array = PackedVector2Array()
			for pt in points:
				if not (pt is Dictionary):
					continue
				var x := int(pt.get("x", 0))
				var y := int(pt.get("y", 0))
				var is_visible := _is_tile_visible(x, y)
				if is_visible:
					current_segment.append(_hex_center(x, y, radius, origin))
				else:
					# End current segment and start new one
					if current_segment.size() >= 2:
						draw_polyline(current_segment, river_color, line_width, false)
					current_segment = PackedVector2Array()
			# Draw final segment if any
			if current_segment.size() >= 2:
				draw_polyline(current_segment, river_color, line_width, false)
		else:
			# FoW disabled - draw entire river
			var poly: PackedVector2Array = PackedVector2Array()
			for pt in points:
				if not (pt is Dictionary):
					continue
				var x := int(pt.get("x", 0))
				var y := int(pt.get("y", 0))
				poly.append(_hex_center(x, y, radius, origin))
			if poly.size() >= 2:
				draw_polyline(poly, river_color, line_width, false)

func set_highlight_rivers(enabled: bool) -> void:
	highlight_rivers = enabled
	queue_redraw()

func _draw_start_marker(radius: float, origin: Vector2) -> void:
	if start_marker.x < 0 or start_marker.y < 0:
		return
	var center := _hex_center(start_marker.x, start_marker.y, radius, origin)
	var size := radius * 0.3
	var color := Color(1.0, 0.86, 0.2, 0.9)
	var points := PackedVector2Array([center + Vector2(size, 0), center + Vector2(0, size), center + Vector2(-size, 0), center + Vector2(0, -size)])
	draw_polyline(points, color, 3.0, true)
	_emit_overlay_legend()

func toggle_terrain_mode() -> void:
	set_overlay_channel("")

func _average(data: PackedFloat32Array) -> float:
	if data.is_empty():
		return 0.0
	var total: float = 0.0
	for value in data:
		total += float(value)
	return total / data.size()

func _hex_center(col: int, row: int, radius: float, origin: Vector2) -> Vector2:
	var axial := _offset_to_axial(col, row)
	return origin + _axial_center(axial.x, axial.y, radius)

func _axial_center(q: int, r: int, radius: float) -> Vector2:
	var fq := float(q)
	var fr := float(r)
	var x: float = radius * (SQRT3 * fq + SQRT3 * 0.5 * fr)
	var y: float = radius * (1.5 * fr)
	return Vector2(x, y)

func _offset_to_axial(col: int, row: int) -> Vector2i:
	# odd-r horizontal layout (flat-top hexes)
	var q := col - ((row - (row & 1)) >> 1)
	var r := row
	return Vector2i(q, r)

func _axial_to_offset(q: int, r: int) -> Vector2i:
	var col: int = q + ((r - (r & 1)) >> 1)
	return Vector2i(col, r)

func _hex_points(center: Vector2, radius: float, closed: bool = false) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in range(6):
		var angle := deg_to_rad(60.0 * float(i) + 30.0)
		points.append(center + Vector2(radius * cos(angle), radius * sin(angle)))
	if closed:
		points.append(points[0])
	return points

func _get_adjusted_viewport_size() -> Vector2:
	var viewport_size: Vector2 = get_viewport_rect().size
	var canvas_scale := get_viewport().get_canvas_transform().get_scale()
	if canvas_scale.x != 0.0 and canvas_scale.y != 0.0:
		# Account for global canvas (camera) scaling so hit-testing matches the drawn map
		viewport_size /= canvas_scale
	return viewport_size

func _update_layout_metrics() -> void:
	if grid_width <= 0 or grid_height <= 0:
		return
	var viewport_size: Vector2 = _get_adjusted_viewport_size()
	if viewport_size.x <= 0.0 or viewport_size.y <= 0.0:
		return
	if bounds_dirty:
		base_bounds = _compute_bounds(1.0)
		bounds_dirty = false
	if base_bounds.size.x <= 0.0 or base_bounds.size.y <= 0.0:
		return
	var radius_from_width: float = viewport_size.x / base_bounds.size.x
	var radius_from_height: float = viewport_size.y / base_bounds.size.y
	base_hex_radius = max(radius_from_width, radius_from_height)
	last_hex_radius = clamp(base_hex_radius * zoom_factor, base_hex_radius * MIN_ZOOM_FACTOR, base_hex_radius * MAX_ZOOM_FACTOR)
	var scaled_bounds := Rect2(base_bounds.position * last_hex_radius, base_bounds.size * last_hex_radius)
	last_map_size = scaled_bounds.size
	last_base_origin = (viewport_size - last_map_size) * 0.5 - scaled_bounds.position
	last_origin = last_base_origin + pan_offset

func _clamp_pan_offset() -> void:
	if last_map_size.x <= 0.0 or last_map_size.y <= 0.0:
		return
	var viewport_size: Vector2 = _get_adjusted_viewport_size()

	# Calculate pan limits based on keeping map bounds within viewport
	# pan_offset affects last_origin, and the map bounds in world coords are:
	#   left edge: last_origin.x + scaled_bounds.position.x
	#   right edge: last_origin.x + scaled_bounds.position.x + last_map_size.x
	# We want: left edge >= 0 and right edge <= viewport_size
	# Simplifies to: pan_offset in range [-(vp-map)/2, (vp-map)/2] relative to centered position

	var delta_x: float = viewport_size.x - last_map_size.x
	var delta_y: float = viewport_size.y - last_map_size.y

	# For X axis:
	if delta_x <= 0.0:
		# Map is wider than or equal to viewport - allow panning within bounds
		var max_pan_x: float = -delta_x / 2.0  # pan right limit (shows left edge)
		var min_pan_x: float = delta_x / 2.0   # pan left limit (shows right edge)
		pan_offset.x = clamp(pan_offset.x, min_pan_x, max_pan_x)
	else:
		# Map is narrower - center it (no horizontal panning)
		pan_offset.x = 0.0

	# For Y axis:
	if delta_y <= 0.0:
		# Map is taller than or equal to viewport - allow panning within bounds
		var max_pan_y: float = -delta_y / 2.0  # pan down limit (shows top edge)
		var min_pan_y: float = delta_y / 2.0   # pan up limit (shows bottom edge)
		pan_offset.y = clamp(pan_offset.y, min_pan_y, max_pan_y)
	else:
		# Map is shorter - center it (no vertical panning)
		pan_offset.y = 0.0

func get_world_center() -> Vector2:
	return last_origin + last_map_size * 0.5

func get_hex_radius() -> float:
	return last_hex_radius

func _compute_bounds(radius: float) -> Rect2:
	var min_x := INF
	var max_x := -INF
	var min_y := INF
	var max_y := -INF
	for col in range(grid_width):
		for row in range(grid_height):
			var axial := _offset_to_axial(col, row)
			var center := _axial_center(axial.x, axial.y, radius)
			min_x = min(min_x, center.x - radius)
			max_x = max(max_x, center.x + radius)
			min_y = min(min_y, center.y - radius)
			max_y = max(max_y, center.y + radius)
	if min_x == INF:
		return Rect2(Vector2.ZERO, Vector2.ONE)
	return Rect2(Vector2(min_x, min_y), Vector2(max_x - min_x, max_y - min_y))

func _point_to_offset(point: Vector2) -> Vector2i:
	if grid_width <= 0 or grid_height <= 0:
		return Vector2i(-1, -1)
	var radius: float = max(last_hex_radius, 0.0001)
	var relative: Vector2 = (point - last_origin) / radius
	var qf: float = (SQRT3 / 3.0) * relative.x - (1.0 / 3.0) * relative.y
	var rf: float = (2.0 / 3.0) * relative.y
	var axial: Vector2i = _cube_round(qf, rf)
	return _axial_to_offset(axial.x, axial.y)

func _cube_round(qf: float, rf: float) -> Vector2i:
	var sf: float = -qf - rf
	var rq: float = round(qf)
	var rr: float = round(rf)
	var rs: float = round(sf)

	var q_diff: float = abs(rq - qf)
	var r_diff: float = abs(rr - rf)
	var s_diff: float = abs(rs - sf)

	if q_diff > r_diff and q_diff > s_diff:
		rq = -rr - rs
	elif r_diff > s_diff:
		rr = -rq - rs
	else:
		rs = -rq - rr

	return Vector2i(int(rq), int(rr))

func _process(delta: float) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if mouse_pan_active and mouse_pan_button != -1 and not Input.is_mouse_button_pressed(mouse_pan_button):
		mouse_pan_active = false
		mouse_pan_button = -1
	var pan_input := Vector2(
		Input.get_action_strength("map_pan_right") - Input.get_action_strength("map_pan_left"),
		Input.get_action_strength("map_pan_down") - Input.get_action_strength("map_pan_up")
	)
	if pan_input != Vector2.ZERO:
		if pan_input.length_squared() > 1.0:
			pan_input = pan_input.normalized()
		_apply_pan(pan_input * KEYBOARD_PAN_SPEED * delta)
	var zoom_direction: float = Input.get_action_strength("map_zoom_in") - Input.get_action_strength("map_zoom_out")
	if not is_zero_approx(zoom_direction):
		var viewport_center: Vector2 = get_viewport_rect().size * 0.5
		_apply_zoom(zoom_direction * KEYBOARD_ZOOM_SPEED * delta, viewport_center)

func _apply_pan(delta: Vector2) -> void:
	if delta == Vector2.ZERO:
		return
	pan_offset += delta
	_update_layout_metrics()
	_clamp_pan_offset()
	queue_redraw()
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()

func _apply_zoom(delta_zoom: float, pivot: Vector2) -> void:
	if is_zero_approx(delta_zoom):
		return
	_update_layout_metrics()
	var previous_zoom: float = zoom_factor
	var previous_radius: float = max(last_hex_radius, 0.0001)
	var previous_origin: Vector2 = last_origin
	zoom_factor = clamp(zoom_factor + delta_zoom, MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
	if is_equal_approx(zoom_factor, previous_zoom):
		return
	var unit_position: Vector2 = (pivot - previous_origin) / previous_radius
	_update_layout_metrics()
	var new_radius: float = last_hex_radius
	var new_base_origin: Vector2 = last_base_origin
	pan_offset = pivot - new_base_origin - unit_position * new_radius
	_clamp_pan_offset()
	_update_layout_metrics()
	queue_redraw()
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()

func _begin_mouse_pan(button_index: int) -> void:
	mouse_pan_active = true
	mouse_pan_button = button_index

func _end_mouse_pan(button_index: int) -> void:
	if mouse_pan_active and mouse_pan_button == button_index:
		mouse_pan_active = false
		mouse_pan_button = -1

func _mark_input_handled() -> void:
	var viewport := get_viewport()
	if viewport != null:
		viewport.set_input_as_handled()

func _ensure_input_actions() -> void:
	var action_keys := {
		"map_pan_left": KEY_A,
		"map_pan_right": KEY_D,
		"map_pan_up": KEY_W,
		"map_pan_down": KEY_S,
		"map_zoom_in": KEY_E,
		"map_zoom_out": KEY_Q,
		"map_toggle_relief": KEY_R,
		"map_switch_strategic_view": KEY_V,
	}
	for action in action_keys.keys():
		if not InputMap.has_action(action):
			InputMap.add_action(action)
		var keycode: int = action_keys[action]
		var needs_event: bool = true
		for existing_event in InputMap.action_get_events(action):
			if existing_event is InputEventKey and existing_event.keycode == keycode:
				needs_event = false
				break
		if needs_event:
			var key_event := InputEventKey.new()
			key_event.keycode = keycode
			key_event.physical_keycode = keycode
			InputMap.action_add_event(action, key_event)

func _fit_map_to_view() -> void:
	zoom_factor = 1.0
	pan_offset = Vector2.ZERO
	_update_layout_metrics()
	_clamp_pan_offset()
	queue_redraw()
	if _minimap_2d != null:
		_minimap_2d.queue_indicator_redraw()

func handle_hex_click(col: int, row: int, button_index: int) -> void:
	# Only handle left mouse button clicks. Right-clicks and other buttons are intentionally ignored.
	if button_index != MOUSE_BUTTON_LEFT:
		return

	if col < 0 or col >= grid_width or row < 0 or row >= grid_height:
		return

	var terrain_id: int = _terrain_id_at(col, row)
	emit_signal("hex_selected", col, row, terrain_id)
	_emit_tile_selection(col, row)

	_handle_entity_selection(col, row)

# --- Terrain Texture System for 2D View (textures loaded via TerrainTextureManager autoload) ---

func _init_terrain_rendering() -> void:
	## Initialize 2D terrain rendering from TerrainTextureManager
	var mgr := TerrainTextureManager
	if mgr.terrain_textures != null and mgr.terrain_textures.get_layers() > 0:
		_build_terrain_priority_map()
		_build_hex_texture_cache()
		if mgr.use_edge_blending:
			_load_edge_masks()
			_build_edge_overlay_cache()

func _build_hex_texture_cache() -> void:
	## Pre-render hex-masked textures from the terrain atlas
	var mgr := TerrainTextureManager
	if mgr.terrain_textures == null:
		return

	_hex_texture_cache.clear()
	var layer_count: int = mgr.terrain_textures.get_layers()

	for terrain_id in range(layer_count):
		var hex_tex := _render_hex_texture(terrain_id)
		if hex_tex != null:
			_hex_texture_cache[terrain_id] = hex_tex

	print("[MapView] Built hex texture cache: %d textures" % _hex_texture_cache.size())

func _render_hex_texture(terrain_id: int) -> ImageTexture:
	## Render a hex-masked texture for the given terrain ID
	var mgr := TerrainTextureManager
	if mgr.terrain_textures == null or terrain_id < 0 or terrain_id >= mgr.terrain_textures.get_layers():
		return null

	var size := _hex_texture_size
	var source_img: Image = mgr.terrain_textures.get_layer_data(terrain_id)
	if source_img == null:
		return null

	# Create output image with alpha
	var output := Image.create(size, size, false, Image.FORMAT_RGBA8)
	if output == null:
		return null

	# Scale source image to output size
	var scaled_source := source_img.duplicate()
	scaled_source.resize(size, size)
	if scaled_source.get_format() != Image.FORMAT_RGBA8:
		scaled_source.convert(Image.FORMAT_RGBA8)

	# Define hex shape (pointy-top)
	var center := Vector2(size * 0.5, size * 0.5)
	# Slightly larger than 0.5 to ensure edge pixels sample valid terrain color
	var hex_radius := size * 0.51

	# First pass: copy all pixels (to avoid black bleeding at edges during filtering)
	for y in range(size):
		for x in range(size):
			var src_color: Color = scaled_source.get_pixel(x, y)
			output.set_pixel(x, y, src_color)

	# Second pass: set alpha to 0 for pixels outside the hex
	for y in range(size):
		for x in range(size):
			var pos := Vector2(x, y)
			if not _point_in_hex(pos, center, hex_radius):
				var existing: Color = output.get_pixel(x, y)
				output.set_pixel(x, y, Color(existing.r, existing.g, existing.b, 0.0))

	return ImageTexture.create_from_image(output)

func _point_in_hex(point: Vector2, center: Vector2, radius: float) -> bool:
	# Check if a point is inside a pointy-top hexagon
	var dx := absf(point.x - center.x)
	var dy := absf(point.y - center.y)

	# Bounding box check
	if dy > radius:
		return false
	if dx > radius * SQRT3 * 0.5:
		return false

	# Edge check for hex shape
	return (radius * SQRT3 * 0.5 - dx) * 2.0 >= (dy - radius * 0.5) * SQRT3

func _draw_hex_textured(center: Vector2, terrain_id: int, radius: float) -> void:
	# Draw a textured hex at the given center position
	var color: Color = _terrain_color_for_id(terrain_id)
	var polygon_points := _hex_points(center, radius)

	var tex: ImageTexture = _hex_texture_cache.get(terrain_id)
	if tex == null:
		# No texture - just draw solid color
		draw_polygon(polygon_points, PackedColorArray([color, color, color, color, color, color]))
		return

	# Calculate UVs for the hex polygon
	# The texture is a square, so we map hex points to UV space
	var uvs := PackedVector2Array()
	for point in polygon_points:
		# Convert point relative to center into 0-1 UV range
		var uv := Vector2(
			(point.x - center.x) / radius * 0.5 + 0.5,
			(point.y - center.y) / radius * 0.5 + 0.5
		)
		uvs.append(uv)

	# Draw hex polygon with texture (clips texture to exact hex shape)
	var colors := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
	draw_polygon(polygon_points, colors, uvs, tex)

func get_terrain_textures_enabled() -> bool:
	var mgr := TerrainTextureManager
	return mgr.use_terrain_textures and mgr.terrain_textures != null

func enable_terrain_textures(enabled: bool) -> void:
	## Toggle terrain texture rendering for 2D view
	TerrainTextureManager.use_terrain_textures = enabled
	queue_redraw()

func _load_edge_masks() -> void:
	# Load the 6 edge gradient mask textures
	_edge_mask_textures.clear()
	const EDGE_PATH := "res://assets/terrain/textures/edges/"

	for edge_idx: int in range(6):
		var filename := "edge_mask_%d.png" % edge_idx
		var filepath := EDGE_PATH + filename
		var abs_path := ProjectSettings.globalize_path(filepath)

		if FileAccess.file_exists(abs_path):
			var img := Image.load_from_file(abs_path)
			if img != null:
				var tex := ImageTexture.create_from_image(img)
				_edge_mask_textures.append(tex)
			else:
				_edge_mask_textures.append(null)
		else:
			_edge_mask_textures.append(null)

	var loaded := _edge_mask_textures.filter(func(t: Variant) -> bool: return t != null).size()
	print("[MapView] Loaded edge masks: %d/6" % loaded)

func _build_edge_overlay_cache() -> void:
	## Pre-render edge overlays for each terrain type and edge direction
	## These are the terrain textures masked by the edge gradient
	var mgr := TerrainTextureManager
	if mgr.terrain_textures == null or _edge_mask_textures.size() < 6:
		return

	_edge_overlay_cache.clear()
	var size := _hex_texture_size
	var layer_count: int = mgr.terrain_textures.get_layers()

	for terrain_id: int in range(layer_count):
		var source_img: Image = mgr.terrain_textures.get_layer_data(terrain_id)
		if source_img == null:
			continue

		# Scale source to our texture size
		var scaled_source: Image = source_img.duplicate()
		scaled_source.resize(size, size)
		if scaled_source.get_format() != Image.FORMAT_RGBA8:
			scaled_source.convert(Image.FORMAT_RGBA8)

		for edge_idx: int in range(6):
			var mask_tex: ImageTexture = _edge_mask_textures[edge_idx]
			if mask_tex == null:
				continue

			var mask_img: Image = mask_tex.get_image()
			if mask_img == null:
				continue

			# Scale mask to match
			var scaled_mask: Image = mask_img.duplicate()
			scaled_mask.resize(size, size)

			# Create masked overlay: terrain texture with alpha from edge mask
			var overlay := Image.create(size, size, false, Image.FORMAT_RGBA8)
			for y: int in range(size):
				for x: int in range(size):
					var src_color: Color = scaled_source.get_pixel(x, y)
					var mask_alpha: float = scaled_mask.get_pixel(x, y).a
					overlay.set_pixel(x, y, Color(src_color.r, src_color.g, src_color.b, mask_alpha))

			var overlay_tex := ImageTexture.create_from_image(overlay)
			var cache_key := "%d_%d" % [terrain_id, edge_idx]
			_edge_overlay_cache[cache_key] = overlay_tex

	print("[MapView] Built edge overlay cache: %d textures" % _edge_overlay_cache.size())

func _build_terrain_priority_map() -> void:
	## Build a map of terrain_id -> priority from config
	## Higher priority terrains draw fringes onto lower priority terrains
	_terrain_priority.clear()

	var config: Dictionary = TerrainTextureManager.terrain_config
	var categories: Dictionary = config.get("categories", {})
	var terrains: Array = config.get("terrains", [])

	# Build category -> priority map
	var category_priority: Dictionary = {}
	for cat_name: String in categories.keys():
		var cat_data: Dictionary = categories[cat_name]
		category_priority[cat_name] = int(cat_data.get("wang_priority", 3))

	# Assign priority to each terrain based on its category
	for terrain_data: Variant in terrains:
		if terrain_data is Dictionary:
			var tid: int = int(terrain_data.get("id", 0))
			var cat: String = str(terrain_data.get("category", "land"))
			_terrain_priority[tid] = category_priority.get(cat, 3)

	print("[MapView] Built terrain priority map: %d terrains" % _terrain_priority.size())

func _get_terrain_priority(terrain_id: int) -> int:
	return int(_terrain_priority.get(terrain_id, 3))

var _edge_debug_done: bool = false

func _draw_terrain_edge_blending(radius: float, origin: Vector2) -> void:
	# Draw edge overlays using the overlay/fringe technique
	# Only HIGHER priority terrains draw fringes onto LOWER priority terrains
	if _cached_terrain_ids.is_empty() or _terrain_grid_width == 0:
		return
	if _edge_overlay_cache.is_empty():
		return

	var tex_size := radius * 2.0

	# Debug: check specific hexes once
	if not _edge_debug_done:
		var t56_25 := _terrain_id_at(56, 25)
		var t56_26 := _terrain_id_at(56, 26)
		var t57_26 := _terrain_id_at(57, 26)
		print("[DEBUG] Hex(56,25) terrain=%d, Hex(56,26) terrain=%d, Hex(57,26) terrain=%d" % [t56_25, t56_26, t57_26])
		print("[DEBUG] Priorities: (56,25)=%d, (56,26)=%d, (57,26)=%d" % [_get_terrain_priority(t56_25), _get_terrain_priority(t56_26), _get_terrain_priority(t57_26)])
		_edge_debug_done = true

	for y: int in range(_terrain_grid_height):
		for x: int in range(_terrain_grid_width):
			var center := _hex_center(x, y, radius, origin)
			var terrain_id := _terrain_id_at(x, y)
			var my_priority := _get_terrain_priority(terrain_id)

			# Check each of the 6 neighbors
			for edge_idx: int in range(6):
				var n_col: int = x + _get_neighbor_offset_x_2d(y, edge_idx)
				var n_row: int = y + _get_neighbor_offset_y_2d(edge_idx)

				if n_col < 0 or n_col >= _terrain_grid_width or n_row < 0 or n_row >= _terrain_grid_height:
					continue

				var neighbor_id := _terrain_id_at(n_col, n_row)

				# Debug: log when hexes 56,26 or 57,26 check neighbor 56,25
				if (x == 56 and y == 26) or (x == 57 and y == 26):
					if n_col == 56 and n_row == 25:
						print("[DEBUG] Hex(%d,%d) terrain=%d checking neighbor (56,25) terrain=%d, same=%s" % [x, y, terrain_id, neighbor_id, str(neighbor_id == terrain_id)])

				if neighbor_id == terrain_id:
					continue

				var neighbor_priority := _get_terrain_priority(neighbor_id)

				# Only draw fringe if neighbor has HIGHER priority than me
				# (neighbor's terrain extends into my hex)
				if neighbor_priority <= my_priority:
					continue

				# Debug: log any fringe drawn onto hex 56,25
				if n_col == 56 and n_row == 25:
					print("[DEBUG] DRAWING fringe onto (56,25) from hex(%d,%d) terrain=%d->%d priority=%d->%d edge=%d" % [x, y, terrain_id, neighbor_id, my_priority, neighbor_priority, edge_idx])

				# Get the edge overlay for the neighbor's terrain at THIS edge
				# (the fringe extends from neighbor toward my center)
				var cache_key := "%d_%d" % [neighbor_id, edge_idx]
				var overlay_tex: ImageTexture = _edge_overlay_cache.get(cache_key)
				if overlay_tex == null:
					continue

				# Draw the overlay at this hex's position
				var rect := Rect2(
					center.x - tex_size * 0.5,
					center.y - tex_size * 0.5,
					tex_size,
					tex_size
				)
				draw_texture_rect(overlay_tex, rect, false)

func _get_neighbor_offset_x_2d(row: int, dir: int) -> int:
	# Hex neighbor X offsets for odd-r offset coordinates
	var is_odd := (row % 2) == 1
	match dir:
		0: return 1   # E
		1: return 1 if is_odd else 0   # NE
		2: return 0 if is_odd else -1  # NW
		3: return -1  # W
		4: return 0 if is_odd else -1  # SW
		5: return 1 if is_odd else 0   # SE
	return 0

func _get_neighbor_offset_y_2d(dir: int) -> int:
	# Hex neighbor Y offsets
	match dir:
		0: return 0   # E
		1: return -1  # NE
		2: return -1  # NW
		3: return 0   # W
		4: return 1   # SW
		5: return 1   # SE
	return 0

# --- End Terrain Texture System ---

# --- 2D Minimap System (uses shared MinimapPanel) ---

func _setup_2d_minimap() -> void:
	_minimap_2d = MinimapPanelScript.new()
	add_child(_minimap_2d)
	_minimap_2d.setup(self, 102)
	_minimap_2d.pan_requested.connect(_on_minimap_2d_pan_requested)
	_minimap_2d.connect_indicator_draw(_draw_minimap_viewport_indicator)

func _update_2d_minimap() -> void:
	if _minimap_2d == null or grid_width == 0 or grid_height == 0:
		return

	# Hide 2D minimap when 3D view is active (3D view has its own minimap)
	if _is_heightfield_visible():
		_minimap_2d.set_visible(false)
		return
	_minimap_2d.set_visible(true)

	# Check if we need to regenerate the minimap image
	var current_size := Vector2i(grid_width, grid_height)
	var needs_rebuild := _minimap_2d_image == null or _minimap_2d_last_grid_size != current_size

	if needs_rebuild:
		_minimap_2d_last_grid_size = current_size
		_rebuild_minimap_2d_image()

	# Update viewport indicator
	_minimap_2d.queue_indicator_redraw()

func _rebuild_minimap_2d_image() -> void:
	if grid_width == 0 or grid_height == 0:
		return

	# Pre-allocate byte array for RGB8 image data (3 bytes per pixel)
	# This is O(n) instead of O(n²) set_pixel() calls
	var pixel_count := grid_width * grid_height
	var data := PackedByteArray()
	data.resize(pixel_count * 3)

	# Cache terrain colors lookup for faster access
	var colors := _get_terrain_colors()
	var fallback_color := Color(0.2, 0.2, 0.2, 1.0)

	# Fill byte array with terrain colors in a single pass
	var byte_index := 0
	for i in range(pixel_count):
		var terrain_id := int(terrain_overlay[i]) if i < terrain_overlay.size() else -1
		var color: Color = colors.get(terrain_id, fallback_color)
		# Convert Color (0-1 floats) to RGB bytes (0-255)
		data[byte_index] = int(color.r * 255.0)
		data[byte_index + 1] = int(color.g * 255.0)
		data[byte_index + 2] = int(color.b * 255.0)
		byte_index += 3

	# Create image from byte array
	_minimap_2d_image = Image.create_from_data(grid_width, grid_height, false, Image.FORMAT_RGB8, data)

	# Create texture from image and update panel
	var tex := ImageTexture.create_from_image(_minimap_2d_image)
	_minimap_2d.set_texture(tex)
	_minimap_2d.set_grid_size(grid_width, grid_height)

## Draw the viewport indicator rectangle on the 2D minimap.
##
## This shows which portion of the map is currently visible in the main view.
## The coordinate transformation uses the same axial hex math as _point_to_offset:
##
## Screen-to-Hex Coordinate Conversion (pointy-top hexes):
##   1. Subtract origin and divide by hex radius to get relative position
##   2. Convert to axial coordinates (q, r) using pointy-top hex formulas:
##      q = (sqrt(3)/3 * x - 1/3 * y)
##      r = (2/3 * y)
##   3. Round to nearest hex using cube coordinate rounding
##   4. Convert axial (q, r) to offset (col, row) coordinates
##
## The resulting hex coordinates are then normalized to [0,1] range and
## mapped to pixel positions within the minimap texture display area.
func _draw_minimap_viewport_indicator() -> void:
	if _minimap_2d == null or grid_width == 0 or grid_height == 0:
		return
	if last_hex_radius <= 0:
		return

	var viewport_size := _get_adjusted_viewport_size()
	if viewport_size.x <= 0 or viewport_size.y <= 0:
		return

	var radius: float = max(last_hex_radius, 0.0001)

	# Convert top-left screen corner to hex offset coordinates
	# Step 1: Get position relative to hex origin, normalized by radius
	var tl_relative: Vector2 = (Vector2.ZERO - last_origin) / radius
	# Step 2: Convert to fractional axial coordinates (pointy-top hex formula)
	var tl_qf: float = (SQRT3 / 3.0) * tl_relative.x - (1.0 / 3.0) * tl_relative.y
	var tl_rf: float = (2.0 / 3.0) * tl_relative.y
	# Step 3: Round to nearest hex center using cube coordinates
	var tl_axial := _cube_round(tl_qf, tl_rf)
	# Step 4: Convert axial to offset (col, row) coordinates
	var tl_offset := _axial_to_offset(tl_axial.x, tl_axial.y)

	# Convert bottom-right screen corner to hex offset coordinates (same steps)
	var br_relative: Vector2 = (viewport_size - last_origin) / radius
	var br_qf: float = (SQRT3 / 3.0) * br_relative.x - (1.0 / 3.0) * br_relative.y
	var br_rf: float = (2.0 / 3.0) * br_relative.y
	var br_axial := _cube_round(br_qf, br_rf)
	var br_offset := _axial_to_offset(br_axial.x, br_axial.y)

	# Normalize hex coordinates to [0,1] range for minimap positioning
	var view_left := clampf(float(tl_offset.x) / float(grid_width), 0.0, 1.0)
	var view_right := clampf(float(br_offset.x + 1) / float(grid_width), 0.0, 1.0)
	var view_top := clampf(float(tl_offset.y) / float(grid_height), 0.0, 1.0)
	var view_bottom := clampf(float(br_offset.y + 1) / float(grid_height), 0.0, 1.0)

	# Map normalized coords to pixel positions within minimap texture display area
	var texture_display_rect: Rect2 = _minimap_2d.get_texture_display_rect()
	var rect := Rect2(
		texture_display_rect.position.x + view_left * texture_display_rect.size.x,
		texture_display_rect.position.y + view_top * texture_display_rect.size.y,
		(view_right - view_left) * texture_display_rect.size.x,
		(view_bottom - view_top) * texture_display_rect.size.y
	)

	var indicator_color := Color(1.0, 1.0, 1.0, 0.8)
	_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)

## Handle minimap click/drag to pan the main view.
##
## Converts the normalized minimap position (0-1) to hex grid coordinates,
## then calculates the pan_offset needed to center that hex in the viewport.
##
## normalized_pos: Position within minimap texture, (0,0)=top-left, (1,1)=bottom-right
func _on_minimap_2d_pan_requested(normalized_pos: Vector2) -> void:
	if grid_width == 0 or grid_height == 0:
		return
	if last_hex_radius <= 0:
		return

	# Convert normalized [0,1] position to hex grid coordinates (col, row)
	var target_col := int(normalized_pos.x * float(grid_width))
	var target_row := int(normalized_pos.y * float(grid_height))
	target_col = clampi(target_col, 0, grid_width - 1)
	target_row = clampi(target_row, 0, grid_height - 1)

	# Get the screen position of target hex at base origin (before any panning)
	var hex_center_at_base := _hex_center(target_col, target_row, last_hex_radius, last_base_origin)

	# Calculate pan_offset to center this hex in the viewport:
	# viewport_center = hex_center_at_base + pan_offset
	# Therefore: pan_offset = viewport_center - hex_center_at_base
	var viewport_size := _get_adjusted_viewport_size()
	var viewport_center := viewport_size * 0.5
	pan_offset = viewport_center - hex_center_at_base

	_clamp_pan_offset()
	_update_layout_metrics()
	queue_redraw()
	_update_2d_minimap()

# --- End 2D Minimap System ---
