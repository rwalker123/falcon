extends RefCounted
class_name TileSurvivability

## Single source of truth for "does this ground KILL the people standing on it".
##
## The temperature-mortality model is OWNED BY THE SIM and published per-run in the snapshot
## (`MapSection.temperatureSurvivability` → the `overlays.survivability_*` keys). The client
## states the range the sim actually kills outside of; it never derives one.
##
## ⛔ **THESE ARE NOT THE CLIMATE BANDS.** `TileClimate`'s cut points name what a temperature is
## CALLED; these name what it DOES to a band, and the two sets of numbers come from different
## configs and do not line up. At the shipped tuning a 3.7 °C tile is labelled Temperate and rated
## Fair — and loses ~4.5 % of every age bracket, every turn, with a full larder (issue #614). The
## client would have to invent a threshold to tie the two together, which is exactly the mistake
## the sim publishing this model removes; nothing here consults `TileClimate` and nothing there
## consults this.
##
## `death_rate` mirrors the cold block of `core_sim/src/systems/population.rs` (lines 275-281)
## EXACTLY, including:
##   * the `abs()` on the deviation from ambient — the tolerance is SYMMETRIC, so heat past
##     `ambient + tolerance` is as lethal as cold below `ambient - tolerance`;
##   * the zero floor on the excess — inside the tolerance band nobody dies of temperature at all;
##   * the `min` against the configured cap, which is what stops an arctic tile from taking a whole
##     band in a single turn.
## The rate is applied to EVERY age bracket and is INDEPENDENT of food: a band with a full larder
## dies at exactly this rate.

## Nothing is judged until the sim publishes the model. We refuse to invent a survival line, so a
## caller checks `has_model()` and skips the readout rather than guessing one.
static var _model_published := false
static var _ambient_temp := 0.0
static var _temp_tolerance := 0.0
static var _mortality_scale := 0.0
static var _max_mortality := 0.0

## Adopt the sim's published mortality constants. Pushed from MapView's overlay ingest — the same
## seam that adopts the published sea level and the climate cut points, and for the same reason (a
## sim-owned threshold the client must not re-derive). A per-run constant, so the last published
## values persist across deltas until a new run republishes them.
static func set_model(ambient: float, tolerance: float, mortality_scale: float,
		max_mortality: float) -> void:
	_ambient_temp = ambient
	_temp_tolerance = tolerance
	_mortality_scale = mortality_scale
	_max_mortality = max_mortality
	_model_published = true

## True once the sim has published its model, so a caller can skip the survivability readout
## entirely rather than judge a tile against constants nobody sent.
static func has_model() -> bool:
	return _model_published

## The coldest survivable temperature (°C) — below it the sim starts killing.
static func survivable_min() -> float:
	return _ambient_temp - _temp_tolerance

## The hottest survivable temperature (°C) — above it the sim starts killing.
static func survivable_max() -> float:
	return _ambient_temp + _temp_tolerance

## True when this temperature (°C) sits outside the survivable range in EITHER direction, i.e. the
## sim's per-turn death fraction is above zero here.
static func is_lethal(temperature: float) -> bool:
	return death_rate(temperature) > 0.0

## Which tail a lethal temperature is in — the cold one. False for a survivable tile and for the
## heat tail alike, so a caller pairs it with `is_lethal`.
static func is_cold(temperature: float) -> bool:
	return _model_published and temperature < survivable_min()

## The per-turn fraction of EVERY age bracket the sim kills at this temperature (°C), regardless of
## food. Mirrors `core_sim/src/systems/population.rs:275-281`; `0.0` inside the tolerance band, and
## `0.0` whenever the model has not been published (there is nothing to mirror yet).
static func death_rate(temperature: float) -> float:
	if not _model_published:
		return 0.0
	var excess := absf(temperature - _ambient_temp) - _temp_tolerance
	if excess <= 0.0:
		return 0.0
	return minf(excess * _mortality_scale, _max_mortality)

## True when the configured CAP is what the rate rests on rather than the deviation — the tile is
## far enough outside the range that getting colder (or hotter) no longer kills faster.
static func is_at_max_rate(temperature: float) -> bool:
	return _model_published and death_rate(temperature) >= _max_mortality
