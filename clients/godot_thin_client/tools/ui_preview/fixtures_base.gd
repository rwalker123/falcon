## Fixture primitives the land, herd and forage fixtures all build on.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const BARREN_PATCH_PER_BIOMASS := 0.01

# **THE HAY MEADOW'S TWO NON-FOOD RATES.** Sized so the meadow's two accounts BIND DIFFERENTLY, which
# is the fixture's whole job: at the seeded stock (room 40 above the food peak) the fodder ceiling is
# 0.20/turn against a crew that gathers 0.13 fodder/worker, so the CEILING binds on fodder; food
# gathers at 0.08/worker against a 0.60 ceiling, so LABOR binds there. A crew can therefore sit
# comfortably inside the patch's food regrowth while stripping its hay — which is only expressible
# because `min(w x per_worker, ceiling)` and the overdraw verdict are both applied PER ACCOUNT.

const FIXTURE_CAPACITY := 100.0

const FIXTURE_STOCK_FRACTION := 0.9

# ---- THE GROWTH TERMS THE FIXTURES PREDATE (slice 4b) -------------------------------------------
# `perWorkerBiomass` and `regrowthSamples` are wire fields no fixture written before them can carry,
# and the chart needs BOTH — without a curve it renders nothing at all, which would silently drop the
# instrument out of ~50 frames. So the adapter seeds them, in the SAME one place it converts the
# stances, and it is careful about which of the two webs it is standing in for.
#
# **THE HARNESS IS STANDING IN FOR THE SIM HERE, and that is the one place a growth model may be
# written in GDScript.** These constants are the shipped config's (`labor_config.forage.ecology` /
# `fauna_config.ecology`) and the shapes are the two the sim publishes: a patch is logistic lifted to
# its reseed floor and therefore NEVER negative, a herd declines at `collapse_rate` below its Allee
# threshold and therefore IS. A fixture that flattened that asymmetry would let the chart clamp a
# herd's crash to zero and still look right.

## Is this dict a HERD? A herd carries `species`; a forage patch carries `committed_species` and never
## a bare one, and the `patch_` prefix settles the tile_info case outright. It decides which growth
## SHAPE the seeded curve takes, so guessing wrong would hand a patch a herd's crash.
static func fixture_is_herd(src: Dictionary, prefix: String) -> bool:
	return prefix == "" and src.has("species")

## The FLOOR each retired stance stood for, so a converted raid table lands on the sim's own sampled
## floors (`snapshot::RAID_FORECAST_FLOOR_SAMPLES` = 0.0, 0.15, 0.30, 0.50, 0.80). Sustain is the food
## peak; the other three are the successively deeper draws they named.
const LEGACY_STANCE_FLOORS := {
	"sustain": 0.5, "surplus": 0.3, "deplete": 0.15, "eradicate": 0.0,
}

## **Seed the per-policy forage ROWS from the flat scalars this fixture already states** (#426).
##
## The wire now carries the tile's whole yield vector as one row per rung — six dicts keyed by policy,
## both the ceiling and the per-worker term, in all three accounts — and the six flat `patch_ceiling_*`
## scalars are deprecated slots. `SourceForecast.forecast_is_known` reads the ROW's PRESENCE as its
## "does the wire describe this source" witness, so a fixture that seeds only the scalars now correctly
## reads as *undescribed* and renders no forecast at all.
##
## Deriving the rows here rather than hand-writing them at ~30 fixture sites keeps each fixture naming
## its numbers ONCE, in the readable scalar form its comments explain, and makes the two
## representations unable to disagree. A state that wants a genuinely NON-derivable row (a
## fodder-paying tile) overwrites the relevant dict after calling this.
##
## Fodder defaults to 0 — the render-only-when-non-zero rule means every existing frame is then
## byte-identical, which is exactly what a reseeding pass must not disturb.
##
## **THE `patch_ceiling_*` KEYS IT READS ARE A FIXTURE-AUTHORING SHORTHAND, NOT WIRE KEYS, AND THIS
## ERASES THEM** (#426). The six flat scalars they are named after are retired `(deprecated)` wire
## slots that `MapView` no longer cross-refs and `SourceForecast` no longer reads — so a tile dict
## left carrying them would be a wire-shaped key with no wire behind it, and the next fixture author
## to reach for one would get silence rather than an error. Consuming them here keeps ~30 fixtures
## naming their numbers once, in the readable form their comments explain, while guaranteeing no
## fixture can hand the HUD a representation the sim stopped sending.
static func seed_forage_rows(tile: Dictionary) -> Dictionary:
	var per_worker := float(tile.get("patch_per_worker_yield", 0.0))
	# **A RE-SEED FALLS BACK TO WHAT IS ALREADY THERE**, and that is what makes the layered fixtures
	# work: most of them are `food_tile_fixture()` (already seeded) plus a few overrides plus a second
	# `seed_forage_rows`. Reading only the scalars would silently zero every account the second caller
	# did NOT restate.
	var peak_food := float(tile.get("patch_ceiling_sustain", 0.0))
	var peak_fodder := 0.0
	if tile.has("patch_provisions_per_biomass"):
		var prior_room := float(tile.get("patch_biomass", 0.0)) \
			- SourceForecast.FLOOR_FOOD_PEAK * float(tile.get("patch_carrying_capacity", 0.0))
		if peak_food <= 0.0:
			peak_food = float(tile["patch_provisions_per_biomass"]) * prior_room
		peak_fodder = float(tile.get("patch_fodder_per_biomass", 0.0)) * prior_room
	# **THE STOCK THE CEILING IS COMPOSED FROM.** A fixture states a ceiling; the wire states the terms
	# a ceiling is built out of, so this reverses the arithmetic the client now does — pinning each
	# fixture's authored `sustain` number to the FOOD PEAK, which is the honest mapping (Sustain took
	# the renewable yield; the peak is the floor that pays the most forever). At the seeded stock the
	# other two presets fall out at 2.25x and 0.25x of it.
	var capacity := FIXTURE_CAPACITY
	var biomass := FIXTURE_STOCK_FRACTION * FIXTURE_CAPACITY
	var room := biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
	tile["patch_carrying_capacity"] = capacity
	# **AND THE GROUND UNDER THAT CEILING** — the tile's own `K`, which the wire ships beside the
	# patch's and which a REMEMBERED card renders in place of the redacted ceiling
	# (`MapView.FOW_DISCOVERED_HIDDEN_KEYS` header). Equal to the ceiling here because every fixture
	# through this seeder stands below the Field rung, where the gain is 1.0 — a fixture that wants the
	# two to genuinely differ states its own pair, which is why this never clobbers one already set.
	if not tile.has("patch_tile_capacity"):
		tile["patch_tile_capacity"] = capacity
	tile["patch_biomass"] = biomass
	# **A BARREN PATCH KEEPS ITS RATES AND LOSES ITS STOCK** — the dead-season case, and the whole of
	# what issue #426 turns on. Its per-biomass vector is a property of what GROWS there and stays
	# positive; what a dead season zeroes is the crew's throughput and the standing crop. Zeroing the
	# RATE instead would make the patch read as one the wire never described, which is the opposite of
	# the state.
	if peak_food <= 0.0 and peak_fodder <= 0.0:
		tile["patch_biomass"] = SourceForecast.FLOOR_FOOD_PEAK * capacity
		tile["patch_provisions_per_biomass"] = BARREN_PATCH_PER_BIOMASS
		tile["patch_fodder_per_biomass"] = 0.0
	else:
		tile["patch_provisions_per_biomass"] = peak_food / room
		tile["patch_fodder_per_biomass"] = peak_fodder / room
	# **THE BUILD DIPS ARE RETIRED** (`docs/plan_standing_upkeep.md` §2.2), so the authoring shorthand
	# `patch_ceiling_<rung>` no longer converts to anything — the native reader stopped publishing
	# `<rung>BuildFraction` and nothing composes one. The keys are dropped here rather than at every
	# fixture, so a fixture that still states one is simply ignored instead of seeding a dict key no
	# reader would ever look at.
	for rung in SourceForecast.FORAGE_IMPROVEMENTS:
		tile.erase("patch_ceiling_%s" % rung)
	for policy in LEGACY_STANCE_FLOORS:
		tile.erase("patch_ceiling_%s" % policy)
	return tile

## ---- THE BUILD, PRICED IN WORK (`docs/plan_unit_costed_work.md` §8) ---------------------------
## `intensification_ladder.json`'s own `work_cost` for the two PLANT rungs. One unit is one
## worker-turn at the food peak with no gear, so the shipped 50 and 75 read themselves — and the
## pair is what makes the two rungs visibly different jobs rather than one meter filling at
## unexplained speeds. Stated here so every plant fixture prices its rungs from one place.
## **WHAT IT COSTS TO HOLD A TENDED PATCH, PER TURN** — `intensification_ladder.json`'s
## `plant:tended` `upkeep.work_per_turn`, `scaled_by: flat`. Stated here so every plant fixture bills
## its keeping from one place, exactly as the two work costs above are.
## **THE SHIPPED DEMAND, AND IT IS A WHOLE NUMBER A PLAYER CAN STAFF EXACTLY** — `plant:tended`
## declares `work_per_turn` **2.0**, `flat`. It was `0.5` here, which is the rung's `meter_decay`
## and was its demand too back when SHORTFALL WAS THE DECAY; splitting those two dials is what let
## the demand move to two hands while the rot rate stayed exactly where it was
## (`docs/plan_standing_upkeep.md` §2.4). A stale copy here is what the offered face would QUOTE as
## the standing price of a Tended Patch, and for one slice it was also a term of every plant frame's
## build estimate — so it is the ladder's number, restated nowhere else.
const PLANT_TENDED_UPKEEP_PER_TURN := 2.0

## The Field's own, one rung up (`plant:field`, **4.0**/`flat`) — a standing crop wants four hands
## where tended ground wants two, which is the ladder's whole claim about a higher rung. It carried
## the rung's `meter_decay` (0.75) for the reason the rate above carried its own.
const PLANT_FIELD_UPKEEP_PER_TURN := 4.0

const PLANT_CULTIVATE_WORK_COST := 50.0

const PLANT_SOW_WORK_COST := 75.0

## The SIM's own estimate of what is left on the running build, at this fixture's crew, floor and
## kit. **The client computes none of it** — it holds neither the crew's output nor the floor
## multiplier nor the kit's contribution — so a fixture states an ANSWER here exactly as it states a
## yield forecast. `SourceForecast.BUILD_TURNS_NO_ESTIMATE` is the other reading, and it renders as
## no line at all.
const BUILD_TURNS_REMAINING := 11

## **NO PLANT ITEM DECLARES THE BUILD STAT YET** (issue #539 is the hoe), so a plant build's gear
## contribution is honestly `0` and the tile card renders no gear line. The ANIMAL fixtures carry a
## real one — `hurdles` ships 8.5 per equipped keeper — which is where that readout is judged.
const PLANT_BUILD_WORK_FROM_GEAR := 0.0

## **THE TERMS THE COMPOSE SHEET EVALUATES ITS OWN ESTIMATE FROM**, beside the sim's answer above.
## The per-worker figure is `intensification::PER_WORKER_OUTPUT` — one worker-turn at the food peak is
## one work unit, which is what makes the shipped 50 and 75 read as turn counts at a single hand — and
## it is stated rather than assumed for the reason the wire publishes it: the sim writes worker output
## as a sum of terms with one term today.
const BUILD_WORK_PER_WORKER_TURN := 1.0

## **NOTHING IS BLEEDING OFF THIS PATCH'S METER**, which is what a kept source publishes and what
## every fixture here wants unless it is staging the rot itself (`docs/plan_standing_upkeep.md` §2.4).
## It is the term the build's closed form nets — `net = crew work − rot` — so a fixture that left it
## unset would be pricing its builds against an absent field rather than against a stated nothing.
##
## **A ROT AND A KEEPING SHORTFALL ARE ONE STATE, so a fixture staging one must state the other**:
## the rot exists BECAUSE the band's keeping pool came up short past this rung's grace, and a patch
## bleeding work with `patch_upkeep_shortfall` at zero is a state no sim can produce.
const PLANT_METER_ROT_NONE := 0.0

## Price a tile's two plant rungs in WORK, deriving each meter's `work_done` from the fraction the
## fixture already states — so a fixture that re-dials its progress cannot end up with a percentage
## and an absolute that disagree, which is the one thing this readout exists to make visible.
##
## `rot` is what this patch's at-risk meter is LOSING per turn — the source-level term, published
## whichever rung is carrying the work, and `PLANT_METER_ROT_NONE` on every kept patch.
static func price_plant_build(tile: Dictionary, turns: int = BUILD_TURNS_REMAINING,
        rot: float = PLANT_METER_ROT_NONE) -> Dictionary:
	tile["patch_cultivation_work_cost"] = PLANT_CULTIVATE_WORK_COST
	tile["patch_cultivation_work_done"] = \
		float(tile.get("patch_cultivation_progress", 0.0)) * PLANT_CULTIVATE_WORK_COST
	tile["patch_field_work_cost"] = PLANT_SOW_WORK_COST
	tile["patch_field_work_done"] = \
		float(tile.get("patch_field_progress", 0.0)) * PLANT_SOW_WORK_COST
	tile["patch_build_turns_remaining"] = turns
	tile["patch_build_work_from_gear"] = PLANT_BUILD_WORK_FROM_GEAR
	tile["patch_build_work_per_worker_turn"] = BUILD_WORK_PER_WORKER_TURN
	# **THE PER-RUNG RATE, PUBLISHED UNCONDITIONALLY BESIDE THE PER-RUNG COST** — the second term of
	# the compose sheet's closed form, and the one term `patch_upkeep_demand` cannot supply: that field
	# is what the patch is BILLED today, so it reads `0` on a patch with no progress. These are the
	# LADDER's rates, so they are set here rather than beside the billed figure and they survive
	# `unbuilt()` — which is exactly the reported repro (a wild patch quoting ≈50 turns on a Cultivate
	# at one builder, against a rung asking 2 work a turn).
	tile["patch_cultivation_upkeep_demand"] = PLANT_TENDED_UPKEEP_PER_TURN
	tile["patch_field_upkeep_demand"] = PLANT_FIELD_UPKEEP_PER_TURN
	# **AND THE ROT, WHICH IS THE TERM THE BUILD'S PACE ACTUALLY NETS** (slice 6a). The rates above
	# are the STANDING price the offered face quotes; this is what an under-kept meter is losing right
	# now, per SOURCE rather than per rung, and it is what decides whether a build climbs, holds or
	# slides back.
	tile["patch_meter_rot_per_turn"] = maxf(rot, PLANT_METER_ROT_NONE)
	# **AND WHERE IT SITS IN THE BAND'S QUEUE** (`docs/plan_standing_upkeep.md` §4.6b). A priced build
	# is one the sim publishes a countdown for, and it publishes one only for a source some band has
	# queued — so a fixture that priced a build and left the position at the sentinel would stage a
	# countdown belonging to nothing, and the sheet would read it as an entry the pool is NOT on.
	# `unbuilt` below puts it back to the sentinel, a patch nobody has declared being in no queue.
	tile["patch_build_queue_position"] = SourceForecast.BUILD_QUEUE_HEAD
	return tile

## **A PATCH NOBODY IS BUILDING — both plant meters at zero, re-priced.**
##
## It exists because the build verb is DERIVED from the meter now
## (`docs/plan_standing_upkeep.md` §2.4): a patch carrying progress IS building that rung, declared
## or not, so the reference tile's own `patch_cultivation_progress` 0.6 makes it a patch
## mid-Cultivate wherever it is used. Every frame whose claim is about an OFFER — a rung on the
## table, a floor walk with no build to stack a second "later" against — has to stage a source at
## zero, and stating that once is what stops each of them zeroing a meter and forgetting to
## re-price the absolutes beside it.
static func unbuilt(tile: Dictionary) -> Dictionary:
	tile["patch_cultivation_progress"] = 0.0
	tile["patch_is_cultivated"] = false
	tile["patch_field_progress"] = 0.0
	tile["patch_is_field"] = false
	# **AND THE KEEPING GOES WITH THE RUNG.** `patch_unwinding_rung` answers `None` with both meters
	# at zero, so a wild patch is billed nothing, owes nothing and — with no work banked anywhere on
	# it — has nothing that could rot. `price_plant_build` below restores the rot to
	# `PLANT_METER_ROT_NONE` for exactly that reason: a wild patch that kept an inherited bleed would
	# be quoting a build against work it does not hold (`docs/plan_standing_upkeep.md` §2.4).
	tile["patch_upkeep_demand"] = 0.0
	tile["patch_upkeep_supplied"] = 0.0
	tile["patch_upkeep_shortfall"] = 0.0
	tile["patch_upkeep_workers_needed"] = 0
	tile["patch_has_neglect_grace"] = false
	tile["patch_neglect_grace_remaining"] = 0
	tile = price_plant_build(tile)
	# …and it is in NO band's queue, nothing having been declared on it. Set after the pricing above,
	# which stamps the head position every priced build gets.
	tile["patch_build_queue_position"] = SourceForecast.NOT_IN_ANY_BUILD_QUEUE
	return tile

static func food_tile_fixture() -> Dictionary:
	var tile := {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		# Fertile steppe: low drain → "Hospitable" (green Tile-card row).
		"habitability": 0.01,
		# Mid-latitude ~18° → "Temperate" climate band (neutral Tile-card row).
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		# A discovered Wondrous Site on this tile → the Tile card shows a "Site: …" line.
		"site_name": "Verdant Basin",
		# Forage patch being worked toward cultivation → the Tile card's "Cultivation 60%" row.
		"patch_cultivation_progress": 0.6,
		"patch_is_cultivated": false,
		"patch_has_owner": true,
		"patch_owner": 0,
		"patch_ecology_phase": "thriving",
		# Standing forage stock vs the patch ceiling (sim default capacity 120) → the Tile card's
		# "Forage biomass 84 / 120" row, the patch counterpart to a herd's Biomass row.
		"patch_biomass": 84.0,
		"patch_carrying_capacity": 120.0,
		# Pre-commit yield forecast (food/turn at THIS biomass, exported at output_multiplier 1.0).
		# Sustain's ceiling admits ceil(0.96 / 0.32) = 3 useful foragers — below band 821's 10 idle
		# workers, so the Foragers stepper caps at 3 and shows the "max 3 workers useful here" note.
		# The higher-policy ceilings admit 6 / 9 / 15, so switching policy visibly moves the cap.
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
		# The Cultivate INVESTMENT rung: while the patch is being prepared it pays only a fraction of
		# its Sustain ceiling (the dip the player is buying with), then flips to the tended yield.
		# Both are food/turn at output_multiplier 1.0, like the ceilings above.
		"patch_ceiling_cultivate": 0.24,
		"patch_tended_yield": 1.20,
		# **THE STANDING UPKEEP** (`docs/plan_standing_upkeep.md` §2). `plant:tended` declares
		# `0.5` work per turn, `flat` — a patch is ONE TILE, so the rate is the cost of the thing
		# existing. The reference patch is KEPT and the bill is met EXACTLY: the sim charges the
		# keeping against what the crew supplied, capped at the demand, so `supplied == demand` is what
		# a paid rung reads. A fixture stating more supplied than demanded would render `1 of 0.5 work`
		# — arithmetic that looks like a defect on the row whose whole job is to be legible.
		"patch_upkeep_demand": PLANT_TENDED_UPKEEP_PER_TURN,
		"patch_upkeep_supplied": PLANT_TENDED_UPKEEP_PER_TURN,
		"patch_upkeep_shortfall": 0.0,
		# `ceil(2.0 / 1.0)` — two hands meet the whole bill, and the same two are the minimum viable
		# BUILD crew while this patch's meter is still going up (`SourceForecast.min_build_crew`).
		"patch_upkeep_workers_needed": 2,
		# THE NEGLECT GRACE — the countdown to this rung reverting, now counted in turns of upkeep
		# SHORTFALL. The reference patch is kept, so it reads the plant:tended rung's full
		# `grace + 1` = 3: "stop paying and you have this long". `has_neglect_grace` is what makes the
		# number readable at all — a wild patch would ship `false`, not a zero.
		"patch_has_neglect_grace": true,
		"patch_neglect_grace_remaining": 3,
		# Plant RUNG 3 — the Field + the Sow verb. This reference tile is ordinary prairie steppe:
		# rich enough to forage, but it will NOT take seed (rung 3 moves seed, it cannot fertilize or
		# irrigate), so the sim's `sow_site_refusal` verdict rides here and the Sow option is gated
		# with the reason. Only ~1% of a real map is sowable, so REFUSED is the common case and is
		# deliberately the default fixture; `ForageFx.sowable_tile_fixture` is the exception.
		"patch_field_progress": 0.0,
		"patch_is_field": false,
		"patch_ceiling_sow": 0.0,
		"patch_field_yield": 0.0,
		"patch_sow_site_refusal": "too_dry",
		# WHAT GROWS HERE (flora roster F1) — the named plants this tile's forage capacity decomposes
		# into. Wire order (share DESC, then species key ASC) is preserved verbatim by the card.
		# The shares are chosen so NAIVE rounding totals 101% (46 + 30 + 25): the card must absorb the
		# remainder into the largest share and render 45 / 30 / 25 — this fixture IS the rounding test.
		# `can_cultivate` / `can_sow` are SPECIES-GLOBAL rung legality (flora roster S1), deliberately
		# mixed here so the crop picker has a greyed row in every frame: Oak Mast climbs nothing (a wild
		# harvest forever) and Ground Nut tends but never sows. `*_yield_ratio` is what committing PAYS
		# relative to gathering wild, on the CORRECTED scale (the sim's ratio omitted
		# `tended_regrowth_gain`, understating every Cultivate figure by exactly 2×) — so above 1.0 is
		# now the NORM and these read: a strong crop (2.40×), an honest middle one (1.70×) and the 0
		# sentinel on the greyed rows. `*_payoff` is the same rung's provisions/turn committed to THAT
		# species, and it is what the compose sheet's "→ then" term quotes once a crop is picked: the two
		# rows differ (1.20 vs 0.85), which is what makes the selection visibly move the forecast.
		# **EVERY `species` HERE IS A REAL `flora_config.json` ID, and the LABELS BESIDE THEM ARE
		# DELIBERATELY NOT** (issue #339). The key is an asset lookup now — `FloraSprites` composes
		# `<species>.png` from it — so an invented key silently renders the crop-role fallback; the
		# display name is this fixture's own and renaming it to the roster's would move frames for a
		# reason that has nothing to do with art. Do not "tidy" `Wild Grain` into `Wild Emmer`.
		"patch_composition": [
			{"species": "wild_emmer", "role": "staple", "display_name": "Wild Grain", "share": 0.455,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "wild_tubers", "role": "staple", "display_name": "Ground Nut", "share": 0.295,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.85, "sow_payoff": 0.0},
			{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.25,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
		# The GRAZE (pasture) layer — the ANIMAL-edible twin of the forage patch above (Grazing Phase
		# 2a). Prairie steppe is the reference pasture: capacity 240, standing full, hence Thriving.
		# Rendered as the `Grazing` row directly under `Foraging` and its basket, so the card states the
		# two facts side by side: what HUMANS can eat here, and what ANIMALS can eat here.
		"graze_biomass": 240.0,
		"graze_capacity": 240.0,
		"graze_ecology_phase": "thriving",
	}
	# The reference plant tile prices BOTH its rungs, because the wire does: `workCost` is published
	# whether or not a build is in flight, which is what lets the compose sheet quote a rung before
	# the player commits to it.
	return seed_forage_rows(price_plant_build(tile))
