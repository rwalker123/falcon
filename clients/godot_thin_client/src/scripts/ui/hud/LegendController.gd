class_name LegendController
extends RefCounted

## Owns the right-dock terrain/overlay legend card: row rendering, the
## terrain-only Name/Count sort header + its display-only sort state, the
## suppress toggle, and the internal-scroll sizing. Extracted from HudLayer
## (composition — Hud holds one of these and delegates). Behaviour is unchanged.

const LEGEND_SWATCH_FRACTION := 0.75
const LEGEND_MIN_ROW_HEIGHT := 20.0
const LEGEND_ROW_PADDING := 6.0
const LEGEND_MAX_HEIGHT := 640.0

const LEGEND_KEY_TERRAIN := "terrain"
const SORT_FIELD_NAME := "name"
const SORT_FIELD_COUNT := "count"
const LEGEND_SORT_ARROW_ASC := "▲"
const LEGEND_SORT_ARROW_DESC := "▼"
const LEGEND_SORT_LABEL := "Sort"
const LEGEND_SORT_ROW_SEPARATION := 6

var _panel: PanelCard = null
var _scroll: ScrollContainer = null
var _list: VBoxContainer = null
var _description: Label = null

var overlay_legend: Dictionary = {}
var legend_suppressed: bool = false
# Terrain-legend sort mode (display-only, persisted across legend pushes).
# Default: Count, descending — most common biome first, a sensible read of a
# map's composition. Direction is remembered per field so toggling is intuitive.
var _legend_sort_field: String = SORT_FIELD_COUNT
var _legend_sort_ascending: Dictionary = {
	SORT_FIELD_NAME: true,
	SORT_FIELD_COUNT: false,
}
# Runtime-built sort header (Name/Count toggles), lazily created, terrain-only.
var _legend_sort_row: HBoxContainer = null
var _legend_sort_name_button: Button = null
var _legend_sort_count_button: Button = null

func _init(panel: PanelCard, scroll: ScrollContainer, list: VBoxContainer, description: Label) -> void:
	_panel = panel
	_scroll = scroll
	_list = list
	_description = description

func update(legend: Dictionary) -> void:
	overlay_legend = legend.duplicate(true) if legend is Dictionary else {}
	if legend_suppressed:
		_hide_panel()
		return
	for child in _list.get_children():
		child.queue_free()
	if overlay_legend.is_empty():
		_hide_panel()
		return
	_panel.visible = true
	var title := String(overlay_legend.get("title", "Map Legend"))
	_panel.set_card_title(title)
	var description := String(overlay_legend.get("description", "")).strip_edges()
	if description == "":
		_description.visible = false
		_description.text = ""
	else:
		_description.visible = true
		_description.text = description
	var rows: Array = overlay_legend.get("rows", [])
	if rows.is_empty():
		_panel.visible = false
		_description.visible = false
		_description.text = ""
		_set_sort_visible(false)
		return
	# The sort control applies to the base terrain legend only; scalar-overlay
	# and tag legends have a meaningful intrinsic order and render unchanged.
	var is_terrain := String(overlay_legend.get("key", "")) == LEGEND_KEY_TERRAIN
	_set_sort_visible(is_terrain)
	if is_terrain:
		rows = _sorted_terrain_rows(rows)
	var row_height := _row_height()
	var swatch_size := _swatch_size(row_height)
	for entry in rows:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		var row := HBoxContainer.new()
		row.custom_minimum_size = Vector2(0, row_height)
		row.size_flags_horizontal = Control.SIZE_EXPAND_FILL

		var swatch := ColorRect.new()
		swatch.custom_minimum_size = swatch_size
		swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		swatch.color = entry.get("color", Color.WHITE)
		row.add_child(swatch)

		var label := Label.new()
		var label_text := str(entry.get("label", ""))
		var value_text := str(entry.get("value_text", "")).strip_edges()
		if value_text != "":
			if label_text == "":
				label.text = value_text
			else:
				label.text = "%s — %s" % [label_text, value_text]
		else:
			label.text = label_text
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(label)

		_list.add_child(row)
	_resize_panel()

func toggle_suppressed() -> void:
	legend_suppressed = not legend_suppressed
	if legend_suppressed:
		_hide_panel()
	else:
		update(overlay_legend)

func on_sort_pressed(field: String) -> void:
	if field == _legend_sort_field:
		# Re-clicking the active field flips its direction (A→Z↔Z→A, high↔low).
		_legend_sort_ascending[field] = not bool(_legend_sort_ascending.get(field, true))
	else:
		_legend_sort_field = field
	_update_sort_buttons()
	# Re-render the current legend so the new order lands immediately.
	update(overlay_legend)

## Re-apply row heights/swatch sizes to already-built rows (typography refresh).
func refresh_rows() -> void:
	var row_height := _row_height()
	var swatch_size := _swatch_size(row_height)
	for child in _list.get_children():
		if child is HBoxContainer:
			var row := child as HBoxContainer
			row.custom_minimum_size = Vector2(0, row_height)
			for grandchild in row.get_children():
				if grandchild is ColorRect:
					(grandchild as ColorRect).custom_minimum_size = swatch_size
	_resize_panel()

func _row_height() -> float:
	return LEGEND_MIN_ROW_HEIGHT + LEGEND_ROW_PADDING

func _swatch_size(row_height: float) -> Vector2:
	var side: float = max(row_height * LEGEND_SWATCH_FRACTION, LEGEND_MIN_ROW_HEIGHT * 0.6)
	return Vector2(side, side)

## Cap the legend's inner scroll so a long list scrolls internally instead of
## stretching the whole right dock. Width and placement come from the PanelCard
## + dock; this only bounds the row list's height.
func _resize_panel() -> void:
	if _scroll == null or _list == null:
		return
	var list_height: float = _list.get_combined_minimum_size().y
	var clamped_height: float = clamp(list_height, LEGEND_MIN_ROW_HEIGHT, LEGEND_MAX_HEIGHT)
	_scroll.custom_minimum_size.y = clamped_height
	_scroll.scroll_vertical = 0

## Sort the terrain legend rows by the active field/direction. Display-only —
## MapView always sends its natural order; the panel owns the preference.
func _sorted_terrain_rows(rows: Array) -> Array:
	var sorted_rows: Array = rows.duplicate()
	var field := _legend_sort_field
	var ascending := bool(_legend_sort_ascending.get(field, true))
	sorted_rows.sort_custom(func(a, b): return _legend_row_less(a, b, field, ascending))
	return sorted_rows

func _legend_row_less(a, b, field: String, ascending: bool) -> bool:
	# Descending reuses the strict comparator with swapped arguments rather than
	# negating it: `not less` would return true for equal rows in both directions,
	# violating the strict-weak-ordering `sort_custom` requires. Swapping args keeps
	# equality false either way (and preserves the alphabetical tie-break inside).
	if ascending:
		return _legend_strict_less(a, b, field)
	return _legend_strict_less(b, a, field)

func _legend_strict_less(a, b, field: String) -> bool:
	if field == SORT_FIELD_COUNT:
		var count_a := int(a.get("count", 0))
		var count_b := int(b.get("count", 0))
		if count_a == count_b:
			# Ties read best alphabetically rather than in arbitrary order.
			return str(a.get("label", "")).naturalnocasecmp_to(str(b.get("label", ""))) < 0
		return count_a < count_b
	return str(a.get("label", "")).naturalnocasecmp_to(str(b.get("label", ""))) < 0

## Build the Name/Count sort header once and insert it above the row list.
func _ensure_sort_row() -> void:
	if _legend_sort_row != null:
		return
	if _scroll == null:
		return
	var content: Node = _scroll.get_parent()
	if content == null:
		return
	_legend_sort_row = HBoxContainer.new()
	_legend_sort_row.add_theme_constant_override("separation", LEGEND_SORT_ROW_SEPARATION)
	_legend_sort_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var caption := Label.new()
	caption.text = LEGEND_SORT_LABEL
	caption.add_theme_color_override("font_color", HudStyle.INK_DIM)
	caption.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_legend_sort_row.add_child(caption)

	_legend_sort_name_button = Button.new()
	_legend_sort_name_button.pressed.connect(on_sort_pressed.bind(SORT_FIELD_NAME))
	_legend_sort_row.add_child(_legend_sort_name_button)

	_legend_sort_count_button = Button.new()
	_legend_sort_count_button.pressed.connect(on_sort_pressed.bind(SORT_FIELD_COUNT))
	_legend_sort_row.add_child(_legend_sort_count_button)

	content.add_child(_legend_sort_row)
	# Sit directly above the row list (below the card header + description).
	content.move_child(_legend_sort_row, _scroll.get_index())

func _set_sort_visible(should_show: bool) -> void:
	if should_show:
		_ensure_sort_row()
		_update_sort_buttons()
	if _legend_sort_row != null:
		_legend_sort_row.visible = should_show

func _update_sort_buttons() -> void:
	if _legend_sort_name_button == null or _legend_sort_count_button == null:
		return
	_legend_sort_name_button.text = _sort_button_text("Name", SORT_FIELD_NAME)
	_legend_sort_count_button.text = _sort_button_text("Count", SORT_FIELD_COUNT)
	# The active field reads as the "primary" cyan treatment; the other is ghost.
	HudStyle.apply_button(_legend_sort_name_button, _sort_variant(SORT_FIELD_NAME))
	HudStyle.apply_button(_legend_sort_count_button, _sort_variant(SORT_FIELD_COUNT))

func _sort_button_text(label: String, field: String) -> String:
	if field != _legend_sort_field:
		return label
	var arrow := LEGEND_SORT_ARROW_ASC if bool(_legend_sort_ascending.get(field, true)) else LEGEND_SORT_ARROW_DESC
	return "%s %s" % [label, arrow]

func _sort_variant(field: String) -> String:
	return "primary" if field == _legend_sort_field else "ghost"

func _hide_panel() -> void:
	if _panel != null:
		_panel.visible = false
	if _description != null:
		_description.visible = false
		_description.text = ""
