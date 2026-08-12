extends RefCounted

## The tile card: food, climate, pasture and rivers.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

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
	return BaseFx.seed_forage_rows(tile)

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

## A base terrain legend (key == "terrain") shaped exactly like
## MapView._build_terrain_legend's output: rows carry color/label/value_text plus
## the numeric `count` the sort control keys off. Counts are deliberately varied
## and out of both name/count order so the sorting is obvious.
## MapView._build_pasture_legend's output, transcribed from the map_preview "pasture" state (it prints
## the legend dict) so the two harnesses cannot disagree. The swatch colors are read off MapView's own
## constants rather than restated, so a ramp retune moves the legend with the map.
func _pasture_legend_fixture() -> Dictionary:
	var poor: Color = h.MAP_VIEW_SCRIPT.PASTURE_POOR_COLOR
	var rich: Color = h.MAP_VIEW_SCRIPT.PASTURE_RICH_COLOR
	return {
		"key": "pasture",
		"title": "Pasture (Graze Capacity)",
		"description": "Graze capacity — the ANIMAL-edible stock (grass and browse; humans cannot digest it).\nStanding stock 100% of capacity across 346 pasture tiles.",
		"rows": [
			{"color": poor.lerp(rich, 8.0 / 240.0), "label": "Poorest pasture", "value_text": "8 graze"},
			{"color": poor.lerp(rich, 138.0 / 240.0), "label": "Average pasture", "value_text": "138 graze"},
			{"color": rich, "label": "Richest pasture", "value_text": "240 graze"},
			{"color": h.MAP_VIEW_SCRIPT.PASTURE_DEAD_COLOR, "label": "Barren ground", "value_text": "50 tiles"},
			{"color": h.MAP_VIEW_SCRIPT.PASTURE_WATER_COLOR, "label": "Water", "value_text": "72 tiles"},
		],
		"stats": {"min": 8.0, "avg": 138.0, "max": 240.0},
	}

func _forage_legend_fixture() -> Dictionary:
	# The HUMAN-food twin of the pasture legend. NOTE the differences that are the whole point: there is
	# NO water row (coastal shelves carry forage and ride the ramp), the barren row is the honest
	# "No forage" (deep ocean/glacier/lava only), and the description carries the gathering-sites
	# sub-count — the tiles actually forageable today, a subset of the potential the ramp paints.
	var poor: Color = h.MAP_VIEW_SCRIPT.FORAGE_POOR_COLOR
	var rich: Color = h.MAP_VIEW_SCRIPT.FORAGE_RICH_COLOR
	return {
		"key": "forage",
		"title": "Forage (Human Food Capacity)",
		"description": "The HUMAN-edible potential of this land — seeds, nuts, tubers, fruit, and fish.\nGathering sites: 18 tiles.",
		"rows": [
			{"color": poor.lerp(rich, 5.0 / 195.0), "label": "Poorest forage", "value_text": "5 food"},
			{"color": poor.lerp(rich, 92.0 / 195.0), "label": "Average forage", "value_text": "92 food"},
			{"color": rich, "label": "Richest forage", "value_text": "195 food"},
			{"color": h.MAP_VIEW_SCRIPT.FORAGE_BARREN_COLOR, "label": "No forage", "value_text": "63 tiles"},
		],
		"stats": {"min": 5.0, "avg": 92.0, "max": 195.0},
	}

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

	# State 2-pasture-legend — the map legend for the `pasture` overlay channel (rows produced by
	# MapView._build_pasture_legend; see map_preview's "pasture" state for the map itself). The barren
	# tones sit OFF the straw→grass ramp: dead ground and water are their own rows, so "no pasture at
	# all" can never be read as "poor pasture".
	# The legend card ships SUPPRESSED (the player opens it with `L`), so every legend state opens it
	# and CLOSES IT AGAIN around its own frames — see `_open_legend` / `_close_legend`.
	h._open_legend()
	h._hud.update_overlay_legend(_pasture_legend_fixture())
	await h._settle()
	await h._save("pasture_legend")
	h._close_legend()
	h._hud.clear_selection()

	# State 2-forage-legend — the map legend for the `forage` overlay channel (rows produced by
	# MapView._build_forage_legend; see map_preview's "forage" state for the map). The twin of the
	# pasture legend, but honest about the OPPOSITE meaning of absence: NO water row (shelves carry
	# forage and ride the ramp), a single "No forage" barren row (deep ocean/glacier/lava only), and a
	# "Gathering sites: N" sub-count so the ramp reads as POTENTIAL without calling the rest dead.
	h._open_legend()
	h._hud.update_overlay_legend(_forage_legend_fixture())
	await h._settle()
	await h._save("forage_legend")
	h._close_legend()
	h._hud.clear_selection()

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
