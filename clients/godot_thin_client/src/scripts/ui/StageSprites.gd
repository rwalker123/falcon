extends RefCounted
class_name StageSprites

## Bundled PNG art for settlement-stage tokens — the sprite half of the `settlement_stage_icon`
## emoji vocabulary, used by the map band token and the band/city panel header.
##
## WHY sprites at all: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font
## decides what a camp or a village looks like — platform-inconsistent, and blobby at token size.
## A bundled silhouette is ours and identical everywhere. Same reasoning as `FaunaSprites`.
##
## HOW THIS FAMILY DIFFERS from `FaunaSprites`/`SiteSprites`: those resolve a client-side key from
## free text (a herd label goes through `FoodIcons.species_key_for`). Here the server already sends
## a stable stage KEY — `settlement_stage_id` — alongside the display glyph, so the lookup is
## direct and needs no resolver.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state, called
## from the map draw loop.

## Server `settlement_stage_id` → bundled texture path.
const SPRITE_DIR := "res://assets/icons/stages/"
const SPRITE_PATHS := {
	"nomadic": SPRITE_DIR + "nomadic.png",
	"camp": SPRITE_DIR + "camp.png",
	"village": SPRITE_DIR + "village.png",
}

## Bundled sprite for a settlement stage, or `null` when this stage has no art (the caller then
## falls back to the server's emoji glyph). The `null` case is load-bearing, not defensive:
## `data/settlement_stage_config.json` is user-editable, so a game can define stages beyond the
## three bundled here, and those must keep rendering their configured emoji. The load-and-cache
## behaviour lives in `IconSprites` — shared with `FaunaSprites`/`SiteSprites` — so this stays a
## pure key→path table.
static func for_stage(stage_id: String) -> Texture2D:
	if stage_id == "" or not SPRITE_PATHS.has(stage_id):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[stage_id]))
