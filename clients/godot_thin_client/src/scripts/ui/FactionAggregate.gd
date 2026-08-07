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
## ask `weighted_mean(bands, key)` and never see how a band is weighted, so a second axis is confined
## to this file and no reader of Morale or Growth changes at all.
##
## **POPULATION IS THE ONLY AXIS TODAY, AND ITS WEIGHT IS THEREFORE MATHEMATICALLY INERT.** With one
## term, `Σ(w·v)/Σ(w)` gives the identical answer for any positive `w` — the weight cancels. It is
## still read from config rather than written as a `1.0` in code, because what is being paid for now
## is the SEAM: the day a second axis lands (proximity to other bands, distance from the capital, a
## settlement's stage) the two weights are what balance it, and a call site that had inlined the
## population term would have to be found and rewritten. Do not read the config key as a live tuning
## lever until there are two axes; it is structure, not a dial.
##
## **AN AXIS THIS BUILD DOES NOT IMPLEMENT IS IGNORED, NEVER SILENTLY ZERO-WEIGHTED.** `band_weight`
## walks the CONFIG's keys and `match`es each against the axis names it knows; an unmatched key hits
## the `_` arm and is skipped **with its weight**, so a config naming an axis this build has no term
## for degrades to the axes it does, rather than quietly dividing every band's weight by a term that
## is always zero. A band whose every axis reads zero (a cohort that has published no size yet) falls
## back to `FALLBACK_WEIGHT` so it still counts as one voice rather than dropping out of its own
## faction's average.
##
## **ADDING AN AXIS IS TWO EDITS, BOTH IN THIS FILE'S SIGHT: a `match` arm in `band_weight` (calling a
## private `_axis_<name>` reader beside `_axis_population`) and a key in the config.** In that order —
## an arm with no config key is never visited, since the walk is over the CONFIG and not over the
## known axes. There is deliberately no registry Dictionary: a `const` cannot hold a `Callable` to a
## static function in GDScript, so a table here would be name→name indirection over a `match` that
## already reads as one.

const CONFIG_PATH := "res://src/config/faction_aggregate_config.json"

## The weight a band carries when every configured axis reads zero. It exists so a band can never be
## silently excluded from a mean it is a member of.
const FALLBACK_WEIGHT := 1.0

## The shipped default, mirroring `faction_aggregate_config.json` so a missing or malformed file
## degrades to the behaviour the file describes rather than to no weighting at all.
const DEFAULT_WEIGHTS := {"population": 1.0}

## The one axis this build implements, spelled once: the config key AND the `match` arm in
## `band_weight` read it, so a rename cannot leave the two disagreeing about which key is live.
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

## One axis's raw magnitude for one band. `band_weight`'s `match` is what reaches these; a caller
## never does.
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
