extends RefCounted

## THE WORK STRIP ON THE COMPOSE SHEETS (issue #877) -- the one control the Work tab's row and the
## source's own sheet both wear, and the rung card its `⌃` opens over the sheet.
##
## One chapter of the `ui_preview` state walk, appended LAST so no earlier frame moves. The Work tab's
## rows are judged in `band_panel_preview`; this chapter is the sheet half: the strip mounted on the
## forage sheet and the hunt sheet, the footer's `Show on map` link, and the claim a frame cannot make
## on its own -- that the rung card is a window drawn ABOVE the sheet that opened it.

## The checkpoints this chapter owes the walk -- assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 14

const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## Every craft known, so the next rung of either web is on offer rather than refused.
const ALL_KNOWN := {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0}

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

func run(harness) -> void:
	h = harness
	h._hud.update_intensification([{"faction": 0, "knowledges": ALL_KNOWN}])
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	await _forage_sheet_strip()
	await _hunt_sheet_strip()
	await _worked_sheet_strip()
	h._hud._rungcard.dismiss()
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_forage_source()
	h._hud._compose.reset_hunt_source()
	await h._settle()

## The forage sheet: the strip is mounted, its `⌃` is on offer, and pressing it opens the rung card
## ABOVE the sheet.
func _forage_sheet_strip() -> void:
	var tile := TileFx.stressed_tile_fixture()
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	var sheet = h._hud._drawercompose._compose_sheet
	await h._save("work_strip_forage_sheet")
	h._assert_hud("the forage sheet mounts ONE work strip", ForageFx.find_strip(sheet) != null)
	h._assert_hud("…with the next rung on offer (got `%s`)" % ForageFx.ladder_kind(sheet),
		ForageFx.ladder_kind(sheet) == WorkStrip.LADDER_OFFER)
	h._assert_hud("…and an UNWORKED source's footer has no `Show on map` (there is no crew to stop or finish)",
		_find_meta(sheet, HudComposeVocab.SHOW_ON_MAP_META) == null)
	var chip := _ladder_button(sheet)
	h._assert_hud("…and the ladder chip is a pressable button", chip != null)
	if chip == null:
		return
	chip.pressed.emit()
	await h._settle()
	await h._save("work_strip_forage_sheet_rung_card")
	h._assert_hud("pressing the chip opens the rung card", h._hud._rungcard.is_open())
	var card: Window = h._hud._rungcard.card()
	var sheet_rect: Rect2 = (sheet as Control).get_global_rect()
	h._assert_hud("…a WINDOW, so it draws above the sheet's CanvasLayer (card %s over sheet %s)"
			% [str(Rect2(card.position, card.size)), str(sheet_rect)],
		card.visible and card.size.x > 0 and card.size.y > 0)
	h._hud._rungcard.dismiss()
	await h._settle()
	h._assert_hud("…and it dismisses", not h._hud._rungcard.is_open())

## The hunt sheet: the same strip on an animal source.
func _hunt_sheet_strip() -> void:
	var herd := HerdFx.herd_fixture()
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	h._compose_herd(herd)
	await h._settle()
	var sheet = h._hud._drawercompose._compose_sheet
	await h._save("work_strip_hunt_sheet")
	h._assert_hud("the hunt sheet mounts ONE work strip", ForageFx.find_strip(sheet) != null)

## A WORKED source's sheet: the same strip, the chip FILLING to the build's progress, and the footer's
## `Show on map` link at the left of `Done` / `Stop working here`.
func _worked_sheet_strip() -> void:
	var worked_band := BandFx.cultivating_forage_band_fixture()
	# The patch's build is the queue HEAD, so the band's builders pool is building it (the strip's
	# `K building, done in ≈T turns` hover).
	worked_band["build_queue"] = [{"kind": SourceForecast.LABOR_KIND_FORAGE,
		"target_x": 66, "target_y": 10, "fauna_id": ""}]
	h._hud._band_labor._player_band = worked_band
	h._hud._band_labor._player_bands = [worked_band]
	var tile := BaseFx.food_tile_fixture()
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	var sheet = h._hud._drawercompose._compose_sheet
	await h._save("work_strip_worked_sheet")
	h._assert_hud("a worked source's sheet mounts the strip with the chip FILLING (got `%s`)"
			% ForageFx.ladder_kind(sheet),
		ForageFx.ladder_kind(sheet) in [WorkStrip.LADDER_BUILDING, WorkStrip.LADDER_STALLED])
	var chip_tip := ForageFx.strip_chip_tip(sheet)
	h._assert_hud("…and the chip's hover carries the builders and the sim's turn estimate (got \"%s\")" % chip_tip,
		chip_tip.contains(HudComposeVocab.BUILD_TURNS_COUNT_FORMAT.split("%")[0])
			and chip_tip.contains(HudWorkVocab.WORK_STRIP_BUILD_DONE_FORMAT.split("%s")[1]))
	h._assert_hud("…and links `Show on map` in the footer",
		_find_meta(sheet, HudComposeVocab.SHOW_ON_MAP_META) != null)

## The ladder chip's press target on a sheet's strip, or `null`.
func _ladder_button(sheet: Node) -> Button:
	var strip := ForageFx.find_strip(sheet)
	if strip == null:
		return null
	var chip := _find_meta(strip, HudWorkVocab.WORK_STRIP_LADDER_META)
	if chip == null:
		return null
	for child in chip.get_children():
		if child is Button:
			return child as Button
	return null

func _find_meta(node: Node, meta: StringName) -> Node:
	if node.has_meta(meta):
		return node
	for child in node.get_children():
		var found := _find_meta(child, meta)
		if found != null:
			return found
	return null
