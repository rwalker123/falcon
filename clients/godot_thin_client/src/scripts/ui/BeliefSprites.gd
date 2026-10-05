extends RefCounted
class_name BeliefSprites

## Bundled PNG art for the BELIEF PLACE MAP MARKER — the urn a selected band's belief anchor (where its
## dead lie) wears on the map, drawn by `BandOverlayRenderer._draw_band_ancestors` centred on a dark
## backing disc ringed in `HudStyle.BELIEF`. The text glyph `BandOverlayRenderer.ANCESTORS_GLYPH` (⚱)
## is the fallback, the contract every art family has with its emoji.
##
## **THE MAP-MARKER HOUSE STYLE, NOT `hud/`'s** — thick charcoal outline, front-on, keyed 256px
## (`assets/icons/icon_prompts.txt` → BELIEF PLACE MARKER), because it draws over the map's terrain
## at marker size, exactly as `ExpeditionSprites`' party markers do.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state.

const SPRITE_DIR := "res://assets/icons/belief/"
## The belief place's one mark.
const URN_PATH := SPRITE_DIR + "urn.png"

## The urn sprite, or `null` when it fails to load — the caller then draws the ⚱ glyph.
##
## Takes `IconSprites.texture_for`'s DEFAULT `warn: true`: this family's coverage is complete for what
## it declares — the one urn has a committed, imported PNG behind it — so a failed load is a DEFECT
## and must surface.
static func urn() -> Texture2D:
	return IconSprites.texture_for(URN_PATH)
