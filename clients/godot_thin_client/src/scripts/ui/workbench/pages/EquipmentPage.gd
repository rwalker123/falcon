extends WorkbenchPage
class_name EquipmentPage

## THE EQUIPMENT PAGE — the TOE roster the world was built with, and what each band resolves to
## under it (`docs/plan_early_game_labor.md` → "Equipment / TOE",
## `.claude/rules/core_sim/equipment.md`).
##
## The kit system was invisible on the designer surface: reading what a kit grants — or why a band is
## hauling 12 instead of 40 — meant reading `equipment.json` or the sim's logs. Everything this page
## shows was already on the wire, so it is **pure read side**: no command, no schema, no sim.
##
## It is a SECOND READER of `KitRoster`, not a second copy of it. The wire keys, the display names,
## the bare-handed tier and the "does this kit use that component?" test all come from there, so the
## compose sheets and this page cannot drift into two answers about one roster.
##
## **`PopulationCohortState.kitId` NAMES THE HUNT TIERS ONLY**, and this page is exactly the shape
## that gets that wrong. For a resident band it is the hunt job's default; the forage tier resolves
## through the *forage* default, which rides the wire once as `SubsistenceSection.defaultForageKitId`.
## Rendering the gather rate under `kitId` reads a gathering number off `big_game`, which has no
## basket component at all — a plausible row that is simply wrong. The sim pins it in
## `kit_selection::a_resident_bands_published_kit_answers_for_the_hunt_tiers_only`; `_forage_kit_id`
## below is where this end honours it, and `workbench_preview` asserts the rendered line.

# ---- the wire's own keys ---------------------------------------------------
# Declared here, beside the one file that reads them, the way `KitRoster` declares the kit keys it
# reads. The per-KIT and per-BAND keys are NOT restated — they are `KitRoster`'s, and a second
# spelling of one is how two readers of one wire start disagreeing.
const KITS_KEY := "kits"
const DEFAULT_HUNT_KIT_KEY := "default_hunt_kit_id"
const DEFAULT_FORAGE_KIT_KEY := "default_forage_kit_id"
const POPULATIONS_KEY := "populations"
const COHORT_ENTITY_KEY := "entity"
const COHORT_FACTION_KEY := "faction"
const COHORT_SIZE_KEY := "size"
const COHORT_IS_EXPEDITION_KEY := "is_expedition"
const COHORT_KIT_KEY := "kit_id"
const COHORT_ASSIGNMENTS_KEY := "labor_assignments"
## **THE COHORT'S RESOLVED TIERS, AND THEY ARE NOT THE KIT'S KEYS.** The two carry axes happen to
## share a spelling with `KitRoster`'s roster keys and the attack axis does NOT — a kit publishes
## `attack`, a cohort publishes `hunterAttack`, because on a band it is the term the combat gate
## `max(0, attack − defense)` compares against a herd's `defense`. Reading the band's attack through
## the roster's key answers the `0.0` default on every band, which is a plausible-looking readout of
## a party that cannot hurt anything; all three are therefore spelled out here rather than borrowed
## from the roster on the strength of two of them coinciding.
const COHORT_ATTACK_KEY := "hunter_attack"
const COHORT_HUNT_CARRY_KEY := "hunt_carry_per_worker_biomass"
const COHORT_FORAGE_CARRY_KEY := "forage_carry_per_worker_biomass"
const ASSIGNMENT_KIND_KEY := "kind"
const ASSIGNMENT_KIT_KEY := "kit_id"

## The three components, paired with the condition key each is read from. One table, walked by both
## the condition line and the "what does this kit consume?" line, so the two cannot list different
## components.
const COMPONENTS: Array[Dictionary] = [
	{
		"label": WorkbenchVocab.EQUIPMENT_COMPONENT_SPEARS,
		"condition": KitRoster.BAND_SPEARS_CONDITION_KEY,
		"axis": KitRoster.KIT_ATTACK_KEY,
	},
	{
		"label": WorkbenchVocab.EQUIPMENT_COMPONENT_SLED,
		"condition": KitRoster.BAND_SLED_CONDITION_KEY,
		"axis": KitRoster.KIT_HUNT_CARRY_KEY,
	},
	{
		"label": WorkbenchVocab.EQUIPMENT_COMPONENT_BASKETS,
		"condition": KitRoster.BAND_BASKETS_CONDITION_KEY,
		"axis": KitRoster.KIT_FORAGE_CARRY_KEY,
	},
]

# ---- state -----------------------------------------------------------------
# ALL of it is the world's, which is what makes `reset()` real here.
var _kits: Array = []
var _default_hunt_kit_id := ""
var _default_forage_kit_id := ""
var _cohorts: Array = []

var _roster_body: VBoxContainer = null
var _bands_body: VBoxContainer = null


# ---- body ------------------------------------------------------------------

func build() -> void:
	add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	add_child(WorkbenchWidgets.build_banner(WorkbenchVocab.EQUIPMENT_BANNER, HudStyle.SIGNAL))

	_roster_body = _block_column()
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.EQUIPMENT_ROSTER_HEADING, _roster_body))
	_bands_body = _block_column()
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.EQUIPMENT_BANDS_HEADING, _bands_body))
	# Built before any frame can arrive, so both wells open on their degraded line rather than empty.
	_render()


## The column one group's blocks stack in.
static func _block_column() -> VBoxContainer:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", WorkbenchWidgets.ROW_GAP)
	return column


# ---- ingest ----------------------------------------------------------------

## **PRESENCE IS NOT A CHANGE SIGNAL; `changed_sections` IS.** `SnapshotDecoder::decode_delta` builds
## a merged frame as `cache.dict.duplicate_shallow()` overwritten with the delta's keys, so **every
## baseline key rides every merged frame** — `data.has(KITS_KEY)` is true on all of them, and a gate
## resting on absence never skips. The manifest is the replacement signal (`SnapshotSections.changed`,
## which reads a frame carrying no manifest — a full snapshot — as "everything changed", so the frame
## that must repaint is never starved). `Main` gates this same key exactly this way.
##
## `SubsistenceSection.kits` is a per-world CONSTANT: its manifest entry moves only on a world rebuild,
## which is what makes the roster half of `reset()` real.
##
## **THE "…OR I AM HOLDING NOTHING" CLAUSE IS LOAD-BEARING — it is what makes the shell's page-switch
## replay work.** A replayed cached frame reports `kits` unchanged, so under a `changed`-only gate a
## page activated between turns would sit empty until the next one; the same clause re-seeds the page
## after `reset()`. The four cases it resolves: first full snapshot (changed → ingest), steady delta
## (unchanged, holding data → skip), page-switch replay or post-reset (unchanged, holding nothing →
## ingest), world rebuild (changed → ingest).
##
## The two defaults are taken from the same frame for the reason `Main.update_kit_roster` takes them
## together: a roster ingested without them names no default anywhere.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	var moved := false
	if data.has(KITS_KEY) and (SnapshotSections.changed(data, KITS_KEY) or _kits.is_empty()):
		var kits: Variant = data.get(KITS_KEY, [])
		_kits = kits if kits is Array else []
		_default_hunt_kit_id = String(data.get(DEFAULT_HUNT_KIT_KEY, ""))
		_default_forage_kit_id = String(data.get(DEFAULT_FORAGE_KIT_KEY, ""))
		moved = true
	if data.has(POPULATIONS_KEY) \
			and (SnapshotSections.changed(data, POPULATIONS_KEY) or _cohorts.is_empty()):
		_cohorts = _player_cohorts(data.get(POPULATIONS_KEY, []))
		moved = true
	if moved:
		_render()


## WORLD BOUNDARY — and unlike `ConfigTuningPage`, this one is NOT a no-op. Every field above came
## from the world that has just ended: the roster is that world's config, the cohorts are its bands.
## Holding them across a rebuild would show the next world the previous one's kits until its first
## frame landed, and — because the roster is only re-sent on a rebuild — a roster the new world may
## never restate.
func reset() -> void:
	_kits = []
	_default_hunt_kit_id = ""
	_default_forage_kit_id = ""
	_cohorts = []
	_render()


## The player faction's cohorts, in wire order. The page is bounded to them deliberately: every
## faction's bands would be a page nobody can read, and the question this page answers ("why is MY
## band hauling 12?") is asked of the player's.
static func _player_cohorts(populations: Variant) -> Array:
	var out: Array = []
	if not (populations is Array):
		return out
	for entry in populations:
		if entry is Dictionary and int((entry as Dictionary).get(COHORT_FACTION_KEY, -1)) \
				== HudConst.PLAYER_FACTION_ID:
			out.append(entry)
	return out


# ---- render ----------------------------------------------------------------

func _render() -> void:
	_render_roster()
	_render_bands()


## The roster group: the bare-handed tier once, then one block per kit in WIRE ORDER — `none` is an
## ordinary member and sorts last because `equipment.json` authors it last, not because anything here
## says so.
func _render_roster() -> void:
	if _roster_body == null:
		return
	HudWidgets.clear_children(_roster_body)
	if _kits.is_empty():
		_roster_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.EQUIPMENT_NO_ROSTER))
		return
	_roster_body.add_child(WorkbenchWidgets.build_caption(_bare_handed_line(), HudStyle.INK_DIM))
	for entry in _kits:
		if entry is Dictionary:
			_roster_body.add_child(_kit_block(entry))


## The unequipped tier on all three axes, read off the ROSTER — every kit publishes the bare-handed
## number on each axis it does not use, so the minimum across the roster is that axis's bare tier.
## An axis no kit states answers `INF`; the tier face renders it as such rather than inventing a
## number the sim never sent.
func _bare_handed_line() -> String:
	return _join([
		WorkbenchVocab.EQUIPMENT_BARE_LABEL,
		WorkbenchVocab.EQUIPMENT_ATTACK_FORMAT % _tier_face(
			KitRoster.unequipped_tier(_kits, KitRoster.KIT_ATTACK_KEY)),
		WorkbenchVocab.EQUIPMENT_HUNT_CARRY_FORMAT % _tier_face(
			KitRoster.unequipped_tier(_kits, KitRoster.KIT_HUNT_CARRY_KEY)),
		WorkbenchVocab.EQUIPMENT_FORAGE_CARRY_FORMAT % _tier_face(
			KitRoster.unequipped_tier(_kits, KitRoster.KIT_FORAGE_CARRY_KEY)),
	])


## One roster entry: its display name on the row, then three captions — identity, the tiers it grants
## a FRESH party, and the components it consumes. Only the name is a non-wrapping Label; everything
## else wraps, which is what keeps a long kit id from swelling the whole content column.
func _kit_block(kit: Dictionary) -> Control:
	var block := _line_column()
	block.add_child(WorkbenchWidgets.build_row_label(KitRoster.kit_display_name(kit)))
	block.add_child(WorkbenchWidgets.build_caption(_kit_identity_line(kit)))
	block.add_child(WorkbenchWidgets.build_caption(_kit_tiers_line(kit)))
	block.add_child(WorkbenchWidgets.build_caption(_kit_consumes_line(kit)))
	return block


func _kit_identity_line(kit: Dictionary) -> String:
	var kit_id := String(kit.get(KitRoster.KIT_ID_KEY, ""))
	var parts: Array[String] = [kit_id, _jobs_face(kit)]
	if kit_id == _default_hunt_kit_id:
		parts.append(WorkbenchVocab.EQUIPMENT_HUNT_DEFAULT_TAG)
	if kit_id == _default_forage_kit_id:
		parts.append(WorkbenchVocab.EQUIPMENT_FORAGE_DEFAULT_TAG)
	return _join(parts)


## The verbs this kit may be sent on. A kit named for a job outside its own list is a COMMAND FAILURE
## server-side, never a silent fall back to a default, which is why the list is worth stating.
static func _jobs_face(kit: Dictionary) -> String:
	var jobs: Variant = kit.get(KitRoster.KIT_JOBS_KEY, [])
	if not (jobs is Array) or (jobs as Array).is_empty():
		return WorkbenchVocab.EQUIPMENT_NO_JOBS
	var names: Array[String] = []
	for job in jobs:
		names.append(String(job))
	return WorkbenchVocab.EQUIPMENT_JOBS_FORMAT % WorkbenchVocab.EQUIPMENT_JOBS_SEPARATOR.join(names)


## **THE THREE TIERS A FRESH PARTY GETS — never a band's.** They are the roster's own numbers, and
## the wear that moves them lives on a cohort, which is the group below this one.
static func _kit_tiers_line(kit: Dictionary) -> String:
	return _join([
		WorkbenchVocab.EQUIPMENT_ATTACK_FORMAT % _tier_face(
			float(kit.get(KitRoster.KIT_ATTACK_KEY, 0.0))),
		WorkbenchVocab.EQUIPMENT_HUNT_CARRY_FORMAT % _tier_face(
			float(kit.get(KitRoster.KIT_HUNT_CARRY_KEY, 0.0))),
		WorkbenchVocab.EQUIPMENT_FORAGE_CARRY_FORMAT % _tier_face(
			float(kit.get(KitRoster.KIT_FORAGE_CARRY_KEY, 0.0))),
	])


## Which components this kit actually spends — the axes on which it beats the bare-handed tier
## (`KitRoster.kit_uses`). `none` consumes nothing and the line says so: a kit that spends no
## durability is what makes a bare-handed comparison free to run.
func _kit_consumes_line(kit: Dictionary) -> String:
	var used: Array[String] = []
	for component in COMPONENTS:
		if KitRoster.kit_uses(_kits, kit, String(component["axis"])):
			used.append(String(component["label"]))
	if used.is_empty():
		return WorkbenchVocab.EQUIPMENT_CONSUMES_NOTHING
	return WorkbenchVocab.EQUIPMENT_CONSUMES_FORMAT \
		% WorkbenchVocab.EQUIPMENT_JOBS_SEPARATOR.join(used)


## The live half: one block per player cohort, in wire order.
func _render_bands() -> void:
	if _bands_body == null:
		return
	HudWidgets.clear_children(_bands_body)
	if _cohorts.is_empty():
		_bands_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.EQUIPMENT_NO_BANDS))
		return
	for cohort in _cohorts:
		_bands_body.add_child(_band_block(cohort))


## One cohort: its head row, its three component conditions, THE TWO TIER LINES, and what each crew's
## yields are priced at.
##
## The tier lines carry a meta handle apiece so the preview harness can reach them by identity — both
## are live numbers plus a kit's display name, so a text search finds either or neither.
func _band_block(cohort: Dictionary) -> Control:
	var entity := int(cohort.get(COHORT_ENTITY_KEY, 0))
	var block := _line_column()
	block.add_child(WorkbenchWidgets.build_row_label(_band_head(cohort, entity)))
	block.add_child(WorkbenchWidgets.build_caption(_condition_line(cohort)))

	var hunt := WorkbenchWidgets.build_caption(_hunt_tier_line(cohort))
	hunt.set_meta(WorkbenchVocab.EQUIPMENT_HUNT_TIER_META, entity)
	block.add_child(hunt)

	var forage := WorkbenchWidgets.build_caption(_forage_tier_line(cohort))
	forage.set_meta(WorkbenchVocab.EQUIPMENT_FORAGE_TIER_META, entity)
	block.add_child(forage)

	block.add_child(WorkbenchWidgets.build_caption(_crews_line(cohort)))
	return block


static func _band_head(cohort: Dictionary, entity: int) -> String:
	var head: String = WorkbenchVocab.EQUIPMENT_PARTY_HEAD_FORMAT if _is_party(cohort) \
		else WorkbenchVocab.EQUIPMENT_BAND_HEAD_FORMAT
	return _join([head % entity,
		WorkbenchVocab.EQUIPMENT_BAND_SIZE_FORMAT % int(cohort.get(COHORT_SIZE_KEY, 0))])


## Remaining condition in each component, on the 0-100 scale. **`0` is a real reading and means DRY**,
## not "unstated": `native/src/dict/population.rs` writes all three durability keys on every cohort, so
## every component always has a number to state.
static func _condition_line(cohort: Dictionary) -> String:
	var parts: Array[String] = []
	for component in COMPONENTS:
		var label := String(component["label"])
		var condition := KitRoster.condition_of(cohort, String(component["condition"]))
		parts.append(WorkbenchVocab.EQUIPMENT_CONDITION_DRY_FORMAT % label \
			if condition <= KitRoster.CONDITION_DRY \
			else WorkbenchVocab.EQUIPMENT_CONDITION_FORMAT % [label, int(condition)])
	return _join(parts)


## The band's RESOLVED hunt tiers, quoted at the kit `PopulationCohortState.kitId` names — which is
## what that field answers for, and only what it answers for.
func _hunt_tier_line(cohort: Dictionary) -> String:
	var line := WorkbenchVocab.EQUIPMENT_HUNT_TIER_FORMAT % [
		_tier_face(float(cohort.get(COHORT_ATTACK_KEY, 0.0))),
		_tier_face(float(cohort.get(COHORT_HUNT_CARRY_KEY, 0.0))),
	]
	var source: String = WorkbenchVocab.EQUIPMENT_QUOTED_PARTY_KIT if _is_party(cohort) \
		else WorkbenchVocab.EQUIPMENT_QUOTED_HUNT_DEFAULT
	return line + _quoted_at(String(cohort.get(COHORT_KIT_KEY, ""))) + source


## …and the FORAGE tier, quoted at the kit `_forage_kit_id` resolves — never at `kitId`.
func _forage_tier_line(cohort: Dictionary) -> String:
	var line := WorkbenchVocab.EQUIPMENT_FORAGE_TIER_FORMAT % _tier_face(
		float(cohort.get(COHORT_FORAGE_CARRY_KEY, 0.0)))
	var source: String = WorkbenchVocab.EQUIPMENT_QUOTED_PARTY_KIT if _is_party(cohort) \
		else WorkbenchVocab.EQUIPMENT_QUOTED_FORAGE_DEFAULT
	return line + _quoted_at(_forage_kit_id(cohort)) + source


## **WHICH KIT THIS COHORT'S `forageCarryPerWorkerBiomass` IS QUOTED AT — the one line this page
## exists to get right.**
##
## An IN-FLIGHT PARTY carries one kit, decided at launch and held for its whole life, so that kit
## covers its forage tier too and `kitId` is the honest answer. A RESIDENT BAND has one kit per
## ASSIGNMENT and this row is per cohort, so its `kitId` is the HUNT job's default and says nothing
## about gathering; the forage tier resolves through the world's forage default, which rides the wire
## once as `defaultForageKitId`. Pairing the band's forage tier with `kitId` reads a gathering rate
## off `big_game`, which has no basket component at all.
func _forage_kit_id(cohort: Dictionary) -> String:
	return String(cohort.get(COHORT_KIT_KEY, "")) if _is_party(cohort) else _default_forage_kit_id


## `"  ·  at <display name>"`, resolved through the roster; `""` when the cohort names no kit, so a
## line with nothing to attribute ends after its numbers rather than trailing a dangling clause.
func _quoted_at(kit_id: String) -> String:
	if kit_id == KitRoster.NO_KIT_ID:
		return ""
	return WorkbenchVocab.EQUIPMENT_QUOTED_AT_FORMAT % KitRoster.display_name_for_id(_kits, kit_id)


## What each crew's yields are priced at, already resolved by the sim. A band-wide role (scout,
## warrior) carries `""` — it consumes no kit component, so it has no kit AXIS, which is a different
## statement from having no kit and is worded as one.
func _crews_line(cohort: Dictionary) -> String:
	var assignments: Variant = cohort.get(COHORT_ASSIGNMENTS_KEY, [])
	if not (assignments is Array) or (assignments as Array).is_empty():
		return WorkbenchVocab.EQUIPMENT_NO_CREWS
	var parts: Array[String] = []
	for entry in assignments:
		if not (entry is Dictionary):
			continue
		var assignment: Dictionary = entry
		var kit_id := String(assignment.get(ASSIGNMENT_KIT_KEY, ""))
		var face := WorkbenchVocab.EQUIPMENT_CREW_NO_KIT if kit_id == KitRoster.NO_KIT_ID \
			else KitRoster.display_name_for_id(_kits, kit_id)
		parts.append(WorkbenchVocab.EQUIPMENT_CREW_FORMAT
			% [String(assignment.get(ASSIGNMENT_KIND_KEY, "")), face])
	if parts.is_empty():
		return WorkbenchVocab.EQUIPMENT_NO_CREWS
	return WorkbenchVocab.EQUIPMENT_CREWS_PREFIX + _join(parts)


# ---- leaves ----------------------------------------------------------------

static func _is_party(cohort: Dictionary) -> bool:
	return bool(cohort.get(COHORT_IS_EXPEDITION_KEY, false))


## The column one block's lines stack in — tighter than the gap BETWEEN blocks, so a block reads as
## one thing.
static func _line_column() -> VBoxContainer:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", WorkbenchWidgets.ROW_LINE_GAP)
	return column


static func _join(parts: Array[String]) -> String:
	return WorkbenchVocab.EQUIPMENT_PART_SEPARATOR.join(parts)


## A tier, at the roster's own precision. `INF` is what `KitRoster.unequipped_tier` answers for an
## axis no kit states, and it is rendered rather than substituted: a made-up bare-handed number is
## exactly the class of invention this page is written to avoid.
static func _tier_face(value: float) -> String:
	if is_inf(value):
		return WorkbenchVocab.EQUIPMENT_TIER_UNSTATED
	return String.num(value, WorkbenchVocab.EQUIPMENT_TIER_DECIMALS)
