extends RefCounted
## A build is worked by its own site's crew, so the count rides each source row (`builders_staffed`
## / `builders_kitted`; `builders_cap` is a fixture's own, since it says a build is declared); a
## chapter dials it once with `BandFx.staff_builders` and every source that passes through
## `ForageFx.floorify` (every show, compose and herd refresh in the harness) is stamped with it.
## `UNSET` leaves a fixture's own numbers alone.

const UNSET := -1

static var staffed: int = UNSET

## Stamp the dialled crew onto `src` (keys spelled under `prefix`).
static func stamp(src: Dictionary, prefix: String) -> void:
	if staffed == UNSET:
		return
	src[prefix + SourceForecast.FORECAST_BUILDERS_STAFFED_KEY] = staffed
	src[prefix + SourceForecast.FORECAST_BUILDERS_KITTED_KEY] = staffed
