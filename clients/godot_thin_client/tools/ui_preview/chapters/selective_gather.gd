extends RefCounted

## THE SELECTIVE GATHER — which plants a forage crew carries home, and what that crew is worth.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## **THE THREE CHIP STATES ARE ONE SUBJECT, and two of them are one glance apart.** With nothing
## ticked every plant is coming home, so every chip reads faintly INCLUDED; ticking one makes it
## PICKED and every other chip EXCLUDED. A frame carrying only the narrowed state cannot say the
## default reads differently from it, so the two are rendered as a PAIR and the chips' states are
## asserted off `HudWidgets.SPECIES_CHIP_STATE_META` rather than off their ink — at the HUD's real
## type size the difference between a faint check and a bright one is a few pixels, which is exactly
## why the states carry a meta at all.
##
## **THE OTHER SUBJECT IS THE PRICE, and no picture can judge it.** The sim publishes a per-species
## conversion rate for every plant in the basket, so ticking a chip moves the forecast exactly as
## stepping the crew does — and the defect this chapter is written against is the sheet sitting
## STILL, which renders a perfectly ordinary readout. Every price claim here is therefore a RELATION
## between two rendered readings of the same tile (`_assert_tick_prices_the_crew`), driven through a
## real press on a real chip, never a magnitude compared against a constant the fixture also states.
##
## It ends by handing the reference band back, so a chapter appended after it starts where every
## other one does.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The gathering band, on its own entity so these states cannot be confused with the reference band
## the rest of the run uses. Two fixtures share it and differ in ONE field — the standing row's
## `take_species` — which is what makes the seeded-selection claim a claim about that field.
const GATHER_ENTITY := 993

## The patch these states compose, on its own hex for the same reason. It sits on the band's own tile
## so the range gate can never be what a failure is about.
const GATHER_X := 40
const GATHER_Y := 12

## The three plants `BaseFx.food_tile_fixture` puts in the basket, and the standing biomass each one
## accounts for. **They MATCH the tile card's own split of the same stock on purpose** — since issue
## #545's follow-up both surfaces read this same wire figure, so a fixture that made them disagree
## would put two different bracket sets for one basket on one screen and read as a defect rather than
## as a claim.
##
## **The claim that the chip reads the WIRE is therefore made by a DRIVEN probe**, not by the numbers:
## `_assert_bracket_follows_the_wire` moves one entry's published biomass to a value no derivation of
## this tile can produce and requires the chip to follow.
const STANDING_EMMER := 40.0
const STANDING_TUBERS := 27.0
const STANDING_MAST := 23.0

## …and the value that probe moves the mast to. It is not `share × stock` for any share on this tile,
## so a chip re-deriving the split cannot render it by accident.
const PROBE_STANDING_MAST := 7.0

## **WHAT ONE UNIT OF EACH PLANT IS WORTH, as WEIGHTS rather than rates.** The wire's contract is an
## identity — `Σ(share × rate) == provisionsPerBiomass` — so a fixture stating three rates freehand
## would describe a patch no server can produce, and the sheet's composition would then be checked
## against arithmetic that does not close. These weights are normalised against the patch's OWN
## basket-averaged rate in `_price_basket`, so the identity holds whatever that rate is.
##
## They are deliberately far apart: the mast is worth an eighth of the grain, which is what makes
## narrowing to it move the forecast by more than a rounding.
const RATE_WEIGHT_EMMER := 2.0
const RATE_WEIGHT_TUBERS := 1.0
const RATE_WEIGHT_MAST := 0.25

## The species the narrowed states pick — the tile's SMALLEST share AND its poorest converter, so
## narrowing to it shrinks BOTH arms of the take's `min`: the stand above the floor, and what a
## worker's basket of it is worth. A selection that moved only one of them could not say the sheet
## composes the pair.
const SCARCE_SPECIES := "oak_mast"

## …and the plant whose rate the UNQUOTED state strips. Any ticked plant the wire priced no rate for
## makes the whole selection unquotable — the composition is a weighted mean, so one missing term is
## not a term that can be left out of it.
const UNQUOTED_SPECIES := "wild_emmer"

## The crew the priced states dial in. Above the mast-only selection's useful count and inside the
## whole basket's, so the tick has room to move the stepper DOWN — which is the useful-worker
## count moving, and the half of the claim a food figure alone cannot make.
const BASKET_WORKERS := 3

## The knowledge these states render at. Cultivation complete, so the single-pick variant's composed
## rung is one the sheet renders a control for rather than a gate reason; **Foddering complete too**,
## or the hay-meadow state's fodder account is locked and the `0.0` food reading it exists to exercise
## has no live sibling beside it. The rest of the ladder is left where the chapters above put it.
const CULTIVATION_KNOWN := 1.0
const FODDERING_KNOWN := 1.0

## ---- The hay meadow — the tile whose selection can pay NO FOOD AT ALL ------------------------
##
## `0.0` is a REAL rate, not a missing one: a cash or fodder crop honestly pays no calories, and the
## whole point of carrying presence on its own key is that this case and an unstated rate must not
## render alike. So one state ticks a plant whose provisions rate is exactly zero and requires the
## sheet to price it — a live fodder reading, no food row, and no *"not priced"* aside anywhere.
const HAY_SPECIES := "hay_grass"
const HAY_DISPLAY := "Hay Grass"
const HAY_SHARE := 0.4
const GRAIN_SHARE := 0.6

## What one unit of hay is worth in FODDER, as a multiple of the patch's own provisions rate — so the
## meadow's hay is worth three times its grain and the account is unmistakable on the frame.
const HAY_FODDER_MULTIPLE := 3.0


## The patch, with its basket carrying the WIRE's own standing biomass AND per-species conversion
## rates. `BaseFx.unbuilt` zeroes both meters, which is what makes the composed Cultivate below a
## DECLARATION the sheet honours rather than a verb the meters have already retired.
func _gather_tile() -> Dictionary:
	var tile := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	tile["x"] = GATHER_X
	tile["y"] = GATHER_Y
	var standing := [STANDING_EMMER, STANDING_TUBERS, STANDING_MAST]
	var basket: Array = tile.get("patch_composition", []) as Array
	for index in basket.size():
		var entry: Dictionary = basket[index]
		entry["standing_biomass"] = float(standing[index]) if index < standing.size() else 0.0
	_price_basket(tile, [RATE_WEIGHT_EMMER, RATE_WEIGHT_TUBERS, RATE_WEIGHT_MAST])
	return tile


## **SPREAD THE PATCH'S OWN BASKET-AVERAGED RATE ACROSS ITS PLANTS, IN THE GIVEN PROPORTIONS.** The
## wire's identity is `Σ(share × rate) == provisionsPerBiomass`, so the rates are DERIVED from the
## figure the fixture already states rather than authored beside it — which is what stops this fixture
## describing a patch the sim could not publish, and what lets the assertions below compare the
## narrowed reading against the whole basket's without either side being a magic number.
##
## The fodder account is left at a structural zero here; the hay meadow states its own. **Every entry
## gets an EMPTY `material_per_biomass`, which is a real answer** — a staple pays no material, and it
## is what keeps "no row" distinguishable from a zero-valued one on the frames below.
func _price_basket(tile: Dictionary, weights: Array) -> void:
	var basket: Array = tile.get("patch_composition", []) as Array
	var normaliser := 0.0
	for index in basket.size():
		var entry: Dictionary = basket[index]
		normaliser += float(entry.get("share", 0.0)) * float(weights[index])
	if normaliser <= 0.0:
		return
	var basket_rate := float(tile.get("patch_provisions_per_biomass", 0.0))
	for index in basket.size():
		var entry: Dictionary = basket[index]
		entry["provisions_per_biomass"] = basket_rate * float(weights[index]) / normaliser
		entry["fodder_per_biomass"] = 0.0
		entry["material_per_biomass"] = []


## The hay meadow — Wild Grain beside a plant that pays no food at all. Built off the gathering tile
## so every other term (the stock, the crew throughput, the curve) is the one the states above use,
## and only the basket and the fodder account move.
func _hay_meadow_tile() -> Dictionary:
	var tile := _gather_tile()
	var basket: Array = tile.get("patch_composition", []) as Array
	var grain: Dictionary = (basket[0] as Dictionary).duplicate(true)
	grain["share"] = GRAIN_SHARE
	grain["standing_biomass"] = STANDING_EMMER
	var patch_rate := float(tile.get("patch_provisions_per_biomass", 0.0))
	# The grain carries the WHOLE of the patch's provisions rate, which is what keeps the identity
	# closed on a basket where the other member pays nothing: `0.6 × (rate / 0.6) + 0.4 × 0 == rate`.
	grain["provisions_per_biomass"] = patch_rate / GRAIN_SHARE
	grain["fodder_per_biomass"] = 0.0
	var hay := {
		"species": HAY_SPECIES,
		"role": "fodder",
		"display_name": HAY_DISPLAY,
		"share": HAY_SHARE,
		"can_cultivate": false,
		"can_sow": false,
		"cultivate_yield_ratio": 0.0,
		"sow_yield_ratio": 0.0,
		"cultivate_payoff": 0.0,
		"sow_payoff": 0.0,
		"standing_biomass": STANDING_TUBERS + STANDING_MAST,
		# **THE ZERO THIS WHOLE STATE EXISTS FOR — a STATED rate, not an absent one.**
		"provisions_per_biomass": 0.0,
		"fodder_per_biomass": patch_rate * HAY_FODDER_MULTIPLE,
		# …and hay pays no MATERIAL, which is the "empty means no row" half of the same contract.
		"material_per_biomass": [],
	}
	tile["patch_composition"] = [grain, hay]
	tile["patch_fodder_per_biomass"] = HAY_SHARE * patch_rate * HAY_FODDER_MULTIPLE
	return tile


## ---- The cash-crop tile — the case the whole feature was argued on ---------------------------
##
## Baskets are made of fibre and baskets are what let a gatherer carry more food, so *tick cotton, see
## how much fibre* is the first thing a player tries. `material_per_biomass` on the PATCH is
## basket-averaged and cannot answer it; `compositionMaterialPerBiomass` is the same quantity per
## plant, and this tile is where the composition is proved.
##
## **TWO OF THE THREE PLANTS PAY ONE MATERIAL, and that is the fixture's whole point.** A composition
## that merged by LAST WRITE rather than by material id passes any single-species selection and is
## wrong by a factor here — which is why the sim's own test uses cotton beside flax.
const CASH_FLAX := "flax"
const CASH_COTTON := "cotton"
const CASH_FIBRE := "fibre"
const CASH_TOBACCO := "tobacco"

## The shares, and the third plant is the CONTROL: `wild_emmer` pays the whole patch's food and NO
## material at all, so "empty means no row" stays a claim this tile can make.
const CASH_GRAIN_SHARE := 0.5
const CASH_FLAX_SHARE := 0.3
const CASH_COTTON_SHARE := 0.2

## What one unit of each cash plant is worth, per material. The two fibre rates are deliberately far
## apart (3.5x) — a merge that took either one alone lands on 0.12 or 0.42 where the mean is 0.24.
const CASH_FLAX_FIBRE := 0.12
const CASH_COTTON_FIBRE := 0.42
const CASH_COTTON_TOBACCO := 0.18

## …and the answers the assertions are made of, spelled out rather than recomputed at the call site:
## `Sum(share x rate) / Sum(share)` over `{flax, cotton}` is `(0.3 x 0.12 + 0.2 x 0.42) / 0.5`.
const CASH_MERGED_FIBRE := 0.24
const CASH_MERGED_TOBACCO := 0.072


## The cash tile — grain beside flax and cotton, with the patch's own vectors composed FROM the
## per-species ones so the wire identity `Sum(share x rate) == <patch rate>` closes on every account.
func _cash_tile() -> Dictionary:
	var tile := _gather_tile()
	var patch_rate := float(tile.get("patch_provisions_per_biomass", 0.0))
	var grain: Dictionary = (tile.get("patch_composition", []) as Array)[0]
	grain["share"] = CASH_GRAIN_SHARE
	grain["standing_biomass"] = STANDING_EMMER
	# The grain carries the whole patch's food, so the identity closes on a basket whose other two
	# members pay none — and it carries NO material, which is the control the merge is read against.
	grain["provisions_per_biomass"] = patch_rate / CASH_GRAIN_SHARE
	grain["fodder_per_biomass"] = 0.0
	grain["material_per_biomass"] = []
	tile["patch_composition"] = [
		grain,
		_cash_entry(CASH_FLAX, "Flax", CASH_FLAX_SHARE, STANDING_TUBERS,
			[{"material_id": CASH_FIBRE, "amount": CASH_FLAX_FIBRE}]),
		_cash_entry(CASH_COTTON, "Cotton", CASH_COTTON_SHARE, STANDING_MAST,
			[{"material_id": CASH_FIBRE, "amount": CASH_COTTON_FIBRE},
				{"material_id": CASH_TOBACCO, "amount": CASH_COTTON_TOBACCO}]),
	]
	# **THE PATCH'S OWN VECTORS ARE THE WEIGHTED SUM OF THE ENTRIES', never authored beside them** —
	# `Sum(share x rate)`, which is the identity the schema pins. A fixture stating them freehand would
	# describe a patch no server can publish, and the whole-basket half of every claim below would then
	# be checked against arithmetic that does not close.
	tile["patch_material_per_biomass"] = [
		{"material_id": CASH_FIBRE,
			"amount": CASH_FLAX_SHARE * CASH_FLAX_FIBRE + CASH_COTTON_SHARE * CASH_COTTON_FIBRE},
		{"material_id": CASH_TOBACCO, "amount": CASH_COTTON_SHARE * CASH_COTTON_TOBACCO},
	]
	# …and the crew's own throughput through those same rates. `seed_growth_terms` recovers
	# `per_worker_biomass` as `per_worker_yield / provisions_per_biomass`, so the carry is stated here
	# the same way rather than waited for — this tile has to price its materials before the first
	# render, and the narrowed source re-composes its own from the selection anyway.
	var carry := float(tile.get("patch_per_worker_yield", 0.0)) / patch_rate
	tile["patch_per_worker_material"] = SourceForecast.scaled_material_rows(
		tile["patch_material_per_biomass"], carry)
	return tile


## One cash-crop basket entry — no food, no fodder, and the materials it is grown for.
func _cash_entry(species: String, display_name: String, share: float, standing: float,
		materials: Array) -> Dictionary:
	return {
		"species": species,
		"role": "cash",
		"display_name": display_name,
		"share": share,
		"can_cultivate": false,
		"can_sow": false,
		"cultivate_yield_ratio": 0.0,
		"sow_yield_ratio": 0.0,
		"cultivate_payoff": 0.0,
		"sow_payoff": 0.0,
		"standing_biomass": standing,
		"provisions_per_biomass": 0.0,
		"fodder_per_biomass": 0.0,
		"material_per_biomass": materials,
	}


## The band, gathering this patch. `take_species` is the ONE field the two fixtures differ in.
func _gather_band(take_species: Array, workers: int) -> Dictionary:
	var band := BandFx.with_band_id({
		"id": "Band 9",
		"size": 24,
		"entity": GATHER_ENTITY,
		"faction": 0,
		"pos": [GATHER_X, GATHER_Y],
		"turns_of_food": 18.0,
		"morale": 0.8,
		"stores": {"provisions": 60.0},
		"working_age": 12,
		"idle_workers": 6,
		"work_range": 3,
		"hunt_reach": 6,
		"scout_reveal_radius": 2,
		"activity": "forage",
		"food_income": 0.4,
		"food_consumption": 0.3,
		"labor_assignments": [{
			"kind": "forage",
			"workers": workers,
			"target_x": GATHER_X,
			"target_y": GATHER_Y,
			"floor": 0.5,
			"actual_yield": 0.08,
			"sustainable_yield": 0.08,
			"workers_needed": 1,
			"overdraws": false,
			"take_species": take_species,
		}],
		"tile_info": {
			"x": GATHER_X, "y": GATHER_Y,
			"terrain_label": "Prairie Steppe",
			"visibility_state": "active",
			"food_module": "savanna_grassland",
			"food_module_label": "Savanna Grassland",
		},
	})
	return band


## The open compose sheet, or `null`.
func _sheet() -> Node:
	return h._hud._drawercompose._compose_sheet


## Every species chip on the open sheet, in render order.
func _chips() -> Array:
	var sheet: Node = _sheet()
	if sheet == null:
		return []
	var row := Q.find_meta_node(sheet, HudWidgets.SPECIES_CHIP_ROW_META)
	if row == null:
		return []
	var found: Array = []
	_collect_chips(row, found)
	return found


func _collect_chips(node: Node, out: Array) -> void:
	if node.has_meta(HudWidgets.SPECIES_CHIP_META):
		out.append(node)
		return
	for child in node.get_children():
		_collect_chips(child, out)


## The chip row's states, keyed by species — the instrument every state claim below is made through.
func _chip_states() -> Dictionary:
	var states := {}
	for chip in _chips():
		states[String(chip.get_meta(HudWidgets.SPECIES_CHIP_META))] = String(
			chip.get_meta(HudWidgets.SPECIES_CHIP_STATE_META))
	return states


## Does any Label on the open sheet carry `needle`? The consequence line and the honest-silence asides
## are all `alloc_hint_label`s, so this is what reads them.
func _sheet_says(needle: String) -> bool:
	var sheet: Node = _sheet()
	return sheet != null and Q.has_label_containing(sheet, needle)


## One material's amount out of a composed vector, `NAN` when the vector never names it — so an
## assertion against a missing material fails rather than reading as a zero.
func _material_amount(rows: Array, material_id: String) -> float:
	for row in rows:
		if String((row as Dictionary).get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, "")) == material_id:
			return float((row as Dictionary).get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
	return NAN


## What this sheet currently quotes in one account, and how many hands it says are useful — the PAIR
## every price claim here is made of. Read off the RENDERED controls: the stepper's own value Label
## (not `ComposeState`, which is the model the harness itself dialled) and the yields row's own
## number beside its unit.
func _quote(account: String) -> Dictionary:
	var sheet: Node = _sheet()
	if sheet == null:
		return {"food": Readout.YIELDS_ACCOUNT_ABSENT, "crew": Readout.STEPPER_VALUE_ABSENT}
	return {
		"food": Readout.yields_account_number(sheet, account),
		"crew": Readout.stepper_value(sheet),
	}


## Open the sheet, then dial the selection and re-open so the render sees it. The `_compose_herd`
## contract, one web over: the FIRST open re-seeds the composition off the band's own row, so a
## selection written before it is thrown away.
func _compose_selection(tile: Dictionary, selection: PackedStringArray, workers: int,
		improvement: String = "") -> void:
	h._compose_forage(tile)
	h._hud._compose.set_forage_take_species(selection)
	h._hud._compose.set_forage_count(workers)
	if improvement != "":
		h._hud._compose.set_forage_improvement(improvement)
	h._compose_forage(tile)


## **THE TICK IS DRIVEN AS A POINTER GESTURE, because the tick is what broke.** The defect was a
## sheet whose forecast sat still while its chips lit up, so a state that wrote the selection into
## `ComposeState` and re-rendered would be asserting the harness's own write rather than the control's
## effect. A chip is a plain `Button` under a mouse-transparent face stack, so a press at its rect
## centre reaches it exactly as a player's does.
##
## The press REBUILDS the compose block and frees the button, so the caller must not touch it after —
## and `_settle` is what lets the freed generation leave the tree before anything is counted.
func _press_chip(species: String) -> bool:
	var chip: Button = null
	for candidate in _chips():
		if String(candidate.get_meta(HudWidgets.SPECIES_CHIP_META)) == species:
			chip = candidate as Button
			break
	if chip == null:
		return false
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		chip.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h._settle()
	return true


## **THE BRACKET IS THE WIRE'S, AND ONLY A DRIVEN PROBE CAN SAY SO.** The fixture's own numbers agree
## with `share × stock`, so a chip that re-derived the split renders the same frame; this moves ONE
## entry's published biomass to a value no share of this tile can produce and requires the chip to
## follow it. PNG-less — the state it stages is a fixture, not a thing a player sees — and it restores
## the tile it borrowed, so the frames after it are the frames before it.
func _assert_bracket_follows_the_wire(tile: Dictionary) -> void:
	var basket: Array = tile.get("patch_composition", []) as Array
	var mast: Dictionary = basket[basket.size() - 1]
	mast["standing_biomass"] = PROBE_STANDING_MAST
	h._compose_forage(tile)
	h._assert_hud("…and the bracket follows the WIRE rather than share × stock",
		_sheet_says(HudFloraVocab.FLORA_SHARE_BIOMASS_CLAUSE_FORMAT % int(PROBE_STANDING_MAST)))
	mast["standing_biomass"] = STANDING_MAST
	h._compose_forage(tile)


func run(harness) -> void:
	h = harness
	var tile := _gather_tile()
	h._hud.update_intensification([{
		"faction": 0,
		"cultivation": CULTIVATION_KNOWN,
		"foddering": FODDERING_KNOWN,
	}])

	# ---- The DEFAULT state — nothing ticked anywhere, so everything is coming home ---------------
	# The band's own row carries NO selection, which is what the sheet seeds from; the state exists to
	# say that reads as every chip faintly INCLUDED rather than as every chip off. It is also the
	# BASELINE every price claim below is a relation against.
	h._hud.update_band_alerts([_gather_band([], BASKET_WORKERS)])
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	h._hud._compose.set_forage_count(BASKET_WORKERS)
	h._compose_forage(tile)
	await h._settle()
	await h._save("forage_take_default")
	var whole_basket := _quote(SourceForecast.YIELD_ACCOUNT_FOOD)
	var default_states := _chip_states()
	h._assert_hud("the chip row carries one chip per named plant in the basket",
		default_states.size() == 3)
	h._assert_hud("…and with nothing ticked every chip reads INCLUDED, never excluded",
		default_states.values().all(func(state: String) -> bool:
			return state == HudFloraVocab.TAKE_STATE_INCLUDED))
	# The bracketed quantity is the WIRE's, and the fixture's numbers are deliberately not
	# `share × biomass` — so a chip that re-derived the split renders a different bracket here.
	h._assert_hud("…and each chip quotes the standing biomass the wire published",
		_sheet_says(HudFloraVocab.FLORA_SHARE_BIOMASS_CLAUSE_FORMAT % int(STANDING_EMMER))
			and _sheet_says(HudFloraVocab.FLORA_SHARE_BIOMASS_CLAUSE_FORMAT % int(STANDING_MAST)))
	h._assert_hud("…and the consequence line says the whole basket comes home",
		_sheet_says(HudFloraVocab.TAKE_NOTE_FORAGE_ALL))
	# The PRECONDITION every price claim below rests on: the whole basket really does quote a take and
	# really does state a useful count, so a later "it moved" is a move rather than an appearance.
	h._assert_hud("…and the whole basket quotes a take and a crew to take it with",
		whole_basket["food"] != Readout.YIELDS_ACCOUNT_ABSENT
			and int(whole_basket["crew"]) == BASKET_WORKERS)
	_assert_bracket_follows_the_wire(tile)

	# ---- TICKING A CHIP PRICES THE CREW, live, exactly as the stepper does -----------------------
	# The reported asymmetry: the worker stepper moved this readout and the chips did not, which
	# taught that narrowing was free when it is the whole decision. The scarce plant shrinks BOTH arms
	# of the take — a quarter of the stand, an eighth of the conversion — so both the quoted food and
	# the useful-worker count must fall, and the pair is the claim: a sheet that re-clamped the crew
	# without re-pricing satisfies one of them.
	var pressed := await _press_chip(SCARCE_SPECIES)
	h._assert_hud("a species chip is a real button and a real press reaches it", pressed)
	await h._save("forage_take_chip_priced")
	var narrowed := _quote(SourceForecast.YIELD_ACCOUNT_FOOD)
	h._assert_hud("…and the tick lit that chip and dimmed the rest",
		_chip_states().get(SCARCE_SPECIES, "") == HudFloraVocab.TAKE_STATE_PICKED)
	h._assert_hud("…and the quoted take MOVED with it, rather than sitting still",
		narrowed["food"] != whole_basket["food"]
			and narrowed["food"] != Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…and it fell, this plant being a quarter of the stand at an eighth of the rate",
		float(String(narrowed["food"])) < float(String(whole_basket["food"])))
	h._assert_hud("…and the useful-worker count fell with it, on the same edit",
		int(narrowed["crew"]) < int(whole_basket["crew"])
			and int(narrowed["crew"]) > 0)
	h._assert_hud("…and the consequence line says the rest is left standing",
		_sheet_says(HudFloraVocab.TAKE_NOTE_FORAGE_NARROWED))
	h._assert_hud("…and a priced narrowing states no honest-silence aside at all",
		not _sheet_says(HudFloraVocab.TAKE_UNQUOTED_NOTE))

	# ---- NARROWED, seeded from the band's own standing row ---------------------------------------
	# Nothing in this block ticks a chip: the selection came off the assignment, which is what stops a
	# re-issued `assign_labor` silently widening a crew the player had narrowed.
	h._hud.update_band_alerts([_gather_band([SCARCE_SPECIES], BASKET_WORKERS)])
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	# **A SECOND SETTLE, because `ComposeSheet.refit` waits TWO frames** — it fits the card's WIDTH,
	# then reads the body's height a frame later so the measurement sees the new wrapping. Every other
	# state here composes twice and pays that frame incidentally; this one composes once, on purpose
	# (the selection must come off the band's row and nothing else), so it pays it explicitly.
	await h._settle()
	await h._save("forage_take_narrowed")
	var narrowed_states := _chip_states()
	h._assert_hud("the sheet opens on the selection the band's own row carries",
		narrowed_states.get(SCARCE_SPECIES, "") == HudFloraVocab.TAKE_STATE_PICKED)
	h._assert_hud("…and every other plant reads EXCLUDED, not merely unlit",
		narrowed_states.get(UNQUOTED_SPECIES, "") == HudFloraVocab.TAKE_STATE_EXCLUDED)
	h._assert_hud("…and a seeded narrowing is priced exactly as a ticked one is",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] == narrowed["food"])

	# ---- A PLANT PAYING NO FOOD AT ALL is priced, not called unpriced -----------------------------
	# `0.0` is a real reading and an unstated rate is not, and the two must not render alike. Ticking
	# the hay alone composes a provisions rate of exactly zero beside a live fodder one: no FOOD row
	# (the render-only-where-it-pays rule), a fodder number, and no *not priced* aside anywhere.
	var meadow := _hay_meadow_tile()
	h._hud.update_band_alerts([_gather_band([], BASKET_WORKERS)])
	h._hud._compose.reset_forage_source()
	h._show_tile(meadow)
	_compose_selection(meadow, PackedStringArray([HAY_SPECIES]), BASKET_WORKERS)
	await h._settle()
	await h._save("forage_take_zero_food")
	h._assert_hud("a selection paying 0.0 food is PRICED, not called unpriced",
		not _sheet_says(HudFloraVocab.TAKE_UNQUOTED_NOTE))
	h._assert_hud("…and it quotes the fodder it does pay",
		_quote(SourceForecast.YIELD_ACCOUNT_FODDER)["food"] != Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…and states NO food row, rather than a `0.00 FOOD` the plant contradicts",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] == Readout.YIELDS_ACCOUNT_ABSENT)
	# The paired POSITIVE, on the same meadow: the grain half still quotes food, so the absence above
	# is this selection's rather than a readout that stopped stating the account.
	_compose_selection(meadow, PackedStringArray([UNQUOTED_SPECIES]), BASKET_WORKERS)
	await h._settle()
	h._assert_hud("…while the grain half of the SAME basket still quotes a food take",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] != Readout.YIELDS_ACCOUNT_ABSENT)

	# ---- A NARROWING THE WIRE PRICED NO RATE FOR states NOTHING -----------------------------------
	# The composition is a weighted mean, so one missing term is not a term that can be left out of
	# it, and there is nothing else this client holds a rate could be recovered from. The PAIR is the
	# claim: every state above proves the readout quotes a number when it legitimately can, so this is
	# a suppression rather than a readout that stopped working.
	var unpriced := _gather_tile()
	var unpriced_basket: Array = unpriced.get("patch_composition", []) as Array
	(unpriced_basket[0] as Dictionary).erase("provisions_per_biomass")
	h._hud._compose.reset_forage_source()
	h._show_tile(unpriced)
	_compose_selection(unpriced, PackedStringArray([UNQUOTED_SPECIES]), BASKET_WORKERS)
	await h._settle()
	await h._save("forage_take_unquoted")
	h._assert_hud("a selection the wire priced no rate for says so in words",
		_sheet_says(HudFloraVocab.TAKE_UNQUOTED_NOTE))
	h._assert_hud("…and quotes no take at all, rather than the whole basket's",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] == Readout.YIELDS_ACCOUNT_ABSENT)

	# ---- CULTIVATING takes ONE, and the mark is what says so -------------------------------------
	# The same chips in the same place, doing a different thing: a Cultivate commits the ground to one
	# crop, so the row is single-select and the consequence line says the rest is weeded out. The
	# affordance has to carry that difference, and the MARK is where it is carried.
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	_compose_selection(tile, PackedStringArray(), BASKET_WORKERS,
		SourceForecast.IMPROVEMENT_CULTIVATE)
	await h._settle()
	await h._save("forage_take_cultivate")
	var cultivate_states := _chip_states()
	var lit := cultivate_states.values().filter(func(state: String) -> bool:
		return state != HudFloraVocab.TAKE_STATE_EXCLUDED)
	h._assert_hud("cultivating lights exactly ONE chip, whatever the take selection was",
		lit.size() == 1)
	h._assert_hud("…and with nothing picked the sheet NAMES the crop it would settle on",
		_sheet_says(HudFloraVocab.TAKE_NOTE_CULTIVATE_DEFAULT_FORMAT % "Wild Grain"))
	# Ticking a crop is what turns the game's answer into the player's, and only then does the
	# consequence line state what committing costs the rest of the stand.
	_compose_selection(tile, PackedStringArray(), BASKET_WORKERS,
		SourceForecast.IMPROVEMENT_CULTIVATE)
	h._hud._compose.set_forage_species(UNQUOTED_SPECIES)
	h._compose_forage(tile)
	await h._settle()
	await h._save("forage_take_cultivate_picked")
	h._assert_hud("a picked crop reads PICKED rather than faintly included",
		_chip_states().get(UNQUOTED_SPECIES, "") == HudFloraVocab.TAKE_STATE_PICKED)
	h._assert_hud("…and the consequence line says cultivating weeds the rest out",
		_sheet_says(HudFloraVocab.TAKE_NOTE_CULTIVATE_NARROWED_FORMAT % String(
			HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[SourceForecast.IMPROVEMENT_CULTIVATE])))

	# ---- TICK COTTON, SEE HOW MUCH FIBRE — the case the feature was argued on --------------------
	# `material_per_biomass` on the PATCH is basket-averaged, so this sheet used to answer a narrowing
	# to a cash crop with an apology. The per-species vector answers it with a number.
	var cash := _cash_tile()
	h._hud._compose.reset_forage_source()
	h._show_tile(cash)
	_compose_selection(cash, PackedStringArray([CASH_COTTON]), BASKET_WORKERS)
	await h._settle()
	await h._save("forage_take_cash_narrowed")
	h._assert_hud("a narrowing to a cash crop quotes the material it is grown for",
		_quote(CASH_FIBRE)["food"] != Readout.YIELDS_ACCOUNT_ABSENT
			and _quote(CASH_TOBACCO)["food"] != Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…and states no honest-silence aside where the apology used to be",
		not _sheet_says(HudFloraVocab.TAKE_UNQUOTED_NOTE))
	h._assert_hud("…and no FOOD row, cotton paying none — the `0.0` is real, not unstated",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] == Readout.YIELDS_ACCOUNT_ABSENT)

	# ---- TWO PLANTS, ONE MATERIAL — the merge a single-species fixture cannot check ---------------
	# Flax and cotton both pay fibre at rates a factor apart, so a composition that took the LAST one
	# rather than merging by material id lands on 2.0 or 7.0 where the weighted mean is 4.0. The claim
	# is made on the COMPOSITION rather than on the rendered take: a take is the mean through two more
	# clamps, and a frame showing a plausible number cannot separate the two answers.
	var cash_basket := SourceForecast.flora_basket_entries(cash.get("patch_composition", []))
	var merged := SourceForecast.selection_rates(cash_basket,
		PackedStringArray([CASH_FLAX, CASH_COTTON]))
	var merged_rows := SourceForecast.material_payoff_rows(
		merged.get(SourceForecast.SELECTION_MATERIAL, []))
	h._assert_hud("the composition merges the two fibre payers into ONE row",
		merged_rows.size() == 2)
	h._assert_hud("…and merges them by WEIGHTED MEAN, not by last write",
		is_equal_approx(_material_amount(merged_rows, CASH_FIBRE), CASH_MERGED_FIBRE))
	h._assert_hud("…and the material only one of them pays composes over the WHOLE selection",
		is_equal_approx(_material_amount(merged_rows, CASH_TOBACCO), CASH_MERGED_TOBACCO))
	# The single-species control, which is what makes the claim above about the MERGE: cotton alone
	# composes its own rate untouched, so the 4.0 is neither plant's number by accident.
	var cotton_only := SourceForecast.selection_rates(cash_basket, PackedStringArray([CASH_COTTON]))
	h._assert_hud("…while one plant alone composes its own rate, unmerged",
		is_equal_approx(_material_amount(SourceForecast.material_payoff_rows(
			cotton_only.get(SourceForecast.SELECTION_MATERIAL, [])), CASH_FIBRE),
			CASH_COTTON_FIBRE))
	_compose_selection(cash, PackedStringArray([CASH_FLAX, CASH_COTTON]), BASKET_WORKERS)
	await h._settle()
	await h._save("forage_take_cash_merged")
	h._assert_hud("and the sheet renders the merged fibre as one row",
		_quote(CASH_FIBRE)["food"] != Readout.YIELDS_ACCOUNT_ABSENT)
	# **THE WIDEST READOUT THIS SHEET CAN PRODUCE, so it is the one measured.** A narrowed take states
	# every account the selection pays, and this selection pays three columns' worth on the row the
	# card is fitted to; the nine standing fit states are all whole-basket sheets and cannot see it.
	h._assert_compose_sheet_fits("forage_take_cash_merged")
	h._assert_compose_sheet_card_holds_its_content("forage_take_cash_merged")

	# ---- A TICKED PLANT THAT PAYS NO MATERIAL STATES NO ROW, never a zero ------------------------
	# The same basket, narrowed to the grain: an empty material list is a real answer and must not
	# render as a crop that pays badly. Without this half, "quotes the materials" is satisfied by a
	# sheet that prints a `0.00 FIBRE` on every selection in the game.
	_compose_selection(cash, PackedStringArray([UNQUOTED_SPECIES]), BASKET_WORKERS)
	await h._settle()
	await h._save("forage_take_cash_grain")
	h._assert_hud("a plant that pays no material states NO material row",
		_quote(CASH_FIBRE)["food"] == Readout.YIELDS_ACCOUNT_ABSENT
			and _quote(CASH_TOBACCO)["food"] == Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…and still quotes the food it does pay",
		_quote(SourceForecast.YIELD_ACCOUNT_FOOD)["food"] != Readout.YIELDS_ACCOUNT_ABSENT)

	# Hand the reference band back, so a chapter appended after this starts where every other one does.
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_forage_source()
	h._hud.update_band_alerts([BandFx.band_fixture()])
	await h._settle()
