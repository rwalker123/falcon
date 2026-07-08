extends ScrollContainer
class_name TerrainInspectorPanel

## Inspector "Terrain" tab: the map-inspection surface. Owns the biome histogram, the
## biome drill-down, the tile list + detail, the runtime terrain-highlight dropdown, and
## the Terrain-tab command buttons (export map / scout).
##
## Snapshot-driven (in _tab_panels): apply_update() ingests tiles / tile_updates /
## tile_removed / food_modules and renders. Collaborators:
##  - set_map_view: terrain-highlight (set_terrain_highlight) + tile height readout.
##  - set_command_hooks: export_map sends directly; scout emits a signal so the
##    coordinator can resolve the active faction (like FaunaPanel).
##  - set_command_connected: gate the tile action buttons.
##  - set_terrain_palette / set_terrain_tag_labels: the biome palette + tag labels arrive
##    on the `overlays` snapshot key, which the coordinator fans out (also to Overlay/Crisis).
##  - focus_tile_from_map: inbound MapView hex-selection (coordinator forwards it).
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")
const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

const MOUNTAIN_KIND_LABELS := {
	0: "None",
	1: "Fold Belt",
	2: "Fault Block Range",
	3: "Volcanic Chain",
	4: "Dome Plateau",
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

const TERRAIN_HISTOGRAM_BAR_WIDTH = 16
const TAG_TOP_LIMIT = 6
const TERRAIN_TILE_DISPLAY_LIMIT = 24
const TERRAIN_BIOME_SAMPLE_LIMIT = 6

@onready var terrain_vbox: VBoxContainer = $TerrainVBox
@onready var terrain_text: RichTextLabel = $TerrainVBox/TerrainText
@onready var export_map_button: Button = $TerrainVBox/ExportRow/ExportMapButton
@onready var terrain_biome_section_label: Label = $TerrainVBox/BiomeSection/BiomeSectionLabel
@onready var terrain_biome_list: ItemList = $TerrainVBox/BiomeSection/BiomeList
@onready var terrain_biome_detail_text: RichTextLabel = $TerrainVBox/BiomeSection/BiomeDetailText
@onready var terrain_tile_section_label: Label = $TerrainVBox/TileSection/TileSectionLabel
@onready var terrain_tile_list: ItemList = $TerrainVBox/TileSection/TileList
@onready var terrain_tile_detail_text: RichTextLabel = $TerrainVBox/TileSection/TileDetailText
@onready var tile_scout_button: Button = $TerrainVBox/TileSection/TileActionRow/TileScoutButton

var _terrain_highlight_dropdown: OptionButton = null

var _terrain_palette: Dictionary = {}
var _terrain_tag_labels: Dictionary = {}
var _tile_records: Dictionary = {}
var _terrain_counts: Dictionary = {}
var _terrain_tag_counts: Dictionary = {}
var _tile_total: int = 0
var _food_modules: Array = []
var _food_module_lookup: Dictionary = {}
var _biome_entries: Array[Dictionary] = []
var _biome_tile_lookup: Dictionary = {}
var _biome_index_lookup: Dictionary = {}
var _selected_biome_id: int = -1
var _selected_tile_entity: int = -1
var _selected_tile_coords: Vector2i = Vector2i(-1, -1)
var _hovered_tile_entity: int = -1
var _tile_coord_lookup: Dictionary = {}

## Pushed by the coordinator; MapView drives terrain highlight + tile height readout.
var _map_view: Node = null
## Command hook: (line: String, success_msg: String) -> bool.
var _send: Callable = Callable()
## Command-log sink: (entry: String) -> void.
var _append_log_sink: Callable = Callable()
var _connected: bool = false

## Panel -> coordinator: faction-needing tile commands (coordinator resolves faction, sends).
signal tile_scout_requested(x: int, y: int)

func _ready() -> void:
	_connect_terrain_ui()
	_setup_terrain_highlight_dropdown()
	if export_map_button != null:
		export_map_button.pressed.connect(_on_export_map_button_pressed)
	if tile_scout_button != null:
		tile_scout_button.pressed.connect(_on_tile_scout_button_pressed)
	_render_terrain()
	_apply_enabled()

## Coordinator contract: ingest tile + food-module snapshot keys, then re-render.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	if data.has("food_modules"):
		_ingest_food_modules(data["food_modules"])
	if full_snapshot and data.has("tiles"):
		_rebuild_tiles(data["tiles"])
	elif data.has("tile_updates"):
		_apply_tile_updates(data["tile_updates"])
	if data.has("tile_removed"):
		_remove_tiles(data["tile_removed"])
	_render_terrain()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_terrain_palette.clear()
	_terrain_tag_labels.clear()
	_tile_records.clear()
	_terrain_counts.clear()
	_terrain_tag_counts.clear()
	_tile_total = 0
	_food_modules.clear()
	_food_module_lookup.clear()
	_selected_tile_entity = -1
	_selected_tile_coords = Vector2i(-1, -1)
	_clear_terrain_ui()
	_render_terrain()
	_apply_enabled()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if terrain_text != null:
		Typography.apply(terrain_text, Typography.STYLE_BODY)
	if terrain_biome_detail_text != null:
		Typography.apply(terrain_biome_detail_text, Typography.STYLE_BODY)
	if terrain_tile_detail_text != null:
		Typography.apply(terrain_tile_detail_text, Typography.STYLE_BODY)
	if terrain_biome_list != null:
		Typography.apply(terrain_biome_list, Typography.STYLE_BODY)
	if terrain_tile_list != null:
		Typography.apply(terrain_tile_list, Typography.STYLE_BODY)
	if terrain_biome_section_label != null:
		Typography.apply(terrain_biome_section_label, Typography.STYLE_HEADING)
	if terrain_tile_section_label != null:
		Typography.apply(terrain_tile_section_label, Typography.STYLE_HEADING)
	# Interactive controls: match MapPanel/OverlayPanel so the buttons + terrain-highlight
	# dropdown carry the same STYLE_CONTROL sizing as every other inspector panel.
	for control in [
		export_map_button, tile_scout_button,
		_terrain_highlight_dropdown
	]:
		if control != null:
			Typography.apply(control, Typography.STYLE_CONTROL)

## Coordinator collaborator: the map view driving highlight + height readout.
func set_map_view(view: Node) -> void:
	_map_view = view

## Coordinator collaborator: inject the command hook + log sink.
func set_command_hooks(send: Callable, append_log: Callable) -> void:
	_send = send
	_append_log_sink = append_log

## Guarded command dispatch — mirrors MapPanel/CrisisPanel so a cleared or not-yet-wired
## hook (during a disconnect/reset flow) is a no-op, not an error.
func _call_send(line: String, message: String) -> bool:
	if _send.is_valid():
		return bool(_send.call(line, message))
	return false

func _call_log(text: String) -> void:
	if _append_log_sink.is_valid():
		_append_log_sink.call(text)

## Coordinator contract: connection-gated enable/disable of the tile action buttons.
func set_command_connected(connected: bool) -> void:
	_connected = connected
	_apply_enabled()

## Coordinator collaborator: the biome palette (arrives on the overlays snapshot key).
## Re-render so biome labels refresh if the palette lands after tiles (or on reload) —
## otherwise the histogram/drill-down stay on the "Terrain N" fallback until the next tile update.
func set_terrain_palette(palette: Dictionary) -> void:
	_terrain_palette = palette.duplicate(true)
	_render_terrain()

## Coordinator collaborator: the tag labels (arrives on the overlays snapshot key).
## Re-render so tag labels refresh if they arrive after tiles — otherwise the drill-down and
## tile detail keep showing the "Tag N" fallback until a later tile selection/update.
func set_terrain_tag_labels(labels: Dictionary) -> void:
	_terrain_tag_labels = labels.duplicate(true)
	_render_terrain()

## Coordinator collaborator: tag labels feed OverlayPanel's terrain-tags channel.
func get_terrain_tag_labels() -> Dictionary:
	return _terrain_tag_labels

func _apply_enabled() -> void:
	var has_tile_target := _selected_tile_coords.x >= 0 and _selected_tile_coords.y >= 0
	if tile_scout_button != null:
		tile_scout_button.disabled = not (_connected and has_tile_target)

func _connect_terrain_ui() -> void:
	if terrain_biome_list != null:
		var biome_selected_callable = Callable(self, "_on_terrain_biome_selected")
		if not terrain_biome_list.is_connected("item_selected", biome_selected_callable):
			terrain_biome_list.item_selected.connect(_on_terrain_biome_selected)
		if not terrain_biome_list.is_connected("item_activated", biome_selected_callable):
			terrain_biome_list.item_activated.connect(_on_terrain_biome_selected)
	if terrain_tile_list != null:
		var tile_selected_callable = Callable(self, "_on_terrain_tile_selected")
		if not terrain_tile_list.is_connected("item_selected", tile_selected_callable):
			terrain_tile_list.item_selected.connect(_on_terrain_tile_selected)
		if not terrain_tile_list.is_connected("item_activated", tile_selected_callable):
			terrain_tile_list.item_activated.connect(_on_terrain_tile_selected)
		var tile_gui_callable = Callable(self, "_on_terrain_tile_gui_input")
		if not terrain_tile_list.is_connected("gui_input", tile_gui_callable):
			terrain_tile_list.gui_input.connect(_on_terrain_tile_gui_input)

func _histogram_bar(fraction: float) -> String:
	var clamped: float = clampf(fraction, 0.0, 1.0)
	var filled: int = clampi(int(round(clamped * float(TERRAIN_HISTOGRAM_BAR_WIDTH))), 0, TERRAIN_HISTOGRAM_BAR_WIDTH)
	return "█".repeat(filled) + "░".repeat(TERRAIN_HISTOGRAM_BAR_WIDTH - filled)

func _render_terrain() -> void:
	if terrain_text == null:
		return

	if _tile_total <= 0:
		terrain_text.text = """[b]Terrain Overlay[/b]
No terrain data received yet. Palette legend remains available on the HUD."""
		_clear_terrain_ui()
		return

	var lines: Array[String] = []
	lines.append("[b]Terrain Overview[/b]")
	lines.append("Tracked tiles: %d" % _tile_total)

	var terrain_entries: Array[Dictionary] = []
	for key in _terrain_counts.keys():
		var terrain_id = int(key)
		var count = int(_terrain_counts[key])
		if count <= 0:
			continue
		var percent = (float(count) / float(max(_tile_total, 1))) * 100.0
		terrain_entries.append({
			"id": terrain_id,
			"count": count,
			"percent": percent,
			"label": _label_for_terrain(terrain_id)
		})
	terrain_entries.sort_custom(Callable(self, "_compare_terrain_entries"))

	if terrain_entries.size() > 0:
		lines.append("Biome histogram (%d biomes, bars scaled to most common):" % terrain_entries.size())
		var max_count: int = 0
		for entry in terrain_entries:
			max_count = max(max_count, int(entry.get("count", 0)))
		for entry in terrain_entries:
			var fraction: float = float(int(entry.get("count", 0))) / float(max(max_count, 1))
			lines.append("[code]%s[/code] %s — %d (%.1f%%)"
				% [_histogram_bar(fraction), entry.get("label", "Unknown"), int(entry.get("count", 0)), float(entry.get("percent", 0.0))])

	var tag_entries: Array[Dictionary] = []
	for key in _terrain_tag_counts.keys():
		var mask = int(key)
		var count = int(_terrain_tag_counts[key])
		if count <= 0:
			continue
		var percent = (float(count) / float(max(_tile_total, 1))) * 100.0
		tag_entries.append({
			"mask": mask,
			"count": count,
			"percent": percent,
			"label": _label_for_tag(mask)
		})
	tag_entries.sort_custom(Callable(self, "_compare_tag_entries"))

	var tag_limit: int = min(tag_entries.size(), TAG_TOP_LIMIT)
	if tag_limit > 0:
		lines.append("")
		lines.append("Tag coverage:")
		for idx in range(tag_limit):
			var entry2: Dictionary = tag_entries[idx]
			lines.append(" • %s: %d tiles (%.1f%%)"
				% [entry2.get("label", "Tag"), int(entry2.get("count", 0)), float(entry2.get("percent", 0.0))])

	terrain_text.text = "\n".join(lines)
	_refresh_biome_section(terrain_entries)

func _clear_terrain_ui() -> void:
	_biome_entries.clear()
	_biome_tile_lookup.clear()
	_biome_index_lookup.clear()
	_tile_coord_lookup.clear()
	_selected_biome_id = -1
	_selected_tile_entity = -1
	_hovered_tile_entity = -1
	# Drop the command target too, so clearing terrain (e.g. tile_removed emptying the
	# panel) re-gates Scout/Found instead of leaving them enabled against a stale hex.
	_selected_tile_coords = Vector2i(-1, -1)
	_apply_enabled()
	if terrain_biome_list != null:
		terrain_biome_list.clear()
	if terrain_biome_detail_text != null:
		terrain_biome_detail_text.text = """[b]Biome Drill-down[/b]
Select a biome once terrain data is available."""
	if terrain_tile_list != null:
		terrain_tile_list.clear()
	if terrain_tile_detail_text != null:
		terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
Hover or select a tile to inspect biome tags and conditions."""

func _refresh_biome_section(entries: Array[Dictionary]) -> void:
	_biome_entries = entries.duplicate(true)
	_build_biome_tile_lookup()
	_biome_index_lookup.clear()
	for idx in range(_biome_entries.size()):
		var entry: Dictionary = _biome_entries[idx]
		var biome_id: int = int(entry.get("id", -1))
		_biome_index_lookup[biome_id] = idx
	_update_biome_list()

func _build_biome_tile_lookup() -> void:
	var lookup: Dictionary = {}
	for key in _tile_records.keys():
		var entity_id: int = int(key)
		var record_variant: Variant = _tile_records[key]
		if not (record_variant is Dictionary):
			continue
		var record: Dictionary = record_variant
		var terrain_id: int = int(record.get("terrain", -1))
		if terrain_id < 0:
			continue
		var tile_list: Array = []
		if lookup.has(terrain_id):
			tile_list = lookup[terrain_id]
		tile_list.append(entity_id)
		lookup[terrain_id] = tile_list
	_biome_tile_lookup = lookup

func _format_biome_list_entry(entry: Dictionary) -> String:
	var label: String = str(entry.get("label", "Biome"))
	var count: int = int(entry.get("count", 0))
	var percent: float = float(entry.get("percent", 0.0))
	return "%s – %d tiles (%.1f%%)" % [label, count, percent]

## Builds the Terrain-tab "highlight all tiles of a type" dropdown. Lists EVERY defined
## terrain (not just those present), so it doubles as a check for absent biomes — e.g.
## selecting AlpineMountain and seeing no highlights confirms none were generated.
func _setup_terrain_highlight_dropdown() -> void:
	if terrain_vbox == null:
		return
	_terrain_highlight_dropdown = OptionButton.new()
	_terrain_highlight_dropdown.name = "TerrainHighlightDropdown"
	_terrain_highlight_dropdown.add_item("Highlight terrain: none", -1)
	var terrains: Array = TerrainDefinitions.get_terrains().duplicate()
	terrains.sort_custom(func(a, b): return String(a.get("label", "")) < String(b.get("label", "")))
	for terrain in terrains:
		var id: int = int(terrain.get("id", -1))
		if id < 0:
			continue
		_terrain_highlight_dropdown.add_item(String(terrain.get("label", "Terrain %d" % id)), id)
	_terrain_highlight_dropdown.item_selected.connect(_on_terrain_highlight_selected)
	terrain_vbox.add_child(_terrain_highlight_dropdown)
	# Sit just under the tab's intro text, above the biome list.
	terrain_vbox.move_child(_terrain_highlight_dropdown, 1)

func _on_terrain_highlight_selected(index: int) -> void:
	if _terrain_highlight_dropdown == null:
		return
	var terrain_id: int = _terrain_highlight_dropdown.get_item_id(index)
	if _map_view != null and _map_view.has_method("set_terrain_highlight"):
		_map_view.call("set_terrain_highlight", terrain_id)

func _update_biome_list() -> void:
	if terrain_biome_list == null:
		return
	var previous_biome: int = _selected_biome_id
	terrain_biome_list.clear()
	var selection_index: int = -1
	for idx in range(_biome_entries.size()):
		var entry: Dictionary = _biome_entries[idx]
		terrain_biome_list.add_item(_format_biome_list_entry(entry))
		terrain_biome_list.set_item_metadata(idx, entry)
		if int(entry.get("id", -1)) == previous_biome:
			selection_index = idx
	var force_tile_reset: bool = false
	if selection_index >= 0:
		terrain_biome_list.select(selection_index)
	elif _biome_entries.size() > 0:
		selection_index = 0
		terrain_biome_list.select(selection_index)
		force_tile_reset = true
	else:
		_selected_biome_id = -1
		_render_selected_biome(true)
		return
	var selected_entry: Dictionary = _biome_entries[selection_index]
	var new_biome_id: int = int(selected_entry.get("id", -1))
	var selection_changed: bool = previous_biome != new_biome_id
	_selected_biome_id = new_biome_id
	_render_selected_biome(force_tile_reset or selection_changed)

func _render_selected_biome(reset_tile_selection: bool, pinned_tile_entity: int = -1) -> void:
	if terrain_biome_list == null:
		return
	var selected_items: PackedInt32Array = terrain_biome_list.get_selected_items()
	if selected_items.is_empty():
		_selected_biome_id = -1
		if terrain_biome_detail_text != null:
			terrain_biome_detail_text.text = """[b]Biome Drill-down[/b]
Select a biome to view tag breakdowns and representative tiles."""
		_refresh_tile_list(true, pinned_tile_entity)
		return
	var index: int = selected_items[0]
	var entry_variant: Variant = terrain_biome_list.get_item_metadata(index)
	var entry: Dictionary = entry_variant if entry_variant is Dictionary else {}
	if entry.is_empty() and index < _biome_entries.size():
		entry = _biome_entries[index]
	var biome_id: int = int(entry.get("id", -1))
	var label: String = str(entry.get("label", "Biome"))
	var count: int = int(entry.get("count", 0))
	var percent: float = float(entry.get("percent", 0.0))
	_selected_biome_id = biome_id

	if terrain_biome_detail_text != null:
		var lines: Array[String] = []
		lines.append("[b]%s[/b]" % label)
		lines.append("Tile coverage: %d (%.1f%% of tracked terrain)" % [count, percent])
		var tile_list: Array = _get_biome_tiles(biome_id)
		lines.append("Tracked tiles in biome: %d" % tile_list.size())
		var tag_summary: Array[Dictionary] = _summarize_biome_tags(biome_id)
		if tag_summary.is_empty():
			lines.append("Tag breakdown: none")
		else:
			lines.append("Tag breakdown:")
			var tag_limit: int = min(tag_summary.size(), TAG_TOP_LIMIT)
			for tag_idx in range(tag_limit):
				var tag_entry: Dictionary = tag_summary[tag_idx]
				lines.append(" • %s: %d tiles (%.1f%%)" % [
					tag_entry.get("label", "Tag"),
					int(tag_entry.get("count", 0)),
					float(tag_entry.get("percent", 0.0))
				])
		var sample_lines: Array[String] = _format_representative_tiles(biome_id)
		lines.append("")
		if sample_lines.is_empty():
			lines.append("Representative tiles: none recorded.")
		else:
			lines.append("Representative tiles:")
			for sample_line in sample_lines:
				lines.append(sample_line)
		terrain_biome_detail_text.text = "\n".join(lines)

	_refresh_tile_list(reset_tile_selection, pinned_tile_entity)

func _summarize_biome_tags(biome_id: int) -> Array[Dictionary]:
	var tile_list: Array = _get_biome_tiles(biome_id)
	if tile_list.is_empty():
		return []
	var counts: Dictionary = {}
	for entity_id in tile_list:
		var record_variant: Variant = _tile_records.get(entity_id, {})
		if not (record_variant is Dictionary):
			continue
		var record: Dictionary = record_variant
		var mask: int = int(record.get("tags", 0))
		if mask == 0:
			continue
		for bit in range(0, 16):
			var bit_value: int = 1 << bit
			if (mask & bit_value) == 0:
				continue
			counts[bit_value] = int(counts.get(bit_value, 0)) + 1
	var result: Array[Dictionary] = []
	var total: float = float(max(tile_list.size(), 1))
	for key in counts.keys():
		var bit_mask: int = int(key)
		var count: int = int(counts[key])
		result.append({
			"mask": bit_mask,
			"count": count,
			"percent": (float(count) / total) * 100.0,
			"label": _label_for_tag(bit_mask)
		})
	result.sort_custom(Callable(self, "_compare_tag_entries"))
	return result

func _get_biome_tiles(biome_id: int) -> Array:
	if biome_id < 0:
		return []
	if not _biome_tile_lookup.has(biome_id):
		return []
	var stored: Variant = _biome_tile_lookup[biome_id]
	if stored is Array:
		return (stored as Array).duplicate()
	return []

func _format_representative_tiles(biome_id: int) -> Array[String]:
	var tile_list: Array = _get_biome_tiles(biome_id)
	if tile_list.is_empty():
		return []
	tile_list.sort()
	var sample_limit: int = min(tile_list.size(), TERRAIN_BIOME_SAMPLE_LIMIT)
	var result: Array[String] = []
	for idx in range(sample_limit):
		var entity_id: int = int(tile_list[idx])
		var record_variant: Variant = _tile_records.get(entity_id, {})
		if not (record_variant is Dictionary):
			continue
		var record: Dictionary = record_variant
		var coords_text: String = _format_tile_coords(record)
		var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
		var tags_text: String = "none"
		if not tags.is_empty():
			tags_text = _join_strings_with_separator(tags, ", ")
		var temperature: float = float(record.get("temperature", 0.0))
		var mass: float = float(record.get("mass", 0.0))
		result.append(" • %s | entity %d | tags: %s | temp %.1f | mass %.1f" % [
			coords_text,
			entity_id,
			tags_text,
			temperature,
			mass
		])
	return result

func _refresh_tile_list(reset_tile_selection: bool, pinned_entity: int = -1) -> void:
	if terrain_tile_list == null:
		return
	var previous_tile: int = _selected_tile_entity
	terrain_tile_list.clear()
	var tile_entities: Array = _get_biome_tiles(_selected_biome_id)
	tile_entities.sort()
	var display_limit: int = min(tile_entities.size(), TERRAIN_TILE_DISPLAY_LIMIT)
	var display_entities: Array = []
	for idx in range(display_limit):
		display_entities.append(int(tile_entities[idx]))
	if pinned_entity >= 0 and tile_entities.has(pinned_entity) and display_entities.find(pinned_entity) == -1:
		display_entities.append(pinned_entity)

	var selected_index: int = -1
	for idx in range(display_entities.size()):
		var entity_id: int = int(display_entities[idx])
		var record_variant: Variant = _tile_records.get(entity_id, {})
		if not (record_variant is Dictionary):
			continue
		var record: Dictionary = record_variant
		terrain_tile_list.add_item(_format_tile_list_entry(entity_id, record))
		var new_index: int = terrain_tile_list.get_item_count() - 1
		terrain_tile_list.set_item_metadata(new_index, entity_id)
		if entity_id == pinned_entity:
			selected_index = new_index
		elif entity_id == previous_tile and selected_index == -1:
			selected_index = new_index

	if terrain_tile_list.get_item_count() == 0:
		_selected_tile_entity = -1
		_render_tile_detail(-1)
		return

	var effective_previous: int = previous_tile
	if pinned_entity >= 0:
		effective_previous = pinned_entity

	var should_reset_tile: bool = reset_tile_selection or effective_previous < 0 or tile_entities.find(effective_previous) == -1
	var target_index: int = selected_index

	if target_index < 0:
		if not should_reset_tile:
			for idx in range(terrain_tile_list.get_item_count()):
				var entity_candidate: int = int(terrain_tile_list.get_item_metadata(idx))
				if entity_candidate == effective_previous:
					target_index = idx
					break
		if target_index < 0:
			target_index = 0

	var target_entity: int = int(terrain_tile_list.get_item_metadata(target_index))
	_selected_tile_entity = target_entity
	terrain_tile_list.select(target_index)
	_hovered_tile_entity = -1
	_render_tile_detail(target_entity)

func _format_tile_list_entry(entity_id: int, record: Dictionary) -> String:
	var coords_text: String = _format_tile_coords(record)
	var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
	var preview_tags: Array[String] = []
	var preview_limit: int = min(tags.size(), 2)
	for idx in range(preview_limit):
		preview_tags.append(tags[idx])
	var parts: Array[String] = []
	parts.append(coords_text)
	parts.append("entity %d" % entity_id)
	var mountain_kind: int = int(record.get("mountain_kind", 0))
	if mountain_kind > 0:
		parts.append(_label_for_mountain_kind(mountain_kind))
	if not preview_tags.is_empty():
		parts.append(_join_strings_with_separator(preview_tags, ", "))
	var module_key: String = String(record.get("food_module", "")).strip_edges()
	if module_key != "":
		parts.append(_label_for_food_module(module_key))
	return _join_strings_with_separator(parts, " • ")

func _format_tile_coords(record: Dictionary) -> String:
	var x: int = int(record.get("x", -1))
	var y: int = int(record.get("y", -1))
	return "@%d,%d" % [x, y]

## Relative "Height" lives only in the normalized ElevationField raster (surfaced by
## MapView), not on the per-tile record, so this asks MapView for the 0..100 reading
## and its shared bar formatting, and returns "" when coords are invalid or no
## elevation data is available yet.
func _tile_height_text(x: int, y: int) -> String:
	if x < 0 or y < 0:
		return ""
	if _map_view == null or not _map_view.has_method("relative_height_at"):
		return ""
	var height: int = int(_map_view.call("relative_height_at", x, y))
	if height < 0:
		return ""
	return String(_map_view.call("format_height", height))

func _render_tile_detail(entity_id: int, preview: bool = false) -> void:
	if terrain_tile_detail_text == null:
		return
	if entity_id < 0 or not _tile_records.has(entity_id):
		_selected_tile_coords = Vector2i(-1, -1)
		_apply_enabled()
		terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
Hover or select a tile to inspect biome tags and conditions."""
		return
	var record_variant: Variant = _tile_records.get(entity_id, {})
	if not (record_variant is Dictionary):
		_selected_tile_coords = Vector2i(-1, -1)
		_apply_enabled()
		terrain_tile_detail_text.text = "No data for tile %d." % entity_id
		return
	var record: Dictionary = record_variant
	_selected_tile_coords = Vector2i(
		int(record.get("x", -1)),
		int(record.get("y", -1))
	)
	var lines: Array[String] = []
	lines.append("[b]Tile %d[/b]" % entity_id)
	lines.append("Location: %s" % _format_tile_coords(record))
	lines.append("Biome: %s" % _label_for_terrain(int(record.get("terrain", -1))))
	var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
	var tags_text: String = "none"
	if not tags.is_empty():
		tags_text = _join_strings_with_separator(tags, ", ")
	lines.append("Tags: %s" % tags_text)
	var module_key: String = String(record.get("food_module", "")).strip_edges()
	if module_key != "":
		var module_label: String = _label_for_food_module(module_key)
		var seasonal_weight: float = float(record.get("food_module_weight", 0.0))
		if seasonal_weight > 0.0:
			lines.append("Food Module: %s (seasonal weight %.2f)" % [module_label, seasonal_weight])
		else:
			lines.append("Food Module: %s" % module_label)
	else:
		lines.append("Food Module: none")
	lines.append("Temperature: %.1f" % float(record.get("temperature", 0.0)))
	lines.append("Mass: %.1f" % float(record.get("mass", 0.0)))
	var height_text: String = _tile_height_text(int(record.get("x", -1)), int(record.get("y", -1)))
	if height_text != "":
		lines.append("Height: %s" % height_text)
	lines.append("Element ID: %d" % int(record.get("element", -1)))
	var mountain_kind: int = int(record.get("mountain_kind", 0))
	if mountain_kind > 0:
		var relief_scale: float = float(record.get("mountain_relief", 1.0))
		lines.append("Mountain: %s (relief ×%.2f)" % [_label_for_mountain_kind(mountain_kind), relief_scale])
	else:
		lines.append("Mountain: none")
	if preview:
		lines.append("")
		lines.append("[i]Hover preview[/i]")
	terrain_tile_detail_text.text = "\n".join(lines)
	_apply_enabled()

func _tag_labels_from_mask(mask: int) -> Array[String]:
	var labels: Array[String] = []
	if mask == 0:
		return labels
	for bit in range(0, 16):
		var bit_value: int = 1 << bit
		if (mask & bit_value) != 0:
			labels.append(_label_for_tag(bit_value))
	return labels

func _label_for_mountain_kind(kind: int) -> String:
	var value: Variant = MOUNTAIN_KIND_LABELS.get(kind, "Unknown")
	return str(value)

func _label_for_food_module(key: String) -> String:
	if key == "":
		return "Unknown"
	return String(FOOD_MODULE_LABELS.get(key, key.capitalize().replace("_", " ")))

func focus_tile_from_map(col: int, row: int, terrain_id: int) -> void:
	if terrain_biome_list == null:
		return
	var coord := Vector2i(col, row)
	var entity_id: int = -1
	if _tile_coord_lookup.has(coord):
		entity_id = int(_tile_coord_lookup[coord])
	else:
		for key in _tile_records.keys():
			var record_variant: Variant = _tile_records[key]
			if not (record_variant is Dictionary):
				continue
			var record: Dictionary = record_variant
			if int(record.get("x", -1)) == col and int(record.get("y", -1)) == row:
				entity_id = int(key)
				_tile_coord_lookup[coord] = entity_id
				break

	if terrain_id >= 0 and _biome_entries.size() > 0:
		var biome_index: int = int(_biome_index_lookup.get(terrain_id, -1))
		if biome_index >= 0:
			var previous_biome: int = _selected_biome_id
			var reset_required: bool = previous_biome != terrain_id
			terrain_biome_list.select(biome_index, false)
			var selected_indices: PackedInt32Array = terrain_biome_list.get_selected_items()
			if selected_indices.is_empty() or selected_indices[0] != biome_index:
				terrain_biome_list.select(biome_index, false)
			_selected_biome_id = terrain_id
			_render_selected_biome(reset_required, entity_id)
		else:
			_render_selected_biome(false, entity_id)
	else:
		_render_selected_biome(false, entity_id)

	if entity_id < 0 and _selected_tile_entity < 0 and terrain_tile_detail_text != null:
		terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
No detailed data available for the selected tile (%d, %d).""" % [col, row]

func _on_terrain_biome_selected(index: int) -> void:
	if terrain_biome_list == null:
		return
	if index < 0 or index >= terrain_biome_list.get_item_count():
		return
	var metadata: Variant = terrain_biome_list.get_item_metadata(index)
	var biome_id: int = -1
	if metadata is Dictionary:
		var entry: Dictionary = metadata
		biome_id = int(entry.get("id", -1))
	elif index < _biome_entries.size():
		biome_id = int(_biome_entries[index].get("id", -1))
	var reset_tiles: bool = biome_id != _selected_biome_id
	_selected_biome_id = biome_id
	_render_selected_biome(reset_tiles)

func _on_terrain_tile_selected(index: int) -> void:
	if terrain_tile_list == null:
		return
	if index < 0 or index >= terrain_tile_list.get_item_count():
		return
	var metadata: Variant = terrain_tile_list.get_item_metadata(index)
	var entity_id: int = int(metadata)
	_selected_tile_entity = entity_id
	_hovered_tile_entity = -1
	_render_tile_detail(entity_id)

func _on_terrain_tile_gui_input(event: InputEvent) -> void:
	if terrain_tile_list == null or event == null:
		return
	if event is InputEventMouseMotion:
		var motion: InputEventMouseMotion = event
		var hovered_index: int = terrain_tile_list.get_item_at_position(motion.position, true)
		if hovered_index < 0:
			if _hovered_tile_entity != -1:
				_hovered_tile_entity = -1
				if _selected_tile_entity >= 0:
					_render_tile_detail(_selected_tile_entity)
			return
		if hovered_index >= terrain_tile_list.get_item_count():
			return
		var metadata: Variant = terrain_tile_list.get_item_metadata(hovered_index)
		var entity_id: int = int(metadata)
		if entity_id == _selected_tile_entity:
			if _hovered_tile_entity != -1:
				_hovered_tile_entity = -1
				_render_tile_detail(_selected_tile_entity)
			return
		if entity_id == _hovered_tile_entity:
			return
		_hovered_tile_entity = entity_id
		_render_tile_detail(entity_id, true)

func _join_strings_with_separator(values: Array[String], separator: String) -> String:
	var result: String = ""
	for idx in range(values.size()):
		result += String(values[idx])
		if idx < values.size() - 1:
			result += separator
	return result

func _compare_terrain_entries(a: Dictionary, b: Dictionary) -> bool:
	var a_count = int(a.get("count", 0))
	var b_count = int(b.get("count", 0))
	if a_count == b_count:
		return int(a.get("id", -1)) < int(b.get("id", -1))
	return a_count > b_count

func _compare_tag_entries(a: Dictionary, b: Dictionary) -> bool:
	var a_count = int(a.get("count", 0))
	var b_count = int(b.get("count", 0))
	if a_count == b_count:
		return int(a.get("mask", 0)) < int(b.get("mask", 0))
	return a_count > b_count

func _label_for_terrain(terrain_id: int) -> String:
	if _terrain_palette.has(terrain_id):
		return str(_terrain_palette[terrain_id])
	for key in _terrain_palette.keys():
		if int(key) == terrain_id:
			return str(_terrain_palette[key])
	return "Terrain %d" % terrain_id

func _label_for_tag(mask: int) -> String:
	if _terrain_tag_labels.has(mask):
		return str(_terrain_tag_labels[mask])
	for key in _terrain_tag_labels.keys():
		if int(key) == mask:
			return str(_terrain_tag_labels[key])
	return "Tag %d" % mask

func _rebuild_tiles(tile_entries) -> void:
	_tile_records.clear()
	_terrain_counts.clear()
	_terrain_tag_counts.clear()
	_tile_coord_lookup.clear()
	_tile_total = 0
	if tile_entries is Array:
		for entry in tile_entries:
			_store_tile(entry)
	_tile_total = _tile_records.size()

func _apply_tile_updates(tile_entries) -> void:
	if not (tile_entries is Array):
		return
	for entry in tile_entries:
		if not (entry is Dictionary):
			continue
		var info: Dictionary = entry
		var entity = int(info.get("entity", -1))
		if entity >= 0:
			_forget_tile(entity)
		_store_tile(info)
	_tile_total = _tile_records.size()

func _remove_tiles(ids) -> void:
	if ids is Array:
		for id_value in ids:
			_forget_tile(int(id_value))
	elif ids is PackedInt64Array:
		var packed: PackedInt64Array = ids
		for idx in packed:
			_forget_tile(int(idx))
	elif ids is PackedInt32Array:
		var packed32: PackedInt32Array = ids
		for idx in packed32:
			_forget_tile(int(idx))
	_tile_total = max(_tile_records.size(), 0)

func _ingest_food_modules(entries) -> void:
	_food_modules.clear()
	_food_module_lookup.clear()
	if not (entries is Array):
		return
	for entry in entries:
		if not (entry is Dictionary):
			continue
		var record: Dictionary = (entry as Dictionary).duplicate(true)
		_food_modules.append(record)
		var x: int = int(record.get("x", -1))
		var y: int = int(record.get("y", -1))
		if x < 0 or y < 0:
			continue
		_food_module_lookup[Vector2i(x, y)] = record

func _store_tile(entry) -> void:
	if not (entry is Dictionary):
		return
	var info: Dictionary = entry
	var entity = int(info.get("entity", -1))
	if entity < 0:
		return
	var terrain_id = int(info.get("terrain", -1))
	var tags_mask = int(info.get("terrain_tags", 0))
	var mountain_kind = int(info.get("mountain_kind", 0))
	var mountain_relief = float(info.get("mountain_relief", 1.0))
	var coord := Vector2i(int(info.get("x", -1)), int(info.get("y", -1)))
	var module_key: String = ""
	var module_weight: float = 0.0
	if coord.x >= 0 and coord.y >= 0 and _food_module_lookup.has(coord):
		var module_entry: Dictionary = _food_module_lookup[coord]
		module_key = String(module_entry.get("module", "")).strip_edges()
		module_weight = float(module_entry.get("seasonal_weight", 0.0))
	var record = {
		"terrain": terrain_id,
		"tags": tags_mask,
		"x": coord.x,
		"y": coord.y,
		"element": int(info.get("element", -1)),
		"temperature": float(info.get("temperature", 0.0)),
		"mass": float(info.get("mass", 0.0)),
		"mountain_kind": mountain_kind,
		"mountain_relief": mountain_relief,
		"food_module": module_key,
		"food_module_weight": module_weight,
	}
	_tile_records[entity] = record
	if coord.x >= 0 and coord.y >= 0:
		_tile_coord_lookup[coord] = entity
	_tile_total = max(_tile_records.size(), _tile_total + 1)
	_bump_terrain_count(terrain_id, 1)
	_bump_tag_counts(tags_mask, 1)

func _forget_tile(entity_id: int) -> void:
	if not _tile_records.has(entity_id):
		return
	var record: Dictionary = _tile_records[entity_id]
	var terrain_id = int(record.get("terrain", -1))
	var tags_mask = int(record.get("tags", 0))
	var coord := Vector2i(int(record.get("x", -1)), int(record.get("y", -1)))
	if _tile_coord_lookup.has(coord):
		_tile_coord_lookup.erase(coord)
	_bump_terrain_count(terrain_id, -1)
	_bump_tag_counts(tags_mask, -1)
	_tile_records.erase(entity_id)
	_tile_total = max(_tile_records.size(), _tile_total - 1)

func _bump_terrain_count(terrain_id: int, delta: int) -> void:
	if terrain_id < 0 or delta == 0:
		return
	var current = int(_terrain_counts.get(terrain_id, 0)) + delta
	if current <= 0:
		_terrain_counts.erase(terrain_id)
	else:
		_terrain_counts[terrain_id] = current

func _bump_tag_counts(mask: int, delta: int) -> void:
	if mask == 0 or delta == 0:
		return
	var remaining = mask
	while remaining != 0:
		var bit = remaining & -remaining
		if bit <= 0:
			break
		if delta > 0 and not _terrain_tag_labels.has(bit):
			_terrain_tag_labels[bit] = "Tag %d" % bit
		var current = int(_terrain_tag_counts.get(bit, 0)) + delta
		if current <= 0:
			_terrain_tag_counts.erase(bit)
		else:
			_terrain_tag_counts[bit] = current
		remaining &= remaining - 1

func _on_export_map_button_pressed() -> void:
	# Fire-and-forget: the server writes the map JSON (terrain + resolved seed)
	# into its exports/ directory. Tile coordinates shown here as "@x,y" index
	# straight into the export's row-major samples.
	_call_send("export_map", "Map export requested; server writing exports/ JSON.")

func _on_tile_scout_button_pressed() -> void:
	if _selected_tile_coords.x < 0 or _selected_tile_coords.y < 0:
		_call_log("Select a tile before issuing a scout order.")
		return
	tile_scout_requested.emit(_selected_tile_coords.x, _selected_tile_coords.y)
