extends RefCounted
class_name BandFoodStatus

## Single source of truth for band food-supply status: the warn/critical
## thresholds (loaded from `band_status_config.json`) and the green/amber/red
## color + BBCode-hex mapping used everywhere a band's days-of-food is surfaced
## (the map band dot in `MapView`, the selection-panel food line and the alerts
## panel in `Hud`). Keeping the thresholds and the color mapping here means the
## config is loaded once and no caller reinvents the day → color rule.
##
## Mapping (given `warn` > `critical`):
##   days >= warn      → HEALTHY (green)
##   warn > days >= critical → WARN (amber)
##   days < critical   → DANGER (red)
## A band that is not food-limited reports `UNLIMITED_DAYS` and reads as HEALTHY.

const CONFIG_PATH := "res://src/config/band_status_config.json"
const DEFAULT_WARN_DAYS := 10.0
const DEFAULT_CRITICAL_DAYS := 5.0
# Server sentinel (snapshot `daysOfFood`) meaning "not food-limited".
const UNLIMITED_DAYS := 999.0

static var _loaded := false
static var _warn_days := DEFAULT_WARN_DAYS
static var _critical_days := DEFAULT_CRITICAL_DAYS

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
	var food_days_variant: Variant = (data as Dictionary).get("food_days", {})
	if food_days_variant is Dictionary:
		var food_days: Dictionary = food_days_variant
		_warn_days = float(food_days.get("warn", DEFAULT_WARN_DAYS))
		_critical_days = float(food_days.get("critical", DEFAULT_CRITICAL_DAYS))

static func warn_days() -> float:
	_ensure_loaded()
	return _warn_days

static func critical_days() -> float:
	_ensure_loaded()
	return _critical_days

## True when the band actually tracks a finite larder runway (i.e. not the
## "not food-limited" sentinel and not a missing/negative value).
static func is_limited(days: float) -> bool:
	return days >= 0.0 and days < UNLIMITED_DAYS

## Days-critical? Used by the alerts panel for the starving alert.
static func is_critical(days: float) -> bool:
	_ensure_loaded()
	return is_limited(days) and days < _critical_days

static func color_for_days(days: float) -> Color:
	_ensure_loaded()
	if not is_limited(days):
		return HudStyle.HEALTHY
	if days < _critical_days:
		return HudStyle.DANGER
	if days < _warn_days:
		return HudStyle.WARN
	return HudStyle.HEALTHY

static func hex_for_days(days: float) -> String:
	_ensure_loaded()
	if not is_limited(days):
		return HudStyle.HEALTHY_HEX
	if days < _critical_days:
		return HudStyle.DANGER_HEX
	if days < _warn_days:
		return HudStyle.WARN_HEX
	return HudStyle.HEALTHY_HEX
