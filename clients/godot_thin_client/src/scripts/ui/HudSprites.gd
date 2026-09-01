extends RefCounted
class_name HudSprites

## Bundled PNG art for HUD MARKS — the sprite half of the text glyphs the HUD's own chrome wears,
## currently the knowledge screen's launcher (`HudKnowledgeVocab.LAUNCH_MARK`).
##
## HOW THIS FAMILY DIFFERS from the map-marker families (`FaunaSprites`/`SiteSprites`/
## `WonderSprites`/`StageSprites`): those draw over the MAP, at marker size, in whatever colours the
## terrain happens to be under them. These are drawn on the dark HUD panel — `HudStyle.PANEL_SOLID`
## — at 13-24px, on an icon button's face or in a popover row. That is the `crops/` situation, not
## the map's, so the art takes the `crops/` sub-style (no outline, front-on, pale fill on near-black)
## and `assets/icons/icon_prompts.txt` documents it per DIRECTORY: everything in `hud/` takes it.
##
## **THE TABLE CARRIES `cairn` ONLY, AND THAT IS COMPLETE COVERAGE FOR WHAT IT DECLARES.**
## `assets/icons/hud/` also holds `forage.png`, `hunt.png` and `scout.png`, which are shipped art
## awaiting issue #249's emoji→sprite migration. They are absent here on purpose: a path with no
## call site is dead data, not coverage, and listing one would make this family's `warn: true`
## (below) a lie about art nobody loads.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state.

## Mark id → bundled texture path.
const SPRITE_DIR := "res://assets/icons/hud/"
const SPRITE_PATHS := {
	"cairn": SPRITE_DIR + "cairn.png",
}

## Bundled sprite for a HUD mark, or `null` when the key is unknown — the caller then falls back to
## its vocabulary's text glyph (`HudKnowledgeVocab.LAUNCH_GLYPH` for the cairn), which is the same
## contract every other art family has with its emoji.
##
## Takes `IconSprites.texture_for`'s DEFAULT `warn: true`: this family's coverage is complete for
## what it declares — every key here has a committed, imported PNG behind it — so a failed load is a
## DEFECT and must surface. (Contrast `FloraSprites`, the one family that passes `false` because a
## species without art is its expected state.)
static func for_mark(mark_id: String) -> Texture2D:
	if mark_id == "" or not SPRITE_PATHS.has(mark_id):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[mark_id]))
