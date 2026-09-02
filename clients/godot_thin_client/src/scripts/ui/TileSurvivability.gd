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
## Fair — and kills every turn, with a full larder (issue #614). The client would have to invent a
## threshold to tie the two together, which is exactly the mistake the sim publishing this model
## removes; nothing here consults `TileClimate` and nothing there consults this.
##
## ⛔ **TWO INDEPENDENT TAILS, NOT A TOLERANCE AROUND AN AMBIENT — AND THAT IS THE WHOLE SHAPE OF
## THIS FILE.** It began as `min((|t - ambient| - tolerance) * scale, cap)`: one midpoint, one
## deviation, one slope, one ceiling, and heat therefore forced to mirror cold about that midpoint.
## **Cold and heat are not symmetric phenomena.** Extreme heat is survivable with shade and water in
## a way -57 °C is not, so the two sides differ in ALL THREE parameters — onset, slope and ceiling
## (shipped: cold from 6 °C at 0.00159/° capped at 10 %; heat from 40 °C at 0.00176/° capped at
## 3 %). A symmetric form cannot express that: pinning the heat onset to mirror 6 °C about an 18 °C
## ambient put heat death at a warm summer day. So there is no `ambient` here and no tolerance, and
## re-deriving one from the two onsets would be re-introducing exactly the model that was removed.
##
## `death_rate` mirrors `active_temperature_tail` + `temperature_fraction` in
## `core_sim/src/systems/population.rs`:
##   * BELOW the cold onset, the degrees below it priced at the cold slope and capped by the cold
##     ceiling; ABOVE the heat onset, the same with the heat tail's own three numbers;
##   * BETWEEN the onsets, zero — the survivable band is `[cold_onset, heat_onset]`, an interval the
##     sim states outright rather than a distance from anything.
##
## ⛔ **IT IS THE TILE'S BASE RATE, NOT A BAND'S.** The sim multiplies this by each age bracket's own
## vulnerability AFTER the cap (children 1.25, workers 1.0, elders 1.5), and those weights are
## deliberately NOT published: this readout describes a TILE, and a tile does not know who is
## standing on it. A client that applied a bracket weight here would be answering a different
## question from the one the chip asks. The rate is INDEPENDENT of food — a full larder does not
## touch it.

## Nothing is judged until the sim publishes the model. We refuse to invent a survival line, so a
## caller checks `has_model()` and skips the readout rather than guessing one.
static var _model_published := false
static var _cold_onset_temp := 0.0
static var _cold_mortality_scale := 0.0
static var _cold_max_mortality := 0.0
static var _heat_onset_temp := 0.0
static var _heat_mortality_scale := 0.0
static var _heat_max_mortality := 0.0

## Adopt the sim's published mortality constants — the two tails, in the wire's own order. Pushed
## from MapView's overlay ingest — the same seam that adopts the published sea level and the climate
## cut points, and for the same reason (a sim-owned threshold the client must not re-derive). A
## per-run constant, so the last published values persist across deltas until a new run republishes
## them.
static func set_model(cold_onset: float, cold_scale: float, cold_max: float,
		heat_onset: float, heat_scale: float, heat_max: float) -> void:
	_cold_onset_temp = cold_onset
	_cold_mortality_scale = cold_scale
	_cold_max_mortality = cold_max
	_heat_onset_temp = heat_onset
	_heat_mortality_scale = heat_scale
	_heat_max_mortality = heat_max
	_model_published = true

## True once the sim has published its model, so a caller can skip the survivability readout
## entirely rather than judge a tile against constants nobody sent.
static func has_model() -> bool:
	return _model_published

## The coldest survivable temperature (°C) — at or above it the cold tail is silent, below it the sim
## starts killing. It is the COLD ONSET itself, published, not a midpoint minus a tolerance.
static func survivable_min() -> float:
	return _cold_onset_temp

## The hottest survivable temperature (°C) — the HEAT ONSET, and unrelated to the cold one.
static func survivable_max() -> float:
	return _heat_onset_temp

## True when this temperature (°C) falls outside `[survivable_min(), survivable_max()]` in EITHER
## direction, i.e. the sim's per-turn death fraction is above zero here.
static func is_lethal(temperature: float) -> bool:
	return death_rate(temperature) > 0.0

## Which tail a lethal temperature is in — the cold one. False for a survivable tile and for the
## heat tail alike, so a caller pairs it with `is_lethal`.
static func is_cold(temperature: float) -> bool:
	return _model_published and temperature < _cold_onset_temp

## The per-turn fraction of a TILE's population the sim kills at this temperature (°C), regardless of
## food — the base rate, before the sim's per-bracket vulnerabilities (see the class docs). `0.0`
## between the two onsets, and `0.0` whenever the model has not been published (there is nothing to
## mirror yet).
static func death_rate(temperature: float) -> float:
	if not _model_published:
		return 0.0
	if temperature < _cold_onset_temp:
		return minf((_cold_onset_temp - temperature) * _cold_mortality_scale, _cold_max_mortality)
	if temperature > _heat_onset_temp:
		return minf((temperature - _heat_onset_temp) * _heat_mortality_scale, _heat_max_mortality)
	return 0.0
