extends RefCounted
class_name CropRoleSprites

## Bundled PNG art for the tile card's CROP-ROLE marks — the sprite half of `FoodIcons`'
## `CROP_ROLE_ICONS` vocabulary, and the fifth art family behind `IconSprites`.
##
## WHY sprites at all: the three marks it replaces were BORROWED — 🌾 is the HUD's food/forage
## mark, 🐄 is `POLICY_CORRAL`'s penned-livestock mark (still, on the work board and the map), and
## the cash fallback was `⇄`, the trade-goods glyph that marked every non-food yield component. A
## reader who learns one meaning met it saying something else here. That collision — not legibility —
## is what this art removes (issue #463). The emoji path's usual complaint applies too: it draws
## through `ThemeDB.fallback_font`, so the OS emoji font owned the look.
##
## **THE `cash` ART OUTLIVED THE ACCOUNT IT WAS DRAWN BESIDE** (arc #527). The trade axis is retired
## and a cash crop pays a NAMED MATERIAL now, so the ⇄ fallback went (🧵 replaced it) — but the bolt
## of dyed cloth was always drawing the PRODUCT rather than the account, which is why the art needed
## no change at all.
##
## HOW THIS FAMILY DIFFERS FROM THE OTHER FOUR — two ways, and both change how it is used:
##
##   1. **IT RETURNS A PATH, NOT A `Texture2D`.** `FaunaSprites` / `SiteSprites` / `WonderSprites` /
##      `StageSprites` all feed `MapView._draw_marker_sprite`, a canvas draw that wants a texture.
##      These marks lead a row of a `RichTextLabel` (`DetailFormat.flora_composition_lines`), so
##      they are consumed as `[img]` BBCode, which addresses its art by `res://` path. The load is
##      still done here through the shared `IconSprites.texture_for`, so "is there art for this
##      role" is answered by the SAME load-and-cache-and-warn-once the other four use — the path is
##      only handed back once that load has succeeded. A path returned for a texture that failed to
##      load would put a broken-image box in the middle of a text row.
##
##   2. **THE KEY IS THE SIM'S `FloraShareInfo.role` TAG VERBATIM.** Like `StageSprites` (and
##      unlike the fauna/site tables, which resolve a key out of free text), the server sends the
##      key, so there is no client-side resolver — a direct table hit. `""` and any unknown tag mean
##      UNSTATED and resolve to no art, never to `staple`; that rule lives in
##      `FoodIcons.for_crop_role` and is not re-decided here.
##
## Static-only by design, same reasoning as `FoodIcons` and the four sibling tables.

## `FloraShareInfo.role` → bundled texture path.
const SPRITE_DIR := "res://assets/icons/crops/"
const SPRITE_PATHS := {
	"staple": SPRITE_DIR + "staple.png",
	"fodder": SPRITE_DIR + "fodder.png",
	"cash": SPRITE_DIR + "cash.png",
}

## The blank slot an UNSTATED role renders — a fully transparent PNG, drawn at the same box size as
## a real mark so one untagged plant cannot shift the whole list's names out of column.
##
## **IT IS A GENERATED FILE, NOT ART**, and is regenerated rather than edited:
##   python3 -c "from PIL import Image; Image.new('RGBA', (256, 256), (0, 0, 0, 0)).save('clients/godot_thin_client/assets/icons/crops/unstated.png')"
##
## A transparent IMAGE rather than spaces because the widths have to match EXACTLY: every mark is
## boxed to `[img=NxN]` regardless of its subject's aspect, so a spacer boxed the same way is the
## only thing guaranteed to occupy the identical width. Sizing spaces to match would re-derive a
## glyph advance the font owns.
const SPACER_PATH := SPRITE_DIR + "unstated.png"

## Bundled art path for a crop ROLE, or `""` when this role has no art (the caller then falls back
## to the emoji glyph in `FoodIcons.CROP_ROLE_ICONS`).
##
## **The `null` fallback is LIVE here, like `WonderSprites`' and `StageSprites`' and unlike
## fauna/sites'.** Coverage is complete across the three roles the sim ships today, but the fallback
## is not merely defensive: it is what keeps the rows rendering during art iteration, and it is the
## whole reason this change could land as a table plus a lookup rather than a flag day.
static func path_for(role: String) -> String:
	if not SPRITE_PATHS.has(role):
		return ""
	return _path_if_loadable(String(SPRITE_PATHS[role]))


## The transparent blank-slot path, or `""` when the spacer is missing — in which case the caller
## falls back to the text spacer (`HudFloraVocab.FLORA_ROLE_ICON_UNSTATED`) and the column is
## slightly off rather than the row carrying a broken-image box.
static func spacer_path() -> String:
	return _path_if_loadable(SPACER_PATH)


## `path` back, but only once it has actually loaded. Routed through `IconSprites` so this family
## shares the one texture cache, the `load()`-not-`preload()` degradation and the warn-once-per-bad
## -path behaviour with the four marker tables, rather than carrying a fifth copy of them.
static func _path_if_loadable(path: String) -> String:
	if IconSprites.texture_for(path) == null:
		return ""
	return path
