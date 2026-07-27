extends Node

## Headless regression guard for the "MapView writes into the decoder's cached world" bug class,
## and for the deep copies that guard used to be paid for with.
##
## `MapView.display_snapshot` ingests five snapshot sub-trees into its own lookups — culture layers,
## food modules, discovered sites, forage patches, and the per-cohort harvest/scout targets. Those
## rows are NOT the client's to own: the native decoder keeps the frame it published as the baseline
## the next delta patches, and it merges by shallow-duplicating the cached array and writing changed
## slots (`native/src/snapshot/cache.rs`), so an unchanged row is literally the same `Dictionary`
## object frame after frame. Holding it is free; **writing to it edits the decoder's world.**
##
## Every ingest used to `duplicate(true)` its rows, which made that impossible but cost ~42 ms/turn
## (issue #389). The copies are gone, and this guard pins the two halves of what replaced them:
##
##   1. **Nothing MapView ingests is written back.** The two ingests that stamp a derived key
##      (`food_modules` → `terrain_id`, `harvest` → `module_label`) take a SHALLOW `duplicate()`
##      first, so the frame's own rows come out of `display_snapshot` byte-for-byte unchanged.
##   2. **Everything else is held by reference**, so the win is real. `is_same()` is the only test
##      that can say this — an equal-valued copy passes any value assertion.
##
## Both directions matter and they pull against each other, which is why they are asserted together:
## dropping the shallow copy fails (1); re-adding a `duplicate()` "for safety" fails (2).
##
## Run as a scene (NOT --script: MapView.gd references the TerrainTextureManager autoload, which
## only registers when the project is loaded). Pure ingest logic, no rendering, so --headless is
## fine:
##   godot --headless --path . res://tools/snapshot_alias_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const MAP_VIEW := preload("res://src/scripts/MapView.gd")

const PLAYER_FACTION := 0
const GRID_W := 4
const GRID_H := 3
# The tile every fixture row sits on, and the terrain id seeded there — so `terrain_id` asserts a
# real lookup rather than the `-1` an unseeded MapView would answer for any coordinate.
const SITE_X := 2
const SITE_Y := 1
const SITE_TERRAIN_ID := 37

var _failures: Array[String] = []

func _ready() -> void:
	var mv: Node = MAP_VIEW.new()
	mv.grid_width = GRID_W
	mv.grid_height = GRID_H
	var terrain := PackedInt32Array()
	terrain.resize(GRID_W * GRID_H)
	terrain[SITE_Y * GRID_W + SITE_X] = SITE_TERRAIN_ID
	mv.terrain_overlay = terrain

	var snapshot := _fixture()
	mv._ingest_culture_layers(snapshot)
	mv._ingest_food_modules(snapshot)
	mv._ingest_discovered_sites(snapshot)
	mv._ingest_forage_patches(snapshot)
	mv._ingest_population_sites(snapshot)

	var key := Vector2i(SITE_X, SITE_Y)
	var src_layer: Dictionary = (snapshot["culture_layers"] as Array)[0]
	var src_food: Dictionary = (snapshot["food_modules"] as Array)[0]
	var src_wonder: Dictionary = ((snapshot["discovered_sites"] as Array)[0] as Dictionary)["sites"][0]
	var src_patch: Dictionary = (snapshot["forage_patches"] as Array)[0]
	var src_cohort: Dictionary = (snapshot["populations"] as Array)[0]
	var src_harvest: Dictionary = src_cohort["harvest"]
	var src_scout: Dictionary = src_cohort["scout"]

	# 1. NO WRITE-BACK. The frame's rows must be exactly what the decoder published — the derived
	#    keys belong on MapView's copies and nowhere else. A failure here means the ingest edited a
	#    dictionary the decoder is still holding as its baseline, so the stamp survives into every
	#    later delta that does not replace the row.
	if src_food.has("terrain_id"):
		_fail("food_modules row was STAMPED IN PLACE with terrain_id — the ingest wrote into the frame")
	if src_harvest.has("module_label"):
		_fail("cohort harvest row was STAMPED IN PLACE with module_label — the ingest wrote into the frame")
	if src_food.size() != 5:
		_fail("food_modules row gained/lost keys (%d, expected 5) — the ingest mutated the frame" % src_food.size())
	if src_harvest.size() != 3:
		_fail("cohort harvest row gained/lost keys (%d, expected 3) — the ingest mutated the frame" % src_harvest.size())

	# 2. HELD BY REFERENCE. Value equality cannot see a reintroduced copy; identity can.
	_expect_same(mv.culture_layer_map.get(7, null), src_layer, "culture_layer_map[7]")
	_expect_same(mv.forage_patch_lookup.get(key, null), src_patch, "forage_patch_lookup[site]")
	_expect_same(mv.discovered_site_lookup.get(key, null), src_wonder, "discovered_site_lookup[site]")
	if mv.discovered_sites.size() == 1:
		_expect_same(mv.discovered_sites[0], src_wonder, "discovered_sites[0]")
	else:
		_fail("discovered_sites size %d, expected 1" % mv.discovered_sites.size())
	var scout_entries: Variant = mv.scout_sites.get(key, null)
	if scout_entries is Array and (scout_entries as Array).size() == 1:
		_expect_same((scout_entries as Array)[0], src_scout, "scout_sites[site][0]")
	else:
		_fail("scout_sites[site] is not a 1-entry Array (got %s)" % str(scout_entries))

	# 2b. The nested sub-tree is what made the forage copy expensive (~25 scalars plus a per-species
	#     `composition` array of dictionaries). Holding the row holds the roster with it.
	var patch_kept: Variant = mv.forage_patch_lookup.get(key, null)
	if patch_kept is Dictionary:
		_expect_same((patch_kept as Dictionary).get("composition", null), src_patch["composition"],
				"forage patch composition")

	# 3. The two SHALLOW copies: a distinct top-level dictionary (so the stamp lands here), the
	#    stamped value resolved off MapView's own state, and the payload carried through.
	var food_kept: Variant = mv.food_site_lookup.get(key, null)
	if not (food_kept is Dictionary):
		_fail("food_site_lookup[site] missing (got %s)" % str(food_kept))
	else:
		var food: Dictionary = food_kept
		if is_same(food, src_food):
			_fail("food site is the FRAME's dictionary — the stamp would write into the decoder's world")
		if int(food.get("terrain_id", -1)) != SITE_TERRAIN_ID:
			_fail("food site terrain_id = %s, expected %d" % [str(food.get("terrain_id", null)), SITE_TERRAIN_ID])
		if String(food.get("module", "")) != "riverine_delta":
			_fail("food site lost its module through the shallow copy (got '%s')" % String(food.get("module", "")))
		if mv.food_sites.size() != 1 or not is_same(mv.food_sites[0], food):
			_fail("food_sites[0] and food_site_lookup[site] are not the same stamped row")

	var harvest_entries: Variant = mv.harvest_sites.get(key, null)
	if not (harvest_entries is Array) or (harvest_entries as Array).size() != 1:
		_fail("harvest_sites[site] is not a 1-entry Array (got %s)" % str(harvest_entries))
	else:
		var harvest: Dictionary = (harvest_entries as Array)[0]
		if is_same(harvest, src_harvest):
			_fail("harvest entry is the FRAME's dictionary — the stamp would write into the decoder's world")
		if String(harvest.get("module_label", "")) == "":
			_fail("harvest entry did not get its module_label stamp")
		if int(harvest.get("target_x", -1)) != SITE_X:
			_fail("harvest entry lost target_x through the shallow copy")

	_finish()
	mv.free()

## One frame's worth of every sub-tree the five ingests read, all pointing at the same tile so the
## lookups can be probed with one key. Deliberately NOT `duplicate(true)`d anywhere: these are the
## dictionaries whose identity the assertions above are about.
func _fixture() -> Dictionary:
	return {
		"culture_layers": [
			{"id": 7, "scope": "Regional", "scope_label": "Regional", "owner": "000000000000000A",
				"owner_value": 10, "parent": -1},
		],
		"food_modules": [
			{"x": SITE_X, "y": SITE_Y, "module": "riverine_delta", "seasonal_weight": 0.75,
				"kind": "forage"},
		],
		"discovered_sites": [
			{"faction": PLAYER_FACTION, "sites": [
				{"x": SITE_X, "y": SITE_Y, "kind": "wonder", "name": "Sky Arch"},
			]},
		],
		"forage_patches": [
			{"x": SITE_X, "y": SITE_Y, "biomass": 12.5, "carrying_capacity": 40.0,
				"cultivation_progress": 0.25, "is_cultivated": false, "per_worker_yield": 0.6,
				"composition": [
					{"species": "hazel", "share": 0.6},
					{"species": "sedge", "share": 0.4},
				]},
		],
		"populations": [
			{"entity": 9001, "faction": PLAYER_FACTION,
				"harvest": {"target_x": SITE_X, "target_y": SITE_Y, "module": "riverine_delta"},
				"scout": {"target_x": SITE_X, "target_y": SITE_Y, "reveal_radius": 3}},
		],
	}

func _expect_same(got: Variant, want: Variant, label: String) -> void:
	if got == null:
		_fail("%s is missing" % label)
	elif not is_same(got, want):
		_fail("%s is a COPY of the frame's row, not the row itself (a duplicate() crept back in)" % label)

func _fail(msg: String) -> void:
	_failures.append(msg)

func _finish() -> void:
	if _failures.is_empty():
		print("snapshot_alias_guard: PASS — ingests hold the frame's rows and never write into them")
		get_tree().quit(0)
	else:
		printerr("snapshot_alias_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
