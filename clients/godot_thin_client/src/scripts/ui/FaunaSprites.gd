extends RefCounted
class_name FaunaSprites

## Bundled PNG art for map herd markers — the sprite half of `FoodIcons`' herd vocabulary.
##
## WHY sprites at all: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji
## font decides what a species looks like. A rabbit is white on macOS and pink on Windows, and
## both go blobby at marker size (10–41 px; `MapView.SECONDARY_ICON_SIZE_FACTOR` × hex radius,
## floored at `SECONDARY_ICON_MIN_SIZE`). A bundled silhouette is ours and identical everywhere.
##
## Coverage is COMPLETE, and `cargo xtask fauna-icon-guard` is what makes that a checked fact
## rather than a claim: it reads the sim's `fauna_config.json` and fails if any species' display
## name does not resolve, through `species_key_for`, to a key here whose PNG exists on disk.
##
## ⛔ THE GUARD CHECKS REACHABILITY, NOT DISTINCTNESS, and the difference is the whole of #439.
## Every species reaching SOME file is a weaker property than every species reaching its OWN
## file, and the guard only asserts the first — so three pairs of species shared one marker each
## while it printed `20 species -> 20 sprites` and exited 0. **That line counts LOOKUPS, not
## files.** Nothing in the repo runs the distinctness check; it is done by hand (resolve every
## `display_name` through this table, count distinct PNGs — currently 20 species -> 20 distinct
## sprites), and it has to be redone whenever a species or an alias is added. Do not read the
## answer off the guard's output.
## The `null` fallback below is still load-bearing — it catches a herd label naming a species the
## client does not know (`species_key_for` returns "") and the `HERD_DEFAULT` case, both of which
## still render the emoji renderer's glyph. Dropping a new PNG in `assets/icons/fauna/` and adding
## its key here is the whole migration step for a species.
##
## THE GUARD EXISTS BECAUSE THIS COMMENT WAS FALSE FOR AS LONG AS IT HAS EXISTED (issue #439).
## "Every key maps to bundled art" was true and was the WRONG QUESTION: **Steppe Runners and Marsh
## Grazers had no key here at all**, so `species_key_for` answered "" and both drew the 🦬
## `HERD_DEFAULT` emoji — an OS glyph, on a live map, for two of the roster's twenty species. A
## check over this table's own keys cannot see a species the table has never heard of, which is
## precisely the case it needed to catch.
##
## Nor could the harness catch it. `map_preview`'s `FAUNA_SPRITE_ROSTER` is a hand-written list on
## THIS side of the wire, so the coverage frame enumerates the client's own vocabulary and a species
## missing from it is invisible by construction — the same blind spot that let four cervids share
## one marker until they were finally stood next to each other. **A coverage claim has to be checked
## against the OTHER side's roster**, which is the one thing neither this table nor that frame was
## doing, and is all the guard does. The tell that surfaced it was a player noticing the odd marker
## FACED LEFT: every bundled sprite obeys `icon_prompts.txt`'s "side profile facing right" clause
## and the OS emoji does not.
##
## Static-only by design (same reasoning as `ServerPortsFile.gd`): a pure lookup with no node
## state, called from the map draw loop.

## Species KEY (a `FoodIcons.HERD_SPECIES` key) → bundled texture path. Keys that share art alias
## the same file, exactly as HERD_SPECIES already aliases emoji — `bison`/`buffalo` both read as the
## aurochs, `oxen` as cattle, `caribou` as the reindeer.
##
## AN ALIAS IS ONLY LEGITIMATE WHEN NO ROSTER SPECIES STANDS BEHIND IT (issue #439). Four keys here
## once pointed at `deer.png` — `deer`, `elk`, `reindeer` and `gazelle` — and `fauna_config.json`
## ships a distinct species under each: Red Deer, Wild Elk, Wild Reindeer and Desert Gazelle. Four
## species, one marker, so the map could not tell an elk herd from a deer herd; they now carry their
## own art. `caribou` is still an alias, and correctly so: it is a second English word for the animal
## `reindeer` already names, with no roster entry of its own. Before aliasing a NEW key, check
## `fauna_config.json` for a species behind it — if there is one, it needs its own PNG.
##
## THE CERVIDS WERE NOT THE LAST OF IT. Three more keys were still aliased onto another species'
## art long after #439 closed: `ibex` -> goat.png beside Crag Goats, `grouse` -> fowl.png beside
## Wild Fowl, `hare` -> rabbit.png beside the Rabbit Warren. Alpine Ibex, Forest Grouse and Snow
## Hare Warren are all roster species, so all three were the #439 defect exactly, and they now
## carry `ibex.png`, `grouse.png` and `snow_hare.png`. The remaining aliases — `bison`,
## `buffalo`, `oxen`, `caribou` — are the honest kind: no `fauna_config.json` entry is named any
## of them.
const SPRITE_DIR := "res://assets/icons/fauna/"
const SPRITE_PATHS := {
	"rabbit": SPRITE_DIR + "rabbit.png",
	"hare": SPRITE_DIR + "snow_hare.png",
	"catfish": SPRITE_DIR + "catfish.png",
	"deer": SPRITE_DIR + "deer.png",
	"elk": SPRITE_DIR + "elk.png",
	"reindeer": SPRITE_DIR + "reindeer.png",
	"caribou": SPRITE_DIR + "reindeer.png",
	"gazelle": SPRITE_DIR + "gazelle.png",
	"boar": SPRITE_DIR + "boar.png",
	"wolf": SPRITE_DIR + "wolf.png",
	"mammoth": SPRITE_DIR + "mammoth.png",
	"aurochs": SPRITE_DIR + "aurochs.png",
	"bison": SPRITE_DIR + "aurochs.png",
	"buffalo": SPRITE_DIR + "aurochs.png",
	"cattle": SPRITE_DIR + "cattle.png",
	"oxen": SPRITE_DIR + "cattle.png",
	"goat": SPRITE_DIR + "goat.png",
	"ibex": SPRITE_DIR + "ibex.png",
	"horse": SPRITE_DIR + "horse.png",
	"sheep": SPRITE_DIR + "sheep.png",
	"seal": SPRITE_DIR + "seal.png",
	"fowl": SPRITE_DIR + "fowl.png",
	"grouse": SPRITE_DIR + "grouse.png",
	"steppe runner": SPRITE_DIR + "steppe_runner.png",
	"marsh grazer": SPRITE_DIR + "marsh_grazer.png",
}

## Bundled sprite for a migratory herd, or `null` when this species has no art yet (the caller
## then falls back to `FoodIcons.for_herd`'s emoji). The load-and-cache behaviour lives in
## `IconSprites` — shared with `SiteSprites` — so this stays a pure key→path table.
static func for_herd(label: String) -> Texture2D:
	var key := FoodIcons.species_key_for(label)
	if key == "" or not SPRITE_PATHS.has(key):
		return null
	return IconSprites.texture_for(String(SPRITE_PATHS[key]))
