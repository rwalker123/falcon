class_name FactionMark
extends Control

## **THE FACTION MARK — ONE GLYPH PER COUNTERPARTY, OURS AND THEIRS ALIKE** (issue #731), so *is this
## us?* has an answer on every row rather than only on a stranger's. The one helper every surface that
## names another band's people calls; a second drawing of it would be free to disagree with this one.
##
## ⛔ **THIS IS THE SLOT THE FACTION'S FLAG FILLS (#647).** Until factions carry a flag, the mark is a
## flag-shaped glyph in the faction's MAP colour (`MapView.faction_color`), the tint the map already
## paints that people's band banners in — so a counterparty reads the same here as on the map. When
## the flag lands it replaces `_draw` and nothing else about any row that holds one.

## The glyph's box, and the pole and cloth drawn inside it.
const MARK_SIZE := Vector2(12.0, 12.0)
const POLE_X := 1.6
const POLE_TOP := 1.0
const POLE_BOTTOM := 11.2
const POLE_WIDTH := 1.1
const CLOTH_RECT := Rect2(2.4, 1.4, 8.4, 5.6)
const CLOTH_OUTLINE_WIDTH := 0.7

var _faction: int = HudConst.PLAYER_FACTION_ID

## A mark for `faction`, hovering to its name and whether it is `own_faction`'s people.
static func make(faction: int, own_faction: int) -> FactionMark:
	var mark := FactionMark.new()
	mark._faction = faction
	mark.custom_minimum_size = MARK_SIZE
	mark.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	# PASS, not STOP: the mark carries its own tooltip and must not swallow the row's hover under it.
	mark.mouse_filter = Control.MOUSE_FILTER_PASS
	mark.tooltip_text = HudTradeVocab.FACTION_TIP_FORMAT % [faction_name(faction),
		HudTradeVocab.FACTION_OURS if faction == own_faction else HudTradeVocab.FACTION_THEIRS]
	return mark

## **THE ONE PEOPLE-NAME RESOLVER** — the name the sim minted for `faction`, read off the
## `faction_names` store (`FactionNames`), exactly as published. `Faction N` only when the store has
## no row for that id (a frame before the table, or a fixture that stages none).
static func faction_name(faction: int) -> String:
	var name := FactionNames.name_of(faction)
	if name != "":
		return name
	return HudTradeVocab.FACTION_NAME_FALLBACK_FORMAT % faction

## The colour the mark is drawn in — the map's, through its one lookup.
func mark_color() -> Color:
	return MapView.faction_color(_faction, MapView.BAND_FACTION_FALLBACK_COLOR)

func faction() -> int:
	return _faction

func _draw() -> void:
	draw_line(Vector2(POLE_X, POLE_TOP), Vector2(POLE_X, POLE_BOTTOM), HudStyle.INK_DIM, POLE_WIDTH)
	draw_rect(CLOTH_RECT, mark_color(), true)
	draw_rect(CLOTH_RECT, HudStyle.GROUND, false, CLOTH_OUTLINE_WIDTH)
