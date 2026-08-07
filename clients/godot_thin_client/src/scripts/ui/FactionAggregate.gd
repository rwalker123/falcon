extends RefCounted
class_name FactionAggregate

## **HOW A PER-BAND SCALAR BECOMES A FACTION SCALAR — the ONE place that is decided.**
##
## Morale and Growth are percentages, and a faction has no percentage of its own: a plain mean over
## bands lets a three-person camp outvote a forty-person town, which is a claim about the faction that
## nobody would defend if it were written down. So a band carries a WEIGHT in the mean, and this file
## owns both the weight and the mean.
##
## **THE FORMULA IS NOT HARD-CODED AT ITS CALL SITES — that is the whole point of the file.** Callers
## ask `weighted_mean(bands, key)` and never see how a band is weighted, so a second axis is a term
## function plus a config key here, and no reader of Morale or Growth changes at all.
##
## **POPULATION IS THE ONLY AXIS TODAY, AND ITS WEIGHT IS THEREFORE MATHEMATICALLY INERT.** With one
## term, `Σ(w·v)/Σ(w)` gives the identical answer for any positive `w` — the weight cancels. It is
## still read from config rather than written as a `1.0` in code, because what is being paid for now
## is the SEAM: the day a second axis lands (proximity to other bands, distance from the capital, a
## settlement's stage) the two weights are what balance it, and a call site that had inlined the
## population term would have to be found and rewritten. Do not read the config key as a live tuning
## lever until there are two axes; it is structure, not a dial.
##
## **AN AXIS WITH NO TERM FUNCTION IS IGNORED, NEVER SILENTLY ZERO-WEIGHTED.** `AXIS_TERMS` is the
## registry, and a config key absent from it is skipped with its weight — so a config naming an axis
## this build does not implement degrades to the axes it does, rather than quietly dividing every
## band's weight by a term that is always zero. A band whose every axis reads zero (a cohort that has
## published no size yet) falls back to `FALLBACK_WEIGHT` so it still counts as one voice rather than
## dropping out of its own faction's average.

const CONFIG_PATH := "res://src/config/faction_aggregate_config.json"

## The weight a band carries when every configured axis reads zero. It exists so a band can never be
## silently excluded from a mean it is a member of.
const FALLBACK_WEIGHT := 1.0

## The default weight of an axis the config does not mention.
const DEFAULT_AXIS_WEIGHT := 0.0

## The shipped default, mirroring `faction_aggregate_config.json` so a missing or malformed file
## degrades to the behaviour the file describes rather than to no weighting at all.
const DEFAULT_WEIGHTS := {"population": 1.0}

## The AXIS REGISTRY: a config key is an axis iff it names a term here. Each entry is the band-dict
## reader for that axis, returning the axis's raw magnitude for one band.
## **Adding an axis is an entry here plus a key in the config** — in that order, since a term with no
## config key simply carries `DEFAULT_AXIS_WEIGHT` (0.0) and is inert until the key exists.
const AXIS_POPULATION := "population"

static var _loaded := false
static var _weights: Dictionary = DEFAULT_WEIGHTS.duplicate()

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
    var weights_variant: Variant = (data as Dictionary).get("weights", {})
    if weights_variant is Dictionary:
        _weights = (weights_variant as Dictionary).duplicate()

## One axis's raw magnitude for one band. The registry `AXIS_TERMS` maps to these; a caller never
## reaches one directly.
static func _axis_population(band: Dictionary) -> float:
    return maxf(float(band.get("size", 0)), 0.0)

## The weight this band carries in a faction mean: the sum of every CONFIGURED axis that this build
## also implements, floored at `FALLBACK_WEIGHT` so no band drops out of its own faction.
static func band_weight(band: Dictionary) -> float:
    _ensure_loaded()
    var weight := 0.0
    for axis in _weights:
        match String(axis):
            AXIS_POPULATION:
                weight += _axis_population(band) * float(_weights[axis])
            _:
                # An axis this build does not implement. Skipped WITH its weight rather than added as
                # a zero term, so an unknown key cannot dilute every band equally and silently.
                continue
    return weight if weight > 0.0 else FALLBACK_WEIGHT

## The faction's value of a per-band scalar: `Σ(weight · value) / Σ(weight)`.
##
## `read` is the per-band reader (a `Callable` taking the band dict and returning a float), so this
## serves Morale, Growth and anything later without knowing what any of them mean. Bands that carry
## no reading at all are excluded by `has`, never defaulted — a band with no morale on the wire is
## not a band with zero morale, and averaging one in would drag the faction toward a number nobody
## published.
static func weighted_mean(bands: Array, key: String) -> float:
    var total := 0.0
    var weight_sum := 0.0
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        if not band.has(key):
            continue
        var weight := band_weight(band)
        total += float(band[key]) * weight
        weight_sum += weight
    return total / weight_sum if weight_sum > 0.0 else 0.0

## True when at least one band publishes `key` — the gate every weighted row needs, since
## `weighted_mean` answers `0.0` both for "the faction's mean is zero" and for "nobody published one",
## and those two must not render the same way.
static func any_band_states(bands: Array, key: String) -> bool:
    for band_variant in bands:
        if band_variant is Dictionary and (band_variant as Dictionary).has(key):
            return true
    return false
