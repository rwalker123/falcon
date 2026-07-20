extends RefCounted
class_name TileClimate

## Single source of truth for the Tile-card Climate readout.
##
## The band cut points are OWNED BY THE SIM and published per-map in the snapshot
## (`MapSection.climateBands` → the `overlays.climate_{polar,boreal,temperate}_max_temp`
## keys, °C). The client renders the band it is TOLD rather than keeping its own threshold:
## the sim now derives a tile's BIOME from these same cut points, so any independent client
## opinion could show a biome and a climate that disagree — the exact defect the Climate
## Authority arc removes. The retired `tile_climate_config.json` `cool_min` (3.0) was that
## independent opinion; it is gone, and so is the whole 5-band local scheme it anchored.
##
## Classification mirrors `climate::climate_band_for_temperature` (core_sim/src/climate.rs)
## EXACTLY, including the inclusive upper bounds — a tile AT a cut point sits in the lower
## (colder) band:
##   temp <= polar_max_temp     → Polar
##   temp <= boreal_max_temp    → Boreal
##   temp <= temperate_max_temp → Temperate
##   else                       → Tropical
##
## Climate is INFORMATIONAL, not a warning: it deliberately does NOT reuse the
## HEALTHY/WARN/DANGER semantic palette the Habitability row owns. The Tile card renders
## the band with the neutral default ink tint. The four band names mirror the sim's own
## `ClimateBand` vocabulary (polar/boreal/temperate/tropical) so the client label can never
## drift from the band the sim decided; they also read naturally beside a biome name.

const BAND_POLAR := "Polar"
const BAND_BOREAL := "Boreal"
const BAND_TEMPERATE := "Temperate"
const BAND_TROPICAL := "Tropical"

## Rendered until the sim publishes its cut points (older sim, or a snapshot with the
## `ClimateBands` table absent). We refuse to invent a threshold that could disagree with
## the biome, so the Climate readout reads as unknown rather than guessing a band.
const BAND_UNKNOWN := "—"

static var _bands_published := false
static var _polar_max := 0.0
static var _boreal_max := 0.0
static var _temperate_max := 0.0

## Adopt the sim's published cut points (°C). Pushed from MapView's overlay ingest — the
## same seam that reads the published sea level, and for the same reason (a sim-owned
## threshold the client must not re-derive). The bands are a per-map constant, so the last
## published values persist across deltas until a regen republishes them.
static func set_cut_points(polar_max: float, boreal_max: float, temperate_max: float) -> void:
	_polar_max = polar_max
	_boreal_max = boreal_max
	_temperate_max = temperate_max
	_bands_published = true

## True once the sim has published cut points, so a caller can skip the Climate row
## entirely rather than render a dash before any snapshot has landed.
static func has_bands() -> bool:
	return _bands_published

## Named climate band for a tile temperature (°C). Returns BAND_UNKNOWN until the sim
## publishes its cut points.
static func band_for(temperature: float) -> String:
	if not _bands_published:
		return BAND_UNKNOWN
	if temperature <= _polar_max:
		return BAND_POLAR
	if temperature <= _boreal_max:
		return BAND_BOREAL
	if temperature <= _temperate_max:
		return BAND_TEMPERATE
	return BAND_TROPICAL
