extends RefCounted

## The tile card: food, climate, pasture and rivers.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 119

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")
## The generic node/text finders, and the REAL command formatter — the road ladder's press is asserted
## through `Main.format_improvement`, the pure static `Main._on_hud_improvement` dispatches to.
const Q := preload("res://tools/ui_preview/node_query.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## …and real pointer input, for the ladder's rung press. **A rung is pressed through the viewport and
## never by `pressed.emit()`**: a faked signal passes on a button the engine would refuse to route to,
## and what fix 3 changed is what the engine DOES with that press.
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# Hex-edge river fixtures. The wire mask is 12 bits, 2 bits per odd-r direction, in the SIM's
# direction order (clockwise from E: 0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE) — built here with the
# same RiverEdges vocabulary the UI decodes with, so the fixture can't drift from the contract.
const RIVER_MASK_NONE := 0

# Minor on E + SE — one class, so one row.
const RIVER_MASK_SINGLE_CLASS := (
	(RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 0))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 1))
)

# Major on NE + NW, Minor on SW — the two-class case: "Major River: NE, NW" then "Minor River: SW".
const RIVER_MASK_TWO_CLASS := (
	(RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 5))
	| (RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 4))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 2))
)

## ---- THE TILE CARD'S TWO FOOD-WEB ROWS — the three claims a frame cannot carry ----------------
##
## A picture shows that the card LOOKS right; none of these can be read off one. They run against the
## REAL line producer (`SubjectDrawerController._tile_terrain_lines`), never against a re-derivation
## here, so a regression in the producer is what fails them.
##
## 1. THE BASKET DECOMPOSES THE STOCK. The indented rows' biomasses must sum to the `Foraging` row's
##    own ceiling — the whole reason each row states an absolute beside its share. Independent
##    rounding does NOT sum (78 + 64 + 64 = 206 against this fixture's 205), so this is a real test of
##    the remainder fold and not of arithmetic that could not fail.
## 2. AN UNSTATED ROLE RENDERS NO ICON. `""` means the roster does not know this species, not
##    "staple", so the row must carry none of the three role marks while its neighbours carry theirs.
## 3. THE TWO ROWS ARE ADJACENT, FORAGING FIRST. Adjacency is what stops the two webs being confused,
##    and it is invisible to any assertion that merely finds both rows present.
func _assert_food_layer_rows() -> void:
	var lines = h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		TileFx.three_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	var forage_index = Readout.detail_row_index(lines, HudFloraVocab.FORAGING_KEY)
	var graze_index = Readout.detail_row_index(lines, HudFloraVocab.GRAZING_KEY)
	h._assert_hud("the tile card states a Foraging row and a Grazing row",
		forage_index >= 0 and graze_index >= 0)
	# The basket rows sit BETWEEN them, so "adjacent" means the animal row follows the human block —
	# the human layer is never split by it, which is exactly what used to happen.
	var basket = ForageFx.flora_basket_rows(lines)
	h._assert_hud("…Foraging leads, and Grazing follows its basket with nothing else between",
		forage_index >= 0 and graze_index == forage_index + basket.size() + 1)
	var basket_total := 0
	for row in basket:
		basket_total += _flora_row_biomass(row)
	# **AGAINST THE STANDING STOCK, never the ceiling.** These rows say what the `150 / 205` above
	# them is MADE OF; summing to 205 would decompose a full patch nobody is looking at, and the card
	# would then hold two numbers disagreeing about which stand is under discussion. The fixture is
	# drawn down precisely so this assertion can tell the two apart.
	h._assert_hud("…and the basket's biomasses sum to the STANDING Foraging stock (%d of %d)" % [
			basket_total, int(ForageFx.THREE_ROLE_STOCK)],
		basket.size() == 3 and basket_total == int(ForageFx.THREE_ROLE_STOCK))
	# The SAME box size the drawer just rendered with, read from the same place it reads it — a
	# literal here would pass against a drawer that had stopped tracking the label's font size.
	var role_px = h._hud._drawer._role_icon_px()
	var unstated = ForageFx.flora_basket_rows(h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		_unstated_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)))
	var species_rows := 0
	var hay_has_icon := true
	var hay_holds_slot := false
	for row in unstated:
		if _flora_row_has_species_icon(row):
			species_rows += 1
		if row.contains("Hay Grass"):
			hay_has_icon = _flora_row_has_role_icon(row, role_px) or _flora_row_has_species_icon(row)
			hay_holds_slot = row.begins_with(
				DetailFormat.MORALE_BREAKDOWN_INDENT + _expected_blank_slot(role_px) + " ")
	h._assert_hud("a species with no art whose role the wire leaves UNSTATED renders NO mark at all",
		unstated.size() == 3 and not hay_has_icon)
	# **THE OTHER TWO ROWS ARE THE OTHER TIER, and naming it is what stops this going vacuous.** Before
	# flora art existed this read "the two roles the wire DOES state still wear theirs"; those two
	# species have art now, so the SPECIES tier outranks the role and they lead with `flora/`. A claim
	# that merely found "some mark" on them would pass with either tier broken.
	h._assert_hud("…while the two species that DO have art lead with the flora mark (%d of 2)"
			% species_rows,
		species_rows == 2)
	# **AND THE UNSTATED ROW STILL HOLDS ITS WIDTH** (#463). "No icon" and "no slot" are one glance
	# apart in a PNG and one character apart in the producer, and the difference is whether one
	# untagged plant shifts every name in the list out of column.
	#
	# **STATED AS A PREFIX, NOT A `contains`, AND THAT IS THE WHOLE ASSERTION.** The first cut asked
	# `row.contains(FoodIcons.crop_role_spacer(role_px))` and PASSED WITH THE SPACER FILE DELETED —
	# that helper answers `""` when there is no art, and `contains("")` is true of every string. An
	# empty needle is the vacuity trap this harness's own rule names ("a needle must be one no other
	# copy can satisfy"), and it is easiest to walk into with a helper that degrades to `""`.
	# `_expected_blank_slot` therefore never returns empty, and the claim is POSITIONAL: the slot
	# sits between the indent and the space before the name. That is what makes deleting the spacer
	# a MODE CHANGE this still passes (the text spacer holds the column just as honestly) while a
	# producer that emitted no slot at all — the actual regression — fails.
	h._assert_hud("…and the UNSTATED row still holds the slot's width", hay_holds_slot)
	# **THE ART IS LIVE, NOT THE EMOJI FALLBACK.** Every assertion above passes unchanged if every
	# PNG fails to load, because `for_crop_role` then answers the emoji and the needles fall back with
	# it — the whole point of the change would be gone with nothing red. So this one names the
	# BUNDLED path directly: it is the only claim here that can tell art from fallback, and the only
	# one a missing/misnamed file breaks. (`role_px > 0` is asserted with it because a zero box would
	# route every row to the emoji for a completely different reason and read identically.)
	var role_art_rows := 0
	var species_art_rows := 0
	for row in ForageFx.flora_basket_rows(lines):
		if row.contains(CropRoleSprites.SPRITE_DIR):
			role_art_rows += 1
		if _flora_row_has_species_icon(row):
			species_art_rows += 1
	# **BOTH TIERS ON ONE TILE, COUNTED SEPARATELY — that split IS the assertion** (issue #339). This
	# read `3 of 3` from `CropRoleSprites.SPRITE_DIR` while flora coverage was zero, and flipped the
	# day art landed, exactly as the liveness precondition was written to. It is re-aimed rather than
	# relaxed: `wild_tubers` and `cotton` have species art and must lead with it, `hay_grass` has NONE
	# — deliberately and permanently, `icon_prompts.txt` records why — and must still lead with its
	# fodder ROLE mark. A single "every row carries bundled art" count would pass with the whole
	# species tier reverted, every row falling back to a role mark that is also bundled art.
	h._assert_hud("the three-role tile splits across BOTH art tiers: %d species + %d role, box %dpx" % [
			species_art_rows, role_art_rows, role_px],
		role_px > 0 and species_art_rows == 2 and role_art_rows == 1)

## ---- THE BASKET ROW'S FOUR-STEP PRECEDENCE: SPECIES ART → ROLE MARK → SPACER → TEXT (issue #339) --
##
## DRIVEN AND PNG-LESS BY CONSTRUCTION — and that is a property of THIS BLOCK, not of the family.
## `FloraSprites` covers 32 of the roster's 33 species, and those 32 icons MOVED 100 FRAMES when they
## landed (`harness-ui-preview.md` lists them). Nothing below saves a frame because everything below
## drives the producer directly: what has to be pinned is that the tier is REAL, that it OUTRANKS the
## role rather than coinciding with it, that the degradation is the shipped behaviour, and that a
## wire key cannot compose a path it should not — none of which a picture can state.
##
## **`FloraSprites.sprite_dir_override` IS WHAT MAKES THOSE CLAIMS INDEPENDENT OF WHICH SPECIES
## HAPPEN TO SHIP ART.** It points the species tier at `CropRoleSprites.SPRITE_DIR`, a directory
## whose contents this block chooses, so a composition keyed `staple` resolves through the SPECIES
## tier to that directory's `staple.png` — and goes on doing so as flora art is drawn, renamed or
## retired. The two tiers are therefore exercised deterministically here, and the claims that must
## be about the SHIPPED directory are made separately below, aimed at the two species that cannot
## change: `wild_emmer`, which has art, and `hay_grass`, which permanently has none.
func _assert_flora_species_precedence() -> void:
	# The SAME box the drawer renders with, read from where it reads it — a literal would pass
	# against a drawer that had stopped tracking its label's font size.
	var role_px = h._hud._drawer._role_icon_px()
	FloraSprites.sprite_dir_override = CropRoleSprites.SPRITE_DIR
	var species_mark := FoodIcons.for_flora_species(SPECIES_TIER_SPECIES, role_px)
	var role_mark := FoodIcons.for_crop_role(SPECIES_TIER_ROLE, role_px)
	var species_rows := ForageFx.flora_basket_rows(DetailFormat.flora_composition_lines(
		_species_tier_composition(), "", 0.0, role_px))
	# **THE PRECONDITION IS HALF THE CLAIM.** Both marks must resolve, and to DIFFERENT files: a
	# claim that merely finds "some art" on the row passes with the whole species tier reverted,
	# because this row's role has bundled art of its own.
	h._assert_hud("the override reaches BOTH tiers, at different files (`%s` vs `%s`)" % [
			SPECIES_TIER_SPECIES, SPECIES_TIER_ROLE],
		role_px > 0 and species_mark != "" and role_mark != "" and species_mark != role_mark)
	# POSITIONAL, and naming the species-resolved path specifically — the row leads with the mark,
	# between the indent and the space before the name (`FLORA_COMPOSITION_SUBLINE_FORMAT`).
	h._assert_hud("a species with ART leads its basket row with the SPECIES mark",
		species_rows.size() == 1 and species_rows[0].begins_with(
			DetailFormat.MORALE_BREAKDOWN_INDENT + species_mark + " "))
	# …and the two never render TOGETHER: species art REPLACES the role mark, it does not sit beside
	# it. Without this the row could carry both and still satisfy the claim above.
	h._assert_hud("…and the ROLE mark is nowhere on that row — species REPLACES it, never joins it",
		species_rows.size() == 1 and not species_rows[0].contains(role_mark))
	# **THE CHARSET GUARD, ASKED WHERE IT CAN ACTUALLY FAIL.** It is asked under the OVERRIDE because
	# the two keys below compose a path that really loads there (measured: `ResourceLoader.exists`
	# answers true for the `..` form and, on a case-insensitive filesystem, for the capitalised one),
	# so an unguarded resolve would hand back a real PNG rather than the `""` an absent file produces
	# for free. That was the whole claim while flora coverage was zero and every key answered `""` at
	# the shipped directory — the empty-needle vacuity this harness's own rules name, which passes
	# with `_is_valid_key` deleted. **With 32 PNGs on disk it is no longer only the override that
	# makes this live**: `Wild_Emmer` composes a path a case-insensitive filesystem resolves to the
	# shipped `wild_emmer.png`, so the guard now bites in the shipped directory too. Asking it here
	# keeps the claim true on a case-SENSITIVE filesystem and independent of the art roster. The
	# traversal half is the portable one; the capitalised half is the display-name shape a careless
	# wire could carry.
	h._assert_hud("a key outside `[a-z0-9_]` is REFUSED even where the composed path would load",
		FloraSprites.path_for(TRAVERSAL_KEY) == "" and FloraSprites.path_for(CAPITALISED_KEY) == "")
	FloraSprites.sprite_dir_override = ""
	# **THE LIVENESS PRECONDITION, AND IT HAS ALREADY FIRED ONCE.** While flora coverage was zero this
	# read "a real species key answers NO PATH", and the day the 32 icons landed it failed loudly —
	# which is exactly what it was written to do, rather than let the fixtures quietly stop being
	# evidence. It is now the positive half: a shipped species must RESOLVE, in the shipped directory,
	# so a PNG that goes missing or is misnamed fails here instead of degrading to a role mark that
	# looks deliberate.
	h._assert_hud("a shipped species key resolves in the SHIPPED directory (`%s`)"
			% SHIPPED_ART_SPECIES,
		FloraSprites.path_for(SHIPPED_ART_SPECIES).begins_with(FloraSprites.SPRITE_DIR))
	# …and the DEGRADATION beside it, on the one species that is deliberately never given art. Both
	# halves are needed: the positive alone passes on a client that resolves everything, the negative
	# alone on one that resolves nothing.
	h._assert_hud("…while the species with no art of its own answers NO PATH (`%s`)"
			% DEGRADED_TIER_SPECIES,
		FloraSprites.path_for(DEGRADED_TIER_SPECIES) == "")
	var degraded_rows := ForageFx.flora_basket_rows(DetailFormat.flora_composition_lines(
		_degraded_tier_composition(), "", 0.0, role_px))
	var degraded_mark := FoodIcons.for_crop_role(DEGRADED_TIER_ROLE, role_px)
	h._assert_hud("…so its row falls through to the ROLE mark in `CropRoleSprites.SPRITE_DIR`",
		degraded_mark.contains(CropRoleSprites.SPRITE_DIR)
			and degraded_rows.size() == 1 and degraded_rows[0].begins_with(
				DetailFormat.MORALE_BREAKDOWN_INDENT + degraded_mark + " "))
	# …and the CONTRACT in full, at the shipped directory: empty, a traversal, a display name
	# (spaces + capitals) and a hyphenated near-miss. **STILL DELIBERATELY VACUOUS — but for a
	# different reason than it once was.** It used to pass for free because coverage was zero and
	# every key answered `""`; with 32 PNGs on disk it passes for free because none of these four
	# SHAPES names a file that exists — `Wild Emmer` and `wild-emmer` miss `wild_emmer.png` on the
	# space and on the hyphen even where the filesystem ignores case. The claim that can actually
	# FAIL is the under-override one above; note that a capitalised key with the right punctuation
	# (`Wild_Emmer`) would now resolve in the shipped directory too, so the guard is live there and
	# not merely stated. This group writes the rule down in full; it does not test it.
	var guarded := FloraSprites.path_for("") == "" \
		and FloraSprites.path_for("../../../evil") == "" \
		and FloraSprites.path_for("Wild Emmer") == "" \
		and FloraSprites.path_for("wild-emmer") == ""
	h._assert_hud("…and every shape of bad key composes NO path — the wire is not trusted with one",
		guarded)

## The driven precedence fixture's species key — a key that IS a filename in `CropRoleSprites`'
## directory, which is what lets the override reach the species tier with no flora art on disk.
const SPECIES_TIER_SPECIES := "staple"

## …and a role whose OWN art is a DIFFERENT file in that same directory, so "the species tier won"
## and "the role tier coincided" are distinguishable answers.
const SPECIES_TIER_ROLE := "cash"

## A roster species that DOES ship art, for the liveness half. `wild_emmer` was the degraded key
## while coverage was zero and is the resolving one now — the same key, on the other side of the
## flip, which is what makes the pair of claims read as one history.
const SHIPPED_ART_SPECIES := "wild_emmer"

## The DEGRADED fixture's species — the one roster member that will NEVER have art, so this claim is
## durable rather than a race with the next batch of PNGs. `hay_grass` is the roster's only `fodder`
## species, so its role mark already names it exactly and uniquely; `icon_prompts.txt` records the
## absence as deliberate ("32 prompts, 33 species"). Its role is `fodder` for the same reason.
const DEGRADED_TIER_SPECIES := "hay_grass"
const DEGRADED_TIER_ROLE := "fodder"

## **THE SHIPPED `forage.cultivation.field_capacity_gain`, READ OFF THE SIM'S OWN CONFIG** — what a
## completed Field multiplies the TILE's own `K` by, and the term this whole block is about.
##
## **IT WAS A HARNESS-LOCAL `2.53`, AND THAT MADE THE BLOCK'S PRECONDITION A TAUTOLOGY.** CLAIM 0 read
## `ground * FIELD_CAPACITY_GAIN > ground` — `x * 2.53 > x`, over two numbers this file writes itself
## — so setting the shipped gain to `1.0` left all three claims passing on a card with no Field payoff
## left to state, which is precisely the vacuity the claim's own docstring promises it prevents. The
## sim-side twin (`forage::climbing_to_field_does_not_compound_the_capacity_gain`) reads the config
## and calls the same reading a PRECONDITION; this one now does too.
##
## The config is the SERVER's file, so it is reached through the project directory rather than `res://`
## — the harness and `core_sim` are two directories of one checkout (a worktree included), which is
## what makes the relative walk stable.
const LABOR_CONFIG_RELATIVE_PATH := "../../core_sim/src/data/labor_config.json"
const LABOR_CONFIG_FORAGE_KEY := "forage"
const LABOR_CONFIG_CULTIVATION_KEY := "cultivation"
const LABOR_CONFIG_FIELD_GAIN_KEY := "field_capacity_gain"

## What an unreadable or unrecognisable config answers. A gain that multiplies NOTHING, deliberately:
## it fails CLAIM 0 rather than letting the block pass on a number nobody managed to read.
const FIELD_CAPACITY_GAIN_UNREAD := 0.0

## A gain of exactly one buys the rung nothing — the value the config would have to hold for the whole
## block to be vacuous, and therefore the one CLAIM 0 is written against.
const FIELD_CAPACITY_GAIN_NO_GAIN := 1.0

func _field_capacity_gain() -> float:
	var path := ProjectSettings.globalize_path("res://").path_join(
		LABOR_CONFIG_RELATIVE_PATH).simplify_path()
	if not FileAccess.file_exists(path):
		return FIELD_CAPACITY_GAIN_UNREAD
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
	if not (parsed is Dictionary):
		return FIELD_CAPACITY_GAIN_UNREAD
	var forage: Variant = (parsed as Dictionary).get(LABOR_CONFIG_FORAGE_KEY, {})
	if not (forage is Dictionary):
		return FIELD_CAPACITY_GAIN_UNREAD
	var cultivation: Variant = (forage as Dictionary).get(LABOR_CONFIG_CULTIVATION_KEY, {})
	if not (cultivation is Dictionary):
		return FIELD_CAPACITY_GAIN_UNREAD
	return float((cultivation as Dictionary).get(LABOR_CONFIG_FIELD_GAIN_KEY,
		FIELD_CAPACITY_GAIN_UNREAD))

## The GROUND's own `K` under that Field. `BaseFx.FIXTURE_CAPACITY`, so the Field fixture stands on
## exactly the ground every other tile fixture in this chapter does and the only thing that changed is
## the rung.
const FIELD_GROUND_CAPACITY := BaseFx.FIXTURE_CAPACITY

## The two guard keys chosen so that the path they WOULD compose under the override resolves to a
## real shipped PNG — which is the only way to ask whether the guard fires while flora coverage is
## zero and every key answers `""` for free.
const TRAVERSAL_KEY := "../crops/staple"
const CAPITALISED_KEY := "Staple"

## ONE plant, so the assertions can be positional on a single row rather than searching a list.
func _species_tier_composition() -> Array:
	return [{
		"species": SPECIES_TIER_SPECIES,
		"role": SPECIES_TIER_ROLE,
		"display_name": "Precedence Probe",
		"share": 1.0,
	}]

func _degraded_tier_composition() -> Array:
	return [{
		"species": DEGRADED_TIER_SPECIES,
		"role": DEGRADED_TIER_ROLE,
		"display_name": "Hay Grass",
		"share": 1.0,
	}]

## THE FOG STOCK/CAPACITY SPLIT (issue #462), asserted over the REAL producer's lines rather than a
## picture, because `— / 205` and a row that never rendered at all look far too alike downscaled —
## and because the bug being guarded was two rows that DISAGREED, which needs both read at once.
##
## The remembered fixture is deliberately NOT redacted (it sets `visibility_state` and leaves every
## key in place), so this drives the branch the way a leaky frame would: the rows must go capacity-only
## because the producer DECIDED to on the visibility, never because a key happened to be missing. Feed
## it a redacted dict instead and the whole assertion goes vacuous. Sabotage-verified.
func _assert_fog_stock_parity() -> void:
	var remembered = h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	var forage_index = Readout.detail_row_index(remembered, HudFloraVocab.FORAGING_KEY)
	var graze_index = Readout.detail_row_index(remembered, HudFloraVocab.GRAZING_KEY)
	# THE ISSUE ITSELF: the remembered card used to state Grazing and no Foraging at all.
	h._assert_hud("a REMEMBERED tile states BOTH food webs, in the live card's order",
		forage_index >= 0 and graze_index == forage_index + 1)
	if forage_index < 0 or graze_index < 0:
		return
	var forage_row = remembered[forage_index]
	var graze_row = remembered[graze_index]
	# Each row keeps its CAPACITY (a property of the ground the sim recomputes from the tile every
	# turn) and loses its STOCK. Matching the unknown form against the vocab const rather than a
	# literal is what stops the em-dash drifting apart from the thing being asserted.
	var forage_unknown := HudFloraVocab.STOCK_UNKNOWN_FORMAT % _sight_forage_capacity()
	var graze_unknown := HudFloraVocab.STOCK_UNKNOWN_FORMAT % _sight_graze_capacity()
	h._assert_hud("…each stating its CAPACITY with the stock unknown (`%s` / `%s`)" % [
			forage_unknown, graze_unknown],
		forage_row.ends_with(forage_unknown) and graze_row.ends_with(graze_unknown))
	# The phase is `classify_ecology_phase`'s reading OF the biomass, so it goes with it. Asserted
	# against the fixture's OWN phase word — a bare "no phase" test would pass on a fixture that never
	# had one, which is exactly the vacuous shape this file's assertion rules warn about.
	var live_phase := DetailFormat.ecology_phase_label(
		String(TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED).get("graze_ecology_phase", "")))
	h._assert_hud("…and NEITHER carries an ecology phase, which would state the stock it just withheld",
		live_phase != "" and not forage_row.contains(live_phase)
			and not graze_row.contains(live_phase))
	# The basket decomposes a STANDING stock into per-plant biomasses, so with no stock it cannot
	# render — the free-floating "three more resources" list the layout exists to stop.
	h._assert_hud("…and the basket does not render under a Foraging row with no stock to decompose",
		ForageFx.flora_basket_rows(remembered).is_empty())
	# THE OTHER HALF, without which the four above pass on a card that shows nothing anywhere: the
	# SAME fixture in sight must still state both stocks in full.
	var live = h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		TileFx.sight_tile_fixture(TileFx.VIS_ACTIVE), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	var live_forage = Readout.detail_row_index(live, HudFloraVocab.FORAGING_KEY)
	var live_graze = Readout.detail_row_index(live, HudFloraVocab.GRAZING_KEY)
	h._assert_hud("the SAME tile in sight states both stocks in full, phase and all",
		live_forage >= 0 and live_graze >= 0
			and not live[live_forage].contains(HudFloraVocab.STOCK_UNKNOWN_GLYPH)
			and live[live_graze].contains(live_phase))
	# AND THE HALF THE FIXTURE ALONE CANNOT REACH: that the shipped REDACTION LIST and the producer
	# above agree about which keys survive. Everything so far runs on an unredacted dict, so a key list
	# that erased `patch_carrying_capacity` would pass every assertion here and still ship a live card
	# with no Foraging row on it — the row's own capacity guard would simply find 0 and emit nothing.
	# The list is applied BY HAND rather than through `_apply_visibility_to_info`, whose discovered
	# branch is exactly this loop plus a visibility-raster lookup this harness has no grid to seed;
	# reading `FOW_DISCOVERED_HIDDEN_KEYS` off MapView itself is what keeps the two from drifting.
	#
	# **`ForageFx.floorify` RUNS FIRST, THEN THE ERASURE** — the order is the whole claim. `ForageFx.seed_growth_terms`
	# fills in whatever growth terms a fixture lacks, and four of the keys it seeds
	# (`patch_regrowth_samples` — its `capacity > 0` branch fires precisely because the capacity
	# survives redaction — plus `patch_per_worker_biomass` and the two phase fractions) are keys the
	# shipped list REMOVES. Floorifying afterwards therefore hands the producer a dict the real path
	# can never produce, and a later assertion about the harvest-floor instrument (that
	# `floor_chart_model` answers `known == false` under the real list) would read a harness-seeded
	# growth curve and pass for the wrong reason.
	var redacted := ForageFx.floorify(
		TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	for key in h.MAP_VIEW_SCRIPT.FOW_DISCOVERED_HIDDEN_KEYS:
		redacted.erase(key)
	var redacted_lines = h._hud._drawer._tile_terrain_lines(redacted)
	h._assert_hud("…and a tile put through the REAL redaction list still states both capacity rows",
		Readout.detail_row_index(redacted_lines, HudFloraVocab.FORAGING_KEY) >= 0
			and Readout.detail_row_index(redacted_lines, HudFloraVocab.GRAZING_KEY) >= 0)
	# The rows must read the SAME on a redacted tile as on the unredacted one above — that equality is
	# what says the capacity-only form comes from the visibility DECISION and not from the erasure.
	h._assert_hud("…reading identically to the unredacted remembered tile, decision not accident",
		redacted_lines == remembered)

## **THE REMEMBERED CARD STATES THE GROUND, NOT THE RUNG** — the half `_assert_fog_stock_parity`
## structurally cannot reach, and the defect this fixture exists for.
##
## `patch_carrying_capacity` on the wire is the tile's own `K` times the interpolated
## `field_capacity_gain`, so a standing Field publishes ~2.53x its biome's base while `patch_is_field`
## and `patch_field_progress` are redacted beside it — a finer reading of the ladder position than the
## boolean being hidden. The fix redacts the GAIN rather than the ceiling: the card falls back to
## `patch_tile_capacity`, which is terrain.
##
## **WHY IT NEEDS ITS OWN FIXTURE.** Everything above runs on `TileFx.sight_tile_fixture`, which
## stands below the Field rung — there the ground's `K` and the patch's ceiling are THE SAME NUMBER,
## so the parity assertion passes whether the fallback works or is not wired at all. It cannot tell
## the defect from the fix. This fixture is the one where the two genuinely differ, and the FIRST
## claim below is that precondition: without it the whole block goes vacuous the moment the gain stops
## applying.
func _assert_fog_field_capacity_is_the_ground() -> void:
	var gain := _field_capacity_gain()
	var ground := FIELD_GROUND_CAPACITY
	var boosted := FIELD_GROUND_CAPACITY * gain
	# CLAIM 0a — the harness actually read the shipped file. Told apart from CLAIM 0 because the two
	# need opposite responses: this one fails when the config moved or the walk to it broke, that one
	# when the RUNG stopped buying anything.
	h._assert_hud("the shipped `%s.%s.%s` was read (got %s)" % [LABOR_CONFIG_FORAGE_KEY,
			LABOR_CONFIG_CULTIVATION_KEY, LABOR_CONFIG_FIELD_GAIN_KEY, gain],
		gain > FIELD_CAPACITY_GAIN_UNREAD)
	# CLAIM 0 — and it is a real multiplier, so the two numbers below are actually different.
	# Everything after it is about which one the card picked, and on equal numbers there is no pick to
	# observe. Written against the SHIPPED gain, so dropping it to 1.0 in `labor_config.json` fails
	# here instead of leaving the block passing over a fixture arguing with itself.
	h._assert_hud("a FIELD's ceiling and the ground under it are DIFFERENT numbers (%.0f vs %.0f at a gain of %s)"
		% [boosted, ground, gain], gain > FIELD_CAPACITY_GAIN_NO_GAIN and boosted > ground)
	# Floorified BEFORE the erasure, exactly as the parity assertion above: the seeder fills growth
	# terms off the capacity, and four of the keys it seeds are keys the shipped list removes.
	var remembered := ForageFx.floorify(
		_field_rung_tile_fixture(TileFx.VIS_DISCOVERED), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	for key in h.MAP_VIEW_SCRIPT.FOW_DISCOVERED_HIDDEN_KEYS:
		remembered.erase(key)
	var remembered_lines = h._hud._drawer._tile_terrain_lines(remembered)
	var remembered_index = Readout.detail_row_index(remembered_lines, HudFloraVocab.FORAGING_KEY)
	var ground_face := HudFloraVocab.STOCK_UNKNOWN_FORMAT % ground
	# CLAIM 1 — the redacted card states the GROUND's capacity. A card that kept the ceiling would read
	# `— / 253` and hand over the rung; one that read the redacted key straight would find nothing,
	# fail its own `capacity > 0` guard and drop the Foraging row altogether.
	h._assert_hud("a REMEMBERED tile on the FIELD rung states the GROUND's capacity (`%s`)"
			% ground_face,
		remembered_index >= 0 and remembered_lines[remembered_index].ends_with(ground_face))
	# CLAIM 2 — and the SAME tile in sight still states the BOOSTED one. Without this half, a fallback
	# that ignored `patch_carrying_capacity` entirely would satisfy claim 1 while quietly deleting the
	# Field's whole payoff from the live card.
	var live := ForageFx.floorify(
		_field_rung_tile_fixture(TileFx.VIS_ACTIVE), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var live_lines = h._hud._drawer._tile_terrain_lines(live)
	var live_index = Readout.detail_row_index(live_lines, HudFloraVocab.FORAGING_KEY)
	var boosted_face := HudFloraVocab.STOCK_FORMAT % [float(live["patch_biomass"]), boosted]
	h._assert_hud("…while the SAME tile in sight states the PATCH's boosted ceiling (`%s`)"
			% boosted_face,
		live_index >= 0 and live_lines[live_index].contains(boosted_face))


## A hex carrying a COMPLETED Field, in a given sight state — the one fixture where the patch's
## ceiling and the tile's own `K` are different numbers.
##
## It states BOTH capacities explicitly, after `BaseFx.seed_forage_rows` has run: the seeder equates
## them (every fixture through it stands below rung 3) and this is the state that must not.
func _field_rung_tile_fixture(visibility_state: String) -> Dictionary:
	var tile := TileFx.sight_tile_fixture(visibility_state)
	tile["patch_tile_capacity"] = FIELD_GROUND_CAPACITY
	tile["patch_carrying_capacity"] = FIELD_GROUND_CAPACITY * _field_capacity_gain()
	tile["patch_is_field"] = true
	tile["patch_field_progress"] = 1.0
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)


## The two webs' capacities on `TileFx.sight_tile_fixture`, read back OFF the fixture so the assertion above
## cannot drift from the numbers it is asserting about.
func _sight_forage_capacity() -> float:
	return float(TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED).get("patch_carrying_capacity", 0.0))

func _sight_graze_capacity() -> float:
	return float(TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED).get("graze_capacity", 0.0))

## Does this basket row lead with a SPECIES mark — the tier ABOVE the role one?
##
## Matched on `FloraSprites.SPRITE_DIR`, not through the resolver, and deliberately unlike its role
## twin: the role marks are a closed table of three that can be enumerated, while the species tier is
## keyed by FILENAME, so a resolver-built needle would need the row's own key in hand. The directory
## is what tells the two tiers apart, which is the only distinction being asserted here.
func _flora_row_has_species_icon(row: String) -> bool:
	return row.contains(FloraSprites.SPRITE_DIR)

## The `(78)` a basket row closes with — parsed back out of the RENDERED row, so this reads what the
## player reads rather than recomputing what it should have been.
func _flora_row_biomass(row: String) -> int:
	var open_paren := row.rfind("(")
	var close_paren := row.rfind(")")
	if open_paren < 0 or close_paren <= open_paren:
		return 0
	return int(row.substr(open_paren + 1, close_paren - open_paren - 1))

## The blank slot an UNSTATED role SHOULD render at this box size — the spacer image where there is
## art to match the width of, else the text spacer, mirroring `DetailFormat.flora_composition_lines`'
## own fallback chain. **Never `""`**, which is the point: the helper it wraps degrades to empty, and
## an empty needle silently satisfies every string test there is.
func _expected_blank_slot(icon_px: int) -> String:
	var spacer := FoodIcons.crop_role_spacer(icon_px)
	if spacer != "":
		return spacer
	return HudFloraVocab.FLORA_ROLE_ICON_UNSTATED

## Does this basket row lead with a real ROLE MARK — as opposed to the blank slot an UNSTATED role
## renders, which since #463 is ALSO an `[img]` and would satisfy any test phrased as "does the row
## carry art"?
##
## **The needles are built through the PRODUCTION resolver at the drawer's own box size**, never
## written as literals. The mark is bundled art now (`CropRoleSprites`) with the emoji as a LIVE
## fallback, so a test pinned to either form alone goes quietly vacuous the moment the other is what
## renders — and it is the emoji form that a failed PNG load produces, i.e. exactly the regression
## worth catching. Asking `FoodIcons.for_crop_role` means this matches whatever the row was actually
## rendered from, and it excludes the spacer by construction (a different file).
func _flora_row_has_role_icon(row: String, icon_px: int) -> bool:
	for role in FoodIcons.CROP_ROLE_ICONS:
		var mark := FoodIcons.for_crop_role(String(role), icon_px)
		if mark != "" and row.contains(mark):
			return true
	return false

## An OVERGRAZED pasture: the standing graze has been drawn deep into the stressed band, so the
## `Pasture ecology` row reads a WARN-amber "⚠ Stressed" — the SAME label + tint a stressed herd or a
## stressed forage patch gets (one ecology vocabulary, one styling path). Nothing eats graze until
## Phase 2b, so this state cannot occur in a live 2a map; it renders the path the tint will take.
## A tile whose Climate row is under test: same card as `BaseFx.food_tile_fixture`, only the
## `temperature` (and a label) vary, so the ONLY thing moving between the four climate_* frames
## is the band the sim's cut points classify that temperature into.
func _climate_tile_fixture(temperature: float, terrain_label: String) -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["temperature"] = temperature
	tile["terrain_label"] = terrain_label
	return tile

## STAGE 2 of the commitment — a band has COMMITTED this patch to Wild Grain and the build is STILL
## RUNNING (`BaseFx.food_tile_fixture` carries `cultivation_progress` 0.6, `is_cultivated` false). The
## commitment is recorded on the FIRST worked turn, so the basket underneath it is the wild one,
## UNCHANGED — 45 / 30 / 25, byte-for-byte what `food_tile` shows. The card must therefore render the
## `Crop: Wild Grain` row AND the whole basket, with Wild Grain marked in SIGNAL; collapsing to the
## crop row alone claimed a mixed tile was already pure the instant the order was given (issue #433).
## The committed species is a MEMBER of the basket on purpose — the mark has nothing to land on
## otherwise, and the sim can only ever commit to a plant the tile actually realizes.
func _committed_crop_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	# The KEY is the roster id it must join the basket on; the LABEL is deliberately the fixture's own
	# (see `BaseFx.food_tile_fixture`'s composition note). They differ on purpose — do not align them.
	tile["patch_committed_species"] = "wild_emmer"
	tile["patch_committed_display_name"] = "Wild Grain"
	return tile

## STAGE 3 — the same commitment once the Tended Patch COMPLETES, which is when the basket finally
## moves. Weeding lifts the favored share to `min(1, share x tended_weeding_gain)` (0.455 x 1.5 =
## 0.6825) and takes the increase off the LEAST abundant members first, so Oak Mast (0.25) absorbs all
## 0.2275 of it and Ground Nut is untouched: 68 / 30 / 2. Read against `food_tile_crop` — same tile,
## same crop, one build later — this pair is the whole point of showing the basket beside the Crop
## row: you can watch Oak Mast fall 25% -> 2% as the work lands.
func _weeded_crop_tile_fixture() -> Dictionary:
	var tile := _committed_crop_tile_fixture()
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	var basket: Array = []
	for entry_variant in tile["patch_composition"]:
		var entry: Dictionary = (entry_variant as Dictionary).duplicate(true)
		match String(entry["species"]):
			# Matched on the roster ID, not on the row's label — the two differ here by design.
			"wild_emmer": entry["share"] = 0.6825
			"oak_mast": entry["share"] = 0.0225
		basket.append(entry)
	tile["patch_composition"] = basket
	# A tended patch reports every policy ceiling == per_worker_yield (see `TileFx.tended_tile_fixture`), so
	# the stepper caps at 1 worker and the frame does not also change the forecast under test.
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return RungFx.stamp_patch(BaseFx.seed_forage_rows(tile), HudComposeVocab.FORAGE_FORECAST_PREFIX)

## **THE SAME TILE WITH ONE PLANT'S ROLE UNSTATED** — the `""` case, which the wire says means "this
## server's roster no longer knows this species", NOT "staple". The row must render its share and its
## biomass with NO icon at all rather than defaulting into a real category, and the two tagged rows
## beside it are what make that visible. The key is OMITTED rather than set to `""` so the fixture also
## covers the shape the decoder produces when the wire carries no role (it only inserts the key when
## the string is there).
func _unstated_role_tile_fixture() -> Dictionary:
	var tile := TileFx.three_role_tile_fixture()
	tile["x"] = 65
	var basket: Array = []
	for entry_variant in tile["patch_composition"]:
		var entry: Dictionary = (entry_variant as Dictionary).duplicate(true)
		# **THE UNSTATED ROW MUST BE ONE WHOSE SPECIES HAS NO ART** (issue #339). It was `cotton`
		# until flora art landed, at which point cotton's row led with `flora/cotton.png` and the
		# blank-slot path — the whole point of this fixture — became unreachable through it: the
		# SPECIES tier outranks the role, so it also outranks the role's absence. `hay_grass` is the
		# durable choice rather than a convenient one: it is the roster's only fodder species, so its
		# role mark already names it exactly, and `icon_prompts.txt` records that it is DELIBERATELY
		# never given art. A row with no species art and no role is the only one that can render the
		# spacer.
		if String(entry["species"]) == "hay_grass":
			entry.erase("role")
		basket.append(entry)
	tile["patch_composition"] = basket
	return tile

func _overgrazed_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["x"] = 68
	tile["graze_biomass"] = 61.0
	tile["graze_ecology_phase"] = "stressed"
	return tile

## Ground that carries NO pasture at all (a glacier — the biome's graze capacity is a stated 0, so the
## sim holds no patch there and the tile carries no graze fields). The card must print NOTHING about
## pasture here — never "0 / 0", which would read as a starved pasture rather than an absent one.
func _no_pasture_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 3,
		"terrain_label": "Glacier",
		"tags_text": "Polar",
		"visibility_state": "active",
		"habitability": 0.09,
		"temperature": -14.0,
	}

## A plain (no forage patch) tile carrying hex-EDGE rivers on some of its sides. Deliberately
## bare of food-module keys so the Tile card is just the terrain-intrinsic rows and the river
## row(s) read unobstructed.
func _river_tile_fixture(river_mask: int) -> Dictionary:
	return {
		"x": 9, "y": 36,
		"terrain_label": "Sinkhole Field",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.03,
		"temperature": 15.0,
		"river_edges": river_mask,
	}


# ---- ROADS ON THE TILE CARD (arc #532) --------------------------------------------------------
#
# A road is IN THE GROUND, so the land drawer is its readout. These fixtures are shaped exactly like
# the native decoder's `routes` rows (`native/src/dict/routes.rs`) — **ONE ROW PER TILE**, with no path
# on it: a road is a per-tile improvement with its own rung, its own meter, its own keeper and its own
# decay.

## The friction multipliers and link spans the four route rungs actually ship in
## `intensification_ladder.json`, transcribed so the frames state the real ladder rather than round
## numbers. Reading them back is what makes the `Buys:` assertions a test of the CONVERSION rather
## than of arithmetic on invented inputs.
const ROAD_FRICTION_PATH := 1.0
const ROAD_FRICTION_TRAIL := 0.85
const ROAD_FRICTION_DIRT := 0.6
const ROAD_FRICTION_PAVED := 0.35
const ROAD_LINK_PATH := 0
const ROAD_LINK_TRAIL := 6
const ROAD_LINK_DIRT := 10
const ROAD_LINK_PAVED := 16
## …and the loss each takes off, as the row states it. Written out rather than computed here: an
## assertion that recomputed the conversion would pass against a producer that had stopped doing it.
const ROAD_SAVING_TRAIL_PERCENT := 15
const ROAD_SAVING_DIRT_PERCENT := 40
const ROAD_SAVING_PAVED_PERCENT := 65

## The three built rungs' own `grace_turns` (2 / 5 / 12) — the DIRECTION is the animal branch's: the
## highest rung is the most forgiving, because the roadbed does the holding. A road whose bill is met
## reads its rung's full grace + 1, which is why these are the numbers a kept road carries.
const ROAD_GRACE_TRAIL := 3
const ROAD_GRACE_DIRT := 6
const ROAD_GRACE_PAVED := 13
## …and what a road actually losing its rung reads: turns of shortfall LEFT, not turns served.
const ROAD_GRACE_LEFT := 2
## The countdown's biting value. `0` is "it is reverting NOW" and gets its own word, which is the one
## claim that separates a countdown from a counter.
const ROAD_GRACE_NONE := 0

## Standing bills that climb with the rung — the branch's whole shape in three numbers.
const ROAD_DEMAND_TRAIL := 1.6
const ROAD_DEMAND_DIRT := 3.4
const ROAD_DEMAND_PAVED := 6.0
## …and a bill half unpaid, so the shortfall and the demand are visibly different numbers on the row.
const ROAD_SHORTFALL_DIRT := 1.7

## `build_fraction` at a rung being worn in, and at one with nothing rising above it. The complete
## reading is the wire's exact `1.0`, never derived by subtraction.
const ROAD_METER_RISING := 0.3
const ROAD_METER_COMPLETE := 1.0
## ⛔ **A ROAD DECLARED AND BANKING NOTHING** — the state the reported defect was found in, and the one
## a fixture cannot reach with `ROAD_METER_RISING`: a `grade` queued behind another job banks nothing
## for dozens of turns, because the head of the queue takes every builder the band has.
const ROAD_METER_UNSTARTED := 0.0
## ⛔ **WHAT A FRESHLY-GRADED ROAD ACTUALLY PUBLISHES, AND IT IS NOT `ROAD_METER_UNSTARTED`.**
## `RouteState.buildFraction` is the meter on the rung at RISK, and `routes::road_at_risk_rung` falls
## back to the rung the road HOLDS whenever nothing is banked above it — so a `grade` that landed this
## turn ships `METER_FULL`, its TRAIL being complete.
##
## **A fixture staging `0.0` there stages a state no server can produce**, and that is exactly how
## `Queued 100%` reached play: the queued frames asserted a reading off a meter the sim never sends,
## so the real one went untested. Every declared-road fixture states this instead.
const ROAD_METER_FRESHLY_GRADED := 1.0
## …the same meter as the percent the row prints, so the assertion reads the producer's own floor.
const ROAD_METER_RISING_PERCENT := 30

## The number of `roadwork` keepers the middle rung's bill wants. The SIM's answer on the wire; a
## client that recomputed it would need the per-worker rate, which it does not hold.
const ROAD_WANTS_DIRT := 4

## ⛔ **THE BILL RAY WAS SHOWN, AND IT IS THE WHOLE REASON THE ROW WAS REBUILT.** A trail carrying 2%
## of the dirt road banked owes ~`0.009` work a turn: `DetailFormat.format_work_units` rounds that
## DOWN to `0.0` while the sim's own `ceil` rounds the same number UP to one keeper, and the row
## printed both — `0.0 work a turn · wants 1 keepers`. It is above `SourceForecast.UPKEEP_WORK_MIN`,
## so the row is genuinely owed and genuinely rendered; nothing here is a degenerate case.
const ROAD_DEMAND_HAIRLINE := 0.009
const ROAD_WANTS_HAIRLINE := 1

## …and the hands the dirt road's shortfall is worth — `ceil(1.7 / PER_WORKER_OUTPUT)`. **Written out
## rather than computed**, and deliberately PLURAL: the shipped row said `wants 1 keepers`.
const ROAD_SHORT_WORKERS_DIRT := 2

## ⛔ **THE PERCENT A DECLARED ROAD'S ROW PRINTS — AND IT IS NOT THE METER IT WAS READ OFF.** The road
## is behind a Tame at the head of its band's queue, and the head takes every builder, so it banks
## nothing for dozens of turns; the WIRE meanwhile publishes `ROAD_METER_FRESHLY_GRADED` for the trail
## it already holds. `HudRouteVocab.queued_progress` is what turns that full meter into this zero, so
## the row reads `0% to dirt road` rather than the bare `Trail` that made a working `grade`
## indistinguishable from a failed one. **There is no `0.0` meter const to stage beside it**, because
## no server sends one on a road holding a rung.
const ROAD_METER_DECLARED_PERCENT := 0

## …and a shortfall worth exactly ONE hand, which is what proves the row pluralizes: the shipped one
## said `wants 1 keepers`.
const ROAD_SHORTFALL_TRAIL := 0.4
const ROAD_ONE_WORKER_SHORT := 1

## ⛔ **THE RETIRED ROW'S KEY, SPELLED AS A LITERAL ON PURPOSE.** `HudRouteVocab.ROAD_KEEPER_ROW` is
## gone, so a claim that the card no longer draws it cannot be written against the const it retired —
## an assertion referring to a deleted symbol does not fail, it fails to compile, and the chapter
## would go silent rather than red.
const RETIRED_KEEPER_ROW_KEY := "Kept by"

## …and the two phrases the bill itself must never print again, for the same reason: both consts they
## came from are gone, so the claim has to state the words.
const RETIRED_UPKEEP_RATE_WORDS := "work a turn"
const RETIRED_UPKEEP_WANTS_WORD := "wants"

## The tile the road fixtures sit on — the river fixture's own hex, so the two frames differ in
## exactly the thing under test. It is the row's IDENTITY now, the retired `RouteId` having gone with
## the stored path.
const ROAD_TILE := Vector2i(9, 36)

## ⛔ **WHOSE JOB THE ROAD IS.** A road tile has ONE keeper and no shares, and it is the band that
## GRADED it — wherever that band has since walked. `NO_KEEPER` is the whole free floor and also a
## built road whose keeping band is gone.
const ROAD_KEEPER_BAND := 1
const ROAD_NO_KEEPER := -1

## ⛔ **WHAT DISTANCE DID TO THE PRICE**, as the multiple the sim quoted when the keeper took the tile
## on. `AT_HOME` is inside `route_range.base_tiles` and on every road nobody keeps; `REMOTE` is the
## shipped `route_range.remote_cost_multiplier`, which prices BOTH the build pile and the standing
## upkeep. **A threshold, not a curve** — and the only thing that can explain a bill larger than the
## rung says.
const ROAD_REMOTENESS_AT_HOME := 1.0
const ROAD_REMOTENESS_REMOTE := 2.0

## …and the bill a REMOTE dirt road actually carries: the rung's own, times that multiple. Written out
## rather than computed, so an assertion that recomputed the product would not pass against a client
## that had started doing the multiplication itself — which it must never do.
const ROAD_DEMAND_DIRT_REMOTE := 6.8
const ROAD_WANTS_DIRT_REMOTE := 7

## A road at one rung. `meter` is the wire's `build_fraction` — the meter on the rung being RAISED,
## which is a DIFFERENT rung, and `1.0` means nothing is rising.
func _road_fixture(rung: String, meter: float, demand: float, shortfall: float,
		workers_needed: int, grace: int, lit: bool, friction: float, link: int,
		keeper: int = ROAD_NO_KEEPER, remoteness: float = ROAD_REMOTENESS_AT_HOME) -> Dictionary:
	return {
		# THE TILE IS THE ROW'S IDENTITY — there is no id and no path beside it.
		"tile_x": ROAD_TILE.x,
		"tile_y": ROAD_TILE.y,
		# **THE BOOL IS READ BEFORE THE ID**, because `0` is a real `BandId` — so a fixture states both
		# rather than leaving the reader to infer one from a sentinel.
		"has_keeper": keeper != ROAD_NO_KEEPER,
		"keeper_band_id": keeper,
		"keeper_remoteness": remoteness,
		"rung": rung,
		"build_fraction": meter,
		"upkeep_demand": demand,
		# **THE SUPPLIED SIDE IS STATED, NEVER LEFT TO BE DERIVED** — the wire holds
		# `demand − supplied == shortfall` verbatim, so a fixture that broke that identity would be
		# testing the client against a frame the sim cannot produce.
		"upkeep_supplied": demand - shortfall,
		"upkeep_shortfall": shortfall,
		"upkeep_workers_needed": workers_needed,
		# `has_neglect_grace == false` is "there is nothing at risk here", which is the path:
		# it declares no upkeep, so it has no meter to lose.
		"has_neglect_grace": demand > 0.0,
		"neglect_grace_remaining": grace,
		"grants_sight": lit,
		"friction_multiplier": friction,
		"holds_link_to_tiles": link,
	}

## The hex the road crosses — the river fixture's ground with a road on it, so the two frames differ
## in exactly the thing under test.
func _road_tile_fixture(road: Dictionary) -> Dictionary:
	var tile := _river_tile_fixture(RIVER_MASK_NONE)
	tile["roads"] = [road]
	return tile

## The road block's own lines, off the REAL producer (`SubjectDrawerController._tile_terrain_lines`),
## so every claim about them is about the shipped readout rather than a re-derivation here.
##
## **THE KEEPER'S NAME RESOLVES INSIDE IT**, through this client's one band-naming rule — a road
## carries an id and nothing else — so a road kept by a band this walk's roster does not hold comes
## back UNNAMED, which is a real state and is asserted as one below.
func _road_lines(road: Dictionary) -> Array[String]:
	return h._hud._drawer._tile_terrain_lines(_road_tile_fixture(road))

## **THE FIRST PLAYER BAND'S OWN `band_id`, read LIVE off the walk's roster.** A road frame that
## hard-coded an id would render *another people* — this client resolves a keeper's name through its
## band roster, and an id nobody holds resolves to nothing — which is a truthful state but the wrong
## one to spend five frames on. `ROAD_NO_KEEPER` where the walk holds no player band, which draws the
## keeperless case rather than a wrong name.
func _road_keeper_band() -> int:
	var bands: Array = h._hud._band_labor.player_bands()
	if bands.is_empty() or not (bands[0] is Dictionary):
		return ROAD_NO_KEEPER
	return int((bands[0] as Dictionary).get("band_id", ROAD_NO_KEEPER))

## **A SECOND PLAYER BAND, so a road can be kept by somebody who is not the actor.** It is the
## fixture band with a different `entity` (and therefore a different derived `band_id`), which is the
## whole of what the keeper comparison reads — every other field on it is irrelevant to this claim and
## is left alone rather than invented.
##
## **THE ROSTER ORDER IS THE NAME.** `update_band_alerts` makes the FIRST player band the acting one,
## and `band_label_for_id` names a band by its roster index — so staged second, this band is `Band 2`
## and the actor is `Band 1`.
func _second_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = SECOND_BAND_ENTITY
	return BandFx.with_band_id(band)

## …and its `band_id`, read back off the fixture rather than recomputed here: the offset that derives
## one from an entity is `fixtures_band.gd`'s and must be spelled in exactly one place.
func _second_band_keeper_id() -> int:
	return int(_second_band_fixture().get("band_id", ROAD_NO_KEEPER))

## ⛔ **THE ROAD FRAMES NEED A BAND ROSTER, AND IT IS PUT BACK ON THE WAY OUT.** A keeper is a BAND,
## and this client resolves a band's name through the roster `update_band_alerts` fills — so with no
## roster every road on screen reads *another people*, which is a true sentence about the wrong
## subject. The roster is SHARED WALK STATE (the chapters after this one render against whatever they
## inherit), so it is captured by value first and restored verbatim afterwards — `turn_orb`'s own rule
## for the knowledge tracks, for the identical reason.
## ⛔ **AND THE ACTING BAND IS RESTORED SEPARATELY, BECAUSE THIS CHAPTER SETS IT SEPARATELY.** The
## forage-range block above writes `_band_labor._player_band` DIRECTLY, bypassing the roster —
## `update_band_alerts` re-derives that field from the list it is handed, so a restore that put only
## the roster back would leave the acting band cleared, and the compose sheet the NEXT chapter opens
## draws nothing at all. It cost a run.
func _with_a_band_roster(bands: Array) -> Dictionary:
	var inherited := {
		"bands": h._hud._band_labor.player_bands().duplicate(true),
		"acting": h._hud._band_labor.player_band().duplicate(true),
	}
	h._hud.update_band_alerts(bands)
	return inherited

func _restore_band_roster(inherited: Dictionary) -> void:
	h._hud.update_band_alerts(inherited["bands"])
	h._hud._band_labor._player_band = inherited["acting"]

## …and the same block with the keeper already NAMED. It calls the composer the drawer calls one line
## after resolving that name, which is the only way to state a claim about the WORDS a named keeper
## reads in without staging a band roster this chapter would then have to put back.
## ⛔ **THE RATE IS THREADED IN, THE WAY THE REAL DRAWER THREADS IT.** `road_lines` takes the branch's
## bare work rate as a parameter (the leaf holds no catalog, so the road→catalog join stays at the call
## site), and the `Upkeep` row needs it to state a shortfall in HANDS. Passing the catalog's own value
## rather than a local literal is what makes these claims about the WIRE's number.
func _road_lines_named(road: Dictionary, keeper_label: String,
		per_worker_turn: float = LADDER_BARE_WORK_RATE) -> Array[String]:
	return HudRouteVocab.road_lines(road, keeper_label, null, per_worker_turn)

## ---- WHAT THE ROAD ROWS SAY -------------------------------------------------------------------
##
## ⛔ **THE BLOCK IS CONDITIONAL NOW, SO HALF THESE CLAIMS ARE ABSENCES.** It used to print five rows
## on every road in the world — two of them prose saying *no* — and the readout the player actually
## meets, a free path, spent four lines to say that it costs nothing and does nothing. Every row
## but the rung row is emitted only where it has something to say, and an assertion that a row is
## PRESENT proves nothing about that: what is asserted below is the exact WORDS each row carries and
## the exact rows a road does NOT draw.
func _assert_road_rows_are_conditional() -> void:
	# ⛔ **THE FREE FLOOR IS ONE ROW.** Nothing to pay, nobody to pay it, nothing bought, nothing at
	# risk — so the rung row is the whole of it, and the four remaining keys are absent from the REAL
	# producer's output rather than present-and-empty.
	var trail_road := _road_fixture(
		HudRouteVocab.RUNG_KEY_PATH, ROAD_METER_RISING, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_PATH, ROAD_LINK_PATH)
	var trail_lines := _road_lines(trail_road)
	h._assert_hud("a free path composes exactly ONE row",
		HudRouteVocab.road_lines(trail_road).size() == 1)
	h._assert_hud("…and the card draws no upkeep row, because the floor declares no bill",
		Readout.detail_row_index(trail_lines, HudRouteVocab.ROAD_UPKEEP_ROW) < 0)
	h._assert_hud("…no payoff row, because a path buys nothing on any axis",
		Readout.detail_row_index(trail_lines, HudRouteVocab.ROAD_BONUS_ROW) < 0)
	# ⛔ **AND THE `Kept by` ROW IS GONE FROM EVERY ROAD IN THE GAME**, not merely from this one. It
	# said WHOSE JOB the road is directly beneath a bill that said how much and how many, and the pair
	# read `Upkeep: 0.0 work a turn · wants 1 keepers` / `Kept by: Band 3`. The name is the bill's
	# headline now, so a card that still drew the second row would be saying it twice.
	h._assert_hud("…and the retired `Kept by` row is drawn on no road at all",
		not "\n".join(trail_lines).contains(RETIRED_KEEPER_ROW_KEY))
	h._assert_hud("…and no countdown, because a rung with no upkeep has nothing to lose",
		Readout.detail_row_index(trail_lines, HudRouteVocab.ROAD_REVERTING_ROW) < 0)

	# ⛔ **AND THE ONE ROW READS AS A COMPLETE PATH PART-WAY TO A TRAIL**, never as a road that
	# is 30% built. The rung is the value; the meter arrives as a qualifier naming where it is GOING.
	h._assert_hud("the rung is the FACT and the percentage is the NEXT rung's approach",
		Readout.detail_row_value(trail_lines, HudRouteVocab.ROAD_ROW)
			== "%s%s%s" % [
				HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_PATH],
				HudRouteVocab.ROAD_CLAUSE_SEPARATOR,
				HudRouteVocab.ROAD_PROGRESS_FORMAT % [ROAD_METER_RISING_PERCENT,
					String(HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL]).to_lower()]])
	# **THE NEGATIVE IS THE HALF THAT NAMES THE OLD DEFECT.** `Trail 30%` — the rung being raised
	# followed by the meter — is exactly what the retired `Wearing in:` row said, and it reads as a
	# road that is part-built rather than a complete one climbing.
	h._assert_hud("…and it never reads as the NEXT rung at a percentage, which is the old wording",
		not Readout.detail_row_value(trail_lines, HudRouteVocab.ROAD_ROW).contains(
			"%s %d%%" % [HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL],
				ROAD_METER_RISING_PERCENT]))

	# THE FIRST RUNG ANYONE PAYS FOR, with traffic already wearing in the one above it: four rows,
	# each one earned. The payoff is UNLABELLED — its key is the blank `ROAD_BONUS_ROW`, which keeps
	# the value in the card's own value column without a label restating that a benefit is a benefit.
	var wearing_lines := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, ROAD_DEMAND_TRAIL, 0.0, 2,
		ROAD_GRACE_TRAIL, true, ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL))
	h._assert_hud("a road HOLDS its rung whole, and the meter names the rung ABOVE it",
		Readout.detail_row_value(wearing_lines, HudRouteVocab.ROAD_ROW)
			== "%s%s%s" % [
				HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL],
				HudRouteVocab.ROAD_CLAUSE_SEPARATOR,
				HudRouteVocab.ROAD_PROGRESS_FORMAT % [ROAD_METER_RISING_PERCENT,
					String(HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_DIRT_ROAD]).to_lower()]])
	# ⛔ **THE PAYOFF ROW IS ONE CLAUSE — the loss it saves — AND THE REST IS ON THE HOVER.** The row
	# printed all three axes and wrapped to three lines under a value that is itself one word;
	# reported from play as *"way too much, maybe just 15% less loss is enough, the rest can be a tool
	# tip"*. The loss figure is the one that moves a decision (it is the reason to pave rather than
	# leave a trail), so it stays and the other two go one hover away.
	h._assert_hud("…and its payoff row is ONE clause: the loss it saves",
		Readout.detail_row_value(wearing_lines, HudRouteVocab.ROAD_BONUS_ROW)
			== HudRouteVocab.ROAD_BONUS_FRICTION_FORMAT % ROAD_SAVING_TRAIL_PERCENT)
	# **NOTHING WAS DELETED — the sight and the span are in `bonus_tooltip`**, which the drawer
	# registers on the block's hover through `Context.row_tooltips`. Asserted because a rendered frame
	# cannot see a tooltip, and a cut that lost the detail would pass the row claim above.
	h._assert_hud("…with the sight and the span one hover away, not gone",
		HudRouteVocab.bonus_tooltip(_road_fixture(
			HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, ROAD_DEMAND_TRAIL, 0.0, 2,
			ROAD_GRACE_TRAIL, true, ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL))
			== HudRouteVocab.ROAD_CLAUSE_SEPARATOR.join([
				HudRouteVocab.ROAD_BONUS_SIGHT,
				HudRouteVocab.ROAD_BONUS_LINK_FORMAT % ROAD_LINK_TRAIL]))

	# A COMPLETE RUNG WITH NOTHING RISING ABOVE IT STATES THE RUNG AND NOTHING ELSE — the wire states
	# exactly `1.0` there (for a rung just finished AND for the top of the ladder), so the test is a
	# plain comparison and never a tolerance.
	var dirt_lines := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT))
	h._assert_hud("a complete rung states the rung BARE — no progress clause at all",
		Readout.detail_row_value(dirt_lines, HudRouteVocab.ROAD_ROW)
			== HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_DIRT_ROAD])
	# ⛔ **THE BILL NAMES WHOEVER IS ON THE HOOK, AND THIS FIXTURE HAS NOBODY.** A road that owes and
	# nobody keeps is decaying towards nobody, and re-issuing the verb is how another band picks it up
	# — adoption being the same act as building, which is what the clause says rather than naming a
	# verb that does not exist.
	h._assert_hud("…its bill reads `Upkeep`, the word the wire and the band card both use",
		Readout.detail_row_value(dirt_lines, HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_KEEPER_NOBODY)
	# ⛔ **AND THE FIGURES ARE ONE HOVER AWAY, NOT GONE.** The exact bill and the keepers it wants are
	# the sim's own numbers and stay available; what changed is that they stopped being the headline,
	# which is the only way `0.0 work a turn` can never render as one.
	h._assert_hud("…with the exact bill and the %d keepers it wants on the hover" % ROAD_WANTS_DIRT,
		HudRouteVocab.upkeep_tooltip(_road_fixture(
			HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
			ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT))
			== HudRouteVocab.ROAD_UPKEEP_TIP_FORMAT % [
				DetailFormat.format_work_units(ROAD_DEMAND_DIRT), ROAD_WANTS_DIRT,
				HudRouteVocab.ROAD_UPKEEP_PLURAL_SUFFIX])
	h._assert_hud("…and it saves %d%% of what is lost hauling" % ROAD_SAVING_DIRT_PERCENT,
		Readout.detail_row_value(dirt_lines, HudRouteVocab.ROAD_BONUS_ROW).begins_with(
			HudRouteVocab.ROAD_BONUS_FRICTION_FORMAT % ROAD_SAVING_DIRT_PERCENT))

	# THE TOP OF THE LADDER — dearest to keep, richest payoff. The pair is the branch's argument.
	var paved_lines := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_PAVED_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_PAVED, 0.0, 7,
		ROAD_GRACE_PAVED, true, ROAD_FRICTION_PAVED, ROAD_LINK_PAVED))
	h._assert_hud("the top rung is DEARER to keep than the one below it, and the hover says so",
		HudRouteVocab.upkeep_tooltip(_road_fixture(
			HudRouteVocab.RUNG_KEY_PAVED_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_PAVED, 0.0, 7,
			ROAD_GRACE_PAVED, true, ROAD_FRICTION_PAVED, ROAD_LINK_PAVED)).begins_with(
				DetailFormat.format_work_units(ROAD_DEMAND_PAVED)))
	h._assert_hud("…and RICHER: %d%% saved, above the dirt road's" % ROAD_SAVING_PAVED_PERCENT,
		Readout.detail_row_value(paved_lines, HudRouteVocab.ROAD_BONUS_ROW)
			== HudRouteVocab.ROAD_BONUS_FRICTION_FORMAT % ROAD_SAVING_PAVED_PERCENT)
	h._assert_hud("…and its %d-tile span is stated on the hover, not lost" % ROAD_LINK_PAVED,
		HudRouteVocab.bonus_tooltip(_road_fixture(
			HudRouteVocab.RUNG_KEY_PAVED_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_PAVED, 0.0, 7,
			ROAD_GRACE_PAVED, true, ROAD_FRICTION_PAVED, ROAD_LINK_PAVED)).contains(
				HudRouteVocab.ROAD_BONUS_LINK_FORMAT % ROAD_LINK_PAVED))
	# **AND THE TOP OF THE LADDER HAS NO RUNG ABOVE IT TO APPROACH.** `1.0` is what the wire states
	# there, so the progress half is simply absent rather than reading `100% to` anything.
	h._assert_hud("…and the top rung states no approach, there being nothing above it",
		Readout.detail_row_value(paved_lines, HudRouteVocab.ROAD_ROW)
			== HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_PAVED_ROAD])

	# THE ROAD IN SHORTFALL. Four claims, and each is a different field of the row.
	var risk_lines := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
		ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_LEFT, false, ROAD_FRICTION_DIRT,
		ROAD_LINK_DIRT))
	h._assert_hud("a road in shortfall carries the hazard mark and the ROUTE web's own word, as a clause",
		Readout.detail_row_value(risk_lines, HudRouteVocab.ROAD_ROW)
			== "%s%s%s %s" % [
				HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_DIRT_ROAD],
				HudRouteVocab.ROAD_CLAUSE_SEPARATOR,
				HudSelectionVocab.RUNG_HAZARD_GLYPH, HudRouteVocab.ROAD_UNDER_KEPT_WORD])
	# ⛔ **THE SHORTFALL IS THE SIM'S FIELD.** The fixture's demand and shortfall are deliberately
	# different numbers, so a row that printed `demand − supplied` would still pass — and one that
	# printed the GROSS demand as the shortfall would not.
	# **A SHORT ROAD NOBODY KEEPS STILL READS *nobody*** — there is no band to be short. The *short N*
	# clause belongs to a NAMED keeper and is asserted where one exists, below.
	h._assert_hud("…and a short road with no keeper says so, rather than blaming a band",
		Readout.detail_row_value(risk_lines, HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_KEEPER_NOBODY)
	h._assert_hud("…and counts DOWN to losing the rung (%d turns left)" % ROAD_GRACE_LEFT,
		Readout.detail_row_value(risk_lines, HudRouteVocab.ROAD_REVERTING_ROW)
			== HudRouteVocab.ROAD_REVERTING_FORMAT % [
				HudSelectionVocab.RUNG_HAZARD_GLYPH, ROAD_GRACE_LEFT])
	# **THE ROAD GOES DARK BEFORE IT DECAYS** — `grants_sight` is the RESOLVED answer, and the clause
	# says why the light went out rather than silently vanishing. It names the bill by the row's own
	# word, `upkeep`, which is the whole point of the rename.
	#
	# ⛔ **IT RODE THE SIGHT CLAUSE TO THE HOVER, and that is right rather than a demotion.** The row
	# above it already states the SHORTFALL in amber and the `Reverting` row states the countdown, so
	# on the row this was the same trouble said a third time; the fact itself is unchanged and one
	# hover away.
	h._assert_hud("…and the hover says the road has gone DARK until its UPKEEP is paid",
		HudRouteVocab.bonus_tooltip(_road_fixture(
			HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
			ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_LEFT, false, ROAD_FRICTION_DIRT,
			ROAD_LINK_DIRT)).contains(HudRouteVocab.ROAD_BONUS_DARK))

	# ⛔ **THE COUNTDOWN, NOT THE COUNTER.** `0` means it is reverting NOW; rendering it through the
	# same format as every other value would print `in 0 turns`, which reads as a turn of grace the
	# road does not have. PNG-less, because the whole claim is a word.
	var now_lines := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
		ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_NONE, false, ROAD_FRICTION_DIRT,
		ROAD_LINK_DIRT))
	h._assert_hud("a countdown of 0 reads NOW, never `in 0 turns`",
		Readout.detail_row_value(now_lines, HudRouteVocab.ROAD_REVERTING_ROW)
			== HudRouteVocab.ROAD_REVERTING_NOW % HudSelectionVocab.RUNG_HAZARD_GLYPH)

## ⛔ **WHOSE JOB THIS ROAD IS, AND WHAT DISTANCE IS CHARGING THEM FOR IT** — the half of the card
## that had no surface at all while the client still read the retired stored path.
##
## The keeper is **the band that BUILT the tile, wherever it has since walked**, not whoever is
## standing on it: the sim's claim walk never reads a keeper's position. So *"who pays for this"* is a
## fact only this row can carry, and the remoteness beside it is the only thing that can explain a
## bill larger than the rung's own — a real decision, priced and refused nowhere.
func _assert_road_rows_say_whose_job_it_is() -> void:
	var band_label := "Band 2"

	# ⛔ **A KEPT ROAD'S BILL IS A NAME.** `Upkeep: Band 2` — nothing else, because nothing else is a
	# decision. It was `Upkeep: 3.4 work a turn · wants 4 keepers` over `Kept by: Band 2`: two rows and
	# three figures, and the one thing a player can act on spread across both.
	var kept_road := _road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		ROAD_KEEPER_BAND)
	var kept := _road_lines_named(kept_road, band_label)
	h._assert_hud("a kept road's `Upkeep` row is the band's own name, and nothing else",
		Readout.detail_row_value(kept, HudRouteVocab.ROAD_UPKEEP_ROW) == band_label)
	# ⛔ **THE NEGATIVE IS THE HALF THAT NAMES THE DEFECT.** `0.0 work a turn · wants 1 keepers` was two
	# roundings of one number in opposite directions, printed side by side; neither phrase may appear
	# on the row again, whatever the figures behind it are.
	h._assert_hud("…and never the retired figures — no work rate and no keeper count on the row",
		not "\n".join(kept).contains(RETIRED_UPKEEP_RATE_WORDS)
			and not "\n".join(kept).contains(RETIRED_UPKEEP_WANTS_WORD))

	# ⛔ **RAY'S OWN ROW, AT THE FIGURES HE WAS SHOWN.** A bill of `0.009` work a turn wanting one
	# keeper: `format_work_units` floors it to `0.0` and the sim's `ceil` lifts it to `1`, which is the
	# pair that read as gibberish. The row says `Band 2`, and the absurdity is gone rather than
	# reworded — while the hover still carries both figures exactly as the sim published them.
	var hairline := _road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, ROAD_DEMAND_HAIRLINE, 0.0,
		ROAD_WANTS_HAIRLINE, ROAD_GRACE_TRAIL, true, ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL,
		ROAD_KEEPER_BAND)
	h._assert_hud("the bill that read `0.0 work a turn · wants 1 keepers` now reads `%s`" % band_label,
		Readout.detail_row_value(_road_lines_named(hairline, band_label),
			HudRouteVocab.ROAD_UPKEEP_ROW) == band_label)
	# **AND THE HOVER KEEPS THE HAIRLINE, SINGULAR** — one keeper, not `1 keepers`.
	h._assert_hud("…with the exact bill and its ONE keeper, singular, one hover away",
		HudRouteVocab.upkeep_tooltip(hairline) == HudRouteVocab.ROAD_UPKEEP_TIP_FORMAT % [
			DetailFormat.format_work_units(ROAD_DEMAND_HAIRLINE), ROAD_WANTS_HAIRLINE, ""])

	# ⛔ **SHORT SAYS *SHORT N WORKERS*, AND THE N IS OFF `upkeepShortfall`.** Never `wants − assigned`:
	# `roadwork` is a band-wide pool, the client holds no per-road head count to subtract from, and this
	# branch has shipped that subtraction as a defect twice. The fixture's demand and shortfall are
	# deliberately different numbers, so a row converting the DEMAND (which would say `4`) fails here.
	var short_road := _road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
		ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_LEFT, false, ROAD_FRICTION_DIRT,
		ROAD_LINK_DIRT, ROAD_KEEPER_BAND)
	h._assert_hud("a keeper falling short is named and the gap is stated in WORKERS (%d, plural)"
			% ROAD_SHORT_WORKERS_DIRT,
		Readout.detail_row_value(_road_lines_named(short_road, band_label),
			HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_UPKEEP_SHORT_FORMAT % [band_label,
				HudRouteVocab.ROAD_UPKEEP_SHORT_MARK, ROAD_SHORT_WORKERS_DIRT,
				HudRouteVocab.ROAD_UPKEEP_WORKER_PLURAL])
	# ⛔ **AND WITH NO CATALOG ON THE WIRE THERE IS NO CLAUSE AT ALL — the containment half.** The rate
	# is `RouteRungState.buildWorkPerWorkerTurn` now, not a constant this client spells, so a client
	# that has not been sent a catalog cannot price the gap in hands. **It must not fall back**: a
	# substituted `1.0` is the retired transcription returning through the side door, and it would go
	# stale in silence the day the sim writes worker output as a sum of more terms. The keeper is still
	# named — they are still on the hook — and `(short 0 workers)` never renders.
	h._assert_hud("with no rate on the wire the row names the keeper and states no gap at all",
		Readout.detail_row_value(
			_road_lines_named(short_road, band_label, LADDER_NO_BARE_WORK_RATE),
			HudRouteVocab.ROAD_UPKEEP_ROW) == band_label)
	# **AND IT PLURALIZES**, which the shipped row did not: `wants 1 keepers` is what a fixed count and
	# a fixed noun produce. One worker short reads `worker`.
	var barely_short := _road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_COMPLETE, ROAD_DEMAND_TRAIL,
		ROAD_SHORTFALL_TRAIL, 2, ROAD_GRACE_LEFT, false, ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL,
		ROAD_KEEPER_BAND)
	h._assert_hud("…and one worker short reads `worker`, never `1 workers`",
		Readout.detail_row_value(_road_lines_named(barely_short, band_label),
			HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_UPKEEP_SHORT_FORMAT % [band_label,
				HudRouteVocab.ROAD_UPKEEP_SHORT_MARK, ROAD_ONE_WORKER_SHORT,
				HudRouteVocab.ROAD_UPKEEP_WORKER_SINGULAR])

	# ⛔ **AND A REMOTE ONE SAYS IT IS COSTING MORE FOR BEING FAR FROM THE KEEPER — ON THE HOVER.** The
	# multiple is the SIM's, presented rather than derived, and it is a real fact with no other surface;
	# it left the row because Ray's complaint was LENGTH and it is both rare and long.
	var remote := _road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT_REMOTE, 0.0,
		ROAD_WANTS_DIRT_REMOTE, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		ROAD_KEEPER_BAND, ROAD_REMOTENESS_REMOTE)
	h._assert_hud("…and the row of a remote road is STILL just the name",
		Readout.detail_row_value(_road_lines_named(remote, band_label),
			HudRouteVocab.ROAD_UPKEEP_ROW) == band_label)
	h._assert_hud("…with what distance charges them (×%s) on the hover"
			% (HudRouteVocab.ROAD_REMOTENESS_FORMAT % ROAD_REMOTENESS_REMOTE),
		HudRouteVocab.upkeep_tooltip(remote).contains(
			HudRouteVocab.ROAD_KEEPER_REMOTE_FORMAT % (
				HudRouteVocab.ROAD_REMOTENESS_FORMAT % ROAD_REMOTENESS_REMOTE)))
	# …and the BILL it quotes is the dearer one, straight off the wire. The two fixtures differ in
	# their demand as well as their multiple, so a hover that stated the rung's base price fails here
	# while one that multiplied the base by the multiple itself would pass — which is why the client
	# must do neither, and why the number is stated rather than computed.
	h._assert_hud("…and the bill beside it is the dearer one the sim quoted (%s)"
			% DetailFormat.format_work_units(ROAD_DEMAND_DIRT_REMOTE),
		HudRouteVocab.upkeep_tooltip(remote).begins_with(
			HudRouteVocab.ROAD_UPKEEP_TIP_FORMAT % [
				DetailFormat.format_work_units(ROAD_DEMAND_DIRT_REMOTE), ROAD_WANTS_DIRT_REMOTE,
				HudRouteVocab.ROAD_UPKEEP_PLURAL_SUFFIX]))

	# **A BAND OUTSIDE THE PLAYER'S ROSTER HAS NO NAME THE PLAYER CAN USE**, and a raw id would be a
	# fact they cannot act on. This one goes through the REAL PRODUCER, which is what makes it a claim
	# about the drawer's own resolution: it looks the keeper's id up in this client's one band-naming
	# rule, gets nothing, and the row says so in words.
	var foreign := _road_lines(kept_road)
	h._assert_hud("a road kept by a people you merely know of is named as such, never by an id",
		Readout.detail_row_value(foreign, HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_KEEPER_FOREIGN)

	# **A ROAD THAT OWES A BILL WITH NOBODY PAYING IT** — the keeping band is gone, so it is decaying
	# towards nobody, and re-issuing the verb is how another band picks it up. Adoption is the same
	# act as building, which is what the clause says rather than naming a verb that does not exist.
	var orphan := _road_lines(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT))
	h._assert_hud("a road with a bill and no keeper says so, and says how to take it on",
		Readout.detail_row_value(orphan, HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_KEEPER_NOBODY)

## ⛔ **THE DRAWER'S OWN RATE LOOKUP, ASSERTED THROUGH THE SHIPPED PRODUCER.** `_road_lines_named` is
## handed a rate; this is not. `SubjectDrawerController` holds `_topbar` for exactly one field and
## resolves `branch_build_work_per_worker_turn` off the rung catalog before composing the block — the
## road→catalog join lives at the CALL SITE, so `HudRouteVocab` stays a leaf with no catalog
## dependency. Nothing but this asserts that the drawer actually does it.
func _assert_the_real_drawer_prices_a_shortfall() -> void:
	var short_road := _road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
		ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_LEFT, false, ROAD_FRICTION_DIRT,
		ROAD_LINK_DIRT, _road_keeper_band())
	var keeper := HudFormat.band_display_name(h._hud._band_labor.player_bands()[0], 1)
	h._assert_hud("the REAL drawer resolves the rate off the catalog and states `short %d workers`"
			% ROAD_SHORT_WORKERS_DIRT,
		Readout.detail_row_value(_road_lines(short_road), HudRouteVocab.ROAD_UPKEEP_ROW)
			== HudRouteVocab.ROAD_UPKEEP_SHORT_FORMAT % [keeper,
				HudRouteVocab.ROAD_UPKEEP_SHORT_MARK, ROAD_SHORT_WORKERS_DIRT,
				HudRouteVocab.ROAD_UPKEEP_WORKER_PLURAL])

## ⛔ **A DECLARED ROAD STOPS LOOKING UN-DECLARED — fix 3, and the pair is the whole claim.**
##
## Reported from play: Ray graded a tile and the `Road` row went on reading a bare `Trail`. Three
## things were true at once and only the third was wrong — the `grade` landed, the road was banking
## zero for the correct reason, and no surface said so.
##
## **THE SPELLING IS `ROAD_PROGRESS_FORMAT`, THE ONE THE ROW ALREADY USES ABOVE ZERO.** Ray asked for
## a percentage — *"it should show a % complete in the road panel"* — so `0% to dirt road` and
## `30% to dirt road` are one sentence about one climb, and there is no second phrasing to tell apart
## from the first.
##
## **THE NEGATIVE IS THE HALF THAT MAKES IT A CLAIM.** Ground nobody has ordered anything on has no
## climb to report, and a `0%` there would invent one — so the SAME fixture with no queue entry must
## state the rung bare.
func _assert_a_declared_road_says_it_is_climbing() -> void:
	# ⛔ **THE METER IS THE ONE THE WIRE ACTUALLY PUBLISHES.** Nothing is banked into the dirt road, so
	# `road_at_risk_rung` falls back to the TRAIL this road holds and ships `METER_FULL`; a `0.0` here
	# is a state no server produces, and it made BOTH claims below pass down the wrong arm —
	# `progress_clause`'s zero-meter guard rather than its full-meter suppression, and
	# `queued_progress`'s pass-through rather than the mapping the queued reading exists for.
	var road := _road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_FRESHLY_GRADED, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL, _road_keeper_band())
	var approach := "%s%s%s" % [
		HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL],
		HudRouteVocab.ROAD_CLAUSE_SEPARATOR,
		HudRouteVocab.ROAD_PROGRESS_FORMAT % [ROAD_METER_DECLARED_PERCENT,
			String(HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_DIRT_ROAD]).to_lower()]]

	# ⛔ **THE NEGATIVE, AND IT RUNS FIRST.** With no entry on the wire the row states the rung and
	# nothing else — which is also the state every frame above this block renders in, so a producer
	# that had started printing `0%` unconditionally fails here.
	h._assert_hud("a road nobody has ordered anything on states its rung BARE, with no `0%` on it",
		Readout.detail_row_value(_road_lines(road), HudRouteVocab.ROAD_ROW)
			== String(HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL]))

	# ⛔ **AND THE QUEUE IS SEEDED, not assumed.** A frame named for the queued reading that draws the
	# un-queued one is this arc's own recurring trap: the join is road tile → `build_queue` entry, so a
	# state that stages no entry renders the fallback while claiming the interesting case.
	var inherited := _with_a_road_queued(ROAD_TILE)
	h._assert_hud("…and the SAME road, once its `grade` is on the wire, says it is climbing — `%s`"
			% approach,
		Readout.detail_row_value(_road_lines(road), HudRouteVocab.ROAD_ROW) == approach)
	# ⛔ **AND THE `Upkeep` ROW STAYS ABSENT.** A road on the free floor owes nothing, and a
	# DECLARATION must not conjure a bill — that is a different row answering a different question, and
	# making it appear here would be a second defect dressed as this fix.
	h._assert_hud("…and declaring it conjures no bill: the free floor still draws no `%s` row"
			% HudRouteVocab.ROAD_UPKEEP_ROW,
		Readout.detail_row_index(_road_lines(road), HudRouteVocab.ROAD_UPKEEP_ROW) < 0)
	# **A ROAD ALREADY CARRYING WORK IS UNTOUCHED**, which is what says the fix opened the zero case
	# rather than replacing the reading above it.
	var working := _road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL, _road_keeper_band())
	h._assert_hud("…while a road with work banked still states its real percentage, unchanged",
		Readout.detail_row_value(_road_lines(working), HudRouteVocab.ROAD_ROW)
			== "%s%s%s" % [HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_TRAIL],
				HudRouteVocab.ROAD_CLAUSE_SEPARATOR,
				HudRouteVocab.ROAD_PROGRESS_FORMAT % [ROAD_METER_RISING_PERCENT,
					String(HudRouteVocab.RUNG_LABELS[HudRouteVocab.RUNG_KEY_DIRT_ROAD]).to_lower()]])
	_restore_road_queue(inherited)

## ⛔ **PUT A ROAD'S `grade` ON THE ACTING BAND'S WIRE QUEUE — and put the band back afterwards.** The
## roster is SHARED WALK STATE (the chapters after this one render against whatever they inherit), so
## the queue is captured by value first and restored verbatim, this chapter's own rule for the roster
## it stages.
##
## The entry is shaped exactly as the sim publishes one: `kind: "roadwork"` carrying the road's TILE,
## which is what the road row's own `tile_x` / `tile_y` join on.
func _with_a_road_queued(tile: Vector2i) -> Array:
	var bands: Array = h._hud._band_labor.player_bands()
	var inherited: Array = []
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		var band: Dictionary = band_variant
		inherited.append((band.get("build_queue", []) as Array).duplicate(true))
		band["build_queue"] = [{"kind": HudConst.LABOR_KIND_ROADWORK,
			"target_x": tile.x, "target_y": tile.y, "fauna_id": ""}]
	return inherited

## ⛔ **A BAND WITH TWO JOBS ALREADY IN FRONT OF THE PRESS** — a patch at the head and a second behind
## it, so the aside has both a head to NAME and a remainder to COUNT. Restored through
## `_restore_road_queue`, the roster being shared walk state.
##
## **FORAGE entries rather than a hunt**, deliberately: a herd's name is resolved through the walk's
## herd roster, which this chapter does not stage — a subject that came back empty would leave the
## expected sentence agreeing with a producer that had stopped naming anything.
const LADDER_QUEUE_HEAD_TILE := Vector2i(70, 30)
const LADDER_QUEUE_SECOND_TILE := Vector2i(71, 31)
const LADDER_QUEUE_AHEAD := 2

func _with_a_busy_queue() -> Array:
	var bands: Array = h._hud._band_labor.player_bands()
	var inherited: Array = []
	for band_variant in bands:
		if not (band_variant is Dictionary):
			continue
		var band: Dictionary = band_variant
		inherited.append((band.get("build_queue", []) as Array).duplicate(true))
		band["build_queue"] = [
			{"kind": SourceForecast.LABOR_KIND_FORAGE, "target_x": LADDER_QUEUE_HEAD_TILE.x,
				"target_y": LADDER_QUEUE_HEAD_TILE.y, "fauna_id": ""},
			{"kind": SourceForecast.LABOR_KIND_FORAGE, "target_x": LADDER_QUEUE_SECOND_TILE.x,
				"target_y": LADDER_QUEUE_SECOND_TILE.y, "fauna_id": ""},
		]
	return inherited

## ⛔ **…AND THE SAME QUEUE WITH THE ROAD ITSELF ON IT** — a `grade` standing on the fixture's own
## tile, LAST, behind the two jobs `_with_a_busy_queue` staged. That is the reported state: the head
## takes every builder, so this entry banks nothing and the row must say so.
##
## **The `kind` token is `roadwork` and the tile is what tells two roads apart** — it is a band-wide
## keeping ROLE and a per-tile build SOURCE under one spelling, so an entry without its tile collapses
## onto every other road the band keeps.
func _with_a_queued_road() -> void:
	for band_variant in h._hud._band_labor.player_bands():
		if not (band_variant is Dictionary):
			continue
		var queue: Array = (band_variant as Dictionary).get("build_queue", []) as Array
		queue.append({"kind": HudConst.LABOR_KIND_ROADWORK,
			"target_x": ROAD_FIXTURE_TILE.x, "target_y": ROAD_FIXTURE_TILE.y, "fauna_id": ""})

## The tile every road fixture in this chapter stands on, so a queue entry naming it and the road row
## itself cannot drift apart.
const ROAD_FIXTURE_TILE := Vector2i(9, 36)

## …and the sentence that queue must produce, composed from the shipped formats but with the SUBJECT
## and the COUNT written out — an expectation recomposed whole from the producer would pass against a
## producer that had stopped composing it.
## ⛔ **AND IT CARRIES NO ESTIMATE NOTE — nor does any road row now.** The clause used to end
## *"— the estimate runs from when it starts"*, which only means anything beside a duration; a priced
## row no longer quotes one. The `waiting behind …` aside that DID quote one is retired outright
## (`HudRouteVocab`'s own retirement note), so this sentence is the only placement the card composes,
## and it rides a row merely OFFERED — never the row being built.
func _expected_queue_aside() -> String:
	return HudRouteVocab.ROAD_LADDER_QUEUE_BEHIND_PLAIN_FORMAT % [
		HudWorkVocab.BUILD_QUEUE_TILE_SUBJECT_FORMAT % [
			LADDER_QUEUE_HEAD_TILE.x, LADDER_QUEUE_HEAD_TILE.y],
		HudRouteVocab.ROAD_LADDER_QUEUE_MORE_FORMAT % (LADDER_QUEUE_AHEAD - 1)]

func _restore_road_queue(inherited: Array) -> void:
	var bands: Array = h._hud._band_labor.player_bands()
	for index in range(mini(bands.size(), inherited.size())):
		if bands[index] is Dictionary:
			(bands[index] as Dictionary)["build_queue"] = inherited[index]

func run(harness) -> void:
	h = harness

	# State 2 — a food tile selected, band WITHIN forage range: the Tile card's "Assign foragers"
	# controls (a "Band:" dropdown naming the actor band + a Foragers −/+ count + an enabled **Forage**
	# button). With one player band the dropdown is a single item ("Band 1").
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("food_tile")
	h._assert_compose_sheet_fits("food_tile")

	# State 2-crop — the SAME tile once a band has committed it under Cultivate/Sow, WITH THE BUILD
	# STILL RUNNING (flora roster S1 + issue #433). A `Crop: Wild Grain` row appears ABOVE the basket
	# and the basket is UNCHANGED (45 / 30 / 25, identical to `food_tile.png`), because the species is
	# recorded on the first worked turn — ~25 turns before any weeding happens. Wild Grain's 🌿 row is
	# marked in SIGNAL, which is what joins the two rows by eye. THREE FRAMES ARE THE TEST, in order:
	# `food_tile` (wild) -> here (committed, nothing grown yet) -> `food_tile_crop_tended` (weeded).
	# A "committed" frame alone would pass while the client still collapsed the basket on commit.
	h._show_tile(_committed_crop_tile_fixture())
	h._compose_forage(_committed_crop_tile_fixture())
	await h._settle()
	await h._save("food_tile_crop")

	# State 2-crop-tended — the third frame: the SAME commitment once the Tended Patch lands and the
	# basket finally REWEIGHTS (Wild Grain 45% -> 68%, Oak Mast 25% -> 2%, Ground Nut untouched, the
	# increase coming off the least abundant member first). The Cultivation row reads "🌾 Tended Patch"
	# beside it, so the frame states the cause and the effect together.
	h._show_tile(_weeded_crop_tile_fixture())
	h._compose_forage(_weeded_crop_tile_fixture())
	await h._settle()
	await h._save("food_tile_crop_tended")

	# State 2-growing — the "What grows here" SECTION on the bare tile card (no compose sheet): a header
	# then one 🌿 row per realized plant, name + share%, in wire order (share DESC). The pair is TWO
	# "Alluvial Plain" tiles with DIFFERENT realized baskets (Wild Emmer 70% + Flax 30% vs Cotton 55% +
	# Flax 45%), so read side by side they are the visible proof that same-biome tiles no longer carry a
	# uniform per-biome roster — the per-tile realization the compose picker already shows, now on the
	# card a player gets by just inspecting a tile. Compose source reset so only the card renders.
	h._hud._compose.reset_forage_source()
	h._show_tile(ForageFx.cash_basket_tile_fixture())
	await h._settle()
	await h._save("tile_growing_here")
	h._show_tile(ForageFx.cash_variant_basket_tile_fixture())
	await h._settle()
	await h._save("tile_growing_here_variant")

	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()

	# State 2-forecast — the same food tile with the Foragers stepper parked AT the forecast cap
	# (3 = the Sustain ceiling's max-useful workers, below the band's 10 idle): the `+` button is
	# DISABLED, the "max 3 workers useful here — more would be idle" note explains why, and the
	# "Expected yield" row reads the ceiling itself (+0.96 /turn = min(3 × 0.32, 0.96)).
	h._hud._compose.set_forage_count(3)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_forecast_cap")

	# State 2-labor — the SAME food tile, but the actor band has only 2 idle workers, BELOW Sustain's
	# max-useful of 3: the Foragers stepper caps at 2 (LABOR, not usefulness) and the note names the
	# reason — "2 of 3 useful — free up idle workers to send more" — so a `+` gone dead at idle reads as
	# fixable by reassigning labor, not as a silent bug. The usefulness ceiling (3) is unchanged; only
	# the note differs from the usefulness-bound `forage_forecast_cap` above.
	var forage_labor_band: Dictionary = BandFx.forage_range_bands()[0].duplicate(true)
	forage_labor_band["idle_workers"] = 2
	h._hud._band_labor._player_band = forage_labor_band
	h._hud._compose.set_forage_band(-1)
	h._hud._compose.set_forage_count(2)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_labor_bound")
	# Restore the 10-idle range band + count for the states that follow.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.set_forage_band(-1)
	h._hud._compose.set_forage_count(3)

	# State 2-tended — a fully-cultivated forage patch: the Tile card's cultivation row reads
	# "🌾 Tended Patch" (SIGNAL tint) with an "Ecology: Thriving" row above it. A tended
	# patch's ceilings all equal its per-worker yield, so the forecast caps the stepper at 1 worker.
	h._show_tile(TileFx.tended_tile_fixture())
	h._compose_forage(TileFx.tended_tile_fixture())
	await h._settle()
	await h._save("tended_tile")

	# State 2-stressed — an over-drawn (uncultivated) forage patch: the Ecology row reads a WARN-amber
	# "⚠ Stressed" right under "Forage biomass", exactly like a stressed herd's Ecology row. Proves the
	# row is NOT gated on cultivation.
	h._hud._compose.set_forage_count(1)
	h._show_tile(TileFx.stressed_tile_fixture())
	await h._settle()
	await h._save("food_tile_stressed")

	# ---- Climate band: rendered off the sim's PUBLISHED cut points (Climate Authority) -----------
	# The Climate row is classified by the sim's cut points (polar ≤0 / boreal ≤3 / temperate ≤18 °C),
	# NOT a client threshold. Drive the same tile card at four temperatures spanning the ladder and
	# confirm the label tracks the sim's inclusive-upper-bound bands. A cold highland reads Polar/Boreal,
	# a warm lowland reads Temperate/Tropical — and "Polar" now appears ONLY where the sim says so, which
	# is the whole point of retiring the client's own cool_min.
	h._show_tile(_climate_tile_fixture(-6.0, "Frost Highland"))
	await h._settle()
	await h._save("climate_polar")
	h._show_tile(_climate_tile_fixture(2.0, "Boreal Upland"))
	await h._settle()
	await h._save("climate_boreal")
	h._show_tile(_climate_tile_fixture(12.0, "Temperate Vale"))
	await h._settle()
	await h._save("climate_temperate")
	h._show_tile(_climate_tile_fixture(27.0, "Tropical Lowland"))
	await h._settle()
	await h._save("climate_tropical")

	# ---- The tile card's TWO FOOD-WEB ROWS ------------------------------------------------------
	# `Foraging` (people) directly above `Grazing` (animals), each carrying its stock and its ecology
	# phase inline, with the human layer's basket indented beneath its row. The pair replaced four
	# interleaved rows under names that inverted each other (`Pasture` bare beside `Forage biomass`
	# qualified; `Pasture ecology` qualified beside `Ecology` bare), which a playtest reader mistook
	# one for the other three times.
	#
	# State food_layers — the reference frame: all THREE crop roles on one patch, so every role icon is
	# in one picture and the card states outright that 62% of what grows on this ground is not food.
	h._hud._compose.set_forage_count(1)
	h._show_tile(TileFx.three_role_tile_fixture())
	await h._settle()
	await h._save("tile_food_layers")

	# State food_layers_unstated — the SAME tile with the cash crop's role missing from the wire. `""`
	# means UNSTATED, not "staple", so that row must render NO icon while its two neighbours keep
	# theirs; a defaulted icon here would invent a fact about the plant.
	h._show_tile(_unstated_role_tile_fixture())
	await h._settle()
	await h._save("tile_food_layers_unstated")

	# The three claims a PICTURE cannot carry, asserted over the REAL producer's lines (the harness
	# pokes `_drawer` directly, the `tile_panel_*` idiom). Each is sabotage-verified.
	_assert_food_layer_rows()
	# The basket row's SPECIES tier (issue #339) — driven and PNG-less: it composes lines through the
	# real producer and saves none of them, so nothing it asserts can move a frame. (The 32 shipped
	# icons certainly did move frames; that is the tier landing, not this block.)
	_assert_flora_species_precedence()
	# The FOG half of the same pair (issue #462) — what each web states on a hex the player remembers
	# but cannot see. `tile_sight_remembered` is its frame; these are the claims that frame cannot make.
	_assert_fog_stock_parity()
	# …and the half that pair cannot reach: a remembered hex standing on the FIELD rung, where the
	# patch's ceiling and the ground's own K are different numbers and the card has a genuine pick to
	# make. Driven and PNG-less, like the block above.
	_assert_fog_field_capacity_is_the_ground()

	# State 2-pasture-stressed — the graze drawn down into the stressed band: "Grazing 61 / 240 ·
	# ⚠ Stressed", the phase inline and WARN-amber, identical in label and tint to a stressed herd or
	# patch. (The healthy pair — `Foraging` above `Grazing`, both Thriving — is on `food_tile`.)
	h._show_tile(_overgrazed_tile_fixture())
	await h._settle()
	await h._save("tile_pasture_stressed")

	# State 2-pasture-none — a GLACIER: the biome carries no pasture at all, so the sim holds no patch
	# and the card prints NOTHING about pasture. "0 / 0" would be a lie of a different kind — a starved
	# pasture rather than an absent one — and this frame is the guard against it.
	h._show_tile(_no_pasture_tile_fixture())
	await h._settle()
	await h._save("tile_pasture_none")

	# **THE PASTURE AND FORAGE LEGEND FRAMES MOVED TO `map_preview`** (`map_pasture_legend` /
	# `map_forage_legend`), with the right dock's `L` card they used to render into. They are better
	# off there: this chapter had to TRANSCRIBE `_build_pasture_legend`'s output into a fixture and
	# hope the two stayed in step, while `map_preview` owns a real MapView and can open the picker's
	# legend on the real builder's rows.

	# ---- Hex-edge rivers on the Tile card (ui/RiverEdges.gd, the shared text formatter) -----------
	# State 2-river-both — the interesting case: a tile whose sides carry BOTH classes. The card must
	# read "Major River: NE, NW" then "Minor River: SW" — Major first (the bigger river reads first),
	# directions in compass order from NE clockwise, NOT the sim's bit order (which starts at E).
	h._show_tile(_river_tile_fixture(RIVER_MASK_TWO_CLASS))
	await h._settle()
	await h._save("river_tile_both")

	# State 2-river-minor — a single-class tile: one "Minor River: E, SE" row, no Major row.
	h._show_tile(_river_tile_fixture(RIVER_MASK_SINGLE_CLASS))
	await h._settle()
	await h._save("river_tile_minor")

	# State 2-river-none — mask 0: NO river row at all (not an empty "River:" label).
	h._show_tile(_river_tile_fixture(RIVER_MASK_NONE))
	await h._settle()
	await h._save("river_tile_none")

	# ---- ROADS ON THE TILE CARD (arc #532) -------------------------------------------------------
	#
	# A band roster first, because a road's keeper IS a band and the card names it the way every other
	# surface in this client names one. Put back below.
	var inherited_bands := _with_a_band_roster([BandFx.band_fixture()])
	#
	# ⛔ **AND THE RUNG CATALOG BEFORE ANY OF THE FRAMES, BECAUSE A PLAYER NEVER SEES A CARD WITHOUT
	# ONE.** It is a per-world constant that arrives with the FIRST snapshot, so a road frame rendered
	# without it draws a state the game cannot reach — and it draws it *quietly*, because the missing
	# rate suppresses the `Upkeep` row's shortfall clause rather than failing. The at-risk frame is
	# where that bites: the one picture whose whole job is to show a road in trouble rendered
	# `Upkeep  Band 1` with the gap silently absent. **A frame named for the interesting case must not
	# render the fallback** — the degraded path is asserted where it belongs, in `_road_lines_named`'s
	# own `LADDER_NO_BARE_WORK_RATE` claim, which states it without spending a picture on it.
	h._hud.update_route_rungs(_route_rung_catalog())
	#
	# The ladder's third branch, one frame per rung, on the same piece of ground. Read as a set they
	# are the whole argument the branch exists to make: the block GROWS down the four — the `Upkeep`
	# row gets dearer and the unlabelled payoff gets richer — which is what makes paving a decision
	# rather than an upgrade. **The floor's block is ONE ROW**, and that contrast is the frame.

	# State road-path — THE FLOOR, and it is a SINGLE ROW: `Road  Path · 30% to trail`.
	# Nothing to pay, nobody paying it, nothing bought and nothing at risk, so no other row is drawn
	# at all. It rendered four rows here until issue #566, two of them prose saying *no*.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_PATH, ROAD_METER_RISING, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_PATH, ROAD_LINK_PATH)))
	await h._settle()
	await h._save("road_tile_path")

	# State road-trail — the first rung anyone pays for, with traffic already wearing in the one above
	# it: `Road  Trail · 30% to dirt road`. The meter belongs to the rung being RAISED, so it reads as
	# an APPROACH to that rung and never as *this trail is 30% built*.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, ROAD_DEMAND_TRAIL, 0.0, 2,
		ROAD_GRACE_TRAIL, true, ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL, _road_keeper_band())))
	await h._settle()
	await h._save("road_tile_trail")

	# State road-dirt — the middle rung, complete and nothing rising above it yet (`build_fraction`
	# exactly 1.0, which the wire states rather than leaving to a subtraction), so the rung row reads
	# BARE: `Road  Dirt road`, with no approach clause on it.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		_road_keeper_band())))
	await h._settle()
	await h._save("road_tile_dirt_road")

	# ⛔ State road-remote — **THE SAME DIRT ROAD, KEPT FROM FAR AWAY**, and the frame this half of the
	# card exists for. A road is kept by the band that BUILT it wherever that band has since walked,
	# and beyond the base keeping range it costs a multiple of the rung's own price — priced, and
	# refused nowhere. That is a decision the player makes with no other surface in the client: the
	# `Kept by:` row names the band and says the road is costing more for being far from them, and the
	# `Upkeep:` row above it is the dearer bill the sim quoted.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT_REMOTE, 0.0,
		ROAD_WANTS_DIRT_REMOTE, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		_road_keeper_band(), ROAD_REMOTENESS_REMOTE)))
	await h._settle()
	await h._save("road_tile_remote")

	# State road-paved — the top of the branch. Dearest to keep and richest payoff, which is the
	# whole shape of the decision.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_PAVED_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_PAVED, 0.0, 7,
		ROAD_GRACE_PAVED, true, ROAD_FRICTION_PAVED, ROAD_LINK_PAVED, _road_keeper_band())))
	await h._settle()
	await h._save("road_tile_paved_road")

	# State road-at-risk — the same dirt road with its bill going unpaid: the rung row carries the
	# branch's own consequence word as a clause, the `Upkeep:` row states the SHORTFALL against the
	# bill and the keepers it wants, the `Reverting:` countdown appears, and the unlabelled payoff
	# says the road has gone DARK — which happens BEFORE the rung decays, and is the honest warning.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT,
		ROAD_SHORTFALL_DIRT, ROAD_WANTS_DIRT, ROAD_GRACE_LEFT, false, ROAD_FRICTION_DIRT,
		ROAD_LINK_DIRT, _road_keeper_band())))
	await h._settle()
	await h._save("road_tile_at_risk")

	# …and the keeper's own claims, which a picture states as plausibly wrong as right.
	_assert_road_rows_say_whose_job_it_is()

	# …and the claims a picture cannot carry, over the REAL producer's lines.
	_assert_road_rows_are_conditional()

	# ⛔ **AND THE ONE A DECLARED ROAD MAKES — fix 3.** It runs here, with the roster staged and the
	# catalog not yet pushed, because the claim is about the `Road` ROW alone and needs neither.
	_assert_a_declared_road_says_it_is_climbing()

	# ---- THE ROAD LADDER, the tile card's route ACTION (arc #532 slice 13) ------------------------
	#
	# ⛔ **ONE ACTION OPENS THE WHOLE BRANCH.** Read as a set these frames are the argument the
	# feature exists to make: the gates are carried by the ROWS, each with its own reason and its own
	# remedy, so a branch the player cannot climb today is still a branch they can read and plan
	# against. A button per verb could state none of that, and a single verb-named button could state
	# exactly one refusal.
	# ⛔ **THE REAL PRODUCER PRICES A SHORTFALL IN HANDS.** Every claim above ran the composer directly
	# with a rate handed in; this one runs `SubjectDrawerController._tile_terrain_lines`, which must
	# resolve the rate off `_topbar.route_rungs()` for itself. **That resolution is the thing under
	# test**, and what gives the claim teeth is the SABOTAGE rather than an ordering: gating the
	# drawer's lookup off fails exactly this claim and nothing else in the run. The catalog is seeded
	# up at the head of the road block, where a player's first snapshot puts it.
	_assert_the_real_drawer_prices_a_shortfall()
	var inherited_knowledge: Dictionary = h._hud._topbar.faction_tracks(HudConst.PLAYER_FACTION_ID).duplicate(true)

	# ⛔ **STATE road-ladder-none — A TILE WITH NO ROAD OFFERS NO ACTION.** PNG-less, because the
	# claim is an ABSENCE and a frame of a card without a button is indistinguishable from a frame of
	# one whose button failed to render. The action appears exactly where the `Road` readout row does,
	# so no dead control grows on every hex in the world.
	h._show_tile(_river_tile_fixture(RIVER_MASK_NONE))
	await h._settle()
	h._assert_hud("a hex with no road offers no road action at all",
		_road_ladder_action() == null and not h._hud.road_ladder_controls.visible)

	# ⛔ **STATE road-ladder-gated — THE FLOOR, WITH EVERY RUNG ABOVE IT REFUSED, ONE LINE EACH.**
	# This is the readout a player meets first: a path worn in by their own traffic, a trail rising
	# above it that nobody orders, and two built rungs standing behind a craft. **A road cannot be
	# built on bare ground** — a dirt road wants a trail beneath it and a trail is worn in only by
	# traffic — and this frame is where that reads as a line rather than as a missing button.
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING: LADDER_ROADBUILDING_PART,
		HudFloraVocab.KNOWLEDGE_TRACK_PAVING: 0.0}}])
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_PATH, ROAD_METER_RISING, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_PATH, ROAD_LINK_PATH)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens from the tile card's `Road ▸`", false)
	else:
		var gated_states := _road_ladder_states()
		var gated_faces := _road_ladder_faces()
		var gated_tips := _road_ladder_tooltips()
		h._assert_hud("the ladder states the WHOLE branch — one row per catalog rung, in climb order",
			gated_states.size() == _route_rung_catalog().size())
		h._assert_hud("the road HOLDS the floor, so the path row is where you are",
			String(gated_states.get(HudRouteVocab.RUNG_KEY_PATH, "")) == RungLadder.STATE_STANDING)
		# ⛔ **THE TRAIL IS NOT LOCKED, AND THAT DISTINCTION IS THE POINT.** Nothing refuses it; there
		# is simply no order to give. It leads with its METER, the only figure it has.
		h._assert_hud("a rung nobody orders reads `%s`, its meter and its cause" % ROW_TRAIL_FACE,
			String(gated_states.get(HudRouteVocab.RUNG_KEY_TRAIL, "")) == RungLadder.STATE_UNORDERED
				and String(gated_faces.get(HudRouteVocab.RUNG_KEY_TRAIL, "")) == ROW_TRAIL_FACE)
		# ⛔ **ONE REFUSAL ON THE ROW.** Both built rungs are gated on the craft AND on the ground, and
		# each states the craft: the ground gate names a rung the ladder is already displaying two
		# lines up, so it is the refusal worth least on a line that holds one clause.
		h._assert_hud("a refused rung leads with its PRICE and states ONE refusal, the nearest",
			String(gated_faces.get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, "")) == ROW_DIRT_GATED_FACE
				and String(gated_faces.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					== ROW_PAVED_GATED_FACE)
		# ⛔ **AND THE WORD `locked` IS NOWHERE ON THE CARD.** A row reading `locked` above a reason
		# said it twice; the reason alone IS the state.
		h._assert_hud("no row anywhere on the ladder says `%s`" % LADDER_LOCKED_WORD,
			not _road_ladder_text().contains(LADDER_LOCKED_WORD))
		# ⛔ **NOTHING WAS DELETED — IT MOVED TO THE HOVER.** Every clause the rows stopped printing is
		# asserted here, because a PNG cannot see a tooltip and a cut that lost the detail instead of
		# relocating it would pass every claim above.
		var dirt_tip := String(gated_tips.get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
		h._assert_hud("the hover pairs the price with the standing bill, which the row cannot",
			dirt_tip.contains(TIP_DIRT_PRICE))
		h._assert_hud("…states what the rung does, in plain English",
			dirt_tip.contains(TIP_DIRT_PAYOFF))
		h._assert_hud("…and lists BOTH refusals, including the one the row had no room for",
			dirt_tip.contains(TIP_ROADBUILDING) and dirt_tip.contains(TIP_NEEDS_TRAIL))
		h._assert_hud("…and the rung nobody orders explains itself there too",
			String(gated_tips.get(HudRouteVocab.RUNG_KEY_TRAIL, "")).contains(TIP_WORN_IN))
		var paved_tip := String(gated_tips.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
		h._assert_hud("the top rung's hover carries its own price, payoff and both refusals",
			paved_tip.contains(TIP_PAVED_PRICE) and paved_tip.contains(TIP_PAVED_PAYOFF)
				and paved_tip.contains(TIP_PAVING) and paved_tip.contains(TIP_NEEDS_DIRT))
		# ⛔ **AND NOT ONE ROW IS PRESSABLE.** The shape is the statement: a rung the branch offers is
		# a `Button`, a rung it refuses is a `Label`, so this is the absence that matters most.
		var pressable := 0
		for entry_variant in _route_rung_catalog():
			if _road_ladder_row_button(String((entry_variant as Dictionary)["rung_key"])) != null:
				pressable += 1
		h._assert_hud("nothing on a path's ladder may be ordered — every row is a Label",
			pressable == 0)
		await h._save("road_ladder_gated")
	_dismiss_road_ladder()

	# ⛔ **STATE road-ladder-grade — THE CRAFT IS LEARNED AND THE GROUND IS READY, so `grade` goes
	# LIVE.** Only two things move between this frame and the one above: the road stands a rung
	# higher, and the faction has finished Roadbuilding. That the dirt-road row turns from a refusal
	# into a priced, pressable button is the whole of what the ladder is for.
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING: 1.0,
		HudFloraVocab.KNOWLEDGE_TRACK_PAVING: 0.0}}])
	# ⛔ **STAFF THE BUILD POOL, because the turns estimate is the ACTING BAND'S.** The fixture band
	# has nobody on `builders`, which is a real state with its own frame below — but the ordinary
	# reading of this card is a band that can actually raise the rung, and that is this one.
	BandFx.staff_builders(h._hud._band_labor, LADDER_BUILDERS)
	# ⛔ **AND THE ROAD HAS BANKED NOTHING AND IS QUEUED NOWHERE, which is what makes this the PRICED
	# face.** The approach row takes two faces now — a price on a rung nobody has ordered, its progress
	# on the one being built — so a fixture carrying banked work would render the OTHER one and this
	# state would quietly stop being about the offer at all.
	#
	# ⛔ **`ROAD_METER_COMPLETE` IS HOW *NOTHING IS RISING* IS SPELT ON THE WIRE, not banked work.**
	# With nothing in the dirt road, `road_at_risk_rung` answers with the TRAIL this road holds, which
	# is complete — a `0.0` here would be a meter no server sends, and `_route_climbing` reads the two
	# the same way only by accident.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_COMPLETE, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens on a trail", false)
	else:
		var live_states := _road_ladder_states()
		var live_faces := _road_ladder_faces()
		var live_tips := _road_ladder_tooltips()
		h._assert_hud("a rung already paid for is BANKED — it is a receipt, never a discount",
			String(live_states.get(HudRouteVocab.RUNG_KEY_PATH, "")) == RungLadder.STATE_BANKED)
		# ⛔ **THE PRICE IS THE BUTTON, and there is no refusal beside it.**
		h._assert_hud("with the craft learned and a trail beneath it, `grade` is a pressable `%s`"
			% DIRT_PRICE_FACE,
			String(live_states.get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, "")) == RungLadder.STATE_OPEN
				and _road_ladder_row_button(HudRouteVocab.RUNG_KEY_DIRT_ROAD) != null
				and String(live_faces.get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
					== DIRT_PRICE_FACE)
		# ⛔ **AND THE OTHER HALF OF THAT PAIR: NO DURATION, ANYWHERE ON THE CARD.** The estimate was
		# divided by the acting band's CURRENT builders — in the reported game all of them on a Tame —
		# and it ignored the queue the press would join, so it moved with a crew that was not going to
		# work this road. Ray: *"the turns make no sense and change when the number of builders
		# change."* The positive above passes on a producer that merely REORDERED the clauses; this is
		# what says the estimate is gone from the rows.
		h._assert_hud("…and no row on a card of priced rungs quotes a turns estimate at all",
			not _road_ladder_text().contains(LADDER_TURNS_MARK))
		# ⛔ **THE PLAYER PICKS WHO KEEPS IT, AND THE CARD SAYS SO BEFORE THE RUNG DOES.** It was
		# `_resolve_assign_band()` alone — whichever band the left panel happened to be showing — so a
		# tile graded while reading another band's page became that band's job for good. The row is the
		# compose sheet's own `Band:` field, so the two cannot line up differently or be named
		# differently, and the acting band opens SELECTED rather than blank.
		h._assert_hud("the card carries a `%s` picker, and it opens on the acting band"
				% HudWorkVocab.BAND_PICKER_LABEL,
			_road_ladder_band_face() == HudFormat.band_display_name(
				h._hud._band_labor.player_bands()[0], 1))
		# ⛔ **ABSENCE — nothing to put down on a road nobody keeps.** The abandon control is offered
		# only where the keeper is in the player's roster: there is nothing to release otherwise, and a
		# button that emitted a command the sim refuses is the shape the ladder's rows exist to avoid.
		h._assert_hud("…and offers nothing to put down, this road having no keeper",
			_road_ladder_abandon_button() == null)
		# ⛔ **THE PRESS QUEUES A JOB, AND THE CARD SAYS SO AT THE MOMENT OF THE DECISION.** Ray:
		# *"it isn't obvious that the road will show up in the build queue, so we need something to
		# indicate that when the job is selected."* With this band's queue empty, the press really does
		# start it — and saying so is what makes the other reading legible when it appears.
		h._assert_hud("the buildable rung says where the press LANDS — `%s`"
				% HudRouteVocab.ROAD_LADDER_QUEUE_EMPTY_ASIDE,
			_road_ladder_text().contains(HudRouteVocab.ROAD_LADDER_QUEUE_EMPTY_ASIDE))
		# ⛔ **ABSENCE — the meter does NOT ride a priced row.** It belongs to the rung being raised,
		# and the tile card's `Road` line one block up already states it; repeating it beside a price
		# is the duplication this cut removed. Here the first row above the standing rung is a PRICED
		# one, so no row on the card shows a meter at all.
		h._assert_hud("no priced row carries a meter clause — the tile card states it once",
			not _road_ladder_text().contains(LADDER_METER_MARK))
		# …and the rung above it is refused for TWO reasons and states one, with both on the hover.
		h._assert_hud("the rung above it states its nearest refusal alone",
			String(live_faces.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, "")) == ROW_PAVED_GATED_FACE)
		h._assert_hud("…and its hover carries both, the craft and the ground",
			String(live_tips.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, "")).contains(TIP_PAVING)
				and String(live_tips.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					.contains(TIP_NEEDS_DIRT))
		# ⛔ **ABSENCE — the keeper gate does NOT fire while a band is selected**, on the row or in the
		# hover. It is the one gate a player closes without leaving the card, and stating it here
		# would be an alarm about nothing.
		h._assert_hud("…and nothing complains about a keeper, a band being selected",
			not _road_ladder_text().contains(HudRouteVocab.GATE_SHORT_ROAD_NO_KEEPER)
				and not String(live_tips.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					.contains(TIP_NO_KEEPER))
		await h._save("road_ladder_grade")
		# ⛔ **WHAT THE PRESS WOULD TRANSMIT, through the REAL formatter.** `grade` is
		# `cultivate`/`sow`'s grammar PLUS a band token, and both handles are integers in a positional
		# grammar — so a payload that omitted the band would still PARSE, as a tile-targeted verb
		# aimed at the wrong coordinates. Asserting the LINE is the only way to see that.
		var captured: Array[Dictionary] = []
		var sink := func(p: Dictionary) -> void: captured.append(p)
		h._hud.improvement_requested.connect(sink)
		_road_ladder_row_button(HudRouteVocab.RUNG_KEY_DIRT_ROAD).pressed.emit()
		h._hud.improvement_requested.disconnect(sink)
		var line := "" if captured.is_empty() \
			else String(MAIN_SCRIPT.format_improvement(captured[0]).get("line", ""))
		print("ui_preview: road ladder grade -> %s" % line)
		h._assert_hud("pressing the Dirt Road row would transmit `%s`" % _expected_grade_line(),
			line == _expected_grade_line())
	_dismiss_road_ladder()

	# ⛔ **STATE road-ladder-no-builders — ZERO BUILDERS IS AN ANSWER, NOT AN ABSENCE.** The same trail,
	# the same craft, the same live `grade` row — with nobody on the band's `builders` pool. There is no
	# pace to divide by, so the row states its price bare and the REMEDY rides beneath it in the
	# ladder's own aside shape: a blank column where every other row states a duration reads as a
	# client that failed to work it out rather than as a crew that is missing.
	BandFx.staff_builders(h._hud._band_labor, LADDER_NO_BUILDERS)
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_RISING, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens with nobody on builders", false)
	else:
		h._assert_hud("with nobody on builders the row states its progress alone — `%s`"
			% DIRT_BUILDING_FACE,
			String(_road_ladder_faces().get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
				== DIRT_BUILDING_FACE)
		h._assert_hud("…and the card says WHY, and what to do about it",
			_road_ladder_text().contains(HudRouteVocab.ROAD_LADDER_NO_BUILDERS_ASIDE))
		# ⛔ **AND IT IS STILL PRESSABLE.** Declaring a road with an empty pool is a legal, ordinary
		# act — the entry waits at the head of the queue for hands — so the remedy is a note and never
		# a gate. A row greyed for this would refuse something the sim accepts.
		h._assert_hud("…and the rung is still orderable, an empty pool being a note and not a refusal",
			_road_ladder_row_button(HudRouteVocab.RUNG_KEY_DIRT_ROAD) != null)
		await h._save("road_ladder_no_builders")
	_dismiss_road_ladder()
	BandFx.staff_builders(h._hud._band_labor, LADDER_BUILDERS)

	# ⛔ **STATE road-ladder-queued — WHAT THE PRESS WOULD WAIT BEHIND.** The same live `grade` row on a
	# band whose queue already holds two jobs. This is the reported game: a road declared behind a Tame
	# banks nothing for dozens of turns, and the card said only `300 work · ≈105 turns` — a figure that
	# silently promises the builders are free. **The estimate is kept**, because it is the right number;
	# what the row adds is what it is measured FROM and what stands in front of it.
	#
	# ⛔ **THE QUEUE IS SEEDED, NOT ASSUMED.** A state named for the queued reading that renders the
	# empty one is this arc's own recurring trap, so the entries are staged and put back below.
	var inherited_queue := _with_a_busy_queue()
	# ⛔ **THE METER IS THE ONE A ROAD HOLDING A TRAIL CAN ACTUALLY PUBLISH.** With nothing banked
	# above it the at-risk rung IS the trail, so the wire ships `METER_FULL`; a `0.0` there is a state
	# no server produces, which is what let `Queued 100%` reach play.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_FRESHLY_GRADED, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens on a band with a queue", false)
	else:
		# **THE HEAD IS NAMED AND THE REST ARE COUNTED**, which is the shape of the decision: the head
		# is what the road would wait behind (it takes every builder), and the count is what a reorder
		# in the BUILD QUEUE block is measured against.
		h._assert_hud("the row names what the press would wait behind — `%s`" % _expected_queue_aside(),
			_road_ladder_text().contains(_expected_queue_aside()))
		# ⛔ **AND THE ROW IS STILL A PRICE, BECAUSE THIS ROAD IS NOT QUEUED — THE QUEUE AHEAD IS.**
		# The distinction is the whole reason the aside exists: the entries are on OTHER tiles, so what
		# the row states is what the press would COST, and the aside states what it would wait behind.
		# The queued reading is the state below, where an entry stands on this tile.
		h._assert_hud("…and the row still quotes its price, this road not being queued yet — `%s`"
			% DIRT_PRICE_FACE,
			String(_road_ladder_faces().get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
				== DIRT_PRICE_FACE)
		# ⛔ **ABSENCE — the empty-queue reading is GONE from the card.** Without this, a producer that
		# printed both sentences passes the claim above.
		h._assert_hud("…and the card no longer claims the press starts now",
			not _road_ladder_text().contains(HudRouteVocab.ROAD_LADDER_QUEUE_EMPTY_ASIDE))
		await h._save("road_ladder_queued")
	_dismiss_road_ladder()

	# ⛔ **STATE road-ladder-declared — THE REPORTED SCREENSHOT, and the reading this whole cut exists
	# for.** The same road, with a `grade` now standing on THIS tile behind the two jobs above. It has
	# banked nothing and will bank nothing for dozens of turns, because the head of the queue takes
	# every builder the band has — and the `300 work` that used to stand here was indistinguishable
	# from a `grade` that never landed. `0%` is the receipt for the press.
	_with_a_queued_road()
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_TRAIL, ROAD_METER_FRESHLY_GRADED, 0.0, 0.0, 0, ROAD_GRACE_NONE, false,
		ROAD_FRICTION_TRAIL, ROAD_LINK_TRAIL)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens on a road already queued", false)
	else:
		# ⛔ **AND THE ROW CARRIES NO PLACEMENT ASIDE AT ALL.** Reported from play on this very card:
		# the row one line up already states the progress AND the turns, so a note explaining what
		# that estimate runs from qualified a number the player was looking at — and the head of the
		# queue on that screenshot was this road, so the sentence named it as the thing it waited
		# behind. **The claim is the PAIR with `road_ladder_queued` one state above**, which still
		# draws its `joins …` sentence: a producer that had dropped every aside would pass this
		# negative on its own. The two surviving forms are named individually rather than by a shared
		# needle, so a producer resurrecting either is caught.
		h._assert_hud("a road already on the list draws NO placement aside — neither `joins … behind`"
			+ " nor `starts now`",
			not _road_ladder_text().contains(_expected_queue_aside())
				and not _road_ladder_text().contains(
					HudRouteVocab.ROAD_LADDER_QUEUE_EMPTY_ASIDE))
		h._assert_hud("a DECLARED road reads its progress, not its price — `%s`"
			% DIRT_QUEUED_ZERO_FACE,
			String(_road_ladder_faces().get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
				== DIRT_QUEUED_ZERO_FACE)
		# ⛔ **THE ABSENCE HALF — the price is GONE from that row.** Without it, a producer that printed
		# both would pass the claim above, which is the shape of the defect being fixed.
		h._assert_hud("…and the price it used to quote is gone from the row",
			not String(_road_ladder_faces().get(HudRouteVocab.RUNG_KEY_DIRT_ROAD, ""))
				.contains(LADDER_UPKEEP_WORD))
		# …and the rung ABOVE it is untouched, which is what says only the row being built swaps.
		h._assert_hud("…while the rung above it still quotes its price, `%s`" % ROW_PAVED_GATED_FACE,
			String(_road_ladder_faces().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
				== ROW_PAVED_GATED_FACE)
		await h._save("road_ladder_declared")
	_dismiss_road_ladder()
	_restore_road_queue(inherited_queue)
	await h._settle()

	# ⛔ **STATE road-ladder-pave — THE TOP OF THE BRANCH, KEPT FROM FAR AWAY.** Paving is learned, the
	# ground is a dirt road, and the row is live. The keeper is beyond the base keeping range, so the
	# ROW states the rung's BASE price and the HOVER states the multiple distance charges: the client
	# never multiplies the two, because that would put a copy of the sim's pricing formula where it
	# can drift.
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING: 1.0,
		HudFloraVocab.KNOWLEDGE_TRACK_PAVING: 1.0}}])
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT_REMOTE, 0.0,
		ROAD_WANTS_DIRT_REMOTE, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		_road_keeper_band(), ROAD_REMOTENESS_REMOTE)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens on a dirt road", false)
	else:
		var pave_states := _road_ladder_states()
		var pave_faces := _road_ladder_faces()
		var pave_tip := String(_road_ladder_tooltips().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
		h._assert_hud("with Paving learned on a dirt road, the top rung is a pressable `%s`"
			% PAVED_PRICE_FACE,
			String(pave_states.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, "")) == RungLadder.STATE_OPEN
				and _road_ladder_row_button(HudRouteVocab.RUNG_KEY_PAVED_ROAD) != null
				and String(pave_faces.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					== PAVED_PRICE_FACE)
		# **AND THE HOVER STATES THE SAME DURATION**, which is the surface a REFUSED rung has left once
		# its one face clause is spent on the refusal — so a rung the player is planning toward is still
		# a rung they can plan against.
		h._assert_hud("…and the hover states the duration too, for the rows whose face cannot",
			pave_tip.contains(TIP_PAVED_TURNS))
		h._assert_hud("…and the hover states what distance does to that price, never folded in",
			pave_tip.contains(TIP_REMOTE))
		h._assert_hud("…the richest payoff on the branch, and the dearest bill to hold",
			pave_tip.contains(TIP_PAVED_PAYOFF) and pave_tip.contains(TIP_PAVED_PRICE))
		# ⛔ **ABSENCE — the meter is exactly `1.0`, so nothing is rising and no row states one.**
		# The wire publishes that `1.0` rather than leaving it to a subtraction, which is what makes
		# this a plain comparison.
		h._assert_hud("a road with nothing rising above it states no meter at all",
			not _road_ladder_text().contains(LADDER_METER_MARK))
		# ⛔ **AND THIS ROAD IS ALREADY YOURS, SO IT CAN BE PUT DOWN.** `unqueue` withdraws a
		# DECLARATION; the moment any work is banked the verb that releases a keeper is `abandon`,
		# which was command-line only — so a road handed to the wrong band could not be taken back from
		# the UI at all. With a band picker above making the keeper a real choice, this is what makes
		# it a reversible one.
		h._assert_hud("a road this band keeps offers a way to stop keeping it",
			_road_ladder_abandon_button() != null)
		await h._save("road_ladder_pave")
		# ⛔ **WHAT THE PRESS WOULD TRANSMIT, through the REAL formatter — and it names a PLACE.**
		# `abandon <faction> <x> <y>` carries NO band token: it drops every band of the faction's
		# holding on that hex, road and patch alike. A client that invented a narrower, road-only form
		# would be lying about what the button does.
		var dropped: Array[Dictionary] = []
		var drop_sink := func(p: Dictionary) -> void: dropped.append(p)
		h._hud.abandon_requested.connect(drop_sink)
		_road_ladder_abandon_button().pressed.emit()
		h._hud.abandon_requested.disconnect(drop_sink)
		var drop_line := "" if dropped.is_empty() \
			else String(MAIN_SCRIPT.format_abandon(dropped[0]).get("line", ""))
		print("ui_preview: road ladder abandon -> %s" % drop_line)
		h._assert_hud("pressing it would transmit `%s`" % _expected_abandon_line(),
			drop_line == _expected_abandon_line())
	_dismiss_road_ladder()

	# ⛔ **STATE road-ladder-other-keeper — THE TILE IS ALREADY SOMEBODY ELSE'S JOB.**
	#
	# `road_verb_refusal` rejects `grade` / `pave` outright when `Road::keeper` names a band other
	# than the one issuing the verb: **one band keeps a road tile, never two.** The ladder did not ask
	# that question for a slice, so the row rendered READY, the player pressed it, and what came back
	# was a command-failure event where a greyed row with a reason belonged.
	#
	# **A SECOND BAND IS STAGED FOR IT, because the claim is about NAMING the keeper.** With only the
	# acting band on the roster every keeper is either itself or a stranger, and *another people* — a
	# true sentence — would not prove the label plumbing works at all. Put back below.
	var inherited_two_bands := _with_a_band_roster([BandFx.band_fixture(), _second_band_fixture()])
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		_second_band_keeper_id())))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens on a road another band keeps", false)
	else:
		# ⛔ **THE CARD OPENS ON THE KEEPER, and that is fix 1's second rule rendered.** Re-issuing on a
		# road you already keep is the ORDINARY case (trail → dirt → paved); a default that took the
		# nearest band instead would open this card on a rung the sim refuses outright, greying its own
		# live row on the frame it appeared.
		h._assert_hud("the card opens on the band that already KEEPS the road, not the nearest one",
			_road_ladder_band_face() == HudFormat.band_display_name(
				h._hud._band_labor.player_bands()[1], 2))
		h._assert_hud("…so the top rung is OPEN for its own keeper, refused for nobody",
			String(_road_ladder_states().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					== RungLadder.STATE_OPEN
				and _road_ladder_row_button(HudRouteVocab.RUNG_KEY_PAVED_ROAD) != null)
		# ⛔ **NOW PICK THE OTHER BAND — AND THE CARD MUST STAY UP AND RE-RENDER.** Every gate on the
		# track is resolved against the acting band, so a pick that left the rows standing would offer
		# a rung the newly chosen band cannot have. This is the pick, the re-render and the gate in one
		# claim: after it the top rung must be refused and must name the keeper.
		await _pick_road_ladder_band(0)
		h._assert_hud("picking another band RE-RENDERS the card in place rather than closing it",
			_road_ladder_card(h._hud) != null and not _road_ladder_states().is_empty()
				and _road_ladder_band_face() == HudFormat.band_display_name(
					h._hud._band_labor.player_bands()[0], 1))
		var taken_states := _road_ladder_states()
		var taken_faces := _road_ladder_faces()
		var taken_tip := String(_road_ladder_tooltips().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
		h._assert_hud("a rung on a tile another band keeps is refused, not offered",
			String(taken_states.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					== RungLadder.STATE_LOCKED
				and _road_ladder_row_button(HudRouteVocab.RUNG_KEY_PAVED_ROAD) == null)
		# ⛔ **THE ROW NAMES WHO, by the band's own name** — resolved through this client's one
		# band-naming rule, the same one the tile card's `Upkeep:` row above it uses, so a road's
		# keeper cannot be called two different things on one card. **It OUTRANKS the craft gate**:
		# no amount of learning helps a tile that is already somebody else's job.
		h._assert_hud("…and the ROW leads with the price and names the band whose job it is",
			String(taken_faces.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, "")) == ROW_PAVED_TAKEN_FACE)
		h._assert_hud("…with what has to happen first on the hover",
			taken_tip.contains(TIP_ANOTHER_KEEPER))
		# ⛔ **ABSENCE — the OTHER keeper gate stays quiet.** A band IS selected here, so *pick a band
		# first* would be an alarm about nothing and would hide the refusal that actually applies.
		# The two gates are one word apart in the player's head and must never both fire.
		h._assert_hud("…and does not also ask for a band, one being selected",
			not _road_ladder_text().contains(HudRouteVocab.GATE_SHORT_ROAD_NO_KEEPER)
				and not taken_tip.contains(TIP_NO_KEEPER))
		await h._save("road_ladder_other_keeper")
	_dismiss_road_ladder()
	_restore_band_roster(inherited_two_bands)
	await h._settle()

	# ⛔ **STATE road-ladder-no-keeper — THE SAME ROAD WITH NOBODY TO NAME AS KEEPER.** `grade` and
	# `pave` carry a band token and that token IS the keeper: issuing the verb declares the job and
	# names who is on the hook for its standing bill, which are one act. `Main.IMPROVEMENT_NO_BAND`
	# refuses a payload carrying none rather than guessing one, so the row has to say so BEFORE the
	# press instead of failing silently after it.
	var held_bands: Array = h._hud._band_labor.player_bands().duplicate(true)
	var held_acting: Dictionary = h._hud._band_labor.player_band().duplicate(true)
	var held_panel: Dictionary = h._hud._band_labor.panel_band().duplicate(true)
	# **THE KEEPER IS READ WHILE THERE IS STILL A ROSTER TO READ IT FROM.** Asked after the clear it
	# answers `ROAD_NO_KEEPER`, and the frame would quietly become a road nobody keeps rather than the
	# kept road this state is about.
	var kept_by := _road_keeper_band()
	h._hud.update_band_alerts([])
	# **THE PANEL BAND IS CLEARED SEPARATELY, because `_resolve_assign_band` asks it SECOND.** An
	# empty roster alone leaves the previous chapter's panel subject standing and the gate never
	# fires — the state would render as its own opposite with nothing on screen to say so.
	h._hud._band_labor.set_panel_band({})
	# ⛔ **AND THE TILE IS RE-SHOWN RATHER THAN INHERITED.** This state used to lean on whatever hex
	# the frame before it had left up, which was the remote dirt road — until the other-keeper state
	# was inserted between them and it silently began asserting against THAT hex instead. The claim
	# still held, but the frame was a road kept by Band 2 under a ladder saying `pick a band`, which
	# is a confusing picture of a correct rule. Stating the tile makes the state independent of its
	# neighbours, which is what every other state here already does.
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT_REMOTE, 0.0,
		ROAD_WANTS_DIRT_REMOTE, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		kept_by, ROAD_REMOTENESS_REMOTE)))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder still opens with no band selected", false)
	else:
		var keeper_states := _road_ladder_states()
		var keeper_faces := _road_ladder_faces()
		var keeper_tip := String(_road_ladder_tooltips().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
		h._assert_hud("with nobody to keep it, the top rung is refused rather than offered",
			String(keeper_states.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
					== RungLadder.STATE_LOCKED
				and _road_ladder_row_button(HudRouteVocab.RUNG_KEY_PAVED_ROAD) == null)
		h._assert_hud("…and the ROW asks for a band in three words, beside the price",
			String(keeper_faces.get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
				== ROW_PAVED_NO_BAND_FACE)
		h._assert_hud("…with WHY on the hover, since whoever builds a road keeps it",
			keeper_tip.contains(TIP_NO_KEEPER))
		# ⛔ **ABSENCE — nothing else is wrong here.** The craft is learned and the ground is ready, so
		# a card that also stated either would be blaming the wrong thing — on the row OR in the hover.
		h._assert_hud("…and complains about nothing else, the craft and the ground both being ready",
			not keeper_tip.contains(TIP_PAVING) and not keeper_tip.contains(TIP_NEEDS_DIRT))
		await h._save("road_ladder_no_keeper")
	_dismiss_road_ladder()
	# ⛔ **THE CLAIM THE `earns_knowledge` FIELD EXISTS FOR — a catalog where the rung that TEACHES a
	# craft is NOT the rung directly beneath the one it gates.** PNG-less: every one of these frames
	# renders a perfectly plausible card whichever rung the remedy names, and the whole difference is
	# one word in one sentence.
	#
	# On the shipped ladder the two rules agree — a trail teaches Roadbuilding and sits directly under
	# the dirt road it opens — so a fixture built on it cannot tell them apart, and the wrong rule
	# passed every claim above for a slice. Here **the TRAIL teaches Paving** while `paved_road` still
	# requires `dirt_road`, so *the rung beneath* and *the rung that teaches it* are different rungs and
	# name different words. `intensification_ladder.json` is free to do exactly this, and a gate reason
	# is a REMEDY: naming the wrong rung there sends the player to stand on the wrong ground.
	h._hud.update_route_rungs(_twisted_teaching_catalog())
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING: 1.0,
		HudFloraVocab.KNOWLEDGE_TRACK_PAVING: 0.0}}])
	h._show_tile(_road_tile_fixture(_road_fixture(
		HudRouteVocab.RUNG_KEY_DIRT_ROAD, ROAD_METER_COMPLETE, ROAD_DEMAND_DIRT, 0.0,
		ROAD_WANTS_DIRT, ROAD_GRACE_DIRT, true, ROAD_FRICTION_DIRT, ROAD_LINK_DIRT,
		_road_keeper_band())))
	await h._settle()
	if not await _open_road_ladder():
		h._assert_hud("the road ladder opens against a catalog of its own", false)
	else:
		# **THE CLAIM IS ON THE HOVER**, that being where a gate's remedy went when the row was cut
		# to one clause — the row says `needs Paving` under either rule and cannot tell them apart.
		var twisted_tip := String(_road_ladder_tooltips().get(HudRouteVocab.RUNG_KEY_PAVED_ROAD, ""))
		h._assert_hud("the craft gate names the rung that TEACHES it, wherever the config puts that rung",
			twisted_tip.contains(TIP_PAVING_BY_TRAIL))
		# ⛔ **AND THIS IS THE HALF THAT FAILS UNDER THE OLD RULE.** `Dirt Road` is the rung directly
		# beneath `Paved Road`, so an inference off `requires_rung` produces exactly this sentence —
		# which is also the sentence the SHIPPED catalog produces, and therefore the sharpest possible
		# discriminator between the two rules.
		h._assert_hud("…and never the rung merely BENEATH it, which is what the retired inference read",
			not twisted_tip.contains(TIP_PAVING))
	_dismiss_road_ladder()
	# The real catalog back, before anything else renders against it.
	h._hud.update_route_rungs(_route_rung_catalog())

	# **PUT THE BANDS AND THE KNOWLEDGE BACK**, this chapter's own rule for shared walk state: the
	# chapters after it render against whatever they inherit.
	h._hud.update_band_alerts(held_bands)
	h._hud._band_labor._player_band = held_acting
	h._hud._band_labor.set_panel_band(held_panel)
	h._hud.update_intensification([{"faction": 0, "knowledges": inherited_knowledge}])
	await h._settle()

	# **PUT THE ROSTER BACK.** Everything after this chapter renders against whatever roster it
	# inherits, and a band left standing here is a band the next chapter never asked for.
	_restore_band_roster(inherited_bands)
	await h._settle()

# ---- THE ROAD LADDER'S OWN FIXTURE NUMBERS AND ITS EXPECTED SENTENCES ---------------------------
#
# ⛔ **THE PRICES AND BILLS ARE THE SHIPPED LADDER'S, TRANSCRIBED** — `intensification_ladder.json`'s
# route branch — so the frames state the real ladder rather than round numbers, and the assertions
# below test the CONVERSION rather than arithmetic on invented inputs.

## `route:trail` declares a `work_cost` and NO verb. Inside the free floor that figure is a duration
## in TRAFFIC rather than a crew's job, which is exactly why the ladder must state no price on that
## row — the fixture carries it so the suppression is tested against a real number.
## ⛔ **THE SIM'S BARE WORKER OUTPUT, ON EVERY ROW — `intensification::PER_WORKER_OUTPUT`.** It is the
## one field on `RouteRungState` that is not derived from the rung, and it is the same value on all
## four: the catalog is where a number identical for every road in the world belongs, and the client
## reads it off the FIRST row on that strength (`core_sim/tests/route_wire.rs` asserts the agreement).
##
## **Stated on all four here anyway, deliberately.** A fixture that carried it on one row would let a
## client that started walking rows pass while a client reading a different row failed, which is a
## fixture testing its own shape rather than the wire's.
const LADDER_BARE_WORK_RATE := 1.0

## …and the reading of a client that has not been sent a catalog. **A measured nothing, not a
## sentinel**: it is what `branch_build_work_per_worker_turn` answers for an empty ladder, and every
## caller must state nothing rather than substitute a rate of its own.
const LADDER_NO_BARE_WORK_RATE := 0.0

const LADDER_TRAIL_WORK_COST := 40.0
const LADDER_DIRT_WORK_COST := 300.0
const LADDER_PAVED_WORK_COST := 800.0
## …and the two built rungs' standing bills, per turn, BEFORE a tile's own load scales them.
const LADDER_DIRT_UPKEEP := 0.45
const LADDER_PAVED_UPKEEP := 0.95

## A faction part-way through Roadbuilding — the gate's live progress, and the reading its reason
## quotes. **Deliberately not zero**: a `0%` reason passes a composer that had stopped reading the
## faction's row at all.
const LADDER_ROADBUILDING_PART := 0.4
const LADDER_ROADBUILDING_PERCENT := 40

## ⛔ **THE STRINGS, WRITTEN OUT — AND THE ROW AND THE HOVER ARE ASSERTED SEPARATELY.** An expectation
## recomposed from the same format the producer uses passes against a producer that has stopped
## composing it, which is the whole failure these claims exist to catch.
##
## **The split is the point of this whole cut.** A ladder row now carries one clause and everything it
## used to print is on its `tooltip_text`, so a change that DROPPED the detail instead of RELOCATING
## it would leave every visible-row claim below passing and every `TIP_*` claim failing. A rendered
## frame cannot see a tooltip; that is why they are asserted here.

# ---- WHAT A ROW SAYS: `<figure> · <nearest refusal>`, and nothing else --------------------------

## The rung nobody orders leads with its METER, that being the only figure it has.
const ROW_TRAIL_FACE := "30% · wearing in"
## …and a rung nobody has started leads with its PRICE — **the pile AND the standing bill**, which is
## the whole commitment: one-off against per-turn. Both of these rungs have TWO unmet gates and state
## one, the craft, because the ground gate names a rung the ladder is already showing two lines up.
##
## ⛔ **THE RATE IS WRITTEN TO TWO DECIMALS** (`HudRouteVocab.ROAD_LADDER_RATE_DECIMALS`). The shared
## `DetailFormat.format_work_units` rounds to ONE, which prints the config's `0.45` as `0.5` and its
## `0.95` as `1.0` — an 11% lie about the figure the player is deciding against. An expectation
## written to the one-decimal spelling would pass a client that had gone back to it.
const ROW_DIRT_GATED_FACE := "300 work · 0.45/turn upkeep · needs Roadbuilding"
const ROW_PAVED_GATED_FACE := "800 work · 0.95/turn upkeep · needs Paving"
## …the keeper pair, which outrank the craft — no amount of learning helps a tile that is taken, and
## picking a band is the one gate on the card closed with a click.
const ROW_PAVED_TAKEN_FACE := "800 work · 0.95/turn upkeep · Band 2 keeps it"
const ROW_PAVED_NO_BAND_FACE := "800 work · 0.95/turn upkeep · pick a band"
## …and a BUILDABLE rung nobody has started, whose price is the button and which has no refusal to
## state beside it. **It states no DURATION at all**, and that is deliberate rather than an omission:
## the estimate was divided by the acting band's CURRENT builders and ignored the queue the press
## would join, so it moved with a crew that was not going to work this road.
const DIRT_PRICE_FACE := "300 work · 0.45/turn upkeep"
const PAVED_PRICE_FACE := "800 work · 0.95/turn upkeep"

## ⛔ **THE NEEDLE FOR "THIS ROW QUOTES A DURATION"**, for the ABSENCE half of the pair above. The
## glyph is `DetailFormat`'s own approximation mark, which no other clause on this card uses, so it
## finds a turns estimate and nothing else.
const LADDER_TURNS_MARK := "≈"

## ⛔ **AND THE WORD A ROW THAT OWES NOTHING TO HOLD MUST NOT CARRY.** The free floor declares
## `upkeep_work_per_turn` of `0`, and `0/turn upkeep` there would be a bill where there is no bill.
const LADDER_UPKEEP_WORD := "upkeep"

## ⛔ **AND THE FACE OF THE RUNG THAT IS BEING BUILT — its PROGRESS, and how long is left.** A rung
## already ordered is not a purchase being weighed, so a price on it answers a question nobody is
## asking; what the player pressed the ladder to find out is whether the press LANDED.
##
## **THE NUMBER IS WRITTEN OUT, and it is a claim about a term the face cannot show on its own.** The
## QUEUED road two states on has banked nothing against the dirt road, so its pile is the full `300`
## and at two builders that is `ceil(300 / 2) = 150` — a reader that netted the wire's `1.0` off the
## pile would quote `≈1 turn` there instead, which is the boundary `_route_turns` draws.
const LADDER_BUILDERS := 2
## …and the pool the fixture band ships with, which is nobody. Named because it is the STATE the frame
## below is about (a band that has staffed no builders), not an unset value.
const LADDER_NO_BUILDERS := 0
## **THE FACE OF A ROW BEING BUILT WITH NOBODY ON `builders`**: there is no pace to divide by, so the
## progress stands alone. **`30%` and not a blank** — the meter is a fact about the road, not about
## the crew.
const DIRT_BUILDING_FACE := "30%"
## ⛔ **…AND THE READING THIS WHOLE CUT EXISTS FOR: a road QUEUED and banking nothing yet.** `0%` is
## the receipt for the press — a road queued behind another job banks nothing for dozens of turns, and
## the `300 work` that used to stand here was indistinguishable from a `grade` that never landed.
const DIRT_QUEUED_ZERO_FACE := "0% · ≈150 turns"
## …and the same figure as the hover states it, on a REFUSED row, where the face has spent its one
## clause on the refusal.
const TIP_PAVED_TURNS := "≈400 turns with this band's builders."

## ⛔ **THE MARKER FOR "THIS ROW SHOWS A METER"**, for the ABSENCE claim. A face leads with a
## PERCENTAGE only where the rung is worn in by traffic or is being BUILT; a rung nobody has ordered
## leads with its price, so this needle finds a meter clause and nothing else.
const LADDER_METER_MARK := "% · "

## ⛔ **THE WORD THAT MUST NOT APPEAR ON ANY ROW.** A row reading `locked` above a reason said it
## twice; the reason alone is the state, and the row stays disabled by its ink and by being a Label.
const LADDER_LOCKED_WORD := "locked"

# ---- WHAT THE HOVER SAYS: everything the row stopped saying ------------------------------------

## The price PAIRED with the standing bill, as a sentence — the ROW now states those same two figures
## one line away, and **the hover spells the rate exactly as the row spells it**: one number told two
## ways on one card is the defect, and the one-decimal spelling (`0.5`, `0.9`) was the wrong one.
const TIP_DIRT_PRICE := "300 work to build, 0.45 work a turn to keep."
const TIP_PAVED_PRICE := "800 work to build, 0.95 work a turn to keep."

## What each rung does, in plain English — the jargon (`buys`, `lost hauling`, `lights its tiles`,
## `links N tiles out`) is retired from every road surface.
const TIP_DIRT_PAYOFF := "40% less loss · you can see along it · links camps up to 10 tiles apart"
const TIP_PAVED_PAYOFF := "65% less loss · you can see along it · links camps up to 16 tiles apart"

## Every refusal, as a sentence — including the ones the row had no room for.
const TIP_WORN_IN := "Traffic wears this in. There is nothing to order."
const TIP_NEEDS_TRAIL := "Needs a trail first."
const TIP_NEEDS_DIRT := "Needs a dirt road first."
## ⛔ **THE CRAFT IS NAMED FROM THE LADDER'S OWN KNOWLEDGE ROSTER**, and the remedy names the rung that
## TEACHES it — never the rung merely beneath the one it gates.
const TIP_ROADBUILDING := "Roadbuilding known 40%. Learn it from a busy trail."
const TIP_PAVING := "Paving known 0%. Learn it from a busy dirt road."
## …the same gate under a catalog where the TRAIL teaches Paving. The const above is then the wrong
## answer, and the fixture asserts its absence — see `_twisted_teaching_catalog`.
const TIP_PAVING_BY_TRAIL := "Paving known 0%. Learn it from a busy trail."
## ⛔ **`Band 2` IS THE SECOND BAND ON THE STAGED ROSTER**, which `HudBandLaborState.band_label_for_id`
## names by roster index — so this claim covers the label plumbing as well as the gate, and a keeper
## resolved as `another people` (the honest fallback for a band outside the roster) would fail it.
const TIP_ANOTHER_KEEPER := "Band 2 keeps it. They must give it up first."
const TIP_NO_KEEPER := "Pick a band first. Whoever builds a road keeps it."
## ⛔ **WHAT DISTANCE DOES TO THE PRICE**, stated APART from the base figure because multiplying the
## two client-side would put a copy of the sim's pricing formula where it can drift.
const TIP_REMOTE := "Far from your band, so it costs ×2.0."

## The second staged band's `entity`, deliberately unlike the fixture band's 904 so its derived
## `band_id` cannot collide with the acting band's — the collision would make the gate answer *this
## is your own road* and pass every claim below for the wrong reason.
const SECOND_BAND_ENTITY := 907

## **THE LINE A PRESS ON THE DIRT ROAD ROW WOULD TRANSMIT.** Composed from the FIXTURE's own values
## rather than read off the payload, so the claim is about the shipped grammar: `grade` is
## `cultivate`/`sow`'s form PLUS a band token, and both handles are integers in a positional grammar —
## the fixture's band id and the tile's `x` are deliberately different numbers, so a builder that
## dropped the band would produce a line that still parses and grades the wrong hex.
func _expected_grade_line() -> String:
	return "%s %d %d %d %d" % [SourceForecast.IMPROVEMENT_GRADE, HudConst.PLAYER_FACTION_ID,
		_road_keeper_band(), ROAD_TILE.x, ROAD_TILE.y]

## **AND THE LINE THE ABANDON ROW WOULD TRANSMIT — three tokens, and NO BAND.** `abandon` names a
## faction and a PLACE: it drops every band of that faction's holding on the hex, the road's keeper and
## its queue entry with it. The band token that `grade` carries is absent here on purpose, and stating
## the whole line is the only way to see that a builder has not helpfully added one.
func _expected_abandon_line() -> String:
	return "abandon %d %d %d" % [HudConst.PLAYER_FACTION_ID, ROAD_TILE.x, ROAD_TILE.y]

## ---- THE ROAD LADDER, the tile card's route ACTION (arc #532 slice 13) -------------------------
##
## ⛔ **THE UNIT THE PLAYER PRESSES IS THE LADDER, NOT THE VERB**, and the frames below exist to hold
## that: one action opens the whole branch, one row per rung in climb order, each carrying its own
## price, its own payoff and its own gate. A button per verb does not scale (highways and railways
## are RUNGS) and one verb-named button forces a single refusal string, which cannot answer *"paving
## is out of reach but railroad is not"*.

## **THE ROUTE BRANCH'S RUNG CATALOG, TRANSCRIBED FROM THE SHIPPED LADDER** — the shape
## `native/src/dict/routes.rs::route_rungs_to_array` decodes off `SubsistenceSection.routeRungs`.
##
## ⛔ **A TRANSCRIPTION, DELIBERATELY NOT A DERIVATION.** A fixture that recomputed the catalog would
## pass against a producer that had stopped producing one; what this file proves is that the CLIENT
## renders whatever catalog arrives. The claim that the transcription matches `intensification_ladder.json`
## is the sim's own (`core_sim/tests/route_wire.rs`).
##
## The three `""` fields are STATES rather than absences — no verb on the free floor, no unlock where
## nothing gates, no prerequisite at the branch's bottom — and every one of them is a row the ladder
## has to say something different about.
func _route_rung_catalog() -> Array:
	return [
		{
			"rung_key": HudRouteVocab.RUNG_KEY_PATH, "order": 1, "display_name": "Path",
			"verb": "", "unlock_knowledge": "", "requires_rung": "", "earns_knowledge": "",
			"work_cost": 0.0, "upkeep_work_per_turn": 0.0,
			"friction_multiplier": ROAD_FRICTION_PATH, "holds_link_to_tiles": ROAD_LINK_PATH,
			"grants_sight": false,
			"build_work_per_worker_turn": LADDER_BARE_WORK_RATE,
		},
		{
			# **A TRAIL COSTS 40 WORK AND DECLARES NO VERB**, which is the config's own shape: inside
			# the free floor the figure is a duration in TRAFFIC rather than a crew's job. The ladder
			# must therefore state no PRICE on this row — a `40 work` face beside a rung nobody can
			# order reads as a job going begging.
			"rung_key": HudRouteVocab.RUNG_KEY_TRAIL, "order": 2, "display_name": "Trail",
			"verb": "", "unlock_knowledge": "", "requires_rung": HudRouteVocab.RUNG_KEY_PATH,
			# **THE TRAIL TEACHES ROADBUILDING**, which opens the rung directly above it — the
			# coincidence that made reading `requires_rung` for the remedy look correct.
			"earns_knowledge": HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING,
			"work_cost": LADDER_TRAIL_WORK_COST, "upkeep_work_per_turn": 0.0,
			"friction_multiplier": ROAD_FRICTION_TRAIL, "holds_link_to_tiles": ROAD_LINK_TRAIL,
			"grants_sight": false,
			"build_work_per_worker_turn": LADDER_BARE_WORK_RATE,
		},
		{
			"rung_key": HudRouteVocab.RUNG_KEY_DIRT_ROAD, "order": 3, "display_name": "Dirt Road",
			"verb": SourceForecast.IMPROVEMENT_GRADE,
			"unlock_knowledge": HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING,
			"requires_rung": HudRouteVocab.RUNG_KEY_TRAIL,
			"earns_knowledge": HudFloraVocab.KNOWLEDGE_TRACK_PAVING,
			"work_cost": LADDER_DIRT_WORK_COST, "upkeep_work_per_turn": LADDER_DIRT_UPKEEP,
			"friction_multiplier": ROAD_FRICTION_DIRT, "holds_link_to_tiles": ROAD_LINK_DIRT,
			"grants_sight": true,
			"build_work_per_worker_turn": LADDER_BARE_WORK_RATE,
		},
		{
			"rung_key": HudRouteVocab.RUNG_KEY_PAVED_ROAD, "order": 4, "display_name": "Paved Road",
			"verb": SourceForecast.IMPROVEMENT_PAVE,
			"unlock_knowledge": HudFloraVocab.KNOWLEDGE_TRACK_PAVING,
			"requires_rung": HudRouteVocab.RUNG_KEY_DIRT_ROAD,
			# The top of the branch teaches nothing: there is nothing above it to open.
			"earns_knowledge": "",
			"work_cost": LADDER_PAVED_WORK_COST, "upkeep_work_per_turn": LADDER_PAVED_UPKEEP,
			"friction_multiplier": ROAD_FRICTION_PAVED, "holds_link_to_tiles": ROAD_LINK_PAVED,
			"grants_sight": true,
			"build_work_per_worker_turn": LADDER_BARE_WORK_RATE,
		},
	]

## ⛔ **A CATALOG WHOSE TEACHING RUNG IS NOT THE RUNG BENEATH — the ONE fixture that can tell the two
## rules apart.**
##
## `_route_rung_catalog` above is the shipped ladder, and on it *the rung beneath the gated one* and
## *the rung that teaches its craft* are the SAME rung on both steps. Every claim this chapter makes
## against it therefore passes under either rule, which is exactly how the wrong one survived a slice.
##
## This one moves ONE field and nothing else: **the TRAIL earns Paving** (the dirt road now teaches
## nothing), while `paved_road` still requires `dirt_road`. So the remedy must read *keep a trail*,
## and an inference off `requires_rung` reads *keep a dirt road* — which is also what the shipped
## catalog reads, making it the sharpest discriminator available.
##
## Built by MUTATING the shipped catalog rather than by restating it, so a rung added to the real
## fixture arrives here too and the two cannot drift into testing different ladders.
func _twisted_teaching_catalog() -> Array:
	var twisted: Array = []
	for entry_variant in _route_rung_catalog():
		var entry: Dictionary = (entry_variant as Dictionary).duplicate(true)
		var key := String(entry[HudRouteVocab.RUNG_CATALOG_KEY])
		if key == HudRouteVocab.RUNG_KEY_TRAIL:
			entry[HudRouteVocab.RUNG_CATALOG_EARNS_KNOWLEDGE] = \
				HudFloraVocab.KNOWLEDGE_TRACK_PAVING
		elif key == HudRouteVocab.RUNG_KEY_DIRT_ROAD:
			entry[HudRouteVocab.RUNG_CATALOG_EARNS_KNOWLEDGE] = ""
		twisted.append(entry)
	return twisted

## Every `Control` under `root` carrying `meta`. `Q.find_meta_node` answers the FIRST — which is the
## right shape for a chart or a verdict, and the wrong one for a card of four rows.
func _collect_meta(root: Node, meta: String, out: Array[Control] = []) -> Array[Control]:
	if root == null:
		return out
	if root is Control and (root as Control).has_meta(meta):
		out.append(root as Control)
	for child in root.get_children():
		_collect_meta(child, meta, out)
	return out

## The tile card's `Road ▸` action, found by META — its face is one word a future rung could change.
func _road_ladder_action() -> Button:
	for control in _collect_meta(h._hud, HudRouteVocab.ROAD_LADDER_ACTION_META, []):
		if control is Button:
			return control as Button
	return null

## Press the action and let the card land. `false` when there was nothing to press, which every
## caller reports as its own failure rather than going on to assert about an empty card.
func _open_road_ladder() -> bool:
	var action := _road_ladder_action()
	if action == null:
		return false
	action.pressed.emit()
	await h._settle()
	return not _road_ladder_states().is_empty()

## Take the card down between states. A `PopupPanel` is a Window and outlives the render that opened
## it, so a frame saved with the previous state's ladder still up is the wrong picture with no tell.
func _dismiss_road_ladder() -> void:
	h._hud._drawercompose._dismiss_road_ladder()

## The open card's rows as `rung_key -> state`. **Keyed by RUNG and never by the improvement verb**:
## two route rungs declare NO verb, so `RUNG_TRACK_ROW_META` carries `""` for both and an assertion
## on it would testify about whichever of them it found first.
func _road_ladder_states() -> Dictionary:
	var out: Dictionary = {}
	for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META, []):
		out[String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META))] = \
			String(control.get_meta(HudWorkVocab.RUNG_TRACK_STATE_META))
	return out

## …and each row's right-hand FACE — the state word where it states one, the price where it does not.
func _road_ladder_faces() -> Dictionary:
	var out: Dictionary = {}
	for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META, []):
		var key := String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META))
		for child in control.get_children():
			if child is Button:
				out[key] = (child as Button).text
			elif child is Label and (child as Label).horizontal_alignment \
					== HORIZONTAL_ALIGNMENT_RIGHT:
				out[key] = (child as Label).text
	return out

## ⛔ **EACH ROW'S HOVER, keyed by rung** — where every clause the row stopped printing now lives. A
## cut that DROPPED the detail rather than relocating it is the failure mode of this whole task, and a
## rendered frame cannot see a tooltip, so these are asserted beside the visible faces.
##
## Read off the LINE (the `HBoxContainer` carrying the row metas). The face control carries the same
## string — a `Button` answers a hover with its own tooltip, so both are set — and reading the line is
## the one place that is true for a Label row and a Button row alike.
func _road_ladder_tooltips() -> Dictionary:
	var out: Dictionary = {}
	for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META, []):
		out[String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META))] = control.tooltip_text
	return out

## ⛔ **THE `Band:` PICKER'S CURRENT FACE** — who the card is acting FOR, which every gate and the
## turns estimate are resolved against. Found by walking the card for an `OptionButton`: the ladder's
## rung rows build plain `Button`s and `Label`s, so the selector is unambiguous, and matching on its
## label text would testify about the row rather than about the control.
func _road_ladder_band_face() -> String:
	var card := _road_ladder_card(h._hud)
	if card == null:
		return ""
	for node in _all_nodes(card, []):
		if node is OptionButton:
			return (node as OptionButton).text
	return ""

## **DRIVE THE PICKER, the way a player does.** `OptionButton` fires `item_selected` with the row
## index, which `HudWidgets.build_option_picker` routes to that entry's `on_pick` — so emitting it is
## the press, and the re-render that follows is the thing under test. The card must survive it.
func _pick_road_ladder_band(index: int) -> void:
	var card := _road_ladder_card(h._hud)
	if card == null:
		return
	for node in _all_nodes(card, []):
		if node is OptionButton:
			(node as OptionButton).item_selected.emit(index)
			break
	await h._settle()

## …and the control that puts the road down, by META rather than by its face: the button's label is a
## sentence, and a harness matching on the words would break the day it is reworded. `null` where the
## card offers nothing to put down, which is the ABSENCE half of the claim.
func _road_ladder_abandon_button() -> Button:
	for control in _collect_meta(h._hud, HudRouteVocab.ROAD_LADDER_ABANDON_META, []):
		if control is Button:
			return control as Button
	return null

## The PRESSABLE row for one rung, or `null` where the ladder refuses it. **The shape IS the
## statement** — a rung the branch offers is a `Button`, a rung it does not is a `Label` — so this is
## how *may I order this* is asked without reading a word.
func _road_ladder_row_button(rung_key: String) -> Button:
	for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META, []):
		if String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META)) != rung_key:
			continue
		for child in control.get_children():
			if child is Button:
				return child as Button
	return null

## ⛔ **PRESS A RUNG THE WAY A PLAYER DOES — through the CARD'S OWN viewport.** The ladder is a
## `PopupPanel`, which is a `Window` and therefore a `Viewport` of its own, so an event pushed into
## the HUD's viewport lands on whatever the card is floating over instead of on the row. A probe is
## the only shape that tests fix 3 at all: `pressed.emit()` calls the handler directly and would pass
## on a control the engine never routes to.
##
## **THE PRESS FREES THE ROW** — it dismisses the card — so nothing may touch the control afterwards.
## `false` where the ladder offers no button for that rung, which every caller reports as its own
## failure rather than asserting about a press that never happened.
func _press_road_ladder_row(rung_key: String) -> bool:
	var button := _road_ladder_row_button(rung_key)
	if button == null:
		return false
	var viewport := button.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, button.get_window(),
		button.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h._settle()
	return true

## Everything the open card SAYS, joined — its title, its row faces and every aside beneath them.
## The gate reasons and the price asides are composed sentences, so what is asserted about them is
## the exact wording rather than a node's presence.
func _road_ladder_text() -> String:
	var card := _road_ladder_card(h._hud)
	if card == null:
		return ""
	var parts: Array[String] = []
	for node in _all_nodes(card, []):
		if node is Button:
			parts.append((node as Button).text)
		elif node is Label:
			parts.append((node as Label).text)
	return "\n".join(parts)

## ⛔ **THE CARD IS A `Window`, SO `Q.find_meta_node` CANNOT SEE IT.** That finder matches on
## `root is Control`, which is right for every other surface in this harness and wrong for a
## `PopupPanel` — it walked straight past the card and answered `null`, so every claim about what the
## ladder SAYS passed a `contains` on an empty string in the wrong direction. A Window is the whole
## reason a height-capped dock card can host a ladder at all, so the finder follows the node kind.
func _road_ladder_card(root: Node) -> Node:
	if root == null:
		return null
	if root.has_meta(HudRouteVocab.ROAD_LADDER_META):
		return root
	for child in root.get_children():
		var found := _road_ladder_card(child)
		if found != null:
			return found
	return null

func _all_nodes(root: Node, out: Array[Node] = []) -> Array[Node]:
	out.append(root)
	for child in root.get_children():
		_all_nodes(child, out)
	return out
