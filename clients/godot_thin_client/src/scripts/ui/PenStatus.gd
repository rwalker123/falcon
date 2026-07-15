extends RefCounted
class_name PenStatus

## Single source of truth for "is this pen's herd starving?" — the ONE test the HUD's herd drawer
## (`Hud._corral_label` + the Pen feed row), the map's distress badge (`MapView._draw_herd`) and the
## turn orb's `starving_pen` attention producer all ask, so no two surfaces can disagree about which
## pen is dying.
##
## A corralled herd is a managed POPULATION (docs/plan_corral_managed_population.md): it cannot
## graze, so its keeper band hauls it `pen_upkeep` food/turn off the larder. The sim exports the
## share of that demand the keeper ACTUALLY paid last turn as `HerdTelemetryState.penFedFraction`:
##
##   fed == 1.0  → fully fed; the herd holds at its escapement operating point (also the value the
##                 sim reports for any herd that is not penned, so a wild herd never reads starving)
##   fed <  1.0  → STARVING: the herd is shrinking by `pen.starve_shrink_rate × (1 − fed) × biomass`
##                 every turn, and its yield falls with it. It recovers if fed again — it never
##                 despawns and never loses the pen — so this is a warning, not an obituary.

## A fully-fed pen. The neutral value: an un-penned herd, and an older snapshot with no such field.
const FULLY_FED := 1.0
## Rounding slack, so float noise on a fully-paid pen (0.999) does not read as a famine. Below this
## the herd is genuinely losing biomass.
const FED_EPSILON := 0.005

## The fed fraction the sim reported for this herd; `FULLY_FED` when it carries none.
static func fed_fraction(herd: Dictionary) -> float:
	return float(herd.get("pen_fed_fraction", FULLY_FED))

## THE starving test. Takes the fraction (not the herd) so a caller that already has it — the drawer
## reads it once for both its rows — does not fetch it twice.
static func is_starving(fed: float) -> bool:
	return fed < FULLY_FED - FED_EPSILON

## For a caller holding only the herd dict (the map marker, the turn-orb producer): is this herd's
## pen unfed? A herd that is NOT corralled is never starving, whatever its (defaulted) fraction says.
static func herd_is_starving(herd: Dictionary) -> bool:
	return bool(herd.get("corralled", false)) and is_starving(fed_fraction(herd))
