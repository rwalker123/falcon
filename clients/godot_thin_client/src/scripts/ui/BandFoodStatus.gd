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
# Fertility-multiplier tint buckets for the Growth readout — the band's birth rate as a share of
# its NORMAL rate (`fertility_hunger x fertility_reserve x fertility_trend`, neutral at 1.0). Unlike
# output it can exceed 1.0 (a well-provisioned band out-breeds its base rate), so `warn` sits BELOW
# neutral: at/above it the row reads neutral ink, below it amber, below `critical` red. `critical`
# brackets the shipped floor a fully-collapsed band damps to (0.75 deficit penalty x a saturated
# 1.5 reserve = 0.375), so "my growth has stalled" reads red rather than merely amber.
const DEFAULT_WARN_FERTILITY := 0.75
const DEFAULT_CRITICAL_FERTILITY := 0.40
# Deviation from the neutral 1.0 below which a fertility factor is trivial and hidden.
const DEFAULT_FERTILITY_BREAKDOWN_EPSILON := 0.002
# A computed `reserve` is >= 1 by construction (`1 + bonus x ramp`), so a ZERO reserve is the sim's
# NOT-PROJECTED sentinel — a rehydrated cohort that has not ticked yet. `hunger` and `trend` both
# legitimately reach 0, which is why neither can serve. Read it as "no reading", NEVER as a famine.
const FERTILITY_NOT_PROJECTED_RESERVE := 0.0
# The neutral point of every fertility factor: 1.0 leaves the base birth rate untouched.
const FERTILITY_NEUTRAL := 1.0

static var _loaded := false
static var _warn_turns := DEFAULT_WARN_TURNS
static var _critical_turns := DEFAULT_CRITICAL_TURNS
static var _warn_morale := DEFAULT_WARN_MORALE
static var _critical_morale := DEFAULT_CRITICAL_MORALE
static var _warn_output := DEFAULT_WARN_OUTPUT
static var _critical_output := DEFAULT_CRITICAL_OUTPUT
static var _morale_breakdown_epsilon := DEFAULT_MORALE_BREAKDOWN_EPSILON
static var _warn_fertility := DEFAULT_WARN_FERTILITY
static var _critical_fertility := DEFAULT_CRITICAL_FERTILITY
static var _fertility_breakdown_epsilon := DEFAULT_FERTILITY_BREAKDOWN_EPSILON

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
	var fertility_variant: Variant = (data as Dictionary).get("fertility", {})
	if fertility_variant is Dictionary:
		var fertility: Dictionary = fertility_variant
		_warn_fertility = float(fertility.get("warn", DEFAULT_WARN_FERTILITY))
		_critical_fertility = float(fertility.get("critical", DEFAULT_CRITICAL_FERTILITY))
		_fertility_breakdown_epsilon = float(fertility.get("breakdown_epsilon", DEFAULT_FERTILITY_BREAKDOWN_EPSILON))

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

## The same buckets as a `Color`, for a readout built out of Labels rather than BBCode — the WORK
## zone head's Output item. Mirrors the `color_for_morale` / `hex_for_morale` pairing, so the panel's
## two productivity surfaces can never grade the same multiplier differently.
static func color_for_output(o: float) -> Color:
	_ensure_loaded()
	if o < _critical_output:
		return HudStyle.DANGER
	if o < _warn_output:
		return HudStyle.WARN
	return HudStyle.INK

## Minimum |per-turn morale contribution| worth listing in the itemized breakdown.
static func morale_breakdown_epsilon() -> float:
	_ensure_loaded()
	return _morale_breakdown_epsilon

## Fertility-multiplier tint for the Growth readout. Mirrors `hex_for_output`'s ink -> amber -> red
## grading rather than the morale/food green palette: a band at or above its normal birth rate is
## simply normal, not a "good", so neutral ink is the top bucket even at 150%.
static func hex_for_fertility(f: float) -> String:
	_ensure_loaded()
	if f < _critical_fertility:
		return HudStyle.DANGER_HEX
	if f < _warn_fertility:
		return HudStyle.WARN_HEX
	return HudStyle.INK_HEX

## The fertility multiplier at/above which growth reads normal (neutral ink) — also the gate on the
## Growth row's "concerning" caret tint.
static func warn_fertility() -> float:
	_ensure_loaded()
	return _warn_fertility

## Minimum |factor - 1.0| worth listing as a fertility breakdown row.
static func fertility_breakdown_epsilon() -> float:
	_ensure_loaded()
	return _fertility_breakdown_epsilon

## Whether the sim published a fertility reading for this band at all. A rehydrated cohort has not
## ticked yet and carries the all-zero default; rendering that as a total collapse of growth would
## be the same "no data read as famine" bug the sim guards on its own side.
static func fertility_is_projected(band: Dictionary) -> bool:
	return float(band.get("fertility_reserve", FERTILITY_NOT_PROJECTED_RESERVE)) > FERTILITY_NOT_PROJECTED_RESERVE
