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
const EXPECTED_CHECKPOINTS := 55

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
const LESSON_LINE := "Needs Worked Land"
const LESSON_TIP := "Working a farm teaches Worked Land; then ground like this can be farmed."
## Seed Selection short of complete: the Farm's own gate is unmet, so this people cannot raise one yet.
const SEED_SELECTION_PARTIAL := 0.4
const RANCH_BONUS_LINE := "each keeper minds 1.5× the head"
const RANCH_ONE_RING_LINE := "1 more ring — 1.5× the head"

func run(harness) -> void:
	h = harness
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(false))
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await _off_site_states()
	await _pen_states()
	_tended_patch_offers_farm()
	h._hud._drawercompose.close_compose_sheet()
	await _another_peoples_build()

## **A TENDED PATCH MUST OFFER THE FARM** (playtest, #175): Seed Selection known, the ground watered
## (the server's `site_refusal` empty), and a crop in the basket that climbs to the farm. The same patch
## with its seed lesson unlearned, and on dry ground, are the controls: the offer is gated by exactly the
## facts the server states, and by nothing the client invents.
func _tended_patch_offers_farm() -> void:
	var knows := {"cultivation": KNOWN, "seed_selection": KNOWN}
	var lacks := {"cultivation": KNOWN, "seed_selection": 0.4}
	var tended := RungFx.stamp_patch({
		"ecology_phase": "thriving", "is_cultivated": true, "cultivation_progress": 1.0,
		"is_field": false, "sow_site_refusal": "",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	})
	var offer := RungGates.next_rung_offered("forage", tended, "", knows)
	h._assert_hud("a tended patch with Seed Selection, watered, and a crop that climbs OFFERS the Farm (got `%s`)"
		% str(offer), String(offer.get("policy", "")) == "sow" and offer.get("reasons", []).is_empty())
	h._assert_hud("...and it is READY (the work row's mark), not merely offered",
		String(RungGates.next_rung_ready("forage", tended, "", knows).get("policy", "")) == "sow")
	var unlearned := RungGates.next_rung_offered("forage", tended, "", lacks)
	h._assert_hud("...with the lesson unlearned it is still OFFERED, gated, with the reason",
		String(unlearned.get("policy", "")) == "sow" and not unlearned.get("reasons", []).is_empty())
	# The reason a tended patch shows NO Farm control is the lesson at 90%: the sheet's aside must name the
	# lesson's progress and the rung that waits on it, and must say nothing once it is learned.
	h._assert_hud("the unlearned lesson's aside names its progress and the rung it holds back",
		SourceForecast.lesson_opens_clause(tended, "", {"cultivation": KNOWN, "seed_selection": 0.9}, false)
			== " Seed Selection 90% - the farm waits on it.")
	h._assert_hud("...and states nothing once the lesson is known",
		SourceForecast.lesson_opens_clause(tended, "", knows, true) == "")
	var dry := RungFx.stamp_patch({
		"ecology_phase": "thriving", "is_cultivated": true, "cultivation_progress": 1.0,
		"sow_site_refusal": "too_dry",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	})
	var dry_offer := RungGates.next_rung_offered("forage", dry, "", knows)
	h._assert_hud("...and dry ground is OFFERED gated with the server's reason, never silently absent",
		String(dry_offer.get("policy", "")) == "sow" and not dry_offer.get("reasons", []).is_empty())

func _knowledge(worked_land: float, seed_selection: float = KNOWN) -> void:
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		"cultivation": KNOWN, "herding": KNOWN, "seed_selection": seed_selection, "penning": KNOWN,
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
	# (0) A people that CANNOT raise a farm yet (the Farm rung's own gate is unmet): the same refused
	# ground carries NO Farming row -- the row would send them to farm when they cannot.
	_knowledge(UNKNOWN, SEED_SELECTION_PARTIAL)
	var early := _off_site_tile(RungQuotes.SITE_REFUSAL_NEEDS_LESSON)
	h._hud._compose.reset_forage_source()
	h._show_tile(early)
	await h._settle()
	await h._save("worked_land_cannot_farm_yet")
	var early_ctx := DetailFormat.Context.new()
	var early_lines: Array = h._hud._drawer._tile_terrain_lines(early, early_ctx)
	h._assert_hud("a people that cannot raise a farm yet gets NO Farming row on lesson-gated ground",
		_row_value(early_lines, HudFloraVocab.SITE_LESSON_ROW) == ""
			and DetailFormat.site_lesson_line(early, {}, {"seed_selection": SEED_SELECTION_PARTIAL}) == "")
	h._assert_hud("…and no hover is registered for the absent row", early_ctx.row_tooltips.is_empty())
	# (a) WITHOUT the lesson: the server says `needs_lesson`.
	_knowledge(UNKNOWN)
	var needs := _off_site_tile(RungQuotes.SITE_REFUSAL_NEEDS_LESSON)
	h._hud._compose.reset_forage_source()
	h._show_tile(needs)
	await h._settle()
	await h._save("worked_land_needs_lesson")
	var lesson_ctx := DetailFormat.Context.new()
	var lines: Array = h._hud._drawer._tile_terrain_lines(needs, lesson_ctx)
	h._assert_hud("off-site ground refused for want of a lesson is NOT workable",
		not DetailFormat.tile_is_workable(needs))
	h._assert_hud("…so no forage block and no compose sheet are offered on it",
		Readout.detail_row_index(lines, HudFloraVocab.FORAGING_KEY) < 0
			and not h._hud._drawercompose._forage_compose_available(needs))
	h._assert_hud("…and the card names the lesson it still needs: `%s`" % LESSON_LINE,
		_row_value(lines, HudFloraVocab.SITE_LESSON_ROW) == LESSON_LINE)
	h._assert_hud("…and its hover names the lesson and the rung from the roster and catalog: `%s`" % LESSON_TIP,
		String(lesson_ctx.row_tooltips.get(HudFloraVocab.SITE_LESSON_ROW, "")) == LESSON_TIP)
	var thin := _off_site_tile("not_gathering_site")
	h._assert_hud("a DIFFERENT refusal (not a gathering site) states no lesson line",
		DetailFormat.site_lesson_line(thin, {}, {"seed_selection": KNOWN}) == "" and not DetailFormat.tile_is_workable(thin))

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
		DetailFormat.site_lesson_line(admitted, {}, {"seed_selection": KNOWN}) == "")
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

# ---- ANOTHER PEOPLE'S BUILD (playtest: a Work row wore a 20% Cultivate meter that was Lothari's) --------

const OTHER_PEOPLE := 1
const OTHER_BUILD_PROGRESS := 0.2
## The words, as the player reads them (never composed through the producers).
const OTHER_TILE_ROW := "\u2691 Lothari \u00b7 20%"

## A wild patch with a 20% Cultivate meter, owned by `owner_faction`.
func _owned_patch(owner_faction: int) -> Dictionary:
	var tile := TileFx.three_role_tile_fixture()
	tile["x"] = OFF_SITE_X
	tile["y"] = OFF_SITE_Y
	tile["patch_is_cultivated"] = false
	tile["patch_cultivation_progress"] = OTHER_BUILD_PROGRESS
	tile["patch_has_owner"] = true
	tile["patch_owner"] = owner_faction
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## Whose build the source carries, as the work strip's row model states it (`foreign_build`), `""` for a
## source whose build is ours or nobody's. The compose sheet states it under the strip.
func _foreign_owner_of(source: Dictionary, prefix: String) -> int:
	return RungQuotes.foreign_owner(source, prefix)

func _band_labor_builder_label(tile: Dictionary) -> String:
	return h._hud._band_labor.builder_label(RungQuotes.builder_band(tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX))

func _another_peoples_build() -> void:
	FactionNames.update([{"faction": OTHER_PEOPLE, "name": "Lothari"}])
	_knowledge(UNKNOWN)
	var theirs := _owned_patch(OTHER_PEOPLE)
	var ours := _owned_patch(HudConst.PLAYER_FACTION_ID)
	h._hud._compose.reset_forage_source()
	h._show_tile(theirs)
	await h._settle()
	await h._save("other_people_build_tile")
	var their_lines: Array = h._hud._drawer._tile_terrain_lines(theirs, DetailFormat.Context.new())
	var our_lines: Array = h._hud._drawer._tile_terrain_lines(ours, DetailFormat.Context.new())
	h._assert_hud("tile card: another people's 20% build names the people, in place of the bare meter",
		_row_value(their_lines, DetailFormat.CULTIVATION_ROW) == OTHER_TILE_ROW)
	h._assert_hud("...and our own build on the same patch reads as today (no flag, no name)",
		not _row_value(our_lines, DetailFormat.CULTIVATION_ROW).contains("Lothari")
			and _row_value(our_lines, DetailFormat.CULTIVATION_ROW) != "")
	h._assert_hud("...in warn ink",
		DetailFormat.rung_value_hex(OTHER_TILE_ROW, "") == HudStyle.WARN_HEX)
	h._assert_hud("the source is another people's ground, and ours is not",
		_foreign_owner_of(theirs, HudComposeVocab.FORAGE_FORECAST_PREFIX) == OTHER_PEOPLE
			and _foreign_owner_of(ours, HudComposeVocab.FORAGE_FORECAST_PREFIX)
				== RungQuotes.NO_FOREIGN_OWNER)
	await _another_peoples_herd()
	await _builder_band_names()
	FactionNames.reset()

const OTHER_TAME_PROGRESS := 0.3
const OTHER_HERD_ROW := "\u2691 Lothari \u00b7 30%"

## A wild herd 30% through the Tame, owned by `owner_faction`.
func _owned_herd(owner_faction: int) -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd["domestication"] = OTHER_TAME_PROGRESS
	herd["corralled"] = false
	herd["has_owner"] = true
	herd["owner"] = owner_faction
	return RungFx.stamp_herd(herd)

func _another_peoples_herd() -> void:
	var theirs := _owned_herd(OTHER_PEOPLE)
	var ours := _owned_herd(HudConst.PLAYER_FACTION_ID)
	h._show_herd(theirs)
	await h._settle()
	await h._save("other_people_build_herd")
	var world: Array = h._hud._band_labor.world_herds()
	var their_lines := DetailFormat.herd_summary_lines(theirs, world)
	var our_lines := DetailFormat.herd_summary_lines(ours, world)
	h._assert_hud("herd card: another people's 30%% Tame names the people (got `%s`)" % _row_value(their_lines, DetailFormat.HUSBANDRY_ROW),
		_row_value(their_lines, DetailFormat.HUSBANDRY_ROW) == OTHER_HERD_ROW)
	h._assert_hud("...and the unstarted Corral carries no meter row at all",
		_row_value(their_lines, DetailFormat.CORRAL_ROW) == "")
	h._assert_hud("...and our own Tame on the same herd reads as today",
		not _row_value(our_lines, DetailFormat.HUSBANDRY_ROW).contains("Lothari")
			and _row_value(our_lines, DetailFormat.HUSBANDRY_ROW) != "")
	h._assert_hud("the herd is another people's, and ours is not",
		_foreign_owner_of(theirs, HudComposeVocab.BARE_FORECAST_PREFIX) == OTHER_PEOPLE
			and _foreign_owner_of(ours, HudComposeVocab.BARE_FORECAST_PREFIX)
				== RungQuotes.NO_FOREIGN_OWNER)

const BUILDER_BAND_ID := 77
const BUILDER_BAND_NAME := "Netherford"
const OWN_BAND_NAME := "Peatmoor"
const OWN_PEOPLE := "Thornfolk"

## `owner_faction`'s 20% patch whose builder band is `builder_band` (0 = the wire names none).
func _built_patch(owner_faction: int, builder_band: int) -> Dictionary:
	var tile := _owned_patch(owner_faction)
	tile["patch_builder_band"] = builder_band
	return tile

func _band_row(id: String, entity: int, faction: int, size: int) -> Dictionary:
	return {"id": id, "entity": entity, "faction": faction, "size": size,
		"pos": [OFF_SITE_X, OFF_SITE_Y], "current_x": OFF_SITE_X, "current_y": OFF_SITE_Y,
		"activity": "idle", "working_age": 10, "idle_workers": 0, "output_multiplier": 1.0,
		"turns_of_food": 20.0, "labor_assignments": []}

func _builder_band_names() -> void:
	FactionNames.update([{"faction": 0, "name": OWN_PEOPLE}, {"faction": OTHER_PEOPLE, "name": "Lothari"}])
	h._hud._band_labor.set_foreign_bands([{"band_id": BUILDER_BAND_ID, "entity": 777, "name": BUILDER_BAND_NAME,
		"faction": OTHER_PEOPLE, "current_x": 70, "current_y": 5, "size": 22}])
	var named := _built_patch(OTHER_PEOPLE, BUILDER_BAND_ID)
	var named_lines: Array = h._hud._drawer._tile_terrain_lines(named, DetailFormat.Context.new())
	h._assert_hud("tile card: a named builder reads `Band, People`",
		_row_value(named_lines, DetailFormat.CULTIVATION_ROW) == "\u2691 Netherford, Lothari \u00b7 20%")
	h._assert_hud("the named builder band resolves to its roster name",
		_band_labor_builder_label(named) == BUILDER_BAND_NAME)
	var gone := _built_patch(OTHER_PEOPLE, BUILDER_BAND_ID + 1)
	var gone_lines: Array = h._hud._drawer._tile_terrain_lines(gone, DetailFormat.Context.new())
	h._assert_hud("...a builder the roster does not hold falls back to the people alone",
		_row_value(gone_lines, DetailFormat.CULTIVATION_ROW) == OTHER_TILE_ROW)
	# EVERY band row, ours included, reads `Band, People`.
	var tile := TileFx.three_role_tile_fixture()
	tile["x"] = OFF_SITE_X
	tile["y"] = OFF_SITE_Y
	tile["units"] = [_band_row(OWN_BAND_NAME, 901, 0, 30), _band_row(BUILDER_BAND_NAME, 902, OTHER_PEOPLE, 22)]
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	await h._settle()
	await h._save("tile_panel_band_people")
	h._assert_hud("tile panel: our own band row reads `Peatmoor, Thornfolk`",
		Q.has_label_containing(h._hud.subject_list, "Peatmoor, Thornfolk"))
	h._assert_hud("tile panel: a foreign band row reads `Netherford, Lothari`",
		Q.has_label_containing(h._hud.subject_list, "Netherford, Lothari"))
	var widest := 0.0
	for row in h._hud.subject_list.get_children():
		if row is Button:
			widest = maxf(widest, (row.get_child(0) as Control).get_combined_minimum_size().x)
	h._assert_hud("tile panel: the widest band row (%d) fits the list (%d)" % [int(widest), int(h._hud.subject_list.size.x)],
		widest <= h._hud.subject_list.size.x)
	h._hud._band_labor.set_foreign_bands([])
