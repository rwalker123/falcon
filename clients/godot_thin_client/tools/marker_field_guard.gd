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
# (`Hud._build_allocation_panel` / `_effective_*`) read off `_selected_unit` (the marker copy),
# plus the marker fields MapView draws from the same copy (e.g. the travel-destination reticle).
# The marker's key set MUST stay a superset of this list; add a key here whenever the panel or a
# selected-unit map draw starts reading a new marker field, and the guard will hold the marker to it.
const PANEL_CONSUMED_KEYS := [
	"entity",              # _emit_assign_labor bits, roster identity
	"faction",             # _is_player_unit gating
	"id",                  # drawer "Unit:" label
	"pos",                 # drawer "Position:" line
	"size",                # drawer "Size:" + allocation header Population
	"days_of_food",        # _band_food_line
	"food_income",         # Food summary line net rate + Gathered/Hunted breakdown
	"food_consumption",    # Food summary line net rate + Eaten breakdown
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
	"hunt_reach",          # herd-hunt affordance local-vs-expedition distance gate
	"scout_reveal_radius", # allocation Scout role hint
	"is_traveling",        # travel-destination map draw gating
	"travel_target_x",     # travel-destination map draw (MapView._draw_travel_destination)
	"travel_target_y",     # travel-destination map draw (MapView._draw_travel_destination)
	"activity",            # roster activity glyph
	"hunt_mode",           # roster / cancel-hunt label
	"accessible_stockpile", # _accessible_stockpile_lines
	"is_expedition",       # expedition panel gating + distinct marker
	"expedition_mission",  # expedition panel mission line
	"expedition_phase",    # expedition marker awaiting state + panel phase line
	"max_expedition_party_size", # outfit stepper max clamp
	"expedition_target_herd", # hunt expedition target herd (panel + marker)
	"expedition_hunt_policy", # hunt expedition policy (panel readout)
	"expedition_carry_cap",   # hunt expedition carry ceiling (panel Carried X / cap)
	"home_band_entity"        # Band/City panel groups a band's active expeditions by this
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
	"food_income": 0.83,
	"food_consumption": 0.60,
	"morale": 0.41,
	"morale_delta": -0.03,
	"morale_cause": 1,
	"activity": "forage",
	"hunt_mode": "sustain",
	"work_range": 2,
	"hunt_reach": 7,
	"scout_reveal_radius": 3,
	"is_traveling": true,
	"travel_target_x": 11,
	"travel_target_y": 9,
	"working_age": 16,
	"idle_workers": 7,
	"output_multiplier": 0.72,
	"discontent_fraction": 0.18,
	"morale_settling": 0.01,
	"morale_terrain": -0.02,
	"morale_climate": -0.015,
	"morale_unrest": -0.005,
	"labor_assignments": [
		{"kind": "forage", "workers": 5, "target_x": 7, "target_y": 6, "actual_yield": 0.42, "sustainable_yield": 0.42},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "actual_yield": 0.31, "sustainable_yield": 0.18},
		{"kind": "scout", "workers": 3},
	],
	"stores": {"provisions": 120.0},
	"accessible_stockpile": {"item": "provisions", "qty": 40.0},
	# Expedition discriminators (distinct non-default values so a dropped copy shows up).
	"is_expedition": true,
	"expedition_mission": "scout",
	"expedition_phase": "awaiting",
	"max_expedition_party_size": 8,
	"expedition_target_herd": "game_deer_07",
	"expedition_hunt_policy": "surplus",
	"expedition_carry_cap": 16.0,
	"home_band_entity": 7777,
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
	_expect_int(marker, "hunt_reach", 7)
	_expect_int(marker, "scout_reveal_radius", 3)
	_expect_int(marker, "travel_target_x", 11)
	_expect_int(marker, "travel_target_y", 9)
	if not bool(marker.get("is_traveling", false)):
		_fail("is_traveling did not round-trip to true (defaulted?)")
	_expect_int(marker, "size", 30)
	_expect_int(marker, "entity", 9001)
	_expect_int(marker, "faction", 0)
	_expect_int(marker, "morale_cause", 1)
	_expect_str(marker, "activity", "forage")
	_expect_str(marker, "hunt_mode", "sustain")
	_expect_str(marker, "expedition_mission", "scout")
	_expect_str(marker, "expedition_phase", "awaiting")
	_expect_int(marker, "max_expedition_party_size", 8)
	_expect_str(marker, "expedition_target_herd", "game_deer_07")
	_expect_str(marker, "expedition_hunt_policy", "surplus")
	_expect_float(marker, "expedition_carry_cap", 16.0)
	_expect_int(marker, "home_band_entity", 7777)
	if not bool(marker.get("is_expedition", false)):
		_fail("is_expedition did not round-trip to true (defaulted?)")
	_expect_float(marker, "morale", 0.41)
	_expect_float(marker, "output_multiplier", 0.72)
	_expect_float(marker, "days_of_food", 12.0)
	_expect_float(marker, "food_income", 0.83)
	_expect_float(marker, "food_consumption", 0.60)

	# labor_assignments must round-trip as a non-empty, value-preserving copy (the
	# allocation panel iterates it to build the per-source steppers + per-source yields).
	var la_variant: Variant = marker.get("labor_assignments", null)
	if not (la_variant is Array):
		_fail("labor_assignments is not an Array (got %s)" % typeof(la_variant))
	else:
		var la: Array = la_variant
		if la.size() != 3:
			_fail("labor_assignments size %d, expected 3" % la.size())
		elif int((la[0] as Dictionary).get("workers", -1)) != 5:
			_fail("labor_assignments[0].workers did not round-trip (expected 5)")
		elif absf(float((la[1] as Dictionary).get("actual_yield", -1.0)) - 0.31) > 0.0001:
			_fail("labor_assignments[1].actual_yield did not round-trip (expected 0.31)")
		elif absf(float((la[1] as Dictionary).get("sustainable_yield", -1.0)) - 0.18) > 0.0001:
			_fail("labor_assignments[1].sustainable_yield did not round-trip (expected 0.18)")

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
