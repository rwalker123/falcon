extends RefCounted
class_name TileHabitability

## Single source of truth for the Tile-card Habitability rating: the bucket
## thresholds (loaded from `tile_habitability_config.json`) and the named-rating +
## color / BBCode-hex mapping. Habitability is the band-independent per-turn morale
## drain of living on a tile's terrain + temperature (snapshot `TileState.habitability`,
## decoded in `native/src/lib.rs`; >=0, bigger = harsher). Mirrors the BandFoodStatus
## bucketing pattern so the config loads once and no caller reinvents the rule.
##
## Mapping (given hospitable_max < fair_max < harsh_max):
##   drain <  hospitable_max → Hospitable (green / HEALTHY)
##   drain <  fair_max       → Fair       (neutral ink)
##   drain <  harsh_max      → Harsh      (amber / WARN)
##   drain >= harsh_max      → Hostile    (red / DANGER)
## The Karst Cavern Mouth (~0.0825) lands in the Harsh band.

const CONFIG_PATH := "res://src/config/tile_habitability_config.json"
const DEFAULT_HOSPITABLE_MAX := 0.02
const DEFAULT_FAIR_MAX := 0.05
const DEFAULT_HARSH_MAX := 0.09

const RATING_HOSPITABLE := "Hospitable"
const RATING_FAIR := "Fair"
const RATING_HARSH := "Harsh"
const RATING_HOSTILE := "Hostile"

static var _loaded := false
static var _hospitable_max := DEFAULT_HOSPITABLE_MAX
static var _fair_max := DEFAULT_FAIR_MAX
static var _harsh_max := DEFAULT_HARSH_MAX

static func _ensure_loaded() -> void:
	if _loaded:
		return
	_loaded = true
	if not FileAccess.file_exists(CONFIG_PATH):
		return
	var file := FileAccess.open(CONFIG_PATH, FileAccess.READ)
	if file == null:
		return
	var text := file.get_as_text()
	file.close()
	var data: Variant = JSON.parse_string(text)
	if not (data is Dictionary):
		return
	var block_variant: Variant = (data as Dictionary).get("habitability", {})
	if block_variant is Dictionary:
		var block: Dictionary = block_variant
		_hospitable_max = float(block.get("hospitable_max", DEFAULT_HOSPITABLE_MAX))
		_fair_max = float(block.get("fair_max", DEFAULT_FAIR_MAX))
		_harsh_max = float(block.get("harsh_max", DEFAULT_HARSH_MAX))

## Named rating for a habitability drain scalar.
static func rating_for(drain: float) -> String:
	_ensure_loaded()
	if drain < _hospitable_max:
		return RATING_HOSPITABLE
	if drain < _fair_max:
		return RATING_FAIR
	if drain < _harsh_max:
		return RATING_HARSH
	return RATING_HOSTILE

static func color_for(drain: float) -> Color:
	match rating_for(drain):
		RATING_HOSPITABLE:
			return HudStyle.HEALTHY
		RATING_FAIR:
			return HudStyle.INK
		RATING_HARSH:
			return HudStyle.WARN
		_:
			return HudStyle.DANGER

## BBCode hex for a habitability rating string (as produced by `rating_for`), so the
## Tile-card value tints without recomputing the bucket from the raw scalar.
static func hex_for_rating(rating: String) -> String:
	match rating:
		RATING_HOSPITABLE:
			return HudStyle.HEALTHY_HEX
		RATING_FAIR:
			return HudStyle.INK_HEX
		RATING_HARSH:
			return HudStyle.WARN_HEX
		_:
			return HudStyle.DANGER_HEX
