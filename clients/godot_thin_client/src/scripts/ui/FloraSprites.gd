extends RefCounted
class_name FloraSprites

## Bundled PNG art for individual FLORA SPECIES — the sixth art family behind `IconSprites`, and the
## per-plant tier above `CropRoleSprites`' three role marks (issue #339).
##
## **COVERAGE IS 32 OF 33, AND THE ONE GAP IS DELIBERATE AND PERMANENT.** `hay_grass` is the roster's
## only `fodder` species, so the fodder ROLE mark — a bound bale — already names it exactly and
## uniquely, and a standing-grass silhouette would collide with the grass spikes it shares a basket
## with; `assets/icons/icon_prompts.txt` ships **32 prompts for 33 species** and says so. Every other
## species answers a real path / texture here, and `hay_grass` is the row the fallback below is now
## aimed at — the one miss that cannot be closed by drawing another PNG.
##
## The wiring shipped BEFORE any art existed, when every call answered `""` / `null`. That is why a
## species' art was a file drop rather than a code change when it landed, and why the fallback is an
## EXERCISED path rather than an assumed one — but "no art anywhere" is history, not the state.
##
## WHY A PER-SPECIES TIER AT ALL: the roster's plants collapse under the emoji palette — every grain
## is 🌾, every nut 🌰, every berry 🫐, every mushroom 🍄 — so a basket row could not tell Wild Emmer
## from Wild Barley by its mark. The ROLE marks do carry a real distinction (food for people / feed
## for animals / a material for the bench) and keep it; art is what the palette cannot supply.
##
## **THE FILENAME IS THE KEY — there is no `SPRITE_PATHS` TABLE, and that is the point.**
## `FloraShareInfo.species` is the sim's own stable key (`wild_emmer`, `kelp`, `rock_tripe`), so
## `wild_emmer` resolves to `SPRITE_DIR + "wild_emmer.png"` by composition. This is the
## `StageSprites` / `CropRoleSprites` case — *the server sends the key, so there is no client-side
## resolver* — taken one step further: those two still keep a table mapping key → file, and this one
## does not, so dropping a PNG into `assets/icons/flora/` wires a species up with **no client edit at
## all**.
##
## **AND THAT IS WHY THERE IS NO `cargo xtask flora-icon-guard` TWIN OF THE FAUNA GUARD.** That guard
## exists because `FaunaSprites` resolves a key out of a free-text DISPLAY NAME and then looks it up
## in a hand-written table, so the table can silently fall out of step with `fauna_config.json` — a
## roster species with no key at all draws an OS emoji and no client-side check can see it. Flora
## resolves nothing and holds no table: the only way a species can miss is that its file is absent,
## which is a fact about the art rather than a drift between two lists.
##
## **THE KEY IS WIRE DATA THAT BECOMES A `res://` PATH, so it is charset-guarded** (`_is_valid_key`).
## Flora keys are snake_case by construction and we expect none to be malformed; the guard is about
## not TRUSTING the wire with path composition, not about a key we think will arrive broken. Anything
## outside `[a-z0-9_]`, and the empty key, answers `""` and composes no path at all.
##
## **TWO ACCESSORS, and that is what makes this family different from all five siblings: it has two
## HOST KINDS.** `CropRoleSprites`' doc already states the rule that the MECHANISM IS CHOSEN BY THE
## HOST WIDGET; this family is simply the first to have both hosts at once.
##   • `path_for` — the tile card's basket rows are a `RichTextLabel` and address art by `res://`
##     path inside `[img]` BBCode (`DetailFormat.flora_composition_lines`).
##   • `texture_for` — the compose sheet's crop-picker rows are `Button`s and carry art on their own
##     `icon` property (`DrawerComposeController`, the `BandPanelController._build_quarry_row`
##     precedent).
## Both go through `IconSprites.texture_for`, so this family shares the one texture cache and the
## `load()`-not-`preload()` degradation with the other five rather than carrying a sixth copy of them
## — and `path_for` hands a path back **only once that load has SUCCEEDED** (the
## `CropRoleSprites._path_if_loadable` rule), so a bad path can never put a broken-image box in the
## middle of a text row.
##
## **THE ONE BEHAVIOUR IT DOES NOT SHARE IS THE WARNING, and that follows from coverage being
## deliberately INCOMPLETE.** For the other five an absent path is a DEFECT and the warning is how it
## surfaces; here a row with no art falls back to its role mark AS INTENDED, so the warning would fire
## for a state that is correct. **The magnitude has collapsed — one species now, not the 33 it was
## before any art existed (16 warnings in one `ui_preview` run, measured) — and the argument is
## unchanged by that, because it is about KIND rather than volume:** a family that warns once per
## session for a row that is behaving teaches the reader to skip its messages, which is what buries
## the real one.
## It would also say the wrong thing: the shared message names *the emoji marker*, and this family
## falls back to the CROP-ROLE mark.
##
## So the shared call is made quietly (`warn = false`) and this file warns on the ONE case that is a
## defect rather than a state: **a source PNG sitting in the directory with no imported resource
## behind it**, which is the missing-`.png.import`-sidecar failure `cargo xtask fauna-icon-guard`
## exists to catch on the fauna side — art that works in the author's checkout and silently draws
## the fallback in every other one. `_note_absent_once` is that check, and it is deliberately the
## only thing left that can speak: "no file at all" is `hay_grass`'s permanent, correct answer.
##
## Static-only by design, same reasoning as `FoodIcons` and the five sibling tables.

## Where a species' art lives. **NO directory is created for it here**: `IconSprites.texture_for`
## guards on `ResourceLoader.exists`, and git cannot track an empty directory anyway — the folder
## arrived with the first PNG.
const SPRITE_DIR := "res://assets/icons/flora/"

const SPRITE_EXTENSION := ".png"

## The characters a species key may contain. Written out rather than matched with a `RegEx` so the
## permitted set is readable at the point of the rule, and so this stays a pure static lookup with no
## compiled-object state to build lazily.
const KEY_ALPHABET := "abcdefghijklmnopqrstuvwxyz0123456789_"

## The scratch sprite directory when a harness/test set one, else the shipped `SPRITE_DIR`. Modelled
## on `ClientSettings.config_path_override` — static, and it isolates the files a test sees from the
## ones the player gets.
##
## **IT WAS ADDED BECAUSE COVERAGE WAS ZERO.** With no flora art on disk the SPECIES tier of the
## basket row's precedence chain was unreachable, so it would have shipped completely unexercised and
## the day the first PNG landed would have been the first time that branch ever ran.
##
## **IT STILL EARNS ITS KEEP AT 32 OF 33**, for a reason that does not expire: it is the only way to
## drive the precedence chain against a directory whose CONTENTS THE HARNESS CONTROLS, so a claim
## about the tier keeps meaning the same thing as art is drawn, renamed or retired. `ui_preview`
## points it at a directory that ships PNGs under keys it chose (`CropRoleSprites.SPRITE_DIR`), drives
## the real producer through it, then clears it — see `chapters/land_readouts.gd`.
static var sprite_dir_override := ""

## Paths `_note_absent_once` has already answered for, so the sidecar report costs one
## `FileAccess.file_exists` per path per session rather than one per basket row per render. Keyed by
## the composed path, so the harness override and the shipped directory cannot mask each other.
static var _reported: Dictionary = {}

## Bundled art path for a flora SPECIES key, or `""` when there is no art for it (the caller then
## falls back to the crop-ROLE mark — `FoodIcons.for_flora_species` states that chain).
##
## Answers `""` for the one species that ships no art of its own (`hay_grass`), for any species whose
## PNG has yet to be drawn, and for a key this family will not compose a path from. The path is handed
## back only once the texture has actually loaded, so a caller embedding it in `[img]` BBCode cannot
## render a broken-image box.
static func path_for(species: String) -> String:
	var path := _path_for_key(species)
	if path == "":
		return ""
	if _texture_at(path) == null:
		return ""
	return path


## Bundled art for a flora SPECIES key as a `Texture2D`, or `null` when there is no art for it — the
## `Button`-hosted twin of `path_for`, for a widget that carries art on its own `icon` property
## rather than in BBCode.
static func texture_for(species: String) -> Texture2D:
	var path := _path_for_key(species)
	if path == "":
		return null
	return _texture_at(path)


## The shared cache's answer for `path`, asked QUIETLY — plus this family's own one-shot report of
## the only miss that is a defect. See the warning note in the header for why the shared warning is
## suppressed rather than inherited.
static func _texture_at(path: String) -> Texture2D:
	var tex := IconSprites.texture_for(path, false)
	if tex == null:
		_note_absent_once(path)
	return tex


## Warn ONCE per path, and only when a source PNG is sitting there that Godot did not import — the
## missing-`.png.import`-sidecar case, which renders correctly for whoever generated the art and
## falls back to the role mark in every other checkout.
##
## **A path with no file behind it says nothing** — that is `hay_grass`'s permanent state and the
## expected state of any species whose art has yet to be drawn.
##
## `FileAccess.file_exists` on a `res://` source is a DEV-RUN check: an exported build ships the
## imported `.ctex` and not the PNG, so this is silent there. That is the right scope — the failure
## it names is committed by a developer and is meant to be caught before an export exists.
static func _note_absent_once(path: String) -> void:
	if _reported.has(path):
		return
	_reported[path] = true
	if not FileAccess.file_exists(path):
		return
	push_warning(
		"FloraSprites: %s is present but has no imported resource — check that its .png.import "
		% path
		+ "sidecar is committed. The basket row falls back to its crop-role mark.")


## The composed `res://` path for a species key, or `""` when the key is not one we will build a path
## out of. Nothing is normalized here — a key is the sim's own identifier and arrives snake_case, so
## lower-casing or stripping an unexpected one would INVENT a key rather than reject it.
static func _path_for_key(species: String) -> String:
	if not _is_valid_key(species):
		return ""
	return _sprite_dir() + species + SPRITE_EXTENSION


## The directory art is resolved out of: the harness override where one is set, else the shipped one.
static func _sprite_dir() -> String:
	if sprite_dir_override != "":
		return sprite_dir_override
	return SPRITE_DIR


## Is this a key we are willing to compose a resource path from — non-empty and `[a-z0-9_]` only?
## The traversal cases (`..`, `/`) fail on the alphabet, so there is no separate path check to keep
## in step with this one.
static func _is_valid_key(species: String) -> bool:
	if species == "":
		return false
	for index in species.length():
		if KEY_ALPHABET.find(species[index]) < 0:
			return false
	return true
