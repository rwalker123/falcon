extends Node

## Headless **regression guard for the "a decoded forage-patch field never reaches `tile_info`" bug
## class** — the plant web's second wiring.
##
## ## The gap this closes
##
## A herd dict travels WHOLE: the compose sheet reads it straight off the snapshot row. A forage
## patch does not. `MapView._tile_info_at` copies the `forage_patches` row across **key by key, from
## an explicit list**, `patch_`-prefixing each one, and every forage compose sheet reads its source
## out of that `tile_info`. So appending a field to `ForagePatchState` is **TWO wirings on the plant
## web**, and only the second is visible in the panel: a field the decoder emits but that list omits
## is silently absent — no error, no zero to notice, just a reading the sheet never states.
##
## It has shipped three times. `perWorkerBiomass` / `regrowthSamples` were decoded for a release and
## never crossed, which removed the harvest-floor chart and both crew targets from every patch
## against a live sim. Then `materialPerBiomass` / `perWorkerMaterial`: a tile of 56% tobacco and 44%
## hay grass rendered a PER TURN box naming the fodder and never the tobacco.
##
## **Both were invisible to `ui_preview` and `band_panel_preview`**, and structurally so: their
## fixtures seed `tile_info` themselves, so no frame in either harness exercises the cross-ref at
## all. A second seeded frame cannot see this class of bug; only a run over the REAL cross-ref can.
##
## ## What it asserts
##
## The run is *wire → `tile_info`*: the generated fixture envelope → the real `SnapshotDecoder` →
## `MapView._ingest_forage_patches` → `MapView._tile_info_at`, with nothing hand-written in between.
## Taking the raw patch from the DECODER rather than from a literal is the whole point — a
## hand-written fixture only carries the keys someone remembered to add, which is precisely the thing
## that fails.
##
##   1. **The PARTITION, forwards.** Every key the decoder puts on the patch arrives on `tile_info`
##      as `patch_<key>`, unless it is declared in `UNCROSSED_KEYS` with a reason. A newly appended
##      wire field therefore fails HERE, at the wiring, rather than in a panel weeks later.
##   2. **The PARTITION, backwards.** Every `patch_`-prefixed key on `tile_info` corresponds to a key
##      the patch actually carries — a misspelled or invented one has no source and fails.
##   3. **The VALUE round-trips.** Each crossed key is compared by value, so a copy that narrows a
##      float with `int(...)` or drops a vector's rows fails even though the key is present.
##   4. **THE CONSUMER PINCER.** Every wire key `SourceForecast` names as a forecast field (its own
##      `FORECAST_*_KEY` constants and `FORECAST_*_KEYS` tables, read reflectively so the list cannot
##      drift) must be crossed if the patch carries it — which is what stops an `UNCROSSED_KEYS`
##      entry being added for a field the forecast layer reads.
##   4b. **THE ROAD'S OWN CROSS-REF, joined on its TILE.** A `routes` row is a per-tile improvement
##      whose identity is its `tile_x`/`tile_y` pair, and it reaches the tile card through
##      `MapView.road_tile_lookup` — a join, not a copy, so it fails one way rather than the patch's
##      key-by-key way: get the key names wrong and EVERY road silently stops existing, with no
##      error and nothing on screen. That is exactly what the retired `path_x`/`path_y` reads were
##      doing (`get`-with-default, an empty path per road, a blank map). It is asserted here rather
##      than in a preview harness for the reason at the top of this file: both harnesses seed
##      `tile_info` themselves and cannot see a cross-ref at all.
##   5. **The FoW redaction.** A crossed patch key is live patch state and belongs in
##      `MapView.FOW_DISCOVERED_HIDDEN_KEYS`, the one rule the whole patch payload follows, unless it
##      is declared in `FOW_EXEMPT_KEYS` as ground knowledge a remembered tile still holds. This is
##      the THIRD wiring, and it fails the same way the second does: silently, on a hex you cannot
##      currently see.
##
## Fixture: `cargo xtask decode-fixture` writes `snapshot_envelope.bin` (gitignored, so a fresh
## checkout has none). Its patch rows are saturated, which is what makes the value claims bite.
##
## Run as a scene (NOT --script: `MapView.gd` reaches the TerrainDefinitions autoload). Pure ingest
## logic, no rendering, so --headless is fine:
##   godot --headless --path . res://tools/patch_crossref_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const MAP_VIEW := preload("res://src/scripts/MapView.gd")
## Reached by PATH and loaded into a `Script`-typed local rather than `preload`ed into a const: this
## guard reads `SourceForecast`'s constants REFLECTIVELY, and `get_script_constant_map()` is an
## instance method on `Script` — called on a const holding a `class_name`d script the compiler
## resolves the name to the CLASS and refuses it with `Cannot call non-static function … directly`,
## which is a load failure, which in a headless run is a HANG rather than a message.
const SOURCE_FORECAST_PATH := "res://src/scripts/ui/hud/SourceForecast.gd"

const FIXTURE_PATH := "res://tests/fixtures/snapshot_envelope.bin"
## The one spelling of the cross-ref's prefix, read from the vocabulary module the compose sheet
## reads it from — `RungGates.forage_gates_from_patch` is the other place the bare↔prefixed mapping
## is written down, and a guard with its own copy would be a third.
const PATCH_PREFIX := HudComposeVocab.FORAGE_FORECAST_PREFIX

## The fixture grid. One tile is enough — the cross-ref is per patch, not per neighbourhood — but the
## coordinates have to be inside it or `_tile_info_at` returns early with the bare pair.
const GRID_W := 4
const GRID_H := 3

## **Wire keys that deliberately do NOT cross, each with the reason it does not.** An entry here is a
## decision, so it is written down beside the key; claim 4 independently refuses one for any key the
## forecast layer reads.
const UNCROSSED_KEYS := {
	"x": "the tile the patch sits on — it IS the lookup key, and `tile_info` carries its own x/y",
	"y": "as x",
	"trade_per_biomass":
		"a retired `(deprecated)` wire slot (arc #527 took the trade account); nothing reads it",
	"tended_trade": "as trade_per_biomass — the rung-2 payoff in the retired account",
	"field_trade": "as trade_per_biomass — the rung-3 payoff in the retired account",
}

## **Crossed keys that are NOT redacted on a remembered tile**, each with the reason. Everything else
## the patch carries is live state a discovered-but-unseen hex cannot know, and belongs in
## `FOW_DISCOVERED_HIDDEN_KEYS`.
const FOW_EXEMPT_KEYS := {
	"patch_tile_capacity":
		"the TILE's own K with no rung gain in it — terrain, which a Discovered hex knows by definition, and what a remembered card states in place of the redacted `patch_carrying_capacity` beside it",
	"patch_composition":
		"what GROWS here is ground knowledge like the terrain label; a remembered tile remembers the mix",
	"patch_committed_species":
		"read only under the Forage line, which is already past the discovered early-return",
	"patch_committed_display_name": "as patch_committed_species",
	"patch_cultivation_upkeep_demand":
		"the rung's standing PRICE, not this patch's bill — both plant rungs are `scaled_by: source_load` and the sim strikes them through the TILE's K, which is terrain, so the figure sent for an unseen hex is the one it last showed",
	"patch_field_upkeep_demand": "as patch_cultivation_upkeep_demand",
	"patch_cultivation_upkeep_material_demand":
		"the same per-rung PRICE as patch_cultivation_upkeep_demand above, in the other currency and struck through the same tender-load — so it carries no rung either, and splitting the pair would have `RungLadder._price_terms` quote a remembered hex a PARTIAL price (it composes ONE clause from the work rate and the goods)",
	"patch_field_upkeep_material_demand": "as patch_cultivation_upkeep_material_demand",
}

## The two vectors whose absence was the reported bug. Asserted NON-EMPTY on the fixture before
## anything is claimed about them: a partition over a key whose value is `[]` passes whether or not
## the rows survive the copy, and this guard exists because that distinction was invisible once.
const WITNESS_VECTOR_KEYS := ["material_per_biomass", "per_worker_material"]

var _failures: Array[String] = []


func _ready() -> void:
	var patch := _decoded_patch()
	if patch.is_empty():
		_finish(0)
		return

	var mv: MapView = MAP_VIEW.new()
	mv.grid_width = GRID_W
	mv.grid_height = GRID_H
	var terrain := PackedInt32Array()
	terrain.resize(GRID_W * GRID_H)
	mv.terrain_overlay = terrain
	mv._ingest_forage_patches({"forage_patches": [patch]})

	var col := int(patch.get("x", 0))
	var row := int(patch.get("y", 0))
	var info := mv._tile_info_at(col, row)

	_assert_fixture_is_saturated(patch)
	_assert_every_wire_key_crosses(patch, info)
	_assert_every_crossed_key_has_a_source(patch, info)
	_assert_forecast_keys_cross(patch, info)
	_assert_crossed_keys_are_fog_redacted(info)
	_assert_the_road_row_joins_on_its_tile(mv)

	mv.free()
	_finish(patch.size())


## ⛔ **CLAIM 4b — A ROAD ROW REACHES THE TILE CARD, AND IT IS JOINED ON ITS TILE.**
##
## A road is a per-tile improvement whose identity is `tile_x`/`tile_y`; `MapView._ingest_road_network`
## resolves that pair once and keys `road_tile_lookup` by it, and `_tile_info_at` cross-refs the hex
## under the cursor against it. **The failure mode is total and silent**: read the pair under the
## wrong names and every road drops out of both the map draw and the tile card, through
## `get`-with-default, with nothing to notice. That is the state this arc found the client in.
##
## Driven off the DECODER's own row rather than a literal, the whole file's discipline — a
## hand-written fixture carries only the keys someone remembered.
func _assert_the_road_row_joins_on_its_tile(mv: MapView) -> void:
	var road := _decoded_road()
	if road.is_empty():
		return
	mv._ingest_road_network([road])
	var tile := Vector2i(int(road.get("tile_x", -1)), int(road.get("tile_y", -1)))
	# The PRECONDITION, so the claim below cannot pass vacuously on a row the fixture never placed.
	if tile.x < 0 or tile.y < 0:
		_fail("the decoded road row carries no tile_x/tile_y — the join has nothing to key on")
		return
	if mv.road_network.size() != 1:
		_fail("a decoded road row did not survive the ingest (road_network holds %d)"
			% mv.road_network.size())
		return
	var found: Array = mv._roads_on_tile(tile.x, tile.y)
	if found.is_empty():
		_fail("the road on (%d, %d) does not cross onto its own tile — `road_tile_lookup` is keyed on something the wire does not send, which drops every road from the map AND the tile card silently"
			% [tile.x, tile.y])
		return
	# …and it is THAT road, by its rung, so a join that stamped some other row on the hex fails too.
	var crossed: Dictionary = found[0]
	if String(crossed.get("rung", "")) != String(road.get("rung", "")):
		_fail("the road crossed onto (%d, %d) is not the row the wire sent (`%s` vs `%s`)"
			% [tile.x, tile.y, crossed.get("rung", ""), road.get("rung", "")])
	# **AND A NEIGHBOURING HEX CARRIES NO ROAD**, which is what stops a lookup that answers "yes" for
	# every tile from passing the claim above.
	if not mv._roads_on_tile(tile.x + 1, tile.y + 1).is_empty():
		_fail("a hex with no road on it answered with one — the join is not keyed on the tile at all")

## Claim 0 — the fixture actually carries the vectors the value claims are about.
func _assert_fixture_is_saturated(patch: Dictionary) -> void:
	for key: String in WITNESS_VECTOR_KEYS:
		var rows: Variant = patch.get(key, null)
		if not (rows is Array) or (rows as Array).is_empty():
			_fail(("the fixture patch carries no `%s` rows, so every claim about that vector is " +
				"vacuous — reseed it in xtask/src/decode_fixture.rs") % key)


## Claim 1 + 3 — every wire key crosses, and its value survives the crossing intact.
func _assert_every_wire_key_crosses(patch: Dictionary, info: Dictionary) -> void:
	for key: String in patch.keys():
		if UNCROSSED_KEYS.has(key):
			continue
		var crossed := PATCH_PREFIX + key
		if not info.has(crossed):
			_fail(("`%s` is on the wire patch but NOT on tile_info as `%s` — the decoder emits it " +
				"and MapView._tile_info_at never copies it, so it is silently absent on the plant " +
				"web (append the cross-ref line, or declare it in UNCROSSED_KEYS with a reason)")
				% [key, crossed])
			continue
		if info[crossed] != patch[key]:
			_fail(("`%s` crossed to `%s` with a CHANGED value (%s → %s) — a narrowing copy " +
				"(int() over a float) or a dropped vector reads as present and is not")
				% [key, crossed, str(patch[key]), str(info[crossed])])


## Claim 2 — nothing `patch_`-prefixed is invented on the way.
func _assert_every_crossed_key_has_a_source(patch: Dictionary, info: Dictionary) -> void:
	for key: String in info.keys():
		if not key.begins_with(PATCH_PREFIX):
			continue
		var bare := key.substr(PATCH_PREFIX.length())
		if not patch.has(bare):
			_fail(("tile_info carries `%s`, but the wire patch has no `%s` — the cross-ref invented " +
				"or misspelled a key, and every reader of it answers the default forever") % [key, bare])


## Claim 4 — the pincer. A key `SourceForecast` reads may never be an `UNCROSSED_KEYS` entry.
func _assert_forecast_keys_cross(patch: Dictionary, info: Dictionary) -> void:
	for key: String in _forecast_wire_keys():
		if not patch.has(key):
			continue   # the patch does not publish it (an animal-web field); nothing to claim
		var crossed := PATCH_PREFIX + key
		if not info.has(crossed):
			_fail(("`%s` is a SourceForecast forecast field the wire patch publishes, and it is not " +
				"on tile_info as `%s` — the compose sheet reads it there and there only") % [key, crossed])


## Claim 5 — the redaction wiring, which fails as silently as the cross-ref does.
func _assert_crossed_keys_are_fog_redacted(info: Dictionary) -> void:
	for key: String in info.keys():
		if not key.begins_with(PATCH_PREFIX):
			continue
		if FOW_EXEMPT_KEYS.has(key):
			continue
		if not MAP_VIEW.FOW_DISCOVERED_HIDDEN_KEYS.has(key):
			_fail(("`%s` crosses onto tile_info but is not in FOW_DISCOVERED_HIDDEN_KEYS — live patch " +
				"state on a hex the player cannot currently see (add it, or declare it in " +
				"FOW_EXEMPT_KEYS as ground knowledge with a reason)") % key)


## Every wire key `SourceForecast` names as a forecast field, read off the script's OWN constants so
## this list cannot drift from the layer it is guarding: the `FORECAST_*_KEY` scalars, plus the
## `FORECAST_*_KEYS` improvement tables, whose VALUES are wire key names keyed by rung.
func _forecast_wire_keys() -> PackedStringArray:
	var keys := PackedStringArray()
	var forecast_script: Script = load(SOURCE_FORECAST_PATH)
	var constants := forecast_script.get_script_constant_map()
	for name: String in constants.keys():
		if not name.begins_with("FORECAST_"):
			continue
		var value: Variant = constants[name]
		if name.ends_with("_KEY") and value is String:
			keys.append(value as String)
		elif name.ends_with("_KEYS") and value is Dictionary:
			for entry: Variant in (value as Dictionary).values():
				if entry is String or entry is StringName:
					keys.append(String(entry))
	return keys


## **THE WHOLE DECODED FIXTURE, decoded ONCE.** Two sections are read out of it now, and decoding a
## second time would be a second chance for the two claims to run against different worlds.
var _decoded_cache: Dictionary = {}
var _decoded_once := false

func _decoded_snapshot() -> Dictionary:
	if _decoded_once:
		return _decoded_cache
	_decoded_once = true
	if not ClassDB.class_exists("SnapshotDecoder"):
		_fail("SnapshotDecoder class is not registered — build the native extension first (cargo xtask godot-build)")
		return {}
	if not FileAccess.file_exists(FIXTURE_PATH):
		_fail("no fixture at %s — generate it with: cargo xtask decode-fixture" % FIXTURE_PATH)
		return {}
	var payload := FileAccess.get_file_as_bytes(FIXTURE_PATH)
	if payload.is_empty():
		_fail("fixture at %s is empty" % FIXTURE_PATH)
		return {}
	var decoder: Object = ClassDB.instantiate("SnapshotDecoder")
	if decoder == null:
		_fail("ClassDB.instantiate(\"SnapshotDecoder\") returned null — the native extension is half-loaded; rebuild with cargo xtask godot-build")
		return {}
	# Taken as an untyped Variant deliberately: a gdext panic returns the method's DEFAULT, and a
	# failed assignment into a typed local ABORTS this function, so `get_tree().quit()` never runs and
	# the headless process hangs (the trap `decode_guard._decode_or_die` exists for).
	var decoded: Variant = decoder.call("decode_snapshot", payload)
	if not (decoded is Dictionary) or (decoded as Dictionary).is_empty():
		_fail("decode_snapshot returned no snapshot for a %d-byte envelope — regenerate the fixture (cargo xtask decode-fixture)" % payload.size())
		return {}
	_decoded_cache = decoded as Dictionary
	return _decoded_cache

## The fixture's first forage patch, straight out of the real decoder.
func _decoded_patch() -> Dictionary:
	var decoded := _decoded_snapshot()
	if decoded.is_empty():
		return {}
	var patches: Variant = decoded.get("forage_patches", [])
	if not (patches is Array) or (patches as Array).is_empty():
		_fail("the decoded fixture carries no forage_patches — this guard has nothing to cross")
		return {}
	return (patches as Array)[0]


## The decoder's own first `routes` row, `{}` when the fixture carries none (which is itself a
## failure — an empty section is a claim that checks nothing).
func _decoded_road() -> Dictionary:
	var decoded := _decoded_snapshot()
	if decoded.is_empty():
		return {}
	var roads: Variant = decoded.get("routes", [])
	if not (roads is Array) or (roads as Array).is_empty():
		_fail("the decoded fixture carries no routes — the road cross-ref has nothing to join")
		return {}
	return (roads as Array)[0]


func _fail(msg: String) -> void:
	_failures.append(msg)


func _finish(wire_keys: int) -> void:
	if _failures.is_empty():
		# The key COUNT is printed for the reason `marker_field_guard` prints its own: a partition over
		# an empty source is vacuously true and otherwise indistinguishable from a real pass.
		print("patch_crossref_guard: PASS — %d wire keys cross onto tile_info intact (%d declared uncrossed)"
			% [wire_keys, UNCROSSED_KEYS.size()])
		get_tree().quit(0)
	else:
		printerr("patch_crossref_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
