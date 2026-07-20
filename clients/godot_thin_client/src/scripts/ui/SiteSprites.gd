extends RefCounted
class_name SiteSprites

## Bundled PNG art for map food-site markers — the sprite half of `FoodIcons`' site vocabulary,
## and the food-module twin of `FaunaSprites`.
##
## WHY sprites at all (same reasoning as `FaunaSprites`): the emoji path draws through
## `ThemeDB.fallback_font`, so the OS emoji font decides what a shellfish bed or a nut grove looks
## like, and both go blobby at marker size. A bundled silhouette is ours and identical everywhere.
##
## Coverage is COMPLETE: every `FoodIcons.ICONS` module plus the three non-module art keys (hunt,
## reeds, default) maps to bundled art, so no food site in the game draws an OS emoji today. The
## `null` fallback is still load-bearing — it keeps the emoji renderer as the safety net if an art
## key ever arrives unmapped (e.g. a new food module added to `ICONS` without a PNG).
##
## Static-only by design: a pure lookup with no node state, called from the map draw loop.

## Site ART KEY (from `FoodIcons.site_key_for`) → bundled texture path.
const SPRITE_DIR := "res://assets/icons/sites/"
## A hunted site is GAME, so it reuses the fauna deer rather than shipping a second copy of the
## same silhouette under `sites/` for the two to drift apart.
const HUNT_SPRITE_PATH := "res://assets/icons/fauna/deer.png"
const SPRITE_PATHS := {
	FoodIcons.SITE_KEY_HUNT: HUNT_SPRITE_PATH,
	FoodIcons.SITE_KEY_REEDS: SPRITE_DIR + "reeds.png",
	"coastal_littoral": SPRITE_DIR + "shell.png",
	# riverine_delta's OPEN-WATER read; the dry-floodplain read resolves to SITE_KEY_REEDS above.
	"riverine_delta": SPRITE_DIR + "fish.png",
	"savanna_grassland": SPRITE_DIR + "grain.png",
	"temperate_forest": SPRITE_DIR + "chestnut.png",
	"boreal_arctic": SPRITE_DIR + "seal.png",
	"montane_highland": SPRITE_DIR + "tuber.png",
	"wetland_swamp": SPRITE_DIR + "lotus.png",
	"semi_arid_scrub": SPRITE_DIR + "cactus.png",
	"coastal_upwelling": SPRITE_DIR + "shrimp.png",
	"mixed_woodland": SPRITE_DIR + "mushroom.png",
	FoodIcons.SITE_KEY_DEFAULT: SPRITE_DIR + "sprig.png",
}

## Bundled sprite for a food site, or `null` when its art key has no art (the caller then falls
## back to `FoodIcons.for_site`'s emoji). Takes the same arguments as `for_site` and resolves them
## through the shared `FoodIcons.site_key_for`, so the sprite and the emoji can never disagree
## about which site this is. Load-and-cache lives in `IconSprites`, shared with `FaunaSprites`.
static func for_site(module_key: String, is_hunt: bool, terrain_id: int = -1) -> Texture2D:
	var key := FoodIcons.site_key_for(module_key, is_hunt, terrain_id)
	if not SPRITE_PATHS.has(key):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[key]))
