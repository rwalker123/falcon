extends RefCounted
class_name RiverEdges

## Single source of truth for the TEXT reading of a tile's hex-EDGE rivers: the river
## classes, the direction names, and the shared `Key: Value` formatter both text surfaces
## use (the Tile card's `Hud._tile_terrain_lines` and the map hover `Hud.show_tooltip`).
## Mirrors the TileHabitability / TileClimate / BandFoodStatus pattern — the vocabulary lives
## here so no caller hardcodes a class name, a direction name, or a bit width.
##
## The wire primitive is `TileState.riverEdges` (decoded in `native/src/lib.rs tile_to_dict`
## as `river_edges`, ingested into `MapView.tile_river_edges`): a 12-bit mask, 2 bits per
## odd-r direction — `class = (mask >> (BITS_PER_DIRECTION * dir)) & CLASS_MASK`, with
## 0 = none, 1 = Minor, 2 = Major (3 reserved). Both hexes flanking an edge carry it, so a
## tile answers "which of MY sides carry water?" locally.
##
## Minor/Major are EDGE rivers. A **Navigable** river is not here at all — it is an ordinary
## water TerrainType (id 37), so the Biome/Terrain row already names it.
##
## TWO direction orders, deliberately:
##   * DECODE order (`SIM_DIRECTION_NAMES`) is the sim's `grid_utils::HEX_NEIGHBOR_OFFSETS`
##     order — clockwise from E — and is a WIRE CONTRACT shared with the shader's
##     `neighbor_offset` table. It indexes the mask. Do not re-derive or reorder it.
##   * DISPLAY order (`DISPLAY_DIRECTION_ORDER`) starts at NE and goes clockwise, because a
##     player parses a compass reading, not the sim's bit layout. It is a pure presentation
##     permutation applied after decoding.

const DIRECTION_COUNT := 6
const BITS_PER_DIRECTION := 2
const CLASS_MASK := 0b11

const CLASS_NONE := 0
const CLASS_MINOR := 1
const CLASS_MAJOR := 2

const LABEL_MINOR := "Minor River"
const LABEL_MAJOR := "Major River"

## Mask bit-position order — the sim's odd-r neighbour order, clockwise from E. WIRE CONTRACT.
const SIM_DIRECTION_NAMES := ["E", "SE", "SW", "W", "NW", "NE"]
## Presentation order: compass-style, clockwise from NE. Values are SIM direction indices.
const DISPLAY_DIRECTION_ORDER := [5, 0, 1, 2, 3, 4]

## Classes in the order they are reported — the bigger river reads first.
const CLASS_REPORT_ORDER := [CLASS_MAJOR, CLASS_MINOR]

const DIRECTION_SEPARATOR := ", "

## River class carried by one edge of the tile, by SIM direction index.
static func class_at(mask: int, direction: int) -> int:
	if direction < 0 or direction >= DIRECTION_COUNT:
		return CLASS_NONE
	return (mask >> (BITS_PER_DIRECTION * direction)) & CLASS_MASK

## True when any edge of the tile carries a known river class.
static func has_river(mask: int) -> bool:
	for direction in range(DIRECTION_COUNT):
		if _label_for_class(class_at(mask, direction)) != "":
			return true
	return false

## One `<Class> River: <dirs>` line per class present, Major first, directions in compass
## order (NE, E, SE, SW, W, NW). Empty array when the tile carries no river — callers append
## the result directly, so a riverless tile renders NO row rather than an empty label.
static func summary_lines(mask: int) -> Array[String]:
	var lines: Array[String] = []
	for river_class in CLASS_REPORT_ORDER:
		var label := _label_for_class(river_class)
		if label == "":
			continue
		var directions := PackedStringArray()
		for direction in DISPLAY_DIRECTION_ORDER:
			if class_at(mask, direction) == river_class:
				directions.append(SIM_DIRECTION_NAMES[direction])
		if directions.is_empty():
			continue
		lines.append("%s: %s" % [label, DIRECTION_SEPARATOR.join(directions)])
	return lines

## "" for none and for the reserved class value — an unknown class is never named.
static func _label_for_class(river_class: int) -> String:
	match river_class:
		CLASS_MINOR:
			return LABEL_MINOR
		CLASS_MAJOR:
			return LABEL_MAJOR
		_:
			return ""
