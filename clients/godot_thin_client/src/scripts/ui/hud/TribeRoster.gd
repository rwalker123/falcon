class_name TribeRoster
extends RefCounted

## **WHO IS IN A TRIBE, AND WHAT THE SWITCHER WALKS** — all-`static`, stateless (`.claude/rules/client/tribe-panel.md`).
## The tribe's identity is `tribe_id`; a band that carries one is a member, a band that carries none
## is its own subject. Every reader here takes the roster as a PARAMETER.

const SUBJECT_KIND := "kind"
const SUBJECT_BAND := "band"
const SUBJECT_TRIBE := "tribe"
const SUBJECT_BAND_DICT := "band_dict"
const SUBJECT_TRIBE_ID := "tribe_id"
const SUBJECT_NAME := "name"
const SUBJECT_MEMBERS := "members"

static func tribe_id_of(band: Dictionary) -> int:
	return int(band.get(HudTribeVocab.KEY_TRIBE_ID, HudTribeVocab.NO_TRIBE))

static func is_member(band: Dictionary) -> bool:
	return tribe_id_of(band) != HudTribeVocab.NO_TRIBE and not bool(band.get("is_expedition", false))

## The own member bands of one tribe, in the order the sim lists them (`tribe_member_bands`, largest
## first); a member the list does not name follows in roster order.
static func members(bands: Array, tribe_id: int) -> Array:
	var found: Array = []
	for band_variant in bands:
		if band_variant is Dictionary and is_member(band_variant) \
				and tribe_id_of(band_variant) == tribe_id:
			found.append(band_variant)
	if found.size() < 2:
		return found
	var order: Array = (found[0] as Dictionary).get(HudTribeVocab.KEY_MEMBER_BANDS, [])
	var ordered: Array = []
	for band_id_variant in order:
		for band in found:
			if int((band as Dictionary).get("band_id", -1)) == int(band_id_variant) \
					and not ordered.has(band):
				ordered.append(band)
	for band in found:
		if not ordered.has(band):
			ordered.append(band)
	return ordered

## One entry per top-level subject, in player-band order: a band in no tribe is its own entry; a tribe
## is one entry at the position of its first member band.
static func subjects(bands: Array) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var seen: Dictionary = {}
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		var band: Dictionary = band_variant
		if bool(band.get("is_expedition", false)):
			continue
		if not is_member(band):
			out.append({SUBJECT_KIND: SUBJECT_BAND, SUBJECT_BAND_DICT: band})
			continue
		var tribe_id := tribe_id_of(band)
		if seen.has(tribe_id):
			continue
		seen[tribe_id] = true
		out.append({SUBJECT_KIND: SUBJECT_TRIBE, SUBJECT_TRIBE_ID: tribe_id,
			SUBJECT_NAME: String(band.get(HudTribeVocab.KEY_TRIBE_NAME, "")),
			SUBJECT_MEMBERS: members(bands, tribe_id)})
	return out

static func subject_index_of_band(subject_list: Array, band_entity: int) -> int:
	for i in range(subject_list.size()):
		var subject: Dictionary = subject_list[i]
		if String(subject[SUBJECT_KIND]) == SUBJECT_BAND \
				and int((subject[SUBJECT_BAND_DICT] as Dictionary).get("entity", -1)) == band_entity:
			return i
	return -1

static func subject_index_of_tribe(subject_list: Array, tribe_id: int) -> int:
	for i in range(subject_list.size()):
		var subject: Dictionary = subject_list[i]
		if String(subject[SUBJECT_KIND]) == SUBJECT_TRIBE and int(subject[SUBJECT_TRIBE_ID]) == tribe_id:
			return i
	return -1

## The band a tribe-addressed command names: the first member in the sim's order.
static func address_band(member_list: Array) -> Dictionary:
	return member_list[0] if not member_list.is_empty() and member_list[0] is Dictionary else {}

## `{band_id: name}` for a tribe's members.
static func names_by_band_id(member_list: Array, name_of: Callable) -> Dictionary:
	var names: Dictionary = {}
	for band in member_list:
		names[int((band as Dictionary).get("band_id", -1))] = String(name_of.call(band))
	return names

## The gathering place, `Vector2i(-1, -1)` when the tribe has no shared anchor.
static func gathering_place(member_list: Array) -> Vector2i:
	for band in member_list:
		if bool((band as Dictionary).get(HudTribeVocab.KEY_HAS_ANCHOR, false)):
			return Vector2i(int((band as Dictionary).get(HudTribeVocab.KEY_ANCHOR_X, -1)),
				int((band as Dictionary).get(HudTribeVocab.KEY_ANCHOR_Y, -1)))
	return Vector2i(-1, -1)

## The ceiling the tribe's levy stands at (every member publishes the same one).
static func levy_ceiling(member_list: Array) -> int:
	if member_list.is_empty():
		return HudTribeVocab.LEVY_NOT_IN_TRIBE
	return int((member_list[0] as Dictionary).get(HudTribeVocab.KEY_LEVY_CEILING, 0))

## The tribe's shared works, as published on the first member.
static func works(member_list: Array) -> Array:
	if member_list.is_empty():
		return []
	var raw: Variant = (member_list[0] as Dictionary).get(HudTribeVocab.KEY_WORKS, [])
	return raw if raw is Array else []

## A band's hands split three ways for the tribe page's crew bar: `{home, tribe, idle}` plus the row counts
## `{jobs, tribe_rows}`. **THE TOTAL IS THE BAND'S OWN PEOPLE SPLIT** (`people_split`: working + idle - the same split
## its People bar draws), so builders, the bench and every hand not on a labor row are in it.
##
## `tribe` is the workers on `for_tribe` rows PLUS the builders on those rows' declared builds, plus the builders on
## this band's own road builds at a `route` entry of the tribe's shared works. A site's build crew is published on
## the SOURCE (`builders_staffed`), keyed the way the work board reads it - the patch by tile, the herd by id, the
## working by tile and material, the tile improvement by tile and branch, the band improvement by branch, the road by
## tile - and one source is counted once. A site crew can mix bands, so the builders are capped at this band's own
## (`builders_of`). `home` is the working hands less those; `idle` the split's. Standing roles are not jobs.
static func crew_split(labor: HudBandLaborState, band: Dictionary, works: Array = []) -> Dictionary:
	var split := labor.people_split(band)
	var working := int(split["working"])
	var idle := int(split["idle"])
	var tribe_workers := 0
	var jobs := 0
	var tribe_rows := 0
	var builders_by_source: Dictionary = {}
	for row_variant in HudBandLaborState.labor_assignments_of(band):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var kind := String(row.get(HudTribeVocab.ROW_KIND, "")).to_lower()
		if bool(row.get(HudTribeVocab.ROW_FOR_TRIBE, false)):
			tribe_workers += HudBandLaborState.staffed_total(row)
			tribe_rows += 1
			_note_row_builders(labor, band, row, kind, builders_by_source)
			continue
		if kind != HudConst.LABOR_KIND_SCOUT and kind != HudConst.LABOR_KIND_WARRIOR:
			jobs += 1
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	for work_variant in works:
		if not (work_variant is Dictionary) \
				or String((work_variant as Dictionary).get(HudTribeVocab.WORK_BRANCH, "")) != HudTribeVocab.ROAD_BRANCH:
			continue
		var road := _road_at(labor, int((work_variant as Dictionary).get(HudTribeVocab.WORK_X, -1)),
			int((work_variant as Dictionary).get(HudTribeVocab.WORK_Y, -1)))
		if not road.is_empty() and HudRouteVocab.has_keeper(road) \
				and HudRouteVocab.keeper_band_id_of(road) == band_id:
			builders_by_source[_road_key(road)] = SourceForecast.build_crew_staffed(
				road, SourceForecast.BARE_SOURCE_PREFIX)
	var builders := 0
	for key in builders_by_source:
		builders += int(builders_by_source[key])
	builders = mini(builders, labor.builders_of(band))
	var tribe := mini(tribe_workers + builders, working)
	return {"home": working - tribe, "tribe": tribe, "idle": idle, "jobs": jobs, "tribe_rows": tribe_rows}

## Record the builders on the source a `for_tribe` row's declared build is on, once per source.
static func _note_row_builders(labor: HudBandLaborState, band: Dictionary, row: Dictionary, kind: String,
		out: Dictionary) -> void:
	var x := int(row.get("target_x", -1))
	var y := int(row.get("target_y", -1))
	var source: Dictionary = {}
	var key := ""
	match kind:
		SourceForecast.LABOR_KIND_FORAGE:
			source = labor.forage_patch_lookup().get(Vector2i(x, y), {})
			key = "forage:%d,%d" % [x, y]
		SourceForecast.LABOR_KIND_HUNT:
			var herd_id := String(row.get("fauna_id", ""))
			source = labor.find_world_herd(herd_id)
			key = "hunt:%s" % herd_id
		HudConst.LABOR_KIND_EXTRACT:
			var material := String(row.get("material", ""))
			source = labor.find_deposit(x, y, material)
			key = "extract:%d,%d:%s" % [x, y, material]
		HudConst.LABOR_KIND_TILE:
			var branch := String(row.get("branch", ""))
			source = labor.tile_improvement_at(x, y, branch)
			key = "tile:%d,%d:%s" % [x, y, branch]
		HudConst.LABOR_KIND_BAND:
			var band_branch := String(row.get("branch", ""))
			out["band:%s" % band_branch] = HudStorageVocab.band_builders_staffed(band, band_branch)
			return
		HudConst.LABOR_KIND_ROADWORK:
			source = _road_at(labor, x, y)
			key = _road_key(source) if not source.is_empty() else ""
	if key != "" and not source.is_empty():
		out[key] = SourceForecast.build_crew_staffed(source, SourceForecast.BARE_SOURCE_PREFIX)

static func _road_at(labor: HudBandLaborState, x: int, y: int) -> Dictionary:
	for road in labor.roads():
		if road is Dictionary and int((road as Dictionary).get("tile_x", -1)) == x \
				and int((road as Dictionary).get("tile_y", -1)) == y:
			return road
	return {}

static func _road_key(road: Dictionary) -> String:
	return "road:%d,%d" % [int(road.get("tile_x", -1)), int(road.get("tile_y", -1))]
