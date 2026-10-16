extends RefCounted

## WORKED LAND AND THE RANCH (issue #175) -- what the client does with the two derived facts the
## ladder catalog now carries.
##
## One chapter of the `ui_preview` state walk, appended LAST so no earlier frame moves.
##
## **Off-site farming is the SERVER'S call and the client only reads it.** A farm's `site_refusal` is
## `needs_lesson` on ground a lesson would open and `""` where this people may sow now. The card and the
## compose sheet are driven off that field alone: the same off-site tile is staged twice, differing in
## that one key (and the lesson), and the claims are that the first offers no work and names the lesson
## while the second offers Sow. **The Pen's size tier is `size_tier` on the herd**, named from the
## catalog's tier row: a pen below it says how many rings reach it, a pen in it says what it does.

## The checkpoints this chapter owes the walk -- assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 28

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const KnowledgeFx := preload("res://tools/ui_preview/fixtures_knowledge.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

const KNOWN := 1.0
const UNKNOWN := 0.0

## Where the off-site tile stands, away from every gathering site fixture.
const OFF_SITE_X := 66
const OFF_SITE_Y := 9

## The pen's ring counts the frames stage: one short of the Ranch tier, and exactly at it.
const RINGS_SHORT := RungFx.RANCH_MIN_RINGS - 1
const RINGS_AT_TIER := RungFx.RANCH_MIN_RINGS
const PEN_FOOTPRINT_SHORT := 7
const PEN_FOOTPRINT_AT_TIER := 19

## The words the claims spell out, as the player reads them (never composed through the producers).
const SOW_BUTTON := "Sow farm"
const RAISE_EYEBROW := "Raise a farm"
const CREW_LINE := "The crew will raise a farm of Wild Tubers here."
const LESSON_LINE := "Learn Worked Land — work a farm"
const RANCH_BONUS_LINE := "each keeper minds 1.5× the head"
const RANCH_ONE_RING_LINE := "1 more ring — 1.5× the head"

func run(harness) -> void:
	h = harness
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(false))
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await _off_site_states()
	await _pen_states()
	h._hud._drawercompose.close_compose_sheet()

func _knowledge(worked_land: float) -> void:
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		"cultivation": KNOWN, "herding": KNOWN, "seed_selection": KNOWN, "penning": KNOWN,
		"worked_land": worked_land, "selective_breeding": UNKNOWN}}])

## Fertile ground that is NOT a gathering site, answered by the server as `refusal` (`needs_lesson`
## until Worked Land is learned, `""` after).
func _off_site_tile(refusal: String) -> Dictionary:
	var tile := TileFx.three_role_tile_fixture()
	tile["x"] = OFF_SITE_X
	tile["y"] = OFF_SITE_Y
	tile["food_module"] = ""
	tile["food_module_label"] = ""
	tile.erase("food_kind")
	tile["patch_sow_site_refusal"] = refusal
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

func _off_site_states() -> void:
	# (a) WITHOUT the lesson: the server says `needs_lesson`.
	_knowledge(UNKNOWN)
	var needs := _off_site_tile(RungQuotes.SITE_REFUSAL_NEEDS_LESSON)
	h._hud._compose.reset_forage_source()
	h._show_tile(needs)
	await h._settle()
	await h._save("worked_land_needs_lesson")
	var lines: Array = h._hud._drawer._tile_terrain_lines(needs)
	h._assert_hud("off-site ground refused for want of a lesson is NOT workable",
		not DetailFormat.tile_is_workable(needs))
	h._assert_hud("…so no forage block and no compose sheet are offered on it",
		Readout.detail_row_index(lines, HudFloraVocab.FORAGING_KEY) < 0
			and not h._hud._drawercompose._forage_compose_available(needs))
	h._assert_hud("…and the card names the lesson and what teaches it: `%s`" % LESSON_LINE,
		_row_value(lines, HudFloraVocab.SITE_LESSON_ROW) == LESSON_LINE)
	var thin := _off_site_tile("not_gathering_site")
	h._assert_hud("a DIFFERENT refusal (not a gathering site) states no lesson line",
		DetailFormat.site_lesson_line(thin, {}) == "" and not DetailFormat.tile_is_workable(thin))

	# (b) WITH the lesson: the server's verdict for the same tile is now empty.
	_knowledge(KNOWN)
	var admitted := _off_site_tile("")
	h._hud._compose.reset_forage_source()
	h._show_tile(admitted)
	h._compose_forage(admitted)
	await h._settle()
	await h._save("worked_land_sow_offered")
	h._assert_hud("the SAME ground, admitted by the server, IS workable",
		DetailFormat.tile_is_workable(admitted) and not DetailFormat.tile_is_gathering_site(admitted))
	h._assert_hud("…the compose sheet opens on it",
		h._hud._drawercompose._forage_compose_available(admitted)
			and h._hud._drawercompose.is_compose_sheet_open())
	var sheet = h._hud._drawercompose._compose_sheet
	var commit := Q.compose_commit_button(sheet)
	h._assert_hud("…and the commit says what it does, `%s` (got `%s`)" % [SOW_BUTTON, commit.text if commit != null else ""],
		commit != null and commit.text == SOW_BUTTON and not commit.disabled)
	h._assert_hud("…the header reads `%s`, not `Assign harvesters`" % RAISE_EYEBROW,
		Q.has_label_containing(sheet, RAISE_EYEBROW.to_upper())
			and not Q.has_label_containing(sheet, "ASSIGN HARVESTERS"))
	h._assert_hud("…the one line says the crew raises a farm of the PICKED crop: `%s`" % CREW_LINE,
		Q.has_label_containing(sheet, CREW_LINE) and not Q.has_label_containing(sheet, "Nothing picked")
			and not Q.has_label_containing(sheet, "Work tab"))
	h._assert_hud("…and no separate Sow checkbox is offered (this order IS the sow)",
		ForageFx.improvement_state(sheet, "sow") == "")
	h._assert_hud("…with no lesson line, the lesson being learned",
		DetailFormat.site_lesson_line(admitted, {}) == "")
	# Off the sites the server takes a crew only with a crop named, so the chips are the single-pick
	# CROP row and the commit carries one; a bare gather is not on offer.
	h._assert_hud("…the crop row is the single-pick `%s` (a crop must be named)" % HudFloraVocab.TAKE_ROW_LABEL_SINGLE,
		Q.has_label_containing(sheet, HudFloraVocab.TAKE_ROW_LABEL_SINGLE.to_upper()))
	var payload: Dictionary = await _committed_payload(sheet)
	var line := String(MAIN_SCRIPT.format_assign_labor(payload).get("line", ""))
	h._assert_hud("…and the commit sends a forage assign that NAMES a crop (got `%s`)" % line,
		String(payload.get("species", "")) != "" and line.contains(" forage ")
			and line.contains(String(payload.get("species", ""))))
	# A gathering site keeps the plain gather: no crop is forced onto it.
	var site := RungFx.stamp_patch(TileFx.three_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._hud._compose.reset_forage_source()
	h._show_tile(site)
	h._compose_forage(site)
	await h._settle()
	h._assert_hud("a gathering site forces no crop on the plain gather",
		DetailFormat.offsite_crop_verb(site) == "")
	h._hud._drawercompose.close_compose_sheet()

## Presses the open sheet's commit and answers the `assign_labor_requested` payload it emitted, rolling
## the pending entry back so the next state reads the band as it was.
func _committed_payload(sheet: Node) -> Dictionary:
	var commit := Q.find_meta_node(sheet, HudWidgets.COMPOSE_COMMIT_META) as Button
	if commit == null:
		return {}
	var captured: Array = []
	var sink := func(payload: Dictionary) -> void: captured.append(payload)
	h._hud.assign_labor_requested.connect(sink)
	commit.pressed.emit()
	h._hud.assign_labor_requested.disconnect(sink)
	await h._settle()
	if captured.is_empty():
		return {}
	var payload: Dictionary = captured[0]
	h._hud.drop_pending_assign(payload)
	return payload

func _pen_states() -> void:
	# (c) a pen fenced one ring short of the Ranch tier.
	var short := _pen_herd(RINGS_SHORT, PEN_FOOTPRINT_SHORT, "")
	h._hud._compose.reset_hunt_source()
	h._show_herd(short)
	await h._settle()
	await h._save("pen_below_ranch")
	var short_lines: Array = DetailFormat.herd_summary_lines(short, [short], "")
	h._assert_hud("a pen below the tier says how many more rings make it a Ranch",
		_row_value(short_lines, "Ranch") == RANCH_ONE_RING_LINE)
	h._assert_hud("…and is not yet NAMED a Ranch (its Corral row reads the plain pen)",
		not _row_value(short_lines, "Corral").contains("Ranch"))

	# (d) the same pen fenced to the tier, with the server's `size_tier` stated.
	var ranch := _pen_herd(RINGS_AT_TIER, PEN_FOOTPRINT_AT_TIER, RungFx.TIER_RANCH)
	h._show_herd(ranch)
	await h._settle()
	await h._save("pen_is_a_ranch")
	var ranch_lines: Array = DetailFormat.herd_summary_lines(ranch, [ranch], "")
	h._assert_hud("a pen in the tier is named by the tier's label, in the Corral row",
		_row_value(ranch_lines, "Corral").contains("Ranch"))
	h._assert_hud("…and says the bonus in one line from the catalog: `%s`" % RANCH_BONUS_LINE,
		_row_value(ranch_lines, "Ranch") == RANCH_BONUS_LINE)
	h._assert_hud("…and no longer counts rings still to go",
		not _row_value(ranch_lines, "Ranch").contains("more ring"))
	# A herd with no pen states no tier line at all (the control that keeps the claims above honest).
	var wild := HerdFx.herd_fixture()
	h._assert_hud("a herd that is not penned carries no size-tier line",
		DetailFormat.pen_tier_lines(wild, "").is_empty())

	# The knowledge screen reads both lessons from the roster, with what each does from the catalog.
	var catalog := RungFx.ladder_rungs(false)
	h._assert_hud("Worked Land's note comes from the catalog's site relaxation",
		KnowledgeRoster.catalog_lesson_note("worked_land", catalog).contains("farm"))
	h._assert_hud("Selective breeding's note comes from the catalog's knowledge gain (1.25×)",
		KnowledgeRoster.catalog_lesson_note("selective_breeding", catalog).contains("1.25"))
	h._assert_hud("a lesson the catalog states no effect for gets no note of its own",
		KnowledgeRoster.catalog_lesson_note("paving", catalog) == "")
	# The Corral-channel feed line a pen reaching a tier writes.
	var phrase := EventDockPanel.detail_phrase(
		"status=tier action=extend_pen tier=ranch herd=game_deer_07 radius=2")
	h._assert_hud("the tier feed line reads as its tier and rings (got `%s`)" % phrase,
		phrase == "Ranch · Radius 2")

## A penned Red Deer herd fenced to `rings` rings, in the size tier the server stated (`""` for none).
func _pen_herd(rings: int, footprint: int, tier: String) -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd["domestication"] = RungFx.DOMESTICATION_COMPLETE
	herd["corralled"] = true
	herd["corral_progress"] = 1.0
	herd["pen_radius"] = rings
	herd["pen_footprint_tiles"] = footprint
	herd["size_tier"] = tier
	return RungFx.stamp_herd(herd)

## The value of the first `Key: value` row named `key`, `""` when absent.
func _row_value(lines: Array, key: String) -> String:
	var prefix := "%s: " % key
	for line in lines:
		if String(line).begins_with(prefix):
			return String(line).substr(prefix.length())
	return ""
