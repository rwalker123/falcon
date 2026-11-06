## THE STANDING RUNG AND THE RUNG QUOTES, DERIVED — the whole test tree's one transcription of how a
## fixture patch or herd states its ladder.
##
## Every fixture patch and every fixture herd carries `current_rung` (the field the client asks *"has
## this rung been built"* of) and `rung_quotes` (one quote per rung above the branch root: the generic
## shape the wire publishes since issue #175, which replaced the per-step fields `is_cultivated`,
## `field_progress`, `corral_yield`, `domestication`, ...). Stating them as literals beside the flags
## they must agree with is how a fixture stages a source the wire cannot produce — a Field standing on
## `plant:wild`, a penned herd that is not enclosed — and then proves something about it. So no fixture
## in `tools/` spells a rung or a quote: they come through here.
##
## **THE FIXTURES STILL AUTHOR THE SHIPPED STEPS WITH THE OLD FLAT KEYS** (`is_cultivated`,
## `cultivation_progress`, `tended_yield`, `domestication`, `corralled`, `corral_yield`,
## `can_cultivate`, `sow_payoff`, ...). That is deliberate: those keys are the harness's own AUTHORING
## vocabulary for the two shipped webs and are read ONLY here, where they are turned into the wire's
## generic quotes. Nothing in `src/` reads them. A fixture that wants a rung the shipped ladder does
## not have (the `terrace` and `stud` steps) authors `rung_quotes` directly and states no flat keys, and
## `stamp_*` leaves such a row alone.
##
## **IT IS THE SIM'S DERIVATION, RESTATED, NOT AN INVENTION.** `forage::patch_rung_key` is *"sown →
## field, cultivated → tended, else wild"* and `fauna`'s animal twin is *"penned → pen, tamed →
## pastoral, else wild"*; a quote's `reached` is *standing at or above this rung*.
##
## **ALL-`static`, NO STATE** — the `input_probe.gd` shape, so `map_preview`, `band_panel_preview`,
## `snapshot_alias_guard` and any chapter can `preload` it without owning a node.

# ---- the shipped rung keys, spelled once (the harness's, not the client's) ---------------------------
const KEY_WILD_PLANT := "plant:wild"
const KEY_TENDED := "plant:tended"
const KEY_FARM := "plant:farm"

## The two lessons the Farm and the Pen teach, the Pen's size tier, and the shipped figures it carries
## (`intensification_ladder.json`): a Pen fenced to `RANCH_MIN_RINGS` is a Ranch, whose keepers mind
## `RANCH_KEEPER_RATIO_GAIN` times the head, and keeping one teaches Selective breeding.
const KNOWLEDGE_WORKED_LAND := "worked_land"
const KNOWLEDGE_SELECTIVE_BREEDING := "selective_breeding"
const TIER_RANCH := "ranch"
const RANCH_MIN_RINGS := 2
const RANCH_KEEPER_RATIO_GAIN := 1.5
const SELECTIVE_BREEDING_REGROWTH_GAIN := 1.25
const KEY_WILD_ANIMAL := "animal:wild"
const KEY_PASTORAL := "animal:pastoral"
const KEY_PEN := "animal:pen"

## The harness-only rungs a fixture catalog adds beyond the shipped ladder, to prove a fourth step
## renders with no client edit (issue #175).
const KEY_TERRACE := "plant:terrace"
const KEY_STUD := "animal:stud"

## The shipped tame meter's full reading, which is how a herd row said it was fully tamed.
const DOMESTICATION_COMPLETE := 1.0

## **THE FIXTURE CATALOG** — `ladder_rungs` rows for the shipped plant and animal branches, in the shape
## the native decoder writes them (`dict::subsistence::ladder_rungs_to_array`), carrying only the fields
## the quotes' join reads. A transcription of `intensification_ladder.json`; the claim that it matches is
## the sim's.
static func ladder_rungs(with_extra_steps: bool = false) -> Array:
	var rows: Array = [
		_rung(KEY_WILD_PLANT, "plant", 1, "", [], "", "cultivation", "", "Wild ground", "patch"),
		_rung(KEY_TENDED, "plant", 2, "cultivate", ["managed"], "cultivation", "seed_selection",
			KEY_WILD_PLANT, "Tended patch", "patch", ["requires_work_room", "requires_crop", "requires_ownership"]),
		_rung(KEY_FARM, "plant", 3, "sow", ["managed", "sown"], "seed_selection", KNOWLEDGE_WORKED_LAND,
			KEY_TENDED, "Farm", "patch", ["requires_crop", "requires_ownership", "places_source"],
			{"site_rule": _site_rule(true, 0.0, true),
				"site_relaxations": [{"knowledge": KNOWLEDGE_WORKED_LAND,
					"rule": _site_rule(false, 80.0, true)}]}),
		_rung(KEY_WILD_ANIMAL, "animal", 1, "", [], "", "herding", "", "Wild herd", "herd"),
		_rung(KEY_PASTORAL, "animal", 2, "tame", ["managed", "domesticated"], "herding", "penning",
			KEY_WILD_ANIMAL, "Tamed herd", "herd", ["requires_work_room", "requires_ownership"]),
		_rung(KEY_PEN, "animal", 3, "corral", ["managed", "domesticated", "enclosed"], "penning",
			"", KEY_PASTORAL, "Pen", "herd", ["requires_held_rung_below", "requires_ownership"],
			{"size_tiers": [{"id": TIER_RANCH, "label": "Ranch", "label_fallback": "a ranch",
				"min_rings": RANCH_MIN_RINGS, "keeper_ratio_gain": RANCH_KEEPER_RATIO_GAIN,
				"earns_knowledge": KNOWLEDGE_SELECTIVE_BREEDING}],
			"knowledge_gains": [{"knowledge": KNOWLEDGE_SELECTIVE_BREEDING,
				"regrowth_gain": SELECTIVE_BREEDING_REGROWTH_GAIN}]}),
	]
	if with_extra_steps:
		rows.append(_rung(KEY_TERRACE, "plant", 4, "terrace", ["managed", "sown"], "terracing", "",
			KEY_FARM, "Terrace", "patch", ["requires_held_rung_below", "requires_crop", "requires_ownership", "places_source"]))
		rows.append(_rung(KEY_STUD, "animal", 4, "stud", ["managed", "domesticated", "enclosed"],
			"studbooks", "", KEY_PEN, "Stud", "herd", ["requires_held_rung_below", "requires_ownership"]))
	return rows

static func _rung(key: String, branch: String, order: int, verb: String, traits: Array,
		unlock: String, earns: String, requires: String, label: String, source_kind: String,
		gates: Array = [], extras: Dictionary = {}) -> Dictionary:
	var flags := {}
	for flag in ["requires_held_rung_below", "requires_crop", "requires_work_room",
			"requires_ownership", "places_source"]:
		flags[flag] = gates.has(flag)
	var row := {"rung_key": key, "branch": branch, "order": order, "verb": verb, "traits": traits,
		"unlock_knowledge": unlock, "earns_knowledge": earns, "requires_rung": requires,
		"label": label, "label_fallback": label.to_lower(), "display_name": label,
		"source_kind": source_kind}
	row.merge(flags)
	row.merge(extras)
	return row

## A catalog `site_rule` row as the native decoder writes it.
static func _site_rule(gathering_site: bool, min_capacity: float, fresh_water: bool) -> Dictionary:
	return {"requires_gathering_site": gathering_site, "min_forage_capacity": min_capacity,
		"requires_fresh_water": fresh_water}

# ---- the standing rung ----------------------------------------------------------------------------

## The plant web's rung, off the pair of bools a patch row carries.
static func patch_rung_key(tended: bool, field: bool) -> String:
	if field:
		return KEY_FARM
	return KEY_TENDED if tended else KEY_WILD_PLANT

## The animal web's rung, off the meter and the flag a herd row carries. `domestication` is compared
## against `DOMESTICATION_COMPLETE` for the same reason the sim stamps `animal:pastoral` there: taming
## has no bool of its own, its achievement IS its meter.
static func herd_rung_key(domestication: float, corralled: bool) -> String:
	if corralled:
		return KEY_PEN
	if domestication >= DOMESTICATION_COMPLETE:
		return KEY_PASTORAL
	return KEY_WILD_ANIMAL

# ---- the quotes -------------------------------------------------------------------------------------

## True when the row still carries the flat authoring keys (`is_cultivated`, `domestication`, ...).
static func _authored_flat(row: Dictionary, prefix: String, keys: Array) -> bool:
	for key in keys:
		if row.has(prefix + String(key)):
			return true
	return false

const _PATCH_FLAT_KEYS := ["is_cultivated", "is_field", "cultivation_progress", "field_progress",
	"tended_yield", "field_yield", "tended_fodder", "field_fodder", "sow_site_refusal",
	"cultivation_work_cost", "field_work_cost", "cultivation_work_done", "field_work_done",
	"cultivation_upkeep_demand", "field_upkeep_demand", "extra_step"]
const _HERD_FLAT_KEYS := ["domestication", "corralled", "corral_progress", "corral_yield",
	"pastoral_yield", "tame_work_cost", "corral_work_cost", "tame_work_done", "corral_work_done",
	"tame_upkeep_demand", "corral_upkeep_demand", "corral_material", "pastoral_material", "extra_step",
	"stud_progress", "stud_work_cost", "stud_work_done", "stud_yield"]

static func _num(row: Dictionary, key: String, default: float = 0.0) -> float:
	return float(row.get(key, default))

static func _quote(rung_key: String, reached: bool, progress: float, work_done: float,
		work_cost: float, build_material: Array, upkeep: float, upkeep_material: Array,
		food_yield: float, fodder: float, material_payoff: Array, site_refusal: String) -> Dictionary:
	return {"rung_key": rung_key, "reached": reached, "progress": progress, "work_done": work_done,
		"work_cost": work_cost, "build_material_cost": build_material, "upkeep_demand": upkeep,
		"upkeep_material_demand": upkeep_material, "food_yield": food_yield, "fodder": fodder,
		"material_payoff": material_payoff, "site_refusal": site_refusal}

static func _patch_quotes(patch: Dictionary, prefix: String) -> Array:
	var field := bool(patch.get(prefix + "is_field", false))
	var tended := bool(patch.get(prefix + "is_cultivated", false)) or field
	var tended_progress := _num(patch, prefix + "cultivation_progress",
		1.0 if tended else 0.0)
	var field_progress := _num(patch, prefix + "field_progress", 1.0 if field else 0.0)
	var quotes := [
		_quote(KEY_TENDED, tended, tended_progress, _num(patch, prefix + "cultivation_work_done"),
			_num(patch, prefix + "cultivation_work_cost"), [],
			_num(patch, prefix + "cultivation_upkeep_demand"),
			patch.get(prefix + "cultivation_upkeep_material_demand", []),
			_num(patch, prefix + "tended_yield"), _num(patch, prefix + "tended_fodder"), [], ""),
		_quote(KEY_FARM, field, field_progress, _num(patch, prefix + "field_work_done"),
			_num(patch, prefix + "field_work_cost"), [],
			_num(patch, prefix + "field_upkeep_demand"),
			patch.get(prefix + "field_upkeep_material_demand", []),
			_num(patch, prefix + "field_yield"), _num(patch, prefix + "field_fodder"), [],
			String(patch.get(prefix + "sow_site_refusal", ""))),
	]
	# `extra_step` authors the harness-only fourth plant rung (`plant:terrace`), reached when its meter is
	# full, so a fixture can stand a Field with a step above it and price that step from `terrace_*` keys.
	if bool(patch.get(prefix + "extra_step", false)):
		var terrace_progress := _num(patch, prefix + "terrace_progress")
		quotes.append(_quote(KEY_TERRACE, terrace_progress >= 1.0, terrace_progress,
			_num(patch, prefix + "terrace_work_done"), _num(patch, prefix + "terrace_work_cost"), [], 0.0, [],
			_num(patch, prefix + "terrace_yield"), 0.0, [], ""))
	return quotes

static func _herd_quotes(herd: Dictionary, prefix: String) -> Array:
	var corralled := bool(herd.get(prefix + "corralled", false))
	var domestication := _num(herd, prefix + "domestication", 1.0 if corralled else 0.0)
	var tamed := corralled or domestication >= DOMESTICATION_COMPLETE
	var quotes := [
		_quote(KEY_PASTORAL, tamed, domestication, _num(herd, prefix + "tame_work_done"),
			_num(herd, prefix + "tame_work_cost"), [], _num(herd, prefix + "tame_upkeep_demand"),
			herd.get(prefix + "tame_upkeep_material_demand", []),
			_num(herd, prefix + "pastoral_yield"), 0.0, herd.get(prefix + "pastoral_material", []), ""),
		_quote(KEY_PEN, corralled, _num(herd, prefix + "corral_progress", 1.0 if corralled else 0.0),
			_num(herd, prefix + "corral_work_done"), _num(herd, prefix + "corral_work_cost"),
			herd.get(prefix + "corral_build_material_cost", []),
			_num(herd, prefix + "corral_upkeep_demand"),
			herd.get(prefix + "corral_upkeep_material_demand", []),
			_num(herd, prefix + "corral_yield"), 0.0, herd.get(prefix + "corral_material", []), ""),
	]
	# `extra_step` authors the harness-only fourth rung (`animal:stud`) as an unreached quote, so a
	# fixture can stand a penned herd that still has a rung left to climb.
	if bool(herd.get(prefix + "extra_step", false)):
		var stud_progress := _num(herd, prefix + "stud_progress")
		quotes.append(_quote(KEY_STUD, stud_progress >= 1.0, stud_progress,
			_num(herd, prefix + "stud_work_done"), _num(herd, prefix + "stud_work_cost"), [], 0.0, [],
			_num(herd, prefix + "stud_yield"), 0.0, [], ""))
	return quotes

## One composition entry's quotes (the crop picker's per-plant figures), off the flat authoring keys.
static func _entry_quotes(entry: Dictionary) -> Array:
	var quotes := [
		{"rung_key": KEY_TENDED, "can_climb": bool(entry.get("can_cultivate", false)),
			"food_payoff": _num(entry, "cultivate_payoff"),
			"yield_ratio": _num(entry, "cultivate_yield_ratio"),
			"fodder_payoff": _num(entry, "cultivate_fodder_payoff"),
			"material_payoff": entry.get("cultivate_material_payoff", []), "work_cost": 0.0},
		{"rung_key": KEY_FARM, "can_climb": bool(entry.get("can_sow", false)),
			"food_payoff": _num(entry, "sow_payoff"), "yield_ratio": _num(entry, "sow_yield_ratio"),
			"fodder_payoff": _num(entry, "sow_fodder_payoff"),
			"material_payoff": entry.get("sow_material_payoff", []),
			"work_cost": _num(entry, "sow_work_cost")},
	]
	if entry.has("can_terrace"):
		quotes.append({"rung_key": KEY_TERRACE, "can_climb": bool(entry.get("can_terrace", false)),
			"food_payoff": _num(entry, "terrace_payoff"), "yield_ratio": _num(entry, "terrace_yield_ratio"),
			"fodder_payoff": 0.0, "material_payoff": [], "work_cost": _num(entry, "terrace_work_cost")})
	return quotes

static func _stamp_composition(patch: Dictionary, prefix: String) -> void:
	var composition: Variant = patch.get(prefix + "composition", [])
	if not (composition is Array):
		return
	for entry_variant in (composition as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if entry.has("rung_quotes") and not _authored_flat(entry, "", ["can_cultivate", "can_sow"]):
			continue
		entry["rung_quotes"] = _entry_quotes(entry)

## **STAMP A PATCH DICT WITH THE RUNG AND THE QUOTES ITS OWN FLAGS IMPLY**, and hand it back for
## chaining. This is the form to reach for after MUTATING a fixture: a chapter that flips
## `is_cultivated` on a copy of a base fixture and forgets to re-stamp has silently built nothing.
## `prefix` is `""` for a bare wire row and `patch_` for a `tile_info` cross-ref — the SAME prefix the
## reader will spell, which is why it is one argument and not a second function.
##
## A row that authored `rung_quotes` itself and states no flat key is left alone (its `current_rung`
## is still derived from the quotes), which is how a fixture stages a rung the shipped ladder lacks.
static func stamp_patch(patch: Dictionary, prefix: String = "") -> Dictionary:
	if _authored_flat(patch, prefix, _PATCH_FLAT_KEYS) or not patch.has(prefix + "rung_quotes"):
		patch[prefix + "rung_quotes"] = _patch_quotes(patch, prefix)
		patch[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = patch_rung_key(
			bool(patch.get(prefix + "is_cultivated", false)),
			bool(patch.get(prefix + "is_field", false)))
	else:
		patch[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = _held_key(patch, prefix,
			KEY_WILD_PLANT)
	_stamp_composition(patch, prefix)
	RungQuotes.enrich_sources([patch], ladder_rungs(true), prefix)
	return patch

## The animal twin. Herd rows are bare-keyed everywhere in this tree (a herd has no `tile_info`
## cross-ref), so the prefix is offered for symmetry and is `""` in every call today.
static func stamp_herd(herd: Dictionary, prefix: String = "") -> Dictionary:
	if _authored_flat(herd, prefix, _HERD_FLAT_KEYS) or not herd.has(prefix + "rung_quotes"):
		herd[prefix + "rung_quotes"] = _herd_quotes(herd, prefix)
		herd[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = herd_rung_key(
			_num(herd, prefix + "domestication"), bool(herd.get(prefix + "corralled", false)))
	else:
		herd[prefix + SourceForecast.FORECAST_CURRENT_RUNG_KEY] = _held_key(herd, prefix,
			KEY_WILD_ANIMAL)
	RungQuotes.enrich_sources([herd], ladder_rungs(true), prefix)
	return herd

## The key of the highest reached authored quote, `root_key` while none is.
static func _held_key(row: Dictionary, prefix: String, root_key: String) -> String:
	var held := root_key
	for quote in RungQuotes.quotes(row, prefix):
		if bool(quote.get("reached", false)):
			held = String(quote.get("rung_key", held))
	return held

## Stamp a source the caller has not classified: a HERD where the row carries a herd's markers (and no
## basket), a patch otherwise. `prefix` is `patch_` for a `tile_info` cross-ref, which is always a patch.
## It is what `ForageFx.floorify` -- the harness's one gate into the HUD for a source fixture -- calls, so
## every fixture that reaches the HUD arrives with its quotes whatever chapter authored it.
static func stamp_source(src: Dictionary, prefix: String = "") -> Dictionary:
	if src.is_empty():
		return src
	# A `tile_info` with no `patch_` key at all carries no patch (a barren hex): stamping it would
	# conjure quotes -- and with them a farm the server would never have admitted -- onto ground that
	# has no patch row.
	if prefix != "":
		var has_patch := false
		for key in src.keys():
			if String(key).begins_with(prefix):
				has_patch = true
				break
		if not has_patch:
			return src
	var is_herd := prefix == "" and not src.has("composition") \
		and (src.has("species") or src.has("husbandry_ceiling") or src.has("body_mass"))
	return stamp_herd(src, prefix) if is_herd else stamp_patch(src, prefix)

## **THE WHOLE-ARRAY FORMS**, for a fixture function that returns its rows straight out of a literal:
## `return RUNG_FX.stamp_patches([...])` stamps every row and hands the same array back.
## Stamp AFTER any mutation.
static func stamp_patches(patches: Array, prefix: String = "") -> Array:
	for row in patches:
		stamp_patch(row, prefix)
	return patches

static func stamp_herds(herds: Array, prefix: String = "") -> Array:
	for row in herds:
		stamp_herd(row, prefix)
	return herds


## **STAMP A WHOLE SNAPSHOT'S SOURCES, for a harness that feeds `MapView` directly** (no `Main`, so no
## ingest-time enrichment). A row carrying the flat authoring keys is stamped from them; a row that
## states only its standing `current_rung` (the form `map_preview` fixtures take) has its flags struck
## off that rung first, so the quotes and the rung cannot disagree. Any quotes already on a row that
## authors no flat key are discarded and re-derived, so a rung moved after an earlier stamp wins.
static func stamp_snapshot(snapshot: Dictionary) -> Dictionary:
	for patch_variant in snapshot.get("forage_patches", []):
		if not (patch_variant is Dictionary):
			continue
		var patch: Dictionary = patch_variant
		if not _authored_flat(patch, "", _PATCH_FLAT_KEYS):
			patch.erase("rung_quotes")
			var standing := String(patch.get(SourceForecast.FORECAST_CURRENT_RUNG_KEY, ""))
			if standing != "":
				patch["is_field"] = standing == KEY_FARM
				patch["is_cultivated"] = standing == KEY_FARM or standing == KEY_TENDED
		stamp_patch(patch)
	for herd_variant in snapshot.get("herds", []):
		if not (herd_variant is Dictionary):
			continue
		var herd: Dictionary = herd_variant
		if not _authored_flat(herd, "", _HERD_FLAT_KEYS):
			herd.erase("rung_quotes")
			var standing_rung := String(herd.get(SourceForecast.FORECAST_CURRENT_RUNG_KEY, ""))
			if standing_rung != "":
				herd["corralled"] = standing_rung == KEY_PEN
				herd["domestication"] = DOMESTICATION_COMPLETE \
					if standing_rung == KEY_PEN or standing_rung == KEY_PASTORAL else 0.0
		stamp_herd(herd)
	return snapshot
