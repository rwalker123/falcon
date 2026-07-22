extends RefCounted
class_name BandFoodStatus

## Single source of truth for band food-supply status: the warn/critical
## thresholds (loaded from `band_status_config.json`) and the green/amber/red
## color + BBCode-hex mapping used everywhere a band's larder runway — the turns
## of food it has left — is surfaced (the map band dot in `MapView`, the
## selection-panel food line and the alerts panel in `Hud`). Keeping the
## thresholds and the color mapping here means the config is loaded once and no
## caller reinvents the turn → color rule.
##
## Mapping (given `warn` > `critical`):
##   turns >= warn      → HEALTHY (green)
##   warn > turns >= critical → WARN (amber)
##   turns < critical   → DANGER (red)
## A band that is not food-limited reports `UNLIMITED_TURNS` and reads as HEALTHY.
##
## Morale is a separate 0–1 scalar (no "unlimited" sentinel): a harsh tile erodes
## it until births collapse below the constant elder-mortality drain, so a band can
## shrink while well-fed. Same green/amber/red palette, mirrored helpers:
##   morale >= warn      → HEALTHY (green)
##   warn > morale >= critical → WARN (amber)
##   morale < critical   → DANGER (red)

const CONFIG_PATH := "res://src/config/band_status_config.json"
const DEFAULT_WARN_TURNS := 10.0
const DEFAULT_CRITICAL_TURNS := 5.0
# Server sentinel (snapshot `turnsOfFood`) meaning "not food-limited".
const UNLIMITED_TURNS := 999.0
# Morale (0–1) UI warn/critical thresholds. Morale drives productivity + migration
# (not births, which are morale-independent); these bracket the migration onset
# (~0.25) so a band reads amber/red as it approaches "people start leaving".
const DEFAULT_WARN_MORALE := 0.40
const DEFAULT_CRITICAL_MORALE := 0.25
# Output-multiplier (0–1) tint buckets (Civilization Wellbeing productivity readout; the
# row only appears below 1.0). output >= warn reads ink (near-full), warn > o >= critical
# reads amber, o < critical reads red.
const DEFAULT_WARN_OUTPUT := 0.85
const DEFAULT_CRITICAL_OUTPUT := 0.60
# Per-turn morale-contribution magnitude below which a breakdown row is trivial and hidden.
const DEFAULT_MORALE_BREAKDOWN_EPSILON := 0.002

static var _loaded := false
static var _warn_turns := DEFAULT_WARN_TURNS
static var _critical_turns := DEFAULT_CRITICAL_TURNS
static var _warn_morale := DEFAULT_WARN_MORALE
static var _critical_morale := DEFAULT_CRITICAL_MORALE
static var _warn_output := DEFAULT_WARN_OUTPUT
static var _critical_output := DEFAULT_CRITICAL_OUTPUT
static var _morale_breakdown_epsilon := DEFAULT_MORALE_BREAKDOWN_EPSILON

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
	var food_turns_variant: Variant = (data as Dictionary).get("food_turns", {})
	if food_turns_variant is Dictionary:
		var food_turns: Dictionary = food_turns_variant
		_warn_turns = float(food_turns.get("warn", DEFAULT_WARN_TURNS))
		_critical_turns = float(food_turns.get("critical", DEFAULT_CRITICAL_TURNS))
	var morale_variant: Variant = (data as Dictionary).get("morale", {})
	if morale_variant is Dictionary:
		var morale: Dictionary = morale_variant
		_warn_morale = float(morale.get("warn", DEFAULT_WARN_MORALE))
		_critical_morale = float(morale.get("critical", DEFAULT_CRITICAL_MORALE))
		_morale_breakdown_epsilon = float(morale.get("breakdown_epsilon", DEFAULT_MORALE_BREAKDOWN_EPSILON))
	var output_variant: Variant = (data as Dictionary).get("output", {})
	if output_variant is Dictionary:
		var output: Dictionary = output_variant
		_warn_output = float(output.get("warn", DEFAULT_WARN_OUTPUT))
		_critical_output = float(output.get("critical", DEFAULT_CRITICAL_OUTPUT))

static func warn_turns() -> float:
	_ensure_loaded()
	return _warn_turns

static func critical_turns() -> float:
	_ensure_loaded()
	return _critical_turns

## True when the band actually tracks a finite larder runway (i.e. not the
## "not food-limited" sentinel and not a missing/negative value).
static func is_limited(turns: float) -> bool:
	return turns >= 0.0 and turns < UNLIMITED_TURNS

## Turns-critical? Used by the alerts panel for the starving alert.
static func is_critical(turns: float) -> bool:
	_ensure_loaded()
	return is_limited(turns) and turns < _critical_turns

static func color_for_turns(turns: float) -> Color:
	_ensure_loaded()
	if not is_limited(turns):
		return HudStyle.HEALTHY
	if turns < _critical_turns:
		return HudStyle.DANGER
	if turns < _warn_turns:
		return HudStyle.WARN
	return HudStyle.HEALTHY

static func hex_for_turns(turns: float) -> String:
	_ensure_loaded()
	if not is_limited(turns):
		return HudStyle.HEALTHY_HEX
	if turns < _critical_turns:
		return HudStyle.DANGER_HEX
	if turns < _warn_turns:
		return HudStyle.WARN_HEX
	return HudStyle.HEALTHY_HEX

static func warn_morale() -> float:
	_ensure_loaded()
	return _warn_morale

static func critical_morale() -> float:
	_ensure_loaded()
	return _critical_morale

## Morale is a plain 0–1 scalar (no "unlimited" sentinel): tiers mirror the turns
## helpers against the morale warn/critical thresholds.
static func color_for_morale(m: float) -> Color:
	_ensure_loaded()
	if m < _critical_morale:
		return HudStyle.DANGER
	if m < _warn_morale:
		return HudStyle.WARN
	return HudStyle.HEALTHY

static func hex_for_morale(m: float) -> String:
	_ensure_loaded()
	if m < _critical_morale:
		return HudStyle.DANGER_HEX
	if m < _warn_morale:
		return HudStyle.WARN_HEX
	return HudStyle.HEALTHY_HEX

## Output multiplier (0–1) tint for the productivity readout. Below full the row grades
## ink → amber → red as discontent bites harder. Distinct from the morale/food palette:
## near-full output reads neutral ink (not green) — it's a productivity note, not a "good".
static func hex_for_output(o: float) -> String:
	_ensure_loaded()
	if o < _critical_output:
		return HudStyle.DANGER_HEX
	if o < _warn_output:
		return HudStyle.WARN_HEX
	return HudStyle.INK_HEX

## Minimum |per-turn morale contribution| worth listing in the itemized breakdown.
static func morale_breakdown_epsilon() -> float:
	_ensure_loaded()
	return _morale_breakdown_epsilon
