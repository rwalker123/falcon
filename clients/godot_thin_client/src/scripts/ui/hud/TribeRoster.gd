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

## A band's hands split three ways for the tribe page's crew bar: `{home, tribe, idle}` and the
## row counts `{jobs, tribe_rows}`. Standing roles (scout, warrior) are not jobs.
static func crew_split(labor: HudBandLaborState, band: Dictionary) -> Dictionary:
	var home := 0
	var tribe := 0
	var jobs := 0
	var tribe_rows := 0
	for row_variant in HudBandLaborState.labor_assignments_of(band):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var workers := HudBandLaborState.staffed_total(row)
		var kind := String(row.get(HudTribeVocab.ROW_KIND, "")).to_lower()
		if bool(row.get(HudTribeVocab.ROW_FOR_TRIBE, false)):
			tribe += workers
			tribe_rows += 1
			continue
		home += workers
		if kind != HudConst.LABOR_KIND_SCOUT and kind != HudConst.LABOR_KIND_WARRIOR:
			jobs += 1
	return {"home": home, "tribe": tribe, "idle": labor.people_idle(band),
		"jobs": jobs, "tribe_rows": tribe_rows}
