extends Node

## Headless regression guard for the "unit marker drops a panel-consumed field" bug class.
##
## The band drawer + labor-allocation panel read their data from `Hud._selected_unit`, which
## is a copy of the MapView unit MARKER built in `MapView._rebuild_unit_markers` (the marker
## copies fields explicitly out of the decoded population entry via `entry.get(...)`). Twice
## now a field the panel reads (`working_age`/`idle_workers`) was simply
## never copied into the marker, so the live panel silently read the default (0 / "") even
## though the server emitted a real value. Neither ui_preview (sets `_selected_unit` directly)
## nor map_preview (map-only) exercises the population-entry → marker path, so it had no
## coverage.
##
## This test feeds ONE realistic population entry through the real `_rebuild_unit_markers` and
## asserts the produced marker (a) round-trips every value the panel actually reads and (b)
## PARTITIONS exactly into its source's keys plus the declared map-only stamps — so any future field
## the decoder adds and the marker forgets to copy fails HERE, at build time, instead of as a silent
## 0 (or a vanished row) in the live HUD.
##
## Run as a scene (NOT --script: MapView.gd references the TerrainTextureManager autoload,
## which only registers when the project is loaded). No GPU / viewport needed — this is pure
## marker-building logic (no rendering), so --headless is fine here:
##   godot --headless --path . res://tools/marker_field_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const MAP_VIEW := preload("res://src/scripts/MapView.gd")

# **THE EXHAUSTIVE CLAIM: every key the marker's SOURCE dict carries survives onto the marker.**
#
# This used to be `PANEL_CONSUMED_KEYS`, a hand-maintained list of "things the panel reads", checked
# one key at a time. It could only ever catch a leak someone had already thought to name — and the
# leak class it exists for is precisely the one nobody thinks to name: the decoder grows a field, the
# panel reads it, and `MapView._rebuild_unit_markers` never hears about it. That shipped three times
# (`hunt_mode`, then `working_age`/`idle_workers`, then the Minimal TOE's six, which made a band's
# `Kit` row vanish on the map-click path and took the ⚠ zero-effective-attack warning silently with it).
#
# Since the marker became a STRUCTURAL copy (`entry.duplicate()` plus stamps), "is this one key on the
# marker" stopped being the question. The claim is now a PARTITION, asserted as an equality:
#
#     marker.keys()  ==  entry.keys()  ∪  MARKER_STAMPED_KEYS  −  MARKER_OMITTED_KEYS
#
# so a key the source carried and the marker dropped fails, AND a key the marker invented without
# declaring it fails. Nothing has to be remembered; a new decoder field is covered the day it exists.
#
# Its power is exactly the fixture's key set, which is why FIXTURE_ENTRY below is a realistic cohort:
# a field absent from the fixture is a field this claim says nothing about. That is the one thing the
# `snapshot_dict.json` golden and `decode_guard` cover from the other side.

# The map-only additions — everything the marker has that its source dict does not. Keep in step with
# the stamps in `_rebuild_unit_markers`; a stamp added there and not named here fails the partition.
const MARKER_STAMPED_KEYS := [
	"pos",               # [current_x, current_y] RESOLVED through the home-tile fallback
	"id",                # the de-duplicated display name ("Band 3"); the cohort's own `label` rides along
	"dest_x",            # travel destination, from `harvest`/`scout` — absent when the band has neither
	"dest_y",
	"travel_task_kind",  # which sub-tree that destination came from
]

# Keys the marker deliberately does NOT carry, each with the reason. **Empty is the correct state**
# under a structural copy — an entry here means someone chose to drop a field the cohort had, and the
# partition assertion makes that choice explicit rather than accidental.
const MARKER_OMITTED_KEYS := {}

# The SECOND bug class this guard exists for: a CONTINUOUS field the native decoder emits as a
# float (a fixed-point Scalar run through `fixed64_to_f64`, or a `float` wire field) that the
# marker copy silently NARROWS with `int(...)`. Presence-only / integral-fixture checks cannot see
# it — the key is there, the value is merely truncated — yet it is live-visible: the marker IS the
# selection payload for a band clicked ON THE MAP (MapView.refresh_selection_payload →
# Hud.show_unit_selection → _selected_unit), so e.g. truncated age brackets made a 30-person band's
# PEOPLE block read 9+16+4 = 29 until the next snapshot re-resolved it from the raw floats.
#
# Every key below is fed a deliberately NON-INTEGER value and must come back within
# FRACTIONAL_EPSILON. Membership rule: the field must be continuous end to end (fixed-point Scalar
# or `float` in snapshot.fbs). Integer counts (`size`, `working_age`, `idle_workers`), entity ids,
# and coordinates are deliberately EXCLUDED — asserting a fraction on them would be a false claim.
# These values are also the fixture's values for these keys (merged over FIXTURE_ENTRY), so the
# fixture cannot drift away from what the round-trip asserts.
const FRACTIONAL_ROUND_TRIP_KEYS := {
	# Age structure — fixed-point Scalars (cohort.children/working/elders → fixed64_to_f64).
	# THE regression: these three were copied with int(...). Values mirror the decoder test.
	"age_children": 9.2925,
	"age_working": 16.5375,
	"age_elders": 4.6425,
	# Morale + its four signed Layer-1 contributions — all fixed-point Scalars.
	"morale": 0.4137,
	"morale_delta": -0.0325,
	"morale_settling": 0.0113,
	"morale_terrain": -0.0217,
	"morale_climate": -0.0154,
	"morale_unrest": -0.0061,
	# Wellbeing scalars — fixed-point.
	"output_multiplier": 0.7225,
	"discontent_fraction": 0.1837,
	# The three fertility factors — fixed-point Scalars, neutral at 1.0 (NOT at 0), so an int(...)
	# copy would flatten a 0.6225 hunger to 0 and read a fed band as starving.
	"fertility_hunger": 0.6225,
	"fertility_reserve": 1.4375,
	"fertility_trend": 0.2575,
	# Food ledger — `float` wire fields (foodIncome / foodConsumption / penFeedUpkeep / turnsOfFood).
	"turns_of_food": 12.75,
	"food_income": 0.8325,
	"food_consumption": 0.6075,
	"pen_feed_upkeep": 1.7425,
	# Expedition + config levers that are `float` in the schema (carry caps, rates, move speed).
	"expedition_carry_cap": 16.25,
	# Next-delivery projected food — a `float` copied onto the marker, so it must survive un-truncated
	# (the detail panel reads it off `_selected_unit`). `expedition_eta_turns` is an integer count and
	# `expedition_recurring` a bool → presence-only, NOT here.
	"expedition_projected_delivery": 14.5,
	"hunt_per_worker_provisions": 0.8125,
	"expedition_per_worker_carry": 4.375,
	"band_move_tiles_per_turn": 3.5,
	# The MINIMAL TOE's six — `float` in the schema end to end, so all six qualify under the rule
	# above. The durabilities are the `equipment.json` 0–100 scale and deliberately carry a half:
	# `87.5` copied through `int()` is 87, which is the exact shape this list exists to catch and
	# which the presence half above cannot see. The other three are resolved rates.
	"hunting_kit_durability": 87.5,
	"sled_kit_durability": 62.25,
	"basket_kit_durability": 41.75,
	"hunter_attack": 1.6875,
	"hunt_carry_per_worker_biomass": 2.3125,
	"forage_carry_per_worker_biomass": 1.5625,
}

# Tolerance for the fractional round-trip. Loose enough for float32 wire fields widened to f64,
# tight enough that any int() narrowing (>= 0.0061 of error for every value above) fails.
const FRACTIONAL_EPSILON := 0.0001

# A full, realistic population entry — the shape the native decoder (`population_to_dict`)
# emits — carrying a distinct non-default value for every panel-consumed field so a dropped
# copy shows up as a defaulted value, not a coincidental match. Every CONTINUOUS field lives in
# FRACTIONAL_ROUND_TRIP_KEYS instead and is merged over this dict at test time, so the fixture and
# the fractional assertion can never quote different values.
const FIXTURE_ENTRY := {
	"entity": 9001,
	# DELIBERATELY DIFFERENT FROM `entity`. Equal values would make this guard vacuous: the
	# defect it exists to catch is a command carrying the entity where the band id belongs, and
	# that reads identical when the two agree.
	"band_id": 4201,
	"faction": 0,
	"current_x": 8,
	"current_y": 6,
	"size": 30,
	"label": "River Band",
	"morale_cause": 1,
	"activity": "forage",
	"work_range": 2,
	"hunt_reach": 7,
	"scout_reveal_radius": 3,
	"is_traveling": true,
	"travel_target_x": 11,
	"travel_target_y": 9,
	"working_age": 16,
	"idle_workers": 7,
	"labor_assignments": [
		{"kind": "forage", "workers": 5, "target_x": 7, "target_y": 6, "actual_yield": 0.42, "sustainable_yield": 0.42},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "actual_yield": 0.31, "sustainable_yield": 0.18},
		{"kind": "scout", "workers": 3},
	],
	"stores": {"provisions": 120.0},
	# Expedition discriminators (distinct non-default values so a dropped copy shows up).
	"is_expedition": true,
	"expedition_mission": "scout",
	"expedition_phase": "awaiting",
	"max_expedition_party_size": 8,
	"expedition_target_herd": "game_deer_07",
	"expedition_floor": 0.3,
	"expedition_eta_turns": 6,       # int count → presence + _expect_int
	"expedition_recurring": true,    # bool → presence + explicit check
	"home_band_entity": 7777,
	# The decoder's PROJECTION of `pendingReveal{X,Y}` — how many tiles this party still owes its home
	# band, never the coordinates. It is a term of the cancel-in-camp predicate the Occupants drawer's
	# Recall/Cancel button reads OFF THE MARKER (`_selected_unit` is the map-click payload), so it has
	# to survive the copy like every other panel-consumed field.
	"pending_reveal_count": 3,
	# Pre-launch hunt-trip forecast levers (global config echoed on every cohort). The horizon is the
	# scale every "never completed" sentinel is relative to, and the IN-FLIGHT denial readout reads it
	# off this marker (a launched party's own cohort), so it has to survive the copy like the warn line.
	"expedition_viability_warn_turns": 20,
	"expedition_forecast_horizon_turns": 60,
}

var _failures: Array[String] = []
## How many keys the source dict carried — printed on the PASS line, because a partition over an EMPTY
## source is vacuously true and reads exactly like a real one.
var _source_key_count := 0

func _ready() -> void:
	var mv: Node = MAP_VIEW.new()
	var entry: Dictionary = FIXTURE_ENTRY.duplicate(true)
	entry.merge(FRACTIONAL_ROUND_TRIP_KEYS, true)
	var snapshot := {"populations": [entry]}
	mv._rebuild_unit_markers(snapshot)

	var markers: Array = mv.units
	if markers.size() != 1:
		_fail("expected exactly 1 marker, got %d" % markers.size())
		_finish()
		mv.free()
		return
	var marker: Dictionary = markers[0]
	_source_key_count = entry.size()

	# 1. **THE PARTITION.** Every key the source dict carried must survive onto the marker, and every
	#    key the marker has beyond that must be a DECLARED stamp. Both directions, because each alone
	#    passes on a broken copy: a subset test is satisfied by a marker that dropped nothing but
	#    invented a field nobody declared, and a superset test by one that copied wholesale and then
	#    quietly stopped.
	for key in entry:
		if MARKER_OMITTED_KEYS.has(key):
			continue
		if not marker.has(key):
			_fail("marker DROPPED source key '%s' — the copy in _rebuild_unit_markers is not structural"
					% key)
	for key in marker:
		if entry.has(key) or MARKER_STAMPED_KEYS.has(key):
			continue
		_fail("marker carries UNDECLARED key '%s' — add it to MARKER_STAMPED_KEYS with its reason" % key)
	# …and the omission list is a partition, not a wish: a key excused here that the source never had
	# is a stale excuse, and reads as coverage that does not exist.
	for key in MARKER_OMITTED_KEYS:
		if not entry.has(key):
			_fail("MARKER_OMITTED_KEYS names '%s', which the source dict does not carry — stale excuse"
					% key)

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
	_expect_int(marker, "band_id", 4201)
	_expect_int(marker, "faction", 0)
	_expect_int(marker, "morale_cause", 1)
	_expect_str(marker, "activity", "forage")
	_expect_str(marker, "expedition_mission", "scout")
	_expect_str(marker, "expedition_phase", "awaiting")
	_expect_int(marker, "max_expedition_party_size", 8)
	_expect_str(marker, "expedition_target_herd", "game_deer_07")
	_expect_float(marker, "expedition_floor", 0.3)
	_expect_int(marker, "home_band_entity", 7777)
	_expect_int(marker, "pending_reveal_count", 3)
	_expect_int(marker, "expedition_viability_warn_turns", 20)
	_expect_int(marker, "expedition_forecast_horizon_turns", 60)
	_expect_int(marker, "expedition_eta_turns", 6)
	if not bool(marker.get("is_expedition", false)):
		_fail("is_expedition did not round-trip to true (defaulted?)")
	if not bool(marker.get("expedition_recurring", false)):
		_fail("expedition_recurring did not round-trip to true (defaulted?)")

	# 3. Fractional round-trip guard: a continuous field must NOT be narrowed by the marker copy.
	for key in FRACTIONAL_ROUND_TRIP_KEYS:
		var want: float = float(FRACTIONAL_ROUND_TRIP_KEYS[key])
		if not marker.has(key):
			continue  # the superset guard above already reported a missing key
		var got := float(marker.get(key))
		if absf(got - want) > FRACTIONAL_EPSILON:
			_fail("%s did NOT round-trip: fed %s, marker returned %s (narrowed with int()?)"
					% [key, str(want), str(got)])

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
		print("marker_field_guard: PASS — marker partitions into its source's %d keys + %d declared stamps, and round-trips values" % [_source_key_count, MARKER_STAMPED_KEYS.size()])
		get_tree().quit(0)
	else:
		printerr("marker_field_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
