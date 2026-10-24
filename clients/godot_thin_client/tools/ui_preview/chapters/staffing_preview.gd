extends RefCounted

## THE STAFFING PREVIEW ON THE COMPOSE SHEETS -- a source the band does NOT yet work is priced off the
## sim's dry run of the real staffing, not off the band's idle pool.
##
## One chapter of the `ui_preview` state walk, appended LAST so no earlier frame moves. The claims:
##   * a band with NO idle hands, on a herd it does not hunt, is handed a crew by the preview (hands taken
##     from lower rows), the strip prices THAT crew, and the take curve is asked wide enough to cover it;
##   * the line under the strip names every source of those hands, a build's builders included;
##   * a preview of nobody is the amber line, and the curve reads an answered EMPTY curve -- never the
##     "Costing..." line that sat there forever when the pool was 0;
##   * a preview still in flight falls back to the idle estimate (here 0), again with no pending line.

## The checkpoints this chapter owes the walk -- assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 20

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const NodeQuery := preload("res://tools/ui_preview/node_query.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The crew the canned preview hands the new row, and where it comes from: the forage row's hands (its
## crew falls `FORAGE_BEFORE` to `FORAGE_AFTER`) and one builder of that patch's build.
const PREVIEW_CREW := 3
const FORAGE_BEFORE := 5
const FORAGE_AFTER := 3
const BUILDERS_BEFORE := 2
const BUILDERS_AFTER := 1
const FORAGE_X := 71
const FORAGE_Y := 18

## A band's working-age people, all of them spoken for: forage 5, scouts 2, warriors 2.
const BAND_WORKING_AGE := 9

const PENDING_LINE_FRAGMENT := "Costing what this crew"

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

func run(harness) -> void:
	h = harness
	ForecastFx.install(h._hud)
	h._hud.forecast_query().reset()
	h._hud._band_labor._player_band = _full_band()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	await _hunt_sheet_takes_the_previewed_crew()
	await _hunt_sheet_nobody_would_come()
	await _hunt_sheet_pending_falls_back()
	await _forage_sheet_takes_the_previewed_crew()
	ForecastFx.staffing_preview_canned = {}
	h._hud.forecast_query().reset()
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_forage_source()
	h._hud._compose.reset_hunt_source()
	await h._settle()

## The shared band with every hand spoken for and no row on the Red Deer: idle 0, and nothing to be
## "assignable" from -- the state the idle-based estimate priced at nobody.
func _full_band() -> Dictionary:
	var band := BandFx.band_fixture()
	var kept: Array = []
	for row in band["labor_assignments"]:
		if String((row as Dictionary).get("kind", "")) != "hunt":
			kept.append(row)
	band["labor_assignments"] = kept
	band["working_age"] = BAND_WORKING_AGE
	band["idle_workers"] = 0
	return band

func _preview(workers: int, losses: Array, idle_before: int, idle_after: int) -> Dictionary:
	return {"workers": workers, "keepers": 0, "kitted": 0, "bare": workers,
		"idle_before": idle_before, "idle_after": idle_after, "losses": losses}

func _forage_loss() -> Dictionary:
	return {"kind": "forage", "x": FORAGE_X, "y": FORAGE_Y, "herd_id": "", "material": "",
		"branch": "", "before": FORAGE_BEFORE, "after": FORAGE_AFTER}

func _build_loss() -> Dictionary:
	return {"kind": "build:forage", "x": FORAGE_X, "y": FORAGE_Y, "herd_id": "", "material": "",
		"branch": "", "before": BUILDERS_BEFORE, "after": BUILDERS_AFTER}

func _open_hunt() -> Node:
	var herd := HerdFx.herd_fixture()
	h._hud.forecast_query().reset()
	ForecastFx.staffing_preview_asks = []
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	h._compose_herd(herd)
	await h._settle()
	await h._settle()
	return h._hud._drawercompose._compose_sheet

func _crew_stamp(sheet: Node) -> int:
	var strip := ForageFx.find_strip(sheet)
	if strip == null:
		return -1
	return int(strip.get_meta(HudComposeVocab.SHEET_CREW_NUMBER_META, -1))

func _hunt_sheet_takes_the_previewed_crew() -> void:
	ForecastFx.staffing_preview_canned = _preview(PREVIEW_CREW, [_forage_loss(), _build_loss()], 0, 0)
	var sheet := await _open_hunt()
	await h._save("staffing_preview_hunt")
	h._assert_hud("a band with 0 idle hands is priced at the PREVIEW's crew (strip stamps %d, want %d)"
			% [_crew_stamp(sheet), PREVIEW_CREW], _crew_stamp(sheet) == PREVIEW_CREW)
	var line := NodeQuery.label_containing(sheet, "If you hunt here")
	h._assert_hud("…and the line under it names the crew (got \"%s\")" % line,
		line.contains("3 hunters"))
	h._assert_hud("…the forage row it takes from, named as the Work tab names it",
		line.contains("%d from Harvest (%d, %d)" % [FORAGE_BEFORE - FORAGE_AFTER, FORAGE_X, FORAGE_Y]))
	h._assert_hud("…and the build's builders, as `the <site> build`",
		line.contains("1 from the Harvest (%d, %d) build" % [FORAGE_X, FORAGE_Y]))
	h._assert_hud("…and no `Costing...` line is left on the sheet",
		not NodeQuery.has_label_containing(sheet, PENDING_LINE_FRAGMENT))
	var asked_wide := false
	for ask in ForecastFx.staffing_preview_asks:
		asked_wide = asked_wide or int((ask as Dictionary).get("max_workers", -1)) >= 0
	h._assert_hud("…the preview was ASKED at the sheet's Priority (%d ask(s))"
			% ForecastFx.staffing_preview_asks.size(),
		not ForecastFx.staffing_preview_asks.is_empty() and asked_wide
			and String((ForecastFx.staffing_preview_asks[0] as Dictionary).get("priority", "")) \
				== HudWorkVocab.WORK_PRIORITY_NORMAL)
	var hover := _tooltip_containing(sheet, "→")
	h._assert_hud("…and its hover lists each row before → after (got \"%s\")" % hover,
		hover.contains("%d → %d" % [FORAGE_BEFORE, FORAGE_AFTER]) and hover.contains("Idle: 0 → 0"))

func _hunt_sheet_nobody_would_come() -> void:
	ForecastFx.staffing_preview_canned = _preview(0, [], 0, 0)
	var sheet := await _open_hunt()
	await h._save("staffing_preview_hunt_nobody")
	var line := NodeQuery.label_containing(sheet, "No one would come")
	h._assert_hud("a preview of nobody is the amber line at Normal (got \"%s\")" % line,
		line == HudComposeVocab.STAFFING_PREVIEW_NOBODY_AT_FORMAT % "Normal")
	h._assert_hud("…and the empty pool reads an answered EMPTY curve, not the `Costing...` line",
		not NodeQuery.has_label_containing(sheet, PENDING_LINE_FRAGMENT))
	h._assert_hud("…the strip prices nobody", _crew_stamp(sheet) == 0)
	h._hud._compose.set_hunt_priority(HudWorkVocab.WORK_PRIORITY_HIGH)
	h._compose_herd(HerdFx.herd_fixture())
	await h._settle()
	await h._settle()
	h._assert_hud("…and at High the words say every worker is on a High row",
		NodeQuery.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudComposeVocab.STAFFING_PREVIEW_NOBODY_AT_HIGH))
	h._hud._compose.set_hunt_priority(HudWorkVocab.WORK_PRIORITY_NORMAL)

func _hunt_sheet_pending_falls_back() -> void:
	# An EMPTY canned reply is never delivered: the preview stays in flight for the whole frame.
	ForecastFx.staffing_preview_canned = {}
	var sheet := await _open_hunt()
	await h._save("staffing_preview_hunt_pending")
	h._assert_hud("a preview still in flight falls back to the idle estimate (strip stamps %d, want 0)"
			% _crew_stamp(sheet), _crew_stamp(sheet) == 0)
	h._assert_hud("…states no preview line", not NodeQuery.has_label_containing(sheet, "If you hunt here")
		and not NodeQuery.has_label_containing(sheet, "No one would come"))
	h._assert_hud("…and an empty pool shows no `Costing...` line either",
		not NodeQuery.has_label_containing(sheet, PENDING_LINE_FRAGMENT))

func _forage_sheet_takes_the_previewed_crew() -> void:
	ForecastFx.staffing_preview_canned = _preview(PREVIEW_CREW, [_forage_loss()], 1, 0)
	var tile := TileFx.stressed_tile_fixture()
	h._hud.forecast_query().reset()
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._settle()
	var sheet: Node = h._hud._drawercompose._compose_sheet
	await h._save("staffing_preview_forage")
	var line := NodeQuery.label_containing(sheet, "If you gather here")
	h._assert_hud("the forage sheet states its preview line (got \"%s\")" % line,
		line.contains("3 harvesters") and line.contains("2 from Harvest") and line.contains("1 idle"))
	h._assert_hud("…and prices the previewed crew (strip stamps %d, want %d)" % [_crew_stamp(sheet),
		PREVIEW_CREW], _crew_stamp(sheet) == PREVIEW_CREW)

func _tooltip_containing(root: Node, text: String) -> String:
	if root is Control and (root as Control).tooltip_text.contains(text):
		return (root as Control).tooltip_text
	for child in root.get_children():
		var found := _tooltip_containing(child, text)
		if found != "":
			return found
	return ""
