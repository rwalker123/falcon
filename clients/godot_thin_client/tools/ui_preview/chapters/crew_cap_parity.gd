extends RefCounted

## ONE CREW CEILING, TWO `+`s — the compose sheet's crew stepper and the Work tab row's stepper, on all
## three webs.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. It is appended LAST and renders NOTHING (no `_save`), so the frame set is
## untouched.
##
## Reported from play: Firbrook's `Wood · 1 tile NE` (deadfall, one forester) — the Groundwork row's
## `+` greyed, while the `Assign foresters` sheet on the same tile let the crew go to two. The two
## surfaces read two different ceilings and two different idle counts. Both read one of each now:
##
##   * the ceiling is the sim's per-row "most hands that still help" through the ONE producer each web
##     has (`SourceForecast.worked_row_ceiling` for forage and hunt,
##     `HudDepositVocab.published_useful_cutters` for a working), read by the sheet whenever it
##     composes the row's own floor and kit (`DrawerComposeController._composed_standing_row`);
##   * the idle count is `effective_idle` on both — the sheet's crew pool
##     (`HudBandLaborState.source_crew_pool_*`) is that plus the effective crew on the source.
##
## **ONE FIXTURE SOURCE FEEDS BOTH SURFACES**: one band carrying a forage row, a hunt row and an
## extract row, and the sheet opened on each of those three sources. Every claim compares the two
## `+`s DIRECTLY, and each case also states which way they must point — a pair that agreed by both
## being dead everywhere would pass a parity-only claim.
##
## It hands the reference band back at the end, so a chapter appended after it starts where every
## other one does.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 34

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_assert_hud`.
var h

## This chapter's own band, so nothing it pushes can be confused with the reference band's rows.
const PARITY_ENTITY := 961

## The hands the band holds back on a settled frame. Two, so the "below the cap" case has a free hand
## for each `+` to offer and the "no free hand" case has something for the pending edit to spend.
const PARITY_IDLE := 2

## The sim's own per-row ceilings, published on the rows. Three each, so a crew one below the cap is
## still a crew (`PARITY_MIN_CEILING` is the premise that keeps that true).
const FORAGE_WORKERS_NEEDED := 3
const HUNT_USEFUL_WORKERS := 3
const WOOD_USEFUL_CUTTERS := 3
const PARITY_MIN_CEILING := 2

## The quarry: the shared assign-preview herd, made a fought one (an engagement rate on it), which is
## what lets the row's published `hunt_useful_workers` stand (`SourceForecast.with_published_useful_crew`).
const PARITY_HERD_ID := "game_parity_deer"
const PARITY_HERD_SPECIES := "Red Deer"
const PARITY_HERD_SUSTAIN_CEILING := 0.6
const PARITY_HERD_ENGAGE_RATE := 0.5

## The working, one hex east of the band and the patch, so all three sources sit inside the apron.
const WOOD_TILE := Vector2i(67, 10)
const WOOD_MATERIAL := "wood"
const WOOD_CAPACITY := 600.0
const WOOD_STOCK := 420.0
const WOOD_REGROWTH := 0.03
const WOOD_TAKE := 0.5
const WOOD_PER_WORKER := 2.0

## The three cases, each a (crew offset from the ceiling, free hands spent by a pending edit) pair.
## AT the cap with a hand to spare is where the CEILING disables both; BELOW it is where both must
## offer the hand; BELOW it with the last free hand spent by a pending edit is where the IDLE RULE
## disables both — the sheet used to count the wire's idle there and offer it anyway.
const CREW_AT_CAP := 0
const CREW_BELOW_CAP := -1
const CREW_ABOVE_CAP := 1
const NO_PENDING_SPEND := 0

## The row's `workers_needed` in the overstaff block — the sim's inversion of the take this crew RAN,
## held BELOW every crew the block stages. On a hunt row and a working that field is never the `+`'s
## ceiling, so a reader that still took it would call a crew the `+` offers a hand to overstaffed.
const INVERTED_TAKE_CREW := 1

## What the overstaff note says, whatever its figures — the needle for its absence.
const OVERSTAFF_NOTE_NEEDLE := "bring anything home"

## The crew nouns the drawer's standing summary is asked with (any noun does — the claim is the note).
const HUNT_SUMMARY_NOUN := "hunters"
const WOOD_SUMMARY_NOUN := "foresters"


func run(harness) -> void:
	h = harness
	var herd := _parity_herd()
	h._set_world_herds([herd])
	h._hud.update_deposits([_parity_wood()])
	await _assert_case("at the cap", CREW_AT_CAP, NO_PENDING_SPEND, false)
	await _assert_case("below the cap", CREW_BELOW_CAP, NO_PENDING_SPEND, true)
	await _assert_case("below the cap, the last free hand spent by a pending edit", CREW_BELOW_CAP,
		PARITY_IDLE, false)
	await _assert_kit_seeded_from_row(herd)
	await _assert_overstaffed_reads_the_plus_ceiling(herd)
	# Back to the state every other chapter runs in.
	h._hud.close_compose_sheet()
	h._hud.update_deposits([])
	h._hud.update_band_alerts([BandFx.band_fixture()])
	await h._settle()

## One case: push the band with every crew at `ceiling + crew_offset`, optionally spend `pending_spend`
## free hands on a pending edit to another role, then read both `+`s on all three webs.
func _assert_case(label: String, crew_offset: int, pending_spend: int, want_enabled: bool) -> void:
	var herd: Dictionary = h._hud._band_labor.find_world_herd(PARITY_HERD_ID)
	var ceilings := _ceilings(herd)
	h._assert_hud("parity (%s) — premise: every web's ceiling leaves a crew below it (%s)"
			% [label, str(ceilings)],
		int(ceilings["forage"]) >= PARITY_MIN_CEILING and int(ceilings["hunt"]) >= PARITY_MIN_CEILING
			and int(ceilings["extract"]) >= PARITY_MIN_CEILING)
	var band := _parity_band(int(ceilings["forage"]) + crew_offset,
		int(ceilings["hunt"]) + crew_offset, int(ceilings["extract"]) + crew_offset)
	h._hud.update_band_alerts([band])
	if pending_spend > 0:
		h._hud._band_labor.record_pending_assign(PARITY_ENTITY, HudConst.LABOR_KIND_SCOUT,
			pending_spend, -1, -1, "", SourceForecast.DEFAULT_HARVEST_FLOOR)
	await h._settle()
	var live: Dictionary = h._hud._band_labor.player_band_by_entity(PARITY_ENTITY)
	var idle := int(h._hud._band_labor.effective_idle(live))
	h._assert_hud("parity (%s) — premise: %d free hand(s) after the pending edit, %d on the wire"
			% [label, idle, int(live.get("idle_workers", 0))],
		idle == PARITY_IDLE - pending_spend and int(live.get("idle_workers", 0)) == PARITY_IDLE)
	for web in ["forage", "hunt", "extract"]:
		var row_plus := _row_plus_enabled(web, live)
		var sheet_plus := await _sheet_plus_enabled(web, herd)
		h._assert_hud(("parity (%s) — %s: the sheet's `+` and the Work row's `+` agree (row %s, sheet "
				+ "%s, both want %s)") % [label, web, _face(row_plus), _face(sheet_plus),
					_face(want_enabled)],
			row_plus == sheet_plus and row_plus == want_enabled)
	if pending_spend > 0:
		h._hud.drop_pending_assign({"pending_entity": PARITY_ENTITY,
			"kind": HudConst.LABOR_KIND_SCOUT, "x": -1, "y": -1, "herd_id": ""})

## The three ceilings, asked of the producers both surfaces read. The extract one is the published
## figure verbatim — a working has no closed form to fall back on.
func _ceilings(herd: Dictionary) -> Dictionary:
	var probe := _parity_band(0, 0, 0)
	var rows: Array = probe["labor_assignments"]
	return {
		"forage": SourceForecast.worked_row_ceiling(SourceForecast.LABOR_KIND_FORAGE, rows[0],
			h._hud._band_labor.forage_patch_lookup().get(_forage_tile_xy(), {})),
		"hunt": SourceForecast.worked_row_ceiling(SourceForecast.LABOR_KIND_HUNT, rows[1], herd),
		"extract": HudDepositVocab.published_useful_cutters(rows[2]),
	}

## The Work tab row's `+`. The food rows' `+` IS `can_add` off `_work_source_models` (the rendered
## stepper takes it verbatim); a working's row is built off the Groundwork section's own models and its
## `+` read off the node.
func _row_plus_enabled(web: String, band: Dictionary) -> bool:
	if web == "extract":
		for model_variant in h._hud._bandpanel._extract_source_models(band):
			var model: Dictionary = model_variant
			if model["tile"] != WOOD_TILE:
				continue
			var row: Control = h._hud._bandpanel._build_extract_row(band, model)
			var plus := Q.find_button_by_text(row, HudWorkVocab.STEPPER_PLUS_FACE)
			var enabled := plus != null and not plus.disabled
			row.free()
			return enabled
		h._fail("parity — the band's working is not on the Groundwork roster")
		return false
	var idle := int(h._hud._band_labor.effective_idle(band))
	for model_variant in h._hud._bandpanel._work_source_models(band, idle):
		var model: Dictionary = model_variant
		if String(model.get("kind", "")) == web:
			return bool(model.get("can_add", false))
	h._fail("parity — the band's %s row is not on the work board" % web)
	return false

## The compose sheet's crew `+`, on a sheet opened fresh on that web's source (the source key reset
## first, so the crew re-seeds off the band's row as it does when a player opens the sheet).
func _sheet_plus_enabled(web: String, herd: Dictionary) -> bool:
	match web:
		"forage":
			h._hud._compose.reset_forage_source()
			h._hud._compose.set_forage_kit_id(KitRoster.NO_KIT_ID)
			h._compose_forage(_forage_tile())
		"hunt":
			h._hud._compose.reset_hunt_source()
			h._hud._compose.reset_hunt_kit()
			h._compose_herd(herd)
		_:
			h._hud._compose.reset_deposit_source()
			h._hud._drawercompose.open_deposit_compose(_parity_wood())
	await h._settle()
	var sheet: Node = h._hud._drawercompose._compose_sheet
	var label := Q.find_meta_node(sheet, HudWidgets.CREW_ROW_LABEL_META)
	if label == null or label.get_parent() == null or label.get_parent().get_parent() == null:
		h._fail("parity — the %s sheet drew no crew row" % web)
		return false
	var plus := Q.find_button_by_text(label.get_parent().get_parent(), HudWorkVocab.STEPPER_PLUS_FACE)
	if plus == null:
		h._fail("parity — the %s sheet's crew row drew no `+`" % web)
		return false
	return not plus.disabled

## ⛔ **THE SHEET OPENS ON THE KIT THE BAND'S OWN ROW CARRIES**, so composing the band's own row is
## the default and the two `+`s agree out of the box. The rows here name a kit that is NOT the
## source's default: a sheet seeded with the default would compose another kit, fall back to its own
## curve, and could disagree with the row. Asked AT the cap, where that fallback shows — and the kit is
## reset before each open, so the seed has to win over a reset rather than over nothing.
##
## Forage and hunt only: this harness's roster offers a working no kit but its own default.
func _assert_kit_seeded_from_row(herd: Dictionary) -> void:
	var ceilings := _ceilings(herd)
	var kits := {
		"forage": _other_kit(KitRoster.JOB_FORAGE, {}),
		"hunt": _other_kit(KitRoster.JOB_HUNT, herd),
	}
	h._assert_hud("kit seed — premise: each food web offers a kit besides its default (%s)" % str(kits),
		String(kits["forage"]) != "" and String(kits["hunt"]) != "")
	var band := _parity_band(int(ceilings["forage"]), int(ceilings["hunt"]),
		int(ceilings["extract"]), {"forage": {"kit_id": kits["forage"]}, "hunt": {"kit_id": kits["hunt"]}})
	h._hud.update_band_alerts([band])
	await h._settle()
	var live: Dictionary = h._hud._band_labor.player_band_by_entity(PARITY_ENTITY)
	for web in ["forage", "hunt"]:
		var row_plus := _row_plus_enabled(web, live)
		var sheet_plus := await _sheet_plus_enabled(web, herd)
		var composed: String = h._hud._compose.forage_kit_id() if web == "forage" \
			else h._hud._compose.hunt_kit_id()
		h._assert_hud("kit seed — %s: the sheet opens on the row's kit, not the default (got %s, want %s)"
				% [web, composed, String(kits[web])],
			composed == String(kits[web]))
		h._assert_hud("kit seed — %s: at the cap on that kit the two `+`s agree (row %s, sheet %s)"
				% [web, _face(row_plus), _face(sheet_plus)],
			row_plus == sheet_plus and not row_plus)

## ⛔ **ONE NUMBER PER WEB: THE OVERSTAFF WARNING IS STRUCK AT THE `+`'s CEILING.** The rows publish a
## `workers_needed` (the take inverted) BELOW the crew. One crew below the cap the `+` is live and NO
## surface may call the crew overstaffed — the board row's note, its hover clause, the map list and the
## drawer's standing summary; one crew above it, every one of them does and the `+` is dead. Hunt and
## a working; forage's ceiling IS its `workers_needed`, so it cannot part from itself.
func _assert_overstaffed_reads_the_plus_ceiling(herd: Dictionary) -> void:
	var ceilings := _ceilings(herd)
	var hunt_cap := int(ceilings["hunt"])
	var wood_cap := int(ceilings["extract"])
	for offset in [CREW_BELOW_CAP, CREW_ABOVE_CAP]:
		var over: bool = offset == CREW_ABOVE_CAP
		var label := "above the cap" if over else "below the cap"
		var band := _parity_band(int(ceilings["forage"]), hunt_cap + offset, wood_cap + offset, {
			"hunt": {"workers_needed": INVERTED_TAKE_CREW},
			"extract": {"workers_needed": INVERTED_TAKE_CREW}})
		h._hud.update_band_alerts([band])
		await h._settle()
		var live: Dictionary = h._hud._band_labor.player_band_by_entity(PARITY_ENTITY)
		var rows: Array = band["labor_assignments"]
		var hunt_model := _work_model(live, SourceForecast.LABOR_KIND_HUNT)
		var board_note := String(hunt_model.get("note", ""))
		var board_tip := String(hunt_model.get("tooltip", ""))
		var map_clause := BandOverlayRenderer.food_overstaffed_text(rows[1], herd,
			SourceForecast.SOURCE_KIND_HERD)
		var hunt_summary := String(h._hud._drawercompose._standing_summary_model(rows[1],
			SourceForecast.LABOR_KIND_HUNT, HUNT_SUMMARY_NOUN).get("note", ""))
		var wood_summary := String(h._hud._drawercompose._standing_summary_model(rows[2],
			HudConst.LABOR_KIND_EXTRACT, WOOD_SUMMARY_NOUN, _parity_wood()).get("note", ""))
		var hunt_note := SourceForecast.OVERSTAFF_NOTE_FORMAT % [hunt_cap, hunt_cap + offset]
		var wood_note := SourceForecast.OVERSTAFF_NOTE_FORMAT % [wood_cap, wood_cap + offset]
		h._assert_hud("overstaff (%s) — hunt: the board row's `+` is %s" % [label,
				"dead" if over else "live"],
			bool(hunt_model.get("can_add", false)) == not over)
		h._assert_hud("overstaff (%s) — hunt: the board note quotes the `+`'s ceiling (got \"%s\")"
				% [label, board_note],
			board_note == hunt_note if over else not board_note.contains(OVERSTAFF_NOTE_NEEDLE))
		h._assert_hud("overstaff (%s) — hunt: the board row never wears the clause beside the note" % label,
			not board_tip.contains(HudDepositVocab.OVERSTAFFED_WORD))
		h._assert_hud("overstaff (%s) — hunt: the map list agrees (got \"%s\")" % [label, map_clause],
			(map_clause != "") == over)
		h._assert_hud("overstaff (%s) — hunt: the drawer summary agrees (got \"%s\")"
				% [label, hunt_summary],
			hunt_summary == hunt_note if over else not hunt_summary.contains(OVERSTAFF_NOTE_NEEDLE))
		h._assert_hud("overstaff (%s) — working: the drawer summary agrees (got \"%s\")"
				% [label, wood_summary],
			wood_summary == wood_note if over else not wood_summary.contains(OVERSTAFF_NOTE_NEEDLE))
		h._assert_hud("overstaff (%s) — working: the Groundwork `+` is %s" % [label,
				"dead" if over else "live"],
			_row_plus_enabled("extract", live) == not over)

## The work board's model for one food web on this band.
func _work_model(band: Dictionary, kind: String) -> Dictionary:
	var idle := int(h._hud._band_labor.effective_idle(band))
	for model_variant in h._hud._bandpanel._work_source_models(band, idle):
		var model: Dictionary = model_variant
		if String(model.get("kind", "")) == kind:
			return model
	h._fail("parity — the band's %s row is not on the work board" % kind)
	return {}

## A kit the sheet would let a player pick on this source that is NOT the one it resolves with
## nothing picked, or `""` where the job offers none.
func _other_kit(job: String, source: Dictionary) -> String:
	var default_kit := _resolved_kit(job, source)
	for kit_variant in KitRoster.kits_for_job(h._hud._band_labor.kits(), job):
		var kit_id := String((kit_variant as Dictionary).get(KitRoster.KIT_ID_KEY, ""))
		if kit_id != default_kit and _resolve(job, source, kit_id) == kit_id:
			return kit_id
	return ""

func _face(enabled: bool) -> String:
	return "live" if enabled else "dead"

## The patch: the shared food tile, which is where the band camps.
func _forage_tile() -> Dictionary:
	return BaseFx.food_tile_fixture()

func _forage_tile_xy() -> Vector2i:
	var tile := _forage_tile()
	return Vector2i(int(tile["x"]), int(tile["y"]))

func _parity_herd() -> Dictionary:
	var herd := HerdFx.assign_preview_herd(PARITY_HERD_ID, PARITY_HERD_SPECIES, "thriving",
		PARITY_HERD_SUSTAIN_CEILING, 0, 0)
	herd[SourceForecast.FORECAST_ENGAGE_RATE_KEY] = PARITY_HERD_ENGAGE_RATE
	return herd

## A renewing deadfall wood, the reported ground: no rung built, nothing owed, no curve on the wire
## (the sheet's own curve is not what this chapter is about — the row's figure is).
func _parity_wood() -> Dictionary:
	return {
		"tile_x": WOOD_TILE.x, "tile_y": WOOD_TILE.y,
		"material": WOOD_MATERIAL,
		"branch": HudDepositVocab.BRANCH_FORESTRY,
		"stock": WOOD_STOCK, "capacity": WOOD_CAPACITY, "reachable": WOOD_STOCK,
		"regrowth_rate": WOOD_REGROWTH,
		"rung": HudDepositVocab.RUNG_KEY_DEADFALL,
		"build_fraction": HudDepositVocab.METER_UNSTARTED,
		"ladder_position": HudDepositVocab.LADDER_UNSTARTED,
		"sustainable_take": WOOD_TAKE, "actual_take": WOOD_TAKE,
		"turns_remaining": HudDepositVocab.RUNWAY_NOT_APPLICABLE,
		"upkeep_demand": 0.0, "upkeep_supplied": 0.0, "upkeep_shortfall": 0.0,
		"upkeep_workers_needed": 0,
		"has_neglect_grace": false, "neglect_grace_remaining": 0,
		"build_turns_remaining": SourceForecast.BUILD_TURNS_NO_ESTIMATE,
		"build_blocked_reason": "", "is_queued": false,
		"build_kit_id": "", "upkeep_kit_id": "", "upkeep_kit_named": false,
		"offered_kit_ids": [], "default_kit_id": "",
		"rung_floor_fraction": 0.0, "per_worker_biomass": WOOD_PER_WORKER,
		"regrowth_samples": PackedFloat32Array(),
	}

## The band: camped on the patch, one row per web at the crews given, the sim's ceiling on each row,
## and `working_age` exactly the crews plus `PARITY_IDLE`, so `effective_idle` and the wire's
## `idle_workers` agree on a settled frame. Each row names the kit the sheet resolves for that source
## with nothing picked — the row's own composition, which is what the sheet opens on.
##
## `extras` merges per-web keys onto that web's row (`{"hunt": {"kit_id": …}}`).
func _parity_band(forage_crew: int, hunt_crew: int, wood_crew: int,
		extras: Dictionary = {}) -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = PARITY_ENTITY
	band.erase("band_id")
	band.erase("name")
	BandFx.with_band_id(band)
	var tile := _forage_tile_xy()
	band["pos"] = [tile.x, tile.y]
	band["current_x"] = tile.x
	band["current_y"] = tile.y
	band["working_age"] = forage_crew + hunt_crew + wood_crew + PARITY_IDLE
	band["idle_workers"] = PARITY_IDLE
	var floor := SourceForecast.DEFAULT_HARVEST_FLOOR
	band["labor_assignments"] = [
		{"kind": SourceForecast.LABOR_KIND_FORAGE, "workers": forage_crew, "floor": floor,
			"target_x": tile.x, "target_y": tile.y,
			"kit_id": _resolved_kit(KitRoster.JOB_FORAGE, {}),
			"workers_needed": FORAGE_WORKERS_NEEDED},
		{"kind": SourceForecast.LABOR_KIND_HUNT, "workers": hunt_crew, "floor": floor,
			"fauna_id": PARITY_HERD_ID, "target_x": tile.x, "target_y": tile.y,
			"kit_id": _resolved_kit(KitRoster.JOB_HUNT, _parity_herd()),
			SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY: HUNT_USEFUL_WORKERS},
		{"kind": HudConst.LABOR_KIND_EXTRACT, "workers": wood_crew, "floor": floor,
			"target_x": WOOD_TILE.x, "target_y": WOOD_TILE.y, "fauna_id": "",
			"material": WOOD_MATERIAL,
			"kit_id": _resolved_kit(KitRoster.JOB_EXTRACT, _parity_wood()),
			HudDepositVocab.ASSIGNMENT_USEFUL_CUTTERS_KEY: WOOD_USEFUL_CUTTERS},
	]
	var webs := ["forage", "hunt", "extract"]
	for i in webs.size():
		(band["labor_assignments"][i] as Dictionary).merge(extras.get(webs[i], {}), true)
	return band

## The kit a sheet resolves for a source with nothing picked — the sheets' own call, so the row the
## fixture states is the row the sheet composes.
func _resolved_kit(job: String, source: Dictionary) -> String:
	return _resolve(job, source, KitRoster.NO_KIT_ID)

## What the sheet's own resolution makes of `picked` on this source.
func _resolve(job: String, source: Dictionary, picked: String) -> String:
	var kits: Array = h._hud._band_labor.kits()
	if job == KitRoster.JOB_EXTRACT:
		kits = KitRoster.extract_kits_for_working(kits, source)
	if job == KitRoster.JOB_HUNT:
		return KitRoster.resolve_selection(kits, job, h._hud._band_labor.default_kit_id(job),
			picked, source, HudComposeVocab.BARE_FORECAST_PREFIX)
	if job == KitRoster.JOB_EXTRACT:
		return KitRoster.resolve_selection(kits, job, h._hud._band_labor.default_kit_id(job),
			picked, source)
	return KitRoster.resolve_selection(kits, job, h._hud._band_labor.default_kit_id(job),
		picked)
