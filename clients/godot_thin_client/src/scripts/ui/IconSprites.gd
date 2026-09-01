extends RefCounted
class_name IconSprites

## Shared texture cache behind ALL SEVEN bundled-art families — `FaunaSprites`, `SiteSprites`,
## `WonderSprites` and `StageSprites` (map markers), `CropRoleSprites` (the tile card's basket-row
## role marks), `FloraSprites` (that row's per-species tier) and `HudSprites` (marks on the HUD's own
## chrome — a button face, a popover row).
##
## Every one of them needs the identical three behaviours — a lazily populated path→texture cache,
## a `load()` (not `preload()`) so a missing file degrades to its family's fallback instead of
## breaking scene load, and a `null` result that family already knows how to fall back from. That
## is one implementation, here, rather than a copy per art family: a new art family is then just a
## `SPRITE_PATHS` table plus a key resolver (or, in `FloraSprites`' case, neither — the filename is
## the key).
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state, called
## from the map draw loop.

# Path → Texture2D, lazily populated on first use of each path. A missing/failed path caches
# `null` so the load is attempted (and warned about) exactly once, not once per marker per frame.
static var _textures: Dictionary = {}

## Bundled texture at `path`, or `null` when it is missing or fails to load — the caller then falls
## back to **whatever ITS family's fallback is**: the emoji glyph for the four marker tables,
## `CropRoleSprites` and `HudSprites`, the crop-ROLE mark for `FloraSprites`. Warns once per bad path,
## never per frame; the warning's own wording is the six's, which is half of why `warn` exists.
##
## **`warn` IS FOR A FAMILY WHOSE COVERAGE IS DELIBERATELY INCOMPLETE, and `FloraSprites` is the
## only one.** For the other six an absent path is a DEFECT — coverage is complete or
## guarded, so the load failing means art went missing — and the warning is how that surfaces. Flora
## art is drawn species by species and a row with none falls back to its crop-role mark BY DESIGN, so
## warning there would fire up to once per roster species per session for the expected state, and
## the noise is what would hide a real one. The caching is unaffected either way: a quiet miss still
## caches `null`, so the load is still attempted exactly once.
##
## It does NOT mean "fail quietly" — `FloraSprites` warns itself, on the one case that IS a defect
## (a source PNG present with no imported resource behind it), with a message naming the fallback
## that family actually takes. The wording here — *the emoji marker* — is the other six's.
static func texture_for(path: String, warn: bool = true) -> Texture2D:
	if path == "":
		return null
	if _textures.has(path):
		return _textures[path]
	var tex: Texture2D = null
	if ResourceLoader.exists(path):
		tex = load(path) as Texture2D
	if tex == null and warn:
		push_warning("IconSprites: no texture at %s; falling back to the emoji marker." % path)
	_textures[path] = tex
	return tex
