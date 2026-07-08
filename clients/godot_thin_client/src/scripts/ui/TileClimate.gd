extends RefCounted
class_name TileClimate

## Single source of truth for the Tile-card Climate readout: the temperature-band
## cutoffs (loaded from `tile_climate_config.json`) and the named-band mapping.
## Climate is derived from the tile's `temperature` (snapshot `TileState.temperature`,
## decoded in `native/src/lib.rs` as `temperature`, °), which is now a latitude +
## elevation climate (equator-in-the-middle) rather than the old element checkerboard.
## Bands make the latitude gradient legible ("far south → Polar"). Mirrors the
## TileHabitability config-load pattern so the thresholds load once and no caller
## reinvents the rule.
##
## Climate is INFORMATIONAL, not a warning: it deliberately does NOT reuse the
## HEALTHY/WARN/DANGER semantic palette the Habitability row owns. The Tile card
## renders the band with the neutral default ink tint.
##
## Band mapping (given cool_min < temperate_min < warm_min < tropical_min):
##   temp >= tropical_min  → Tropical
##   temp >= warm_min      → Warm
##   temp >= temperate_min → Temperate
##   temp >= cool_min      → Cool
##   temp <  cool_min      → Polar

const CONFIG_PATH := "res://src/config/tile_climate_config.json"
const DEFAULT_TROPICAL_MIN := 26.0
const DEFAULT_WARM_MIN := 20.0
const DEFAULT_TEMPERATE_MIN := 12.0
const DEFAULT_COOL_MIN := 3.0

const BAND_TROPICAL := "Tropical"
const BAND_WARM := "Warm"
const BAND_TEMPERATE := "Temperate"
const BAND_COOL := "Cool"
const BAND_POLAR := "Polar"

static var _loaded := false
static var _tropical_min := DEFAULT_TROPICAL_MIN
static var _warm_min := DEFAULT_WARM_MIN
static var _temperate_min := DEFAULT_TEMPERATE_MIN
static var _cool_min := DEFAULT_COOL_MIN

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
	var block_variant: Variant = (data as Dictionary).get("climate", {})
	if block_variant is Dictionary:
		var block: Dictionary = block_variant
		_tropical_min = float(block.get("tropical_min", DEFAULT_TROPICAL_MIN))
		_warm_min = float(block.get("warm_min", DEFAULT_WARM_MIN))
		_temperate_min = float(block.get("temperate_min", DEFAULT_TEMPERATE_MIN))
		_cool_min = float(block.get("cool_min", DEFAULT_COOL_MIN))

## Named climate band for a tile temperature (°).
static func band_for(temperature: float) -> String:
	_ensure_loaded()
	if temperature >= _tropical_min:
		return BAND_TROPICAL
	if temperature >= _warm_min:
		return BAND_WARM
	if temperature >= _temperate_min:
		return BAND_TEMPERATE
	if temperature >= _cool_min:
		return BAND_COOL
	return BAND_POLAR
