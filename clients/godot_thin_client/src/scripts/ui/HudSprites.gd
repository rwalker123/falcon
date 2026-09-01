extends RefCounted
class_name HudSprites

## Bundled PNG art for HUD MARKS — the sprite half of the text glyphs the HUD's own chrome wears:
## the knowledge screen's launcher (`HudKnowledgeVocab.LAUNCH_MARK`) and the three ACTIVITY marks the
## work board's filter chips and the roster's band rows wear (`HudWorkVocab` / `HudSelectionVocab`).
##
## HOW THIS FAMILY DIFFERS from the map-marker families (`FaunaSprites`/`SiteSprites`/
## `WonderSprites`/`StageSprites`): those draw over the MAP, at marker size, in whatever colours the
## terrain happens to be under them. These are drawn on the dark HUD panel — `HudStyle.PANEL_SOLID`
## — at 13-24px, on an icon button's face or in a popover row. That is the `crops/` situation, not
## the map's, so the art takes the `crops/` sub-style (no outline, front-on, pale fill on near-black)
## and `assets/icons/icon_prompts.txt` documents it per DIRECTORY: everything in `hud/` takes it.
##
## **THE TABLE NOW CARRIES ALL FOUR MARKS `assets/icons/hud/` SHIPS** — the `cairn` of issue #581,
## and `forage` / `hunt` / `scout` wired by issue #249. It held the cairn alone while the other three
## had no call site, because a path nothing loads is dead data rather than coverage; giving them
## call sites is what let them in.
##
## **THE MARK ID IS THE ACTIVITY, NOT THE SURFACE, and that is why `hunt` is one entry rather than
## two.** It replaces `FoodIcons.HUNT` (the work board's filter chip) and the roster's activity glyph
## alike — they meant the same thing and differed only in where they drew, so the migration was the
## moment to collapse them onto one file. A second id per surface would let the two drift back apart,
## which is the whole failure this family exists to prevent.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state.

## Mark id → bundled texture path.
const SPRITE_DIR := "res://assets/icons/hud/"
const SPRITE_PATHS := {
	"cairn": SPRITE_DIR + "cairn.png",
	"forage": SPRITE_DIR + "forage.png",
	"hunt": SPRITE_DIR + "hunt.png",
	"scout": SPRITE_DIR + "scout.png",
	"warrior": SPRITE_DIR + "warrior.png",
	"deny": SPRITE_DIR + "deny.png",
	"trade": SPRITE_DIR + "trade.png",
	"workers": SPRITE_DIR + "workers.png",
	"starving": SPRITE_DIR + "starving.png",
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
