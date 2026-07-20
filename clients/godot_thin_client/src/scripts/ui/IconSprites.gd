extends RefCounted
class_name IconSprites

## Shared texture cache behind the bundled map-marker art (`FaunaSprites`, `SiteSprites`).
##
## Both sprite tables need the identical three behaviours — a lazily populated path→texture cache,
## a `load()` (not `preload()`) so a missing file degrades to the emoji path instead of breaking
## scene load, and a `null` result the marker renderers already know how to fall back from. That
## is one implementation, here, rather than a copy per art family: a new art family is then just a
## `SPRITE_PATHS` table plus a key resolver.
##
## Static-only by design (same reasoning as `FoodIcons`): a pure lookup with no node state, called
## from the map draw loop.

# Path → Texture2D, lazily populated on first use of each path. A missing/failed path caches
# `null` so the load is attempted (and warned about) exactly once, not once per marker per frame.
static var _textures: Dictionary = {}

## Bundled texture at `path`, or `null` when it is missing or fails to load (the caller then falls
## back to its emoji glyph). Warns once per bad path, never per frame.
static func texture_for(path: String) -> Texture2D:
	if path == "":
		return null
	if _textures.has(path):
		return _textures[path]
	var tex: Texture2D = null
	if ResourceLoader.exists(path):
		tex = load(path) as Texture2D
	if tex == null:
		push_warning("IconSprites: no texture at %s; falling back to the emoji marker." % path)
	_textures[path] = tex
	return tex
