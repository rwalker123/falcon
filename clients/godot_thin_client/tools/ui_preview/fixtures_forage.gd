## Forage, crop and improvement fixtures.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")

# A floor BELOW the food peak, for the frames that need "this crew is drawing the source down" — the
# `deplete`/`surplus` stances these fixtures were written against. It is one of the sim's own raid
# samples, so a converted raid table lands on a real row rather than an interpolated one.
const DEEP_DRAW_FLOOR := 0.15

# The would-be herder crew on `herd_tame_worker_cap` — see `_tame_worker_cap_herd_fixture` for why it
# has to clear the Tame rung's own take-useful (~27 since the build dip moved onto the crew).

const COMPOSE_FLOOR_UNSET := -1.0

# The per-biomass rate a DEAD-SEASON patch keeps. A rate says what grows here, which a season does
# not change; the season empties the stock and the crew's throughput. Any positive value serves —
# the patch's stock is pinned AT the food peak, so every ceiling is 0 whatever this is.

const FLOOR_CHART_CREW := 3
# `Readout.crew_target_count`'s answer when the target is not rendered at all. NOT 0, which is a real reading
# ("nothing to clear"), and the distinction is the dead-season assertion's whole subject.

const FIXTURE_REGROWTH_SAMPLES := 11

const FIXTURE_PLANT_REGROWTH_RATE := 0.25

const FIXTURE_ANIMAL_REGROWTH_RATE := 0.05
# **THE PHASE BANDS, WHICH ARE ALSO THE ANIMAL CURVE'S ALLEE POINT.** `collapse_fraction` is one
# number in the sim doing two jobs on the animal web — the boundary `classify_ecology_phase` calls a
# herd Collapsing at, and the stock `net_biomass_delta` turns negative below — so the seeded curve and
# the seeded zone read it from ONE constant here too. Splitting them would let a fixture draw a chart
# whose red band and whose crash begin at different heights, which is precisely the disagreement
# `floor_chart_herd_allee` exists to catch. `labor_config.forage.ecology` and `fauna_config.ecology`
# state the same pair today (0.15 / 0.40); the plant web simply has no Allee term behind its cut.

const FIXTURE_COLLAPSE_FRACTION := 0.15

const FIXTURE_STRESSED_FRACTION := 0.40

const FIXTURE_COLLAPSE_RATE := 0.20

const FIXTURE_RESEED_FLOOR_FRACTION := 0.02
# `per_worker_biomass_capacity` for each web, used only where the fixture's own rates cannot state the
# throughput (a source that pays no food — the exact case the wire field was added for).

const FIXTURE_PLANT_PER_WORKER_BIOMASS := 8.0

const FIXTURE_ANIMAL_PER_WORKER_BIOMASS := 40.0

# ---- THE STALE-VERB PATCH: the played tile, at the SHIPPED numbers ------------------------------
# Reported from play, and the reason `SourceForecast.live_improvement` exists. Every constant here is
# a shipped config value rather than a fixture convenience, because the defect is only visible at the
# proportions a live patch has: `crew_to_hold` divides a regrowth the LAND owns by a carry the CREW
# owns, so the 4× a stale build dip puts on the crew shows up as a crew target 3× too large, and a
# fixture whose regrowth is small next to its carry rounds the whole error away.

const STALE_VERB_CAPACITY := 195.0
# Just above the floor it is worked at, so the crew is REGROWTH-bound rather than room-bound — the
# steady state in which "how many hands hold this patch" is the question the sheet is answering.

const STALE_VERB_STOCK := 112.0

const STALE_VERB_FLOOR := 0.57

const STALE_VERB_CREW := 2
# `labor_config.forage.per_worker_biomass_capacity` (8.0) × the tile's seasonal weight. Worldgen sets
# every food module's weight to `INITIAL_SEASONAL_WEIGHT` (1.0) and no system ever moves it, so this
# IS a live patch's published throughput — the season is not what dips a forager today.

const STALE_VERB_PER_WORKER_BIOMASS := 8.0
# The basket's share-weighted food rate: wild tubers 0.35 × 0.065 + wild rice 0.15 × 0.070. Cotton and
# flax pay no food at all, which is why the patch converts at well under `provisions_per_biomass`.

const STALE_VERB_FOOD_PER_BIOMASS := 0.03325
# (Its trade-rate sibling went with arc #527's yield axis; a cash crop's non-food product is a
# per-material vector on the composition entry, not a rate on the patch.)
# The plant rungs' `yield_fraction_while_building` (`intensification_ladder.json`) — the factor that
# must NOT ride a crew whose build has already landed.

const STALE_VERB_BUILD_FRACTION := 0.25
# Two throughputs are "the same" when they agree to within the resolution the panel states a rate at.

## Rewrite one source dict IN PLACE. `prefix` is "" for a raw herd / wire patch, `patch_` for the
## tile_info cross-ref. Returns the same dict, so call sites read `floorify(fixture)`.
static func floorify(src: Dictionary, prefix: String = "") -> Dictionary:
	if src.is_empty():
		return src
	floorify_ceilings(src, prefix)
	seed_growth_terms(src, prefix)
	return src

## Seed `per_worker_biomass` + `regrowth_samples` + the two phase-band cuts on a fixture that predates
## them. Each is skipped when the fixture states its own, so a state authored to exercise a particular
## curve — or a particular boundary — keeps it.
static func seed_growth_terms(src: Dictionary, prefix: String) -> void:
	var is_herd := BaseFx.fixture_is_herd(src, prefix)
	if not src.has(prefix + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY):
		# Recover it from the fixture's own numbers where they can state it — that is EXACT and keeps
		# every existing frame's expected-yield line unchanged — and fall back to the config's
		# throughput on a source that pays no food, where the recovery is `0/0`.
		var rate := float(src.get(prefix + "provisions_per_biomass", 0.0))
		var per_worker := float(src.get(prefix + "per_worker_yield", 0.0))
		var carry := (per_worker / rate) if rate > 0.0 and per_worker > 0.0 \
			else (FIXTURE_ANIMAL_PER_WORKER_BIOMASS if is_herd else FIXTURE_PLANT_PER_WORKER_BIOMASS)
		src[prefix + SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY] = carry
	var capacity := float(src.get(prefix + "carrying_capacity", 0.0))
	if capacity > 0.0 and not src.has(prefix + SourceForecast.FORECAST_REGROWTH_SAMPLES_KEY):
		var samples := PackedFloat32Array()
		for i in range(FIXTURE_REGROWTH_SAMPLES):
			var fraction := float(i) / float(FIXTURE_REGROWTH_SAMPLES - 1)
			samples.push_back(fixture_regrowth_delta(fraction, capacity, is_herd))
		src[prefix + SourceForecast.FORECAST_REGROWTH_SAMPLES_KEY] = samples
	# THE PHASE BANDS the chart draws as zones. Seeded on BOTH webs (the cut points are ecology config,
	# which every source has) and skipped when a fixture states its own, so a state authored to put a
	# particular boundary under the floor line keeps it.
	if not src.has(prefix + SourceForecast.FORECAST_COLLAPSE_FRACTION_KEY):
		src[prefix + SourceForecast.FORECAST_COLLAPSE_FRACTION_KEY] = FIXTURE_COLLAPSE_FRACTION
	if not src.has(prefix + SourceForecast.FORECAST_STRESSED_FRACTION_KEY):
		src[prefix + SourceForecast.FORECAST_STRESSED_FRACTION_KEY] = FIXTURE_STRESSED_FRACTION
	if not is_herd:
		return
	# **THE WHOLE-ANIMAL QUANTUM, IN BIOMASS.** `crew_to_hold` rounds up to one body on this web
	# (mirroring the sim's `hunt_haul_workers`), and `body_mass` is the term it rounds to — in the same
	# units as the curve, unlike `food_per_animal`, which is that body already converted to provisions.
	# Derived from the fixture's own provisions pair, so it cannot disagree with the rates beside it; a
	# species that states neither leaves it absent and the rounding is simply not applied. (A trade
	# pair stood beside the food one until arc #527 retired that account.)
	if src.has(prefix + SourceForecast.FORECAST_BODY_MASS_KEY):
		return
	var per_animal := float(src.get(prefix + "food_per_animal", 0.0))
	var rate := float(src.get(prefix + "provisions_per_biomass", 0.0))
	if per_animal > 0.0 and rate > 0.0:
		src[prefix + SourceForecast.FORECAST_BODY_MASS_KEY] = per_animal / rate

## One sample of the seeded curve: the source's one-turn biomass delta at `fraction` of K.
static func fixture_regrowth_delta(fraction: float, capacity: float, is_herd: bool) -> float:
	var stock := fraction * capacity
	if is_herd:
		# **THE ANIMAL CURVE GOES NEGATIVE BELOW THE ALLEE POINT.** Past that threshold the herd
		# declines whether or not it is hunted, which is why floor 0 ENDS a herd on this web.
		if fraction < FIXTURE_COLLAPSE_FRACTION:
			return -FIXTURE_COLLAPSE_RATE * stock
		return FIXTURE_ANIMAL_REGROWTH_RATE * stock * (1.0 - fraction)
	# **THE PLANT CURVE NEVER DOES.** A stripped stand is lifted to its reseed floor before it
	# regrows, so the delta at 0 is the lift itself — positive, and the reason a patch comes back.
	var lift := maxf(stock, FIXTURE_RESEED_FLOOR_FRACTION * capacity)
	var grown := minf(capacity, lift + FIXTURE_PLANT_REGROWTH_RATE * lift * (1.0 - lift / capacity))
	return grown - stock

static func floorify_ceilings(src: Dictionary, prefix: String) -> void:
	var legacy := "hunt_policy_ceilings" if prefix == "" and src.has("hunt_policy_ceilings") \
		else "forage_policy_ceilings"
	var rows: Variant = src.get(prefix + legacy, null)
	if not (rows is Dictionary):
		floorify_estimates(src)
		return
	var peak_food := float((rows as Dictionary).get("sustain", 0.0))
	var peak_fodder := legacy_peak(src, prefix, "forage_policy_fodder_ceilings")
	# The stock the ceiling is composed from. Reuse the fixture's own pair when it leaves real room
	# above the peak; otherwise seed one, since dividing by a zero room would fabricate an infinity.
	# **A SOURCE WITH A POSITIVE FOOD-PEAK CEILING IS BY DEFINITION ABOVE THE PEAK**, and several
	# fixtures predate that being expressible: they author a healthy Sustain take on a herd standing
	# BELOW `K/2`, which the four-row model let them get away with and the one-curve model cannot. The
	# capacity is kept (the drawer's "Biomass: B / K" pair is a readout of its own) and the stock is
	# raised to `FIXTURE_STOCK_FRACTION` of it, which is what the authored ceiling was always claiming.
	var capacity := float(src.get(prefix + "carrying_capacity", 0.0))
	var biomass := float(src.get(prefix + "biomass", 0.0))
	# **A SOURCE WITH NO CAPACITY HAS NO FLOOR AXIS AT ALL** — `max(0, B - floor*K)` is `B` at every
	# floor when `K` is 0, so every preset would quote one number and the picker would silently claim
	# the dial does nothing. Several fixtures state a stock without one (nothing read it before), so a
	# capacity is derived from the stock rather than the other way round, which leaves the drawer's
	# "Biomass" reading untouched.
	if capacity <= 0.0:
		capacity = (biomass / BaseFx.FIXTURE_STOCK_FRACTION) if biomass > 0.0 else BaseFx.FIXTURE_CAPACITY
		src[prefix + "carrying_capacity"] = capacity
	var room := biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
	if room <= 0.0:
		biomass = BaseFx.FIXTURE_STOCK_FRACTION * capacity
		room = biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
		src[prefix + "biomass"] = biomass
	src[prefix + "provisions_per_biomass"] = peak_food / room
	src[prefix + "fodder_per_biomass"] = peak_fodder / room
	for key in ["hunt_policy_ceilings", "hunt_policy_trade_ceilings", "forage_policy_ceilings",
			"forage_policy_trade_ceilings", "forage_policy_fodder_ceilings",
			"forage_policy_per_worker", "forage_policy_per_worker_trade",
			"forage_policy_per_worker_fodder"]:
		src.erase(prefix + key)
	floorify_estimates(src)

static func legacy_peak(src: Dictionary, prefix: String, key: String) -> float:
	var rows: Variant = src.get(prefix + key, null)
	return float((rows as Dictionary).get("sustain", 0.0)) if rows is Dictionary else 0.0

## Re-key a legacy `"<stance>:<party>"` raid table onto `"<floor>:<party>"`, and put the two fields
## the client SCANS on each row (`floor` / `party_workers`) — it no longer rebuilds the key, since the
## real key renders the floor with Rust's float Display.
##
## **IT MUST BE IDEMPOTENT, AND IT WAS NOT.** A converted row's key is `"0.5:4"`, whose leading token
## is not a stance, so a SECOND pass over the same dict skipped every row and left an EMPTY table
## behind — and `floorify_ceilings` reaches here even on its early return, so any state that calls
## `_show_herd(h)` and then `_compose_herd(h)` with the SAME dict silently lost its whole raid table.
## Every expedition frame in the `_hunt_assign_forecast_states` block and the boar-raid set did exactly
## that: `hunt_trip_forecast` answered `available: false`, the sheet rendered no forecast at all, and
## the states went on passing because nothing asserted on a readout those frames no longer had. A row
## already carrying the floor field is therefore kept verbatim rather than dropped.
## The raid ROW's own floor field. It was `SourceForecast.HUNT_ESTIMATE_FLOOR_KEY` until the forecast
## query retired the snapshot table those keys named; a row still carries `floor`, and this fixture
## helper is the last thing in the client that builds one out of a legacy stance key.
const RAID_ROW_FLOOR_KEY := "floor"

static func floorify_estimates(src: Dictionary) -> Dictionary:
	var estimates: Variant = src.get("hunt_trip_estimates", null)
	if not (estimates is Dictionary):
		return src
	var rekeyed := {}
	for key in (estimates as Dictionary):
		var converted: Variant = (estimates as Dictionary)[key]
		if converted is Dictionary \
				and (converted as Dictionary).has(RAID_ROW_FLOOR_KEY):
			rekeyed[key] = converted
			continue
		var parts := String(key).split(":")
		if parts.size() != 2:
			continue
		var stance := String(parts[0])
		if not BaseFx.LEGACY_STANCE_FLOORS.has(stance):
			continue
		var floor_value := float(BaseFx.LEGACY_STANCE_FLOORS[stance])
		var party := int(parts[1])
		var row: Dictionary = (estimates as Dictionary)[key].duplicate()
		row["floor"] = floor_value
		row["party_workers"] = party
		rekeyed["%s:%d" % [str(floor_value), party]] = row
	src["hunt_trip_estimates"] = rekeyed
	return src

## Six narrative beats in the `mythic` register, transcribed VERBATIM from the authored copy in
## `core_sim/src/data/beat_definitions.json` with their nouns filled in as the sim would fill them.
## Real copy, not lorem: the panel's whole job is prose, and placeholder text of the wrong length
## would make both the wrapping and the density read wrong.
##
## The first entry is `cold_open.bone_ground` — the LONGEST line in the catalog (225 chars) — so
## the multi-line wrap case is exercised in every telling frame rather than by luck.
static func telling_fixture_events() -> Array:
	return [
		{"tick": 0, "kind": "narrative_beat",
			"label": "We are 24. The ground behind us is bone, and we will not go back to it. Ahead lies a country with no names — not the hills, not the waters, not the years to come. Naming it is your work now. Walk well, and be remembered.",
			"detail": "turn.index = 0 · band.count = 24"},
		{"tick": 3, "kind": "narrative_beat",
			"label": "The scouts came back thinner and louder than they left. Salt Pillar Reach, they said, over and over, until we all knew the word.",
			"detail": "sites.discovered_this_turn = 1"},
		{"tick": 9, "kind": "narrative_beat",
			"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
			"detail": "provisions.total falling for 3 turns"},
		{"tick": 14, "kind": "narrative_beat",
			"label": "A woman pressed seed into the mud to see what it would do. The mud answered. We know a new thing.",
			"detail": "knowledge.cultivation = 1.00"},
		{"tick": 18, "kind": "narrative_beat",
			"label": "The chase is longer every season and ends in less. The aurochs were the road we walked; the road is going quiet under us.",
			"detail": "herd.ecology_phase = collapsing"},
		{"tick": 22, "kind": "narrative_fork",
			"label": "There are paths here now, worn by our own feet, going to places only we go. That is how a country becomes a home, or a trap.",
			"detail": "sedentarization.score = 41"},
	]

## The two remedies a STANDING-but-gated Cultivate must still spell out (issue #420). Each is the tail
## of its `HudFloraVocab` reason, so the assertion reads the sentence the player reads and not just the
## rung's presence: the paused build's ease-off advice, and the finished patch's harvest advice.
## **THE `· then` GRAMMAR THE TWO FACES USED TO CLOSE ON, kept as the needle for its ABSENCE.** The
## payoff has left both the OFFERED and the RUNNING face and reads in the PER TURN readout instead
## (`Readout.improvement_deal_text`): the face's `· then <payoff>` sat one line above a box quoting a
## different number for the same source, and nothing said which question either was answering.
##
## **ABSENCE ALONE IS VACUOUS** — a sheet that lost the payoff outright satisfies it — so every frame
## that asserts this needle is gone from the face also asserts the payoff is PRESENT in the readout,
## by `HudWidgets.IMPROVEMENT_DEAL_META`. Assert the pair, never the half.
const IMPROVEMENT_PAYOFF_NEEDLE := "· then "

## A crop `BaseFx.food_tile_fixture`'s basket really carries, used to prove the crop list is ABSENT under a
## gated offer. Naming a real crop matters: a needle no basket contains would make the assertion pass
## whether the list rendered or not.
const GATED_CROP_NEEDLE := "Wild Grain"

## The crew the two zero-crew submits are composed at. Named because 0 is the WHOLE subject of those
## frames — it is the sim's unassign on a worked source and a no-op on an unworked one — and a bare 0
## beside `COMPOSE_COUNT_UNSET` reads like an omission.
const ZERO_CREW := 0
## The crew the two stance-beside-a-build frames are DIALED at — past every stance's cap, so the sheet's
## own clamp decides the crew and the deal's terms are the CEILING rather than the number typed here.
## It used to be described as "enough to saturate the patch on EVERY stance (Eradicate 4.80 / 0.32 =
## 15)", which stopped being true when the cap learned about the dip (#442): a BUILDING crew is capped
## on `stance × 0.25`, so Sustain clamps to 2 (the build crew) and Deplete to 3. Both frames still show
## the ceiling binding — that is what the clamp guarantees — and the pair still differs only by stance.
# **LABOUR-BOUND UNDER BOTH FLOORS, deliberately.** The build term is floor-independent only where
# the crew is the binding side, so this sits under the food-peak ceiling's dipped crew count
# (0.96 / (0.32 x 0.25) = 12). Fifteen was chosen when the dip rode the CEILING and the frame's claim
# was the opposite one; at 15 the peak's ceiling binds and the two floors' build terms differ.

const IMPROVEMENT_STANCE_FRAME_FORAGERS := 8

## **THE SIM'S OWN `workers_needed` FOR A CULTIVATING CREW ON THE REFERENCE PATCH.** Its derivation from
## the ladder's and the fixture's numbers is on `BandFx.cultivating_forage_band_fixture`, which ships it on the
## wire; `improvement_build_crew` asserts the compose cap equals what the sheet READS BACK off that
## assignment, so the control is the sim's published answer rather than a number the harness chose twice.
const CULTIVATE_SIM_WORKERS_NEEDED := 12

## A CROP-PICKER ROW by the plant it names. A row's face is `<name> <share>% · <payoff>×`, whose share
## and payoff digits are the fixture's business and change whenever a basket is retuned, so the row is
## found by its NAME PREFIX — never by full text, which would make every crop assertion a duplicate of
## the fixture. Returns null when the basket carries no such plant.
static func find_crop_row(root: Node, crop_name: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text.begins_with(crop_name + " "):
		return root as Button
	for child in root.get_children():
		var found := find_crop_row(child, crop_name)
		if found != null:
			return found
	return null

## The improvement control's FACE text, whichever of its three node shapes it is in — the handle the
## meter assertions read. "" when the control is absent.
static func improvement_face(root: Node, improvement: String) -> String:
	var control := find_improvement_control(root, improvement)
	if control is CheckBox:
		return (control as CheckBox).text
	if control is Label:
		return (control as Label).text
	return ""

static func find_improvement_control(root: Node, improvement: String) -> Control:
	if root == null:
		return null
	if root is Control and (root as Control).get_meta(
			HudWidgets.IMPROVEMENT_CONTROL_META, "") == improvement:
		return root as Control
	for child in root.get_children():
		var found := find_improvement_control(child, improvement)
		if found != null:
			return found
	return null

## The indented basket rows, in order. They are the only indented rows the LAND drawer emits.
static func flora_basket_rows(lines: Array[String]) -> Array[String]:
	var rows: Array[String] = []
	for line in lines:
		if line.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT):
			rows.append(line)
	return rows

## A basket with a FODDER crop (Flora roster F3): Hay Grass is fodder-dominant, so a `N.N×` row alone
## would call it worthless. Under Sow the picker reads `Hay Grass 30% · 1.80 hay` beside the staple's
## `Wild Emmer 70% · 3.2×` — each row stating every account it pays. On sowable ground so both rows
## are legal and pressable: a fodder crop is a legal, valuable choice.
##
## **NEITHER PLANT PAYS A MATERIAL AT EITHER RUNG**, and the empty vectors are stated OUT LOUD rather
## than omitted: an absent key would exercise the reader's default instead of the wire's own contract,
## which is that the key is always present and empty means "no row" (arc #527).
##
## **Hay `can_cultivate` too (issue #419)** — its `cultivation_ceiling` is `field`, so the Cultivate rung
## reaches it, and its rung-2 hay is its own number (0.72), not the Field's 1.8. This fixture greyed it
## and shipped only the Field figure, so a Cultivate row here quoted a sown field's hay.
static func fodder_basket_tile_fixture() -> Dictionary:
	var tile := sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
		{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.30,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.25, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.12, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.72, "sow_fodder_payoff": 1.8,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
	]
	return tile

## A basket with a CASH crop (Flora roster F4): Flax pays a MATERIAL rather than calories, so its
## provisions payoff is a fraction of the staple's and the `N.N×` row alone would call it worthless.
## Both rows state every account they pay — `Wild Emmer 70% · 3.2×` beside `Flax 30% · 0.72 fibre`
## under Sow.
##
## **THE TWO RUNGS DIFFER IN KIND HERE, WHICH IS THE POINT OF THE PAIR OF FRAMES** (arc #527). A sown
## Field is 100% its crop, so the emmer's Field quotes NO material at all; a TENDED patch is a weeded
## basket whose flax volunteers are still standing, so the same emmer honestly quotes their fibre. A
## fixture that scaled one rung's vector from the other's would make that indistinguishable.
##
## **BOTH RUNGS ARE POPULATED, and flax `can_cultivate` (issue #419).** This fixture had
## `can_cultivate: false` on the cash crop and no `cultivate_*_payoff` at all, which is a fiction: every
## cash crop's `cultivation_ceiling` is `field`, so `allows_cultivate()` passes and the row is fully
## pressable on the Cultivate rung. Greying it here meant the Cultivate rung of a cash basket had **no
## frame in the harness**, which is how the picker came to print a *sown Field's* trade on the Cultivate
## row unseen. The rung-2 numbers are the shape the sim actually ships (measured: cotton at rung 2 pays
## ~1/3 of its Field trade, and still pays the volunteers' calories at a rate BELOW gathering wild —
## #433 weeds rather than replaces, so the food ratio is a real, warn-inked loss and not a 0).
static func cash_basket_tile_fixture() -> Dictionary:
	var tile := sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			# A TENDED emmer patch keeps its flax volunteers, so it honestly quotes their fibre; a sown
			# emmer FIELD is 100% grain and quotes nothing.
			"cultivate_material_payoff": [{"material_id": "fibre", "amount": 0.04}],
			"sow_material_payoff": []},
		{"species": "flax", "role": "cash", "display_name": "Flax", "share": 0.30,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.30, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.15, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [{"material_id": "fibre", "amount": 0.29}],
			"sow_material_payoff": [{"material_id": "fibre", "amount": 0.72}]},
	]
	return tile

## ---- THE CASH-CROP TILE THE COMPOSE SHEET IS JUDGED ON ------------------------------------------
## **THE PATCH-LEVEL MATERIAL RATES, which are a DIFFERENT question from the crop picker's.** The
## picker's `sow_material_payoff` / `cultivate_material_payoff` are per PLANT and per RUNG — what one
## species would pay if you built on it. These two are the PATCH's own rates for the wild rung being
## gathered right now, and they are what the compose sheet's yields row reads.
##
##   room at the food-peak floor = 84 − 0.5 × 120 = 24 biomass
##   fibre ceiling               = 24 × 0.03       = 0.72 /turn
##   tobacco ceiling             = 24 × 0.02       = 0.48 /turn
##   three foragers (the cap)    = 3 × 0.09 / 0.06 = 0.27 fibre · 0.18 tobacco   ← the binding terms
##
## **THE CREW BINDS, NOT THE CEILING**, for the reason the wolf's does: a ceiling-bound frame renders
## the same string whether or not the per-worker term is read at all, so the `min` it exists to prove
## would be decorative.
##
## **TWO MATERIALS, NOT ONE, AND THAT IS THE POINT OF THE TILE.** A one-material patch passes just as
## well against a producer that summed the vector into a single materials/turn figure — the retired
## trade axis under a new name. Their amounts are deliberately unequal so a sum is visibly not either
## of them.
const CASH_PATCH_FIBRE_PER_BIOMASS := 0.03
const CASH_PATCH_TOBACCO_PER_BIOMASS := 0.02
const CASH_PATCH_FIBRE_PER_WORKER := 0.09
const CASH_PATCH_TOBACCO_PER_WORKER := 0.06
## The two ids are real `materials.json` ids, and the catalogue ships no display name — so the id IS
## the display word, exactly as it is on the picker's basket rows and on a herd's readout.
const CASH_PATCH_FIBRE_ID := "fibre"
const CASH_PATCH_TOBACCO_ID := "tobacco"

## **THE TILE FROM THE SCREENSHOT: 32% cotton, 26% tobacco, and a gather that banks both.** Its
## compose sheet read `0.24 → 0.18 FOOD · — FODDER` and never mentioned the fibre and tobacco the
## crew actually brings back, because `_forage_yield_model` passed FOUR arguments to `yield_rows`
## where its hunt twin passed five. This fixture is what that frame is judged on.
##
## The staple share is deliberately kept: the food row must still read exactly as it did, or "quote
## the materials" would be satisfied by a sheet that had stopped quoting the food.
static func cash_crop_gather_tile_fixture() -> Dictionary:
	var tile := sowable_tile_fixture()
	tile["x"] = 69
	tile["y"] = 13
	tile["patch_material_per_biomass"] = [
		{"material_id": CASH_PATCH_FIBRE_ID, "amount": CASH_PATCH_FIBRE_PER_BIOMASS},
		{"material_id": CASH_PATCH_TOBACCO_ID, "amount": CASH_PATCH_TOBACCO_PER_BIOMASS},
	]
	# **THE SEASONAL WEIGHT IS ALREADY FOLDED INTO THIS TERM** (as it is into `per_worker_biomass`), so
	# it is honestly EMPTY in a dead season and nothing may divide by it. The fixture states it at full
	# season, which is what this tile is.
	tile["patch_per_worker_material"] = [
		{"material_id": CASH_PATCH_FIBRE_ID, "amount": CASH_PATCH_FIBRE_PER_WORKER},
		{"material_id": CASH_PATCH_TOBACCO_ID, "amount": CASH_PATCH_TOBACCO_PER_WORKER},
	]
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.42,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
		{"species": "cotton", "role": "cash", "display_name": "Cotton", "share": 0.32,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.28, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.14, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [{"material_id": CASH_PATCH_FIBRE_ID, "amount": 0.43}],
			"sow_material_payoff": [{"material_id": CASH_PATCH_FIBRE_ID, "amount": 1.08}]},
		{"species": "tobacco", "role": "cash", "display_name": "Tobacco", "share": 0.26,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.22, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.11, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [{"material_id": CASH_PATCH_TOBACCO_ID, "amount": 0.31}],
			"sow_material_payoff": [{"material_id": CASH_PATCH_TOBACCO_ID, "amount": 0.78}]},
	]
	return tile

## PER-TILE FLORA REALIZATION (Flora roster F4) — the SECOND Alluvial Plain tile. Same biome as
## `cash_basket_tile_fixture` (both "Alluvial Plain"), but a DIFFERENT realized basket: two tiles of
## one biome no longer carry the uniform per-biome roster, they carry a seeded per-tile SUBSET. This
## one is cash-DOMINANT — Cotton 55% + Flax 45%, both cash crops paying fibre — where its twin was
## grain-dominant (Wild Emmer 70% + Flax 30%). Rendered beside it, the pair is the visible proof that
## same-biome tiles realize different species/shares. A different coord so it reads as its own tile.
static func cash_variant_basket_tile_fixture() -> Dictionary:
	var tile := sowable_tile_fixture()
	tile["x"] = 68
	tile["y"] = 12
	tile["patch_composition"] = [
		{"species": "cotton", "role": "cash", "display_name": "Cotton", "share": 0.55,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.28, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.14, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [{"material_id": "fibre", "amount": 0.43}],
			"sow_material_payoff": [{"material_id": "fibre", "amount": 1.08}]},
		{"species": "flax", "role": "cash", "display_name": "Flax", "share": 0.45,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.30, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.15, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [{"material_id": "fibre", "amount": 0.29}],
			"sow_material_payoff": [{"material_id": "fibre", "amount": 0.72}]},
	]
	return tile

## **THE TILE THE FOOD-LAYER ROWS ARE JUDGED ON — all three crop ROLES on one patch.** A river-delta
## stand carrying a staple, a cash crop and a fodder crop, so the card's basket shows one of every
## role icon and states outright that most of what grows on this ground is not food: 38% staple
## against 62% cash + fodder. Every other basket fixture is staple-dominant, so until this one existed
## the role icons had no frame that could tell them apart.
##
## **IT STATES ITS OWN STOCK AND CAPACITY, so it deliberately does NOT go through `BaseFx.seed_forage_rows`**
## (the `_stale_verb_tile_fixture` precedent), which pins every fixture it touches to one
## `FIXTURE_CAPACITY`. The capacity is what each basket row's absolute biomass is a share OF, so it has
## to be a number the three rows can be checked against by eye — and 205 is chosen so the naive
## rounding of `38 / 31 / 31` percent MISSES it by one (78 + 64 + 64 = 206), making this frame the
## biomass-remainder test exactly as `BaseFx.food_tile_fixture`'s 46/30/25 is the percentage one.
##
## Standing at full capacity, so `Foraging 205 / 205` and the three rows sum to both numbers at once —
## the clearest possible reading of "these decompose the row above".
const THREE_ROLE_CAPACITY := 205.0

## **DELIBERATELY BELOW THE CEILING.** The basket decomposes what is STANDING, and a full patch
## cannot tell that apart from one decomposing the capacity — the two coincide there, so the
## assertion below would pass either way and prove nothing. 150 of 205 makes the claim testable.
const THREE_ROLE_STOCK := 150.0

const THREE_ROLE_GRAZE_CAPACITY := 130.0

## QUALIFYING GROUND for `Sow` — an alluvial plain beside fresh water, i.e. one of the ~46 tiles of
## 4160 (1.1%) on the standard map that will actually take seed. `patch_sow_site_refusal` is "" (the
## sim's verdict: no fault), so the ▦ Sow option ENABLES once Seed Selection is known. The Sow
## forecast pair is deliberately asymmetric with Cultivate's: `ceiling_sow` is ~0 because a sown
## patch has no standing crop to take a fraction of (a bare-ground sow is PURE investment), and
## `field_yield` is 2× the tended yield — the payoff that makes the ladder's top plant rung worth it.
## SOW'S BUILD DIP, as the wire's own FRACTION of the food-peak ceiling — `0.02 / 0.96`, i.e. the
## near-zero absolute dip this fixture's docstring describes over `food_tile_fixture`'s own Sustain
## ceiling. A sown patch has no standing crop to take a fraction of, so a bare-ground sow is PURE
## investment: that asymmetry against Cultivate's quarter is rung 3's whole bargain, and it is what
## the readout's `without the build` row is measured against on a Sow sheet.
const SOW_BUILD_FRACTION := 0.02 / 0.96

static func sowable_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	# Kept WITHIN the reference band's forage range (it sits on 66,10 with work_range 2) so the Forage
	# button ENABLES: this state exists to judge the Sow affordance, and an out-of-range tile disables
	# the button for an unrelated reason and hides exactly what the frame is for.
	tile["x"] = 67
	tile["y"] = 11
	tile["terrain_label"] = "Alluvial Plain"
	tile["tags_text"] = "Fertile, Fresh Water"
	tile["food_module"] = "riverine_delta"
	tile["food_module_label"] = "Riverine Delta"
	tile["site_name"] = ""
	# The ground answers the site requirement: rich enough AND watered. No refusal.
	tile["patch_sow_site_refusal"] = ""
	# **THE FRACTION IS STATED OUTRIGHT, NOT VIA `patch_ceiling_sow`, AND THAT IS A REPAIR.**
	# `seed_forage_rows` converts the authoring shorthand only `if not tile.has(<fraction key>)` — and
	# `food_tile_fixture()` has already run it once, writing `patch_sow_build_fraction = 0.0` off its own
	# `patch_ceiling_sow` of 0 and ERASING the shorthand key. A layered fixture restating the shorthand
	# is therefore ignored on the re-seed, so this patch carried a build fraction of ZERO,
	# `improvement_forecast` answered `{}` for Sow, and the whole rung quoted no deal on any frame: no
	# payoff row, no `without the build` row, and a bare face before those rows existed. The docstring's
	# own escape hatch — "a fixture that states a fraction outright wins" — is what closes it.
	tile["patch_sow_build_fraction"] = SOW_BUILD_FRACTION
	tile["patch_field_yield"] = 2.40
	return BaseFx.seed_forage_rows(tile)

## A patch mid-SOW: the rung-3 build meter is running, so the Field row reads "Sowing 45%". It sits
## BESIDE the Cultivation row (this ground was tended first) — the two meters are independent and
## both are the SOURCE's own, which is the per-source half of the two-meter split.
static func sowing_tile_fixture() -> Dictionary:
	var tile := sowable_tile_fixture()
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	tile["patch_field_progress"] = 0.45
	tile["patch_is_field"] = false
	return tile

static func field_tile_fixture() -> Dictionary:
	var tile := sowing_tile_fixture()
	tile["patch_field_progress"] = 1.0
	tile["patch_is_field"] = true
	# A completed Field reports every ceiling == per_worker_yield (a managed source needs one worker),
	# exactly as a tended patch does — so the stepper caps at 1.
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return BaseFx.seed_forage_rows(tile)
