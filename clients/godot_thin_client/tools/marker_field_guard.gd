extends Node

## Headless regression guard for the "unit marker drops a panel-consumed field" bug class.
##
## The band drawer + labor-allocation panel read their data from `Hud._selected_unit`, which
## is a copy of the MapView unit MARKER built in `MapView._rebuild_unit_markers` (the marker
## copies fields explicitly out of the decoded population entry via `entry.get(...)`). Twice
## now a field the panel reads (`hunt_mode`, then `working_age`/`idle_workers`) was simply
## never copied into the marker, so the live panel silently read the default (0 / "") even
## though the server emitted a real value. Neither ui_preview (sets `_selected_unit` directly)
## nor map_preview (map-only) exercises the population-entry → marker path, so it had no
## coverage.
##
## This test feeds ONE realistic population entry through the real `_rebuild_unit_markers` and
## asserts the produced marker (a) round-trips every value the panel actually reads and (b)
## carries a superset of PANEL_CONSUMED_KEYS — so any future field the panel consumes but the
## marker forgets to copy fails HERE, at build time, instead of as a silent 0 in the live HUD.
##
## Run as a scene (NOT --script: MapView.gd references the TerrainTextureManager autoload,
## which only registers when the project is loaded). No GPU / viewport needed — this is pure
## marker-building logic (no rendering), so --headless is fine here:
##   godot --headless --path . res://tools/marker_field_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const MAP_VIEW := preload("res://src/scripts/MapView.gd")

# Every key the band drawer (`Hud._unit_summary_lines`) and the labor-allocation panel
# (`Hud._build_allocation_panel` / `_effective_*`) read off `_selected_unit` (the marker copy).
# The marker's key set MUST stay a superset of this list; add a key here whenever the panel
# starts reading a new marker field, and the guard will hold the marker to it.
const PANEL_CONSUMED_KEYS := [
	"entity",              # _emit_assign_labor bits, roster identity
	"faction",             # _is_player_unit gating
	"id",                  # drawer "Unit:" label
	"pos",                 # drawer "Position:" line
	"size",                # drawer "Size:" + allocation header Population
	"days_of_food",        # _band_food_line
	"stores",              # _band_food_line provisions
	"morale",              # _band_morale_line / _morale_is_concerning
	"morale_delta",        # _band_morale_line trend
	"morale_cause",        # _band_morale_line named cause
	"output_multiplier",   # _band_output_line
	"morale_settling",     # _morale_breakdown_lines
	"morale_terrain",      # _morale_breakdown_lines
	"morale_climate",      # _morale_breakdown_lines
	"morale_unrest",       # _morale_breakdown_lines
	"working_age",         # allocation header Workers / _effective_idle
	"idle_workers",        # allocation header Idle / quick_assign_hunters
	"labor_assignments",   # allocation "Current actions" steppers
	"work_range",          # selected-band map highlights
	"scout_reveal_radius", # allocation Scout role hint
	"activity",            # roster activity glyph
	"hunt_mode",           # roster / cancel-hunt label
	"accessible_stockpile" # _accessible_stockpile_lines
]

# A full, realistic population entry — the shape the native decoder (`population_to_dict`)
# emits — carrying a distinct non-default value for every panel-consumed field so a dropped
# copy shows up as a defaulted value, not a coincidental match.
const FIXTURE_ENTRY := {
	"entity": 9001,
	"faction": 0,
	"current_x": 8,
	"current_y": 6,
	"size": 30,
	"label": "River Band",
	"days_of_food": 12.0,
	"morale": 0.41,
	"morale_delta": -0.03,
	"morale_cause": 1,
	"activity": "forage",
	"hunt_mode": "sustain",
	"work_range": 2,
	"scout_reveal_radius": 3,
	"working_age": 16,
	"idle_workers": 7,
	"output_multiplier": 0.72,
	"discontent_fraction": 0.18,
	"morale_settling": 0.01,
	"morale_terrain": -0.02,
	"morale_climate": -0.015,
	"morale_unrest": -0.005,
	"labor_assignments": [
		{"kind": "forage", "workers": 5, "target_x": 7, "target_y": 6},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain"},
		{"kind": "scout", "workers": 3},
	],
	"stores": {"provisions": 120.0},
	"accessible_stockpile": {"item": "provisions", "qty": 40.0},
}

var _failures: Array[String] = []

func _ready() -> void:
	var mv: Node = MAP_VIEW.new()
	var snapshot := {"populations": [FIXTURE_ENTRY]}
	mv._rebuild_unit_markers(snapshot)

	var markers: Array = mv.units
	if markers.size() != 1:
		_fail("expected exactly 1 marker, got %d" % markers.size())
		_finish()
		mv.free()
		return
	var marker: Dictionary = markers[0]

	# 1. Superset guard: no panel-consumed key may be missing from the marker.
	for key in PANEL_CONSUMED_KEYS:
		if not marker.has(key):
			_fail("marker is MISSING panel-consumed key '%s' (dropped in _rebuild_unit_markers)" % key)

	# 2. Round-trip guard: the fields most prone to silent-default drops must preserve
	#    the input value, not fall back to a default.
	_expect_int(marker, "working_age", 16)
	_expect_int(marker, "idle_workers", 7)
	_expect_int(marker, "work_range", 2)
	_expect_int(marker, "scout_reveal_radius", 3)
	_expect_int(marker, "size", 30)
	_expect_int(marker, "entity", 9001)
	_expect_int(marker, "faction", 0)
	_expect_int(marker, "morale_cause", 1)
	_expect_str(marker, "activity", "forage")
	_expect_str(marker, "hunt_mode", "sustain")
	_expect_float(marker, "morale", 0.41)
	_expect_float(marker, "output_multiplier", 0.72)
	_expect_float(marker, "days_of_food", 12.0)

	# labor_assignments must round-trip as a non-empty, value-preserving copy (the
	# allocation panel iterates it to build the per-source steppers).
	var la_variant: Variant = marker.get("labor_assignments", null)
	if not (la_variant is Array):
		_fail("labor_assignments is not an Array (got %s)" % typeof(la_variant))
	else:
		var la: Array = la_variant
		if la.size() != 3:
			_fail("labor_assignments size %d, expected 3" % la.size())
		elif int((la[0] as Dictionary).get("workers", -1)) != 5:
			_fail("labor_assignments[0].workers did not round-trip (expected 5)")

	# pos must be the [current_x, current_y] the drawer reads.
	var pos_variant: Variant = marker.get("pos", null)
	if not (pos_variant is Array) or (pos_variant as Array).size() != 2 \
			or int((pos_variant as Array)[0]) != 8 or int((pos_variant as Array)[1]) != 6:
		_fail("pos did not round-trip to [8, 6] (got %s)" % str(pos_variant))

	_finish()
	mv.free()

func _expect_int(marker: Dictionary, key: String, want: int) -> void:
	var got := int(marker.get(key, -999999))
	if got != want:
		_fail("%s = %d, expected %d (defaulted?)" % [key, got, want])

func _expect_str(marker: Dictionary, key: String, want: String) -> void:
	var got := String(marker.get(key, "<missing>"))
	if got != want:
		_fail("%s = '%s', expected '%s' (defaulted?)" % [key, got, want])

func _expect_float(marker: Dictionary, key: String, want: float) -> void:
	var got := float(marker.get(key, -999999.0))
	if absf(got - want) > 0.0001:
		_fail("%s = %f, expected %f (defaulted?)" % [key, got, want])

func _fail(msg: String) -> void:
	_failures.append(msg)

func _finish() -> void:
	if _failures.is_empty():
		print("marker_field_guard: PASS — marker carries every panel-consumed field and round-trips values")
		get_tree().quit(0)
	else:
		printerr("marker_field_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
