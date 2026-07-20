extends RefCounted
class_name FaunaSprites

## Bundled PNG art for map herd markers — the sprite half of `FoodIcons`' herd vocabulary.
##
## WHY sprites at all: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji
## font decides what a species looks like. A rabbit is white on macOS and pink on Windows, and
## both go blobby at marker size (10–41 px; `MapView.SECONDARY_ICON_SIZE_FACTOR` × hex radius,
## floored at `SECONDARY_ICON_MIN_SIZE`). A bundled silhouette is ours and identical everywhere.
##
## Coverage is now COMPLETE: every `FoodIcons.HERD_SPECIES` key maps to bundled art, so no herd
## species in the game draws an OS emoji today. The `null` fallback below is still load-bearing —
## it catches a herd label naming a species the client does not know (`species_key_for` returns
## "") and the `HERD_DEFAULT` case, both of which still render the emoji renderer's glyph.
## Dropping a new PNG in `assets/icons/fauna/` and adding its key here is the whole migration
## step for a species.
##
## Static-only by design (same reasoning as `ServerPortsFile.gd`): a pure lookup with no node
## state, called from the map draw loop.

## Species KEY (a `FoodIcons.HERD_SPECIES` key) → bundled texture path. Species that share art
## alias the same file, exactly as HERD_SPECIES already aliases emoji — reindeer/caribou/elk all
## read as the deer silhouette.
const SPRITE_DIR := "res://assets/icons/fauna/"
const SPRITE_PATHS := {
	"rabbit": SPRITE_DIR + "rabbit.png",
	"deer": SPRITE_DIR + "deer.png",
	"reindeer": SPRITE_DIR + "deer.png",
	"caribou": SPRITE_DIR + "deer.png",
	"elk": SPRITE_DIR + "deer.png",
	"boar": SPRITE_DIR + "boar.png",
	"mammoth": SPRITE_DIR + "mammoth.png",
	"aurochs": SPRITE_DIR + "aurochs.png",
	"bison": SPRITE_DIR + "aurochs.png",
	"buffalo": SPRITE_DIR + "aurochs.png",
	"cattle": SPRITE_DIR + "cattle.png",
	"oxen": SPRITE_DIR + "cattle.png",
	"goat": SPRITE_DIR + "goat.png",
	"ibex": SPRITE_DIR + "goat.png",
	"horse": SPRITE_DIR + "horse.png",
	"sheep": SPRITE_DIR + "sheep.png",
	"fowl": SPRITE_DIR + "fowl.png",
}

# Path → Texture2D, lazily populated on first use of each species. A missing/failed path caches
# `null` so the load is attempted (and warned about) exactly once, not once per marker per frame.
static var _textures: Dictionary = {}

## Bundled sprite for a migratory herd, or `null` when this species has no art yet (the caller
## then falls back to `FoodIcons.for_herd`'s emoji). Uses `load()` rather than `preload()` so a
## missing file degrades to the emoji path instead of breaking scene load.
static func for_herd(label: String) -> Texture2D:
	var key := FoodIcons.species_key_for(label)
	if key == "" or not SPRITE_PATHS.has(key):
		return null
	var path := String(SPRITE_PATHS[key])
	if _textures.has(path):
		return _textures[path]
	var tex: Texture2D = null
	if ResourceLoader.exists(path):
		tex = load(path) as Texture2D
	if tex == null:
		push_warning("FaunaSprites: no texture at %s; falling back to the emoji herd marker." % path)
	_textures[path] = tex
	return tex
