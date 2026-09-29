extends RefCounted
class_name WorkingsSprites

## Bundled PNG art for WORKING MAP MARKERS — the sprite half of `FoodIcons.MATERIAL_ICONS` (🪵 / 🪨),
## the mark a hex wears while a crew is cutting it (`SecondaryMarkerRenderer.face_for_material`, which
## both the marker and the band source list's row icon read, so the two cannot drift).
##
## **THE MAP-MARKER HOUSE STYLE** (`assets/icons/icon_prompts.txt` → WORKINGS): drawn over terrain in
## an edge slot, the fauna situation, so the art keeps the thick charcoal outline. The two subjects
## differ by SILHOUETTE — round cut log ends against squared blocks — because both can sit in two
## slots of one hex over brown-and-grey ground.
##
## **THE KEY IS THE SIM'S MATERIAL ID** (`core_sim/src/data/materials.json`), normalised the way
## `FoodIcons.for_material` normalises it, so the art and the emoji answer for the same string.
##
## **COVERAGE IS PARTIAL BY DESIGN AND EXPRESSED BY THE TABLE, NOT BY FAILED LOADS.** A material the
## minerals arc adds has no entry here until its art lands, and answers `null` without a load being
## attempted — the caller then draws its emoji. So every path this table DOES list has a committed
## PNG behind it, which is why `for_material` takes `IconSprites.texture_for`'s default `warn: true`:
## a listed path that fails to load is a DEFECT and must surface. `FloraSprites` passes `false`
## because it composes a path for EVERY species and expects most of them to miss; this table never
## composes a path it does not own.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state.

## Material id → bundled texture path.
const SPRITE_DIR := "res://assets/icons/workings/"
const SPRITE_PATHS := {
	"wood": SPRITE_DIR + "wood.png",
	"stone": SPRITE_DIR + "stone.png",
}

## Bundled sprite for a working's material, or `null` when the material has no art (an unknown or
## not-yet-drawn material) — the caller then draws `FoodIcons.for_material`'s emoji.
static func for_material(material: String) -> Texture2D:
	var key := material.strip_edges().to_lower()
	if not SPRITE_PATHS.has(key):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[key]))
