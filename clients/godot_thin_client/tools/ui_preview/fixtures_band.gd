## Band and expedition fixtures.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")

static func band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"size": 148,
		"entity": 904,
		"faction": 0,
		"pos": [71, 18],
		# Good food state: a long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the
		# Food line reads "… · +0.26 /turn" and the category breakdown is collapsed (clickable open).
		"turns_of_food": 22.0,
		# Good morale (≥ warn, not falling) → the Morale row is collapsed with a ▸ caret. The signed
		# Layer-1 contributions (above the breakdown epsilon) give the disclosure real content on expand.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		# Thriving growth (docs/plan_population_growth_model.md): fed (hunger 1.0, so that factor is
		# neutral and its row is omitted), a saturated larder (reserve 1.5) and net-positive food
		# (trend 1.25) → 1.0 × 1.5 × 1.25 = 188% of normal. Reads neutral ink — normal growth is
		# normal, not a "good" — and its disclosure shows what is HELPING, which is the good-state
		# case the row must still be openable in.
		"fertility_hunger": 1.0,
		"fertility_reserve": 1.5,
		"fertility_trend": 1.25,
		"stores": {"provisions": 84.0},
		# Early-Game Labor (slice 3b): 16 working-age workers, 3 idle, split across a
		# Forage tile, a Hunt herd, and the Scout + Warrior band-wide roles.
		"working_age": 16,
		"idle_workers": 3,
		# Server's hard party-size cap (expedition config, default 8) — the outfit stepper maxes at
		# min(idle, this).
		"max_expedition_party_size": 8,
		# Global config levers echoed on every cohort. They are DISPLAY levers — neither computes
		# a trip length. The targeting banner's turns-to-fill is a PURE LOOKUP into the target herd's
		# `hunt_trip_estimates` (the sim forward-simulates the trip and exports the answer); the client
		# does ZERO arithmetic for an expedition and never divides a carry cap by a rate.
		#   expedition_viability_warn_turns — the viable/not-viable threshold applied to turns_to_fill.
		#   hunt_per_worker_provisions      — one hunter's throughput, used ONLY by the resident-band
		#     LOCAL hunt preview, which IS arithmetic: min(workers × 0.8, band_ceiling) × output_mult.
		# Band = flow arithmetic; expedition = lookup.
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0): the forecast shows the HAUL a filled pack delivers as
		# party × this (blessed party×lever arithmetic, NOT the turns-to-fill lookup).
		"expedition_per_worker_carry": 4.0,
		"work_range": 2,
		# Hunt reach (work_range + hunt leash) — large enough here that BOTH the reference herd_fixture
		# (9 tiles from this band's pos) and the occupied-hex herd (16 tiles) stay WITHIN reach, so those
		# herd states render the LOCAL "Hunt Here" controls (the far-herd expedition path has its
		# own dedicated fixtures, hunt_distance_bands).
		"hunt_reach": 16,
		"scout_reveal_radius": 2,
		"activity": "forage",
		# Band food flow (Food summary line): total income across the worked sources vs the cohort's
		# consumption. Net = 0.94 − 0.68 = +0.26 (positive → larder growing), shown green on the Food
		# line. Per-source actual/sustainable yields live on the assignments below.
		# The Gathered/Hunted breakdown sums the assignment actual_yields (0.48 / 0.46) by kind.
		"food_income": 0.94,
		"food_consumption": 0.68,
		# `workers_needed` is the overstaffing axis, INDEPENDENT of the overdraw (⚠) axis — the two
		# rows below deliberately cross them so one frame proves both, AND proves the ⚠ now keys off the
		# sim-answered `overdraws` bool, not the client-derived `actual > sustainable`:
		#   • forage: 5 assigned but only 1 needed (the patch's ceiling caps the take) → the amber
		#     "· only 1 of 5 working" note, and NO ⚠ (Sustain patch, overdraws=false).
		#   • hunt: 4 assigned, 4 needed → no overstaff note. `actual_yield 0.46 > sustainable_yield 0.20`
		#     (a banked whole animal cashed on this KILL turn), yet `overdraws=false` under Sustain → the
		#     row reads CLEAN, NO ⚠. Under the old client test this row false-tripped the flag — the fix.
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "workers_needed": 4, "overdraws": false},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Freshwater Marsh",
			"tags_text": "Freshwater, Wetland",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A scouting expedition (docs/plan_exploration_and_sites.md §2) in its awaiting-orders phase:
## a detached party (is_expedition) carrying a mission/phase + party size + provisions. The drawer
## renders the dedicated expedition readout + Recall/Move panel, not the labor-allocation UI.
static func expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"size": 6,
		"entity": 7001,
		"faction": 0,
		"pos": [80, 30],
		"turns_of_food": 9.0,
		"stores": {"provisions": 48.0},
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "awaiting",
		"tile_info": {
			"x": 80, "y": 30,
			"terrain_label": "Highland Tundra",
			"tags_text": "Cold, Exposed",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## Distance-aware herd-hunt (docs/plan_exploration_and_sites.md §2b): two player bands at DIFFERENT
## distances from ONE herd — a NEAR band ON the herd tile (within hunt_reach → LOCAL hunt) and a FAR
## band ~27 tiles away (beyond reach → hunting EXPEDITION). Proves the SELECTED band (band-picker)
## drives the local-vs-expedition label + command + band-entity target — the case single-band
## playtest can't surface. Both carry idle workers + a party cap so either verb is dialable.
static func hunt_distance_bands() -> Array:
	return [
		{"entity": 811, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "forage", "labor_assignments": []},
		{"entity": 812, "faction": 0, "size": 80, "current_x": 86, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "hunt", "labor_assignments": []},
	]

## Range-aware forage: two player bands at DIFFERENT distances from the (66,10) food tile — a NEAR band
## 1 tile away (within work_range 2 → forage ENABLED) and a FAR band ~21 tiles away (beyond range →
## forage DISABLED + out-of-range hint). Foraging is stationary gathering, so out-of-range has NO
## expedition fallback — just a disabled button. Proves the SELECTED band (band-picker) drives the
## enabled-vs-disabled state — the case single-band playtest can't surface.
static func forage_range_bands() -> Array:
	return [
		{"entity": 821, "faction": 0, "size": 120, "current_x": 67, "current_y": 10,
			# **THE IDLE COUNT HAS TO CLEAR THE DIPPED BUILD CREW.** `improvement_build_crew` asserts the
			# stepper reaches the sim's own `workers_needed` (12 since the dip moved onto the crew), and
			# the stepper caps at `idle + already staffed` — so 10 idle pinned it one short and the frame
			# would have failed on the labour bound rather than on the thing it is testing.
			"working_age": 20, "idle_workers": 16, "work_range": 2, "activity": "forage", "labor_assignments": []},
		{"entity": 822, "faction": 0, "size": 80, "current_x": 80, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "work_range": 2, "activity": "forage", "labor_assignments": []},
	]

## The near band of `forage_range_bands`, ALREADY WORKING the (66,10) food tile — the fixture behind
## the drawer's standing-assignment summary (§14). The assignment deliberately crosses the two
## INDEPENDENT flags the summary shares with a Band-panel Current-actions row: `overdraws` true (a
## Deplete patch drawing past regrowth — the ecological ⚠) AND 4 workers where 2 are needed (the labor
## "· only 2 of 4 working" note). `realized_yield` is the steady average the summary headlines.
## The near band of `forage_range_bands`, ALREADY WORKING the (66,10) food tile at a MODEST staffing —
## the fixture behind the compose sheet's UNASSIGN state. Deliberately separate from
## `_standing_forage_band_fixture`, whose assignment is tuned to trip the drawer summary's overdraw and
## overstaff flags; this one is a plain, healthy Cultivate crew, so the unassign frame is judged on the
## button/forecast pair and nothing else.
##
## It is also the fixture behind the two STANDING-BUT-GATED frames (issue #420), which is why the tile
## is a PARAMETER: a standing assignment is matched by TILE, so a frame selecting a patch other than
## the (66,10) reference — the finished Tended Patch at (67,11) — would read as UNSTAFFED there, i.e.
## exactly the "not standing" case those frames must not render. Both defaults keep every existing
## caller on the reference tile.
## **`workers_needed` IS THE SIM'S OWN ANSWER, AND IT IS WHAT THE COMPOSE CAP IS JUDGED AGAINST.**
## Derived here by the sim's rule rather than picked, so the assertion on `improvement_build_crew` has a
## control it did not write itself. For this patch under Sustain + Cultivate
## (`BaseFx.food_tile_fixture`: per-worker 0.32, Sustain ceiling 0.96, cultivate fraction 0.25, crew 2):
##   take        = min(w × 0.32 × 0.25, 0.96)       (`forage::forage_take` — **THE DIP RIDES THE CREW**)
##   take crew   = ceil(0.96 / (0.32 × 0.25)) = 12  (`systems::labor::workers_needed_for_take`)
##   workers_needed = max(build crew 2, take crew 12) = 12  (`systems::labor::source_crew_needed`)
## **THE NUMBER QUADRUPLED when the dip moved off the ceiling** (`docs/plan_harvest_floor.md` §3.1),
## and that is its whole player-visible consequence: a crew big enough to saturate the source's stock
## pays no dip at all, so the remedy for a slow build is HANDS — at a 25% carry, four times as many.
## It read `2` under the dipped ceiling and `1` before either half of that existed.
static func cultivating_forage_band_fixture(x: int = 66, y: int = 10) -> Dictionary:
	var band: Dictionary = forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 1, "target_x": x, "target_y": y, "floor": 0.5,
		"improvement": "cultivate",
		"actual_yield": 0.08, "sustainable_yield": 0.96, "realized_yield": 0.08,
		"workers_needed": ForageFx.CULTIVATE_SIM_WORKERS_NEEDED, "overdraws": false,
	}]
	return band

## The band the herd-panel LOCAL preview states staff: it sits ON the (66,10) herd (distance 0 ≤ reach
## 7 → local branch) and runs at a REDUCED `output_multiplier` (0.9), so the yield preview visibly
## applies the band's morale/discontent productivity modifier — the one term that makes a resident
## hunt's take differ from an expedition's.
static func hunt_preview_local_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 832, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 14, "idle_workers": 10,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 0.9,
		"activity": "hunt", "labor_assignments": [],
	}

# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) ------------------------------------
# Shipped tiers, one pair per kit, at the values `equipment.json` / `labor_config` authorise. They are
# named rather than inlined because both the Kit ROW's frames and the hunt sheet's COMBAT-GATE frames
# are judged against them, in two different chapters — and because the pairing of a kit with its own
# tier is the fact these fixtures exist to hold. `attack 1` is the creatures.json person, which is
# below every megafauna's `defense`, so it is also what makes the gate's refusal reachable at all.
const KIT_ATTACK_EQUIPPED := 20.0

const KIT_ATTACK_BARE := 1.0

## **THE SLED'S TIER, AND IT IS NOT THE BASKET'S.** A carcass is one lumpy object you drag out whole,
## so losing the sled cuts the HUNT's haul to 12 and touches gathering not at all.
const KIT_HUNT_CARRY_EQUIPPED := 40.0

const KIT_HUNT_CARRY_BARE := 12.0

## **THE BASKET'S TIER, AND IT IS NOT THE SLED'S.** Berries are bounded by what you can hold, so the
## bare-handed ratio here is far harsher — a fifth, against the hunt's drag-something-anyway 30%.
const KIT_FORAGE_CARRY_EQUIPPED := 8.0

const KIT_FORAGE_CARRY_BARE := 1.6

# The three conditions a kitted band ships with. **DELIBERATELY THREE DIFFERENT NUMBERS** on the
# 0-100 scale: a fixture that gave two kits one value would pass every assertion with their accessors
# swapped, which is the exact defect class this arc keeps reproducing.
const KIT_CONDITION_SPEARS := 87.0

const KIT_CONDITION_SLED := 54.0

const KIT_CONDITION_BASKETS := 31.0

# ---- THE KIT ROSTER (`docs/plan_denial_raid.md`, `SubsistenceSection.kits`) -----------------------
# The ids the wire carries and the two job defaults. Named because the `kit <id>` COMMAND token is
# asserted against them and because "which id is the default" is half of what the picker's frames
# claim — a literal in two harnesses is how those two claims come apart.
const KIT_ID_BIG_GAME := "big_game"
const KIT_ID_GATHERING := "gathering"
const KIT_ID_NONE := "none"
const KIT_DEFAULT_HUNT := KIT_ID_BIG_GAME
const KIT_DEFAULT_FORAGE := KIT_ID_GATHERING

## The world's kit roster, in `equipment.json` order — the picker's list, and the ONE roster both
## preview harnesses drive (`band_panel_preview` preloads this module for it, so the two cannot quote
## different tiers or a different default).
##
## **EVERY ENTRY STATES ALL THREE TIERS, and the ones its kit does not use are the BARE ones.** That
## is the wire's own shape and it is what `KitRoster.unequipped_tier` reads the bare-handed tier off:
## the minimum across the roster on an axis IS that axis's unequipped tier, so a fixture that left an
## unused axis at its equipped value would make the client's step-down silently unreachable.
##
## **`none` IS AN ORDINARY MEMBER AND IT IS AUTHORED LAST**, exactly as `equipment.json` authors it —
## which is the whole of why the picker renders it last. The client sorts nothing.
static func kit_roster_fixture() -> Array:
	return [
		{
			"id": KIT_ID_BIG_GAME, "display_name": "Big-game kit", "jobs": ["hunt"],
			"attack": KIT_ATTACK_EQUIPPED,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_EQUIPPED,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
		},
		{
			"id": KIT_ID_GATHERING, "display_name": "Gathering kit", "jobs": ["forage"],
			"attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_EQUIPPED,
		},
		{
			"id": KIT_ID_NONE, "display_name": "No kit", "jobs": ["hunt", "forage"],
			"attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
		},
	]

## A band carrying ALL THREE kits, each at its own condition and each role at its equipped tier.
static func with_equipped_kit(band: Dictionary) -> Dictionary:
	band["hunting_kit_durability"] = KIT_CONDITION_SPEARS
	band["sled_kit_durability"] = KIT_CONDITION_SLED
	band["basket_kit_durability"] = KIT_CONDITION_BASKETS
	band["hunter_attack"] = KIT_ATTACK_EQUIPPED
	band["hunt_carry_per_worker_biomass"] = KIT_HUNT_CARRY_EQUIPPED
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_EQUIPPED
	return band

## **ONE KIT DRY, THE OTHER TWO INTACT** — the state that proves the three wear independently. The
## baskets have run out, so the FORAGE carry has stepped down to bare hands and the hunt's has not:
## a band that has gathered its baskets to pieces still drags carcasses home on an untouched sled.
## This is the frame a readout rendering one carry on the other's row fails.
static func with_baskets_dry(band: Dictionary) -> Dictionary:
	band = with_equipped_kit(band)
	band["basket_kit_durability"] = 0.0
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_BARE
	return band

## A band that has run EVERY kit dry — the bare-hands state, which is permanent: there is no
## replenishment path, so every role has stepped down and stays there. Its `hunter_attack` of 1 is
## what the combat gate refuses megafauna on.
static func with_bare_hands(band: Dictionary) -> Dictionary:
	band["hunting_kit_durability"] = 0.0
	band["sled_kit_durability"] = 0.0
	band["basket_kit_durability"] = 0.0
	band["hunter_attack"] = KIT_ATTACK_BARE
	band["hunt_carry_per_worker_biomass"] = KIT_HUNT_CARRY_BARE
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_BARE
	return band
