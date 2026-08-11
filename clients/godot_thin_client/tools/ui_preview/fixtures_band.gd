## Band and expedition fixtures.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")

## The shipped `expedition_config.hunt.forecast_horizon_turns` — how far the sim simulates a raid before
## giving up on it, echoed onto every cohort as `expedition_forecast_horizon_turns`. **It is not a trip
## length**: it bounds the HUNTING only, so an unbounded raid's floor is this PLUS the round-trip travel
## the band's own position implies. Named here so every band fixture in every chapter states one number.
const FORECAST_HORIZON_TURNS := 60

## **THE DURABLE BAND HANDLE, DERIVED FROM THE ENTITY AND DELIBERATELY DIFFERENT FROM IT.**
## `band_id` is what every band-addressed command and every forecast QUERY names (`HudConst.NO_BAND_ID`);
## `entity` is client-local ECS allocation state. Both are plain ints, so a fixture where the two agree
## cannot tell a correct emit from one that sent the entity — the defect `band_panel_preview` already
## carries this offset for. The offset keeps ids readable (band 904 → 4904) while guaranteeing they
## differ; `band_panel_preview` reads it from here, so the two harnesses cannot drift apart on it.
const FIXTURE_BAND_ID_OFFSET := 4000

## **A COHORT WITHOUT `band_id` ASKS NOTHING.** The pre-launch raid forecasts are a query on the command
## socket now, and every asker refuses to compose a question about a band holding `NO_BAND_ID` — so a
## band fixture missing the field renders the PENDING placeholder in place of every raid readout, on a
## HUD that is behaving correctly. Every band fixture here is therefore stamped.
static func with_band_id(band: Dictionary) -> Dictionary:
	band["band_id"] = int(band.get("entity", 0)) + FIXTURE_BAND_ID_OFFSET
	return band

static func band_fixture() -> Dictionary:
	return with_band_id({
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
		#   expedition_forecast_horizon_turns — the SCALE the "never completed" sentinels are relative
		#     to, so an unbounded raid can be quoted a floor instead of "many turns".
		"expedition_forecast_horizon_turns": FORECAST_HORIZON_TURNS,
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
	})

## A scouting expedition (docs/plan_exploration_and_sites.md §2) in its awaiting-orders phase:
## a detached party (is_expedition) carrying a mission/phase + party size + provisions. The drawer
## renders the dedicated expedition readout + Recall/Move panel, not the labor-allocation UI.
static func expedition_fixture() -> Dictionary:
	return with_band_id({
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
	})

## Distance-aware herd-hunt (docs/plan_exploration_and_sites.md §2b): two player bands at DIFFERENT
## distances from ONE herd — a NEAR band ON the herd tile (within hunt_reach → LOCAL hunt) and a FAR
## band ~27 tiles away (beyond reach → hunting EXPEDITION). Proves the SELECTED band (band-picker)
## drives the local-vs-expedition label + command + band-entity target — the case single-band
## playtest can't surface. Both carry idle workers + a party cap so either verb is dialable.
static func hunt_distance_bands() -> Array:
	return [
		with_band_id({"entity": 811, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "forage", "labor_assignments": []}),
		with_band_id({"entity": 812, "faction": 0, "size": 80, "current_x": 86, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "hunt", "labor_assignments": []}),
	]

## Range-aware forage: two player bands at DIFFERENT distances from the (66,10) food tile — a NEAR band
## 1 tile away (within work_range 2 → forage ENABLED) and a FAR band ~21 tiles away (beyond range →
## forage DISABLED + out-of-range hint). Foraging is stationary gathering, so out-of-range has NO
## expedition fallback — just a disabled button. Proves the SELECTED band (band-picker) drives the
## enabled-vs-disabled state — the case single-band playtest can't surface.
static func forage_range_bands() -> Array:
	return [
		with_band_id({"entity": 821, "faction": 0, "size": 120, "current_x": 67, "current_y": 10,
			# **THE IDLE COUNT HAS TO CLEAR THE DIPPED BUILD CREW.** `improvement_build_crew` asserts the
			# stepper reaches the sim's own `workers_needed` (12 since the dip moved onto the crew), and
			# the stepper caps at `idle + already staffed` — so 10 idle pinned it one short and the frame
			# would have failed on the labour bound rather than on the thing it is testing.
			"working_age": 20, "idle_workers": 16, "work_range": 2, "activity": "forage", "labor_assignments": []}),
		with_band_id({"entity": 822, "faction": 0, "size": 80, "current_x": 80, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "work_range": 2, "activity": "forage", "labor_assignments": []}),
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
	return with_band_id({
		"id": "Band 1", "entity": 832, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 14, "idle_workers": 10,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 0.9,
		"activity": "hunt", "labor_assignments": [],
	})

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

## **THE PEN'S TIER, AND IT IS NOT THE SLED'S.** A sled drags a carcass in off the range and a pen
## stands at the camp, so a kit carrying only a sled collects a pen at the bare rate. The equipped
## side is `labor_config.hunt.per_worker_biomass_capacity` (the number a pen harvest has always been
## capped by); the bare side is `equipment.json`'s `husbandry_gear` declaration.
##
## **NO ENTRY OF `kit_roster_fixture()` EQUIPS IT**, deliberately — the shared roster carries no
## `husbandry` kit, so adding one would change what every hunt picker in both harnesses lists. The
## equipped tier is here for the chapters that build their OWN roster to exercise the axis.
const KIT_PEN_CARRY_EQUIPPED := 40.0

const KIT_PEN_CARRY_BARE := 12.0

## **WHAT A POSTED SCOUT VANTAGE CAN MAKE OUT** — `labor_config.scout.vantage_range` equipped, the
## `wayfinding` item's own declaration bare. How far out the vantage is POSTED is not a kit axis at
## all (that is three separate `labor_config` dials), so nothing here states it.
const KIT_SCOUT_VANTAGE_EQUIPPED := 2.0

const KIT_SCOUT_VANTAGE_BARE := 1.0

# The three conditions a kitted band ships with. **DELIBERATELY THREE DIFFERENT NUMBERS** on the
# 0-100 scale: a fixture that gave two kits one value would pass every assertion with their accessors
# swapped, which is the exact defect class this arc keeps reproducing.
## Traps ride the trapping kit; a fixture that omitted them would render a band the
## server never publishes, since the condition list is driven by the config's item table.
const KIT_CONDITION_TRAPS := 68.0
const KIT_CONDITION_SPEARS := 87.0

const KIT_CONDITION_SLED := 54.0

const KIT_CONDITION_BASKETS := 31.0

## The expanded roster's three, on the same 0-100 scale and — for the same reason as the four above —
## three more DISTINCT numbers, none of them equal to each other or to spears/sled/baskets/traps. The
## gear popover states one row per item, so two items sharing a condition would pass every assertion
## with their rows swapped.
const KIT_CONDITION_HUSBANDRY_GEAR := 45.0

const KIT_CONDITION_WAYFINDING := 66.0

const KIT_CONDITION_CLUBS := 22.0

## **THE ITEM IDS** (`equipment.json` `items`) — named because they appear TWICE in every band fixture:
## once in the roster entry's `item_ids` (which kit carries what) and once in the band's condition rows
## (how worn the band's own copy is). A literal in both places is exactly how a hint comes to quote the
## condition of gear its kit does not carry.
const KIT_ITEM_SPEARS := "spears"
const KIT_ITEM_SLED := "sled"
const KIT_ITEM_BASKETS := "baskets"
const KIT_ITEM_TRAPS := "traps"
## The expanded roster's three. They are named for the same reason as the four above and they carry
## one more job: the band-wide role CARDS read their gear line off `KitOption.item_ids`, so these
## appear in the wayfinding and warrior roster entries as well as in the condition rows.
const KIT_ITEM_HUSBANDRY_GEAR := "husbandry_gear"
const KIT_ITEM_WAYFINDING := "wayfinding"
const KIT_ITEM_CLUBS := "clubs"

## **THE BAND'S HUNT HEAD COUNT** (issue #520) — the denominator `hunt_crews` sums to and the number
## every coverage reading on these fixtures is quoted out of. Named because it appears in the crew
## rows, in each item's `workers_holding` and in the sentences the harness asserts by equality.
const KIT_HUNT_HEADCOUNT := 17.0

## **THE PARTLY-EQUIPPED SPLIT.** Four spears among seventeen hunters, so four clear a defence the
## other thirteen cannot touch at any headcount. The two DO sum to the head count above — a fixture
## whose parts did not would make the apportionment's whole reason unobservable — and they are
## deliberately unlike each other, so a readout quoting the wrong side reads as a different number.
##
## **THE ARMED RUN IS DELIBERATELY SMALLER THAN A COMPOSE SHEET'S PARTY.** A hunt sheet's split line
## is about the party being SENT, so a party that fits inside the armed run has no split to state —
## and the harness bands here can field ~6–10 hunters. An armed run above that would make every
## compose frame silent and the positive claim unreachable, which is the shape this arc shipped with
## first. The BAND-level readouts (the Gear row, the popover) quote `4 of 17` off the same rows.
const KIT_SHORT_SPEARS_ARMED := 4.0
const KIT_SHORT_SPEARS_BARE := KIT_HUNT_HEADCOUNT - KIT_SHORT_SPEARS_ARMED

## **THE HEAD COUNT OF THE JOB EACH NON-HUNT ITEM IS QUOTED AT.** They are three DIFFERENT numbers,
## and none of them is the hunt's: the wire quotes every row at its own job, so a fixture that gave
## the four jobs one denominator would pass a client that read the hunt's head count for all of them —
## which is exactly the private path this arc removed.
const KIT_FORAGE_HEADCOUNT := 4.0
const KIT_SCOUT_HEADCOUNT := 2.0
const KIT_WARRIOR_HEADCOUNT := 3.0

## **NOBODY IS STAFFED ON THE JOB, which is NOT a shortfall.** `0 of 0` — the band assigned no keepers,
## so no handling gear was needed and none went unheld, and it must render quietly. Named rather than
## inlined because it is a rendering CONTRACT, not an incidental zero.
const KIT_UNSTAFFED_HEADCOUNT := 0.0

## One `KitItemCondition` row. **NEITHER WORKER FIGURE IS DERIVABLE FROM `remaining`** — the condition
## says how much life is left in the item, `workers_holding` how many people it reaches and
## `workers_on_quoted_job` how many are on the job that wanted it. All three are stated outright,
## because a fixture letting one imply another passes a client that reads the wrong one.
static func kit_condition_row(item_id: String, remaining: float, workers_holding: float,
		workers_on_quoted_job: float) -> Dictionary:
	return {"item_id": item_id, "remaining": remaining, "workers_holding": workers_holding,
		"workers_on_quoted_job": workers_on_quoted_job}

## The seven item rows a kitted band publishes — **each quoted at ITS OWN job's head count**, which is
## the shape the wire really has: spears and the sled at the hunt, baskets at the forage default,
## wayfinding at the scout's, clubs at the warrior's. Everything reaches its whole job unless a caller
## says otherwise.
##
## **THE HANDLING GEAR'S JOB IS UNSTAFFED**, and it is the fixture's quiet-zero case: this band keeps
## no pen, so its keeper head count is `0` and a live item reaching nobody must render as nothing at
## all rather than as `0 of 0`. It is deliberately the item with a healthy condition, so a reader
## that mistook the quiet zero for a shortfall would light up a perfectly good piece of gear.
static func kit_condition_rows(spears_holding: float = KIT_HUNT_HEADCOUNT,
		baskets_holding: float = KIT_FORAGE_HEADCOUNT) -> Array:
	return [
		kit_condition_row(KIT_ITEM_SPEARS, KIT_CONDITION_SPEARS, spears_holding,
			KIT_HUNT_HEADCOUNT),
		kit_condition_row(KIT_ITEM_SLED, KIT_CONDITION_SLED, KIT_HUNT_HEADCOUNT,
			KIT_HUNT_HEADCOUNT),
		kit_condition_row(KIT_ITEM_BASKETS, KIT_CONDITION_BASKETS, baskets_holding,
			KIT_FORAGE_HEADCOUNT),
		kit_condition_row(KIT_ITEM_TRAPS, KIT_CONDITION_TRAPS, KIT_HUNT_HEADCOUNT,
			KIT_HUNT_HEADCOUNT),
		kit_condition_row(KIT_ITEM_HUSBANDRY_GEAR, KIT_CONDITION_HUSBANDRY_GEAR,
			KIT_UNSTAFFED_HEADCOUNT, KIT_UNSTAFFED_HEADCOUNT),
		kit_condition_row(KIT_ITEM_WAYFINDING, KIT_CONDITION_WAYFINDING, KIT_SCOUT_HEADCOUNT,
			KIT_SCOUT_HEADCOUNT),
		kit_condition_row(KIT_ITEM_CLUBS, KIT_CONDITION_CLUBS, KIT_WARRIOR_HEADCOUNT,
			KIT_WARRIOR_HEADCOUNT),
	]

## **A UNIFORMLY-EQUIPPED BAND PUBLISHES EXACTLY ONE CREW, NEVER AN EMPTY LIST.** The sim's own rule,
## and the reason no client reader needs a "no crews" branch — so a fixture that omitted the field
## would be a band no server can produce, and would leave every uniform case untested.
static func hunt_crews_uniform(attack: float, item_ids: Array) -> Array:
	return [{"workers": KIT_HUNT_HEADCOUNT, "hunter_attack": attack, "item_ids": item_ids}]

# ---- THE KIT ROSTER (`docs/plan_denial_raid.md`, `SubsistenceSection.kits`) -----------------------
# The ids the wire carries and the two job defaults. Named because the `kit <id>` COMMAND token is
# asserted against them and because "which id is the default" is half of what the picker's frames
# claim — a literal in two harnesses is how those two claims come apart.
const KIT_ID_BIG_GAME := "big_game"
## **A SHIPPED KIT THAT `kit_roster_fixture()` DELIBERATELY DOES NOT OFFER.** The trapping kit exists in
## `equipment.json`, and the traps' own condition is on every kitted band here — but adding a fourth
## picker entry would move every rendered kit-picker frame in both harnesses for a claim only one
## chapter makes, so that chapter stages it on a roster of its own. The id lives here because the BAND's
## `kit_tiers` answer sheet states a row for it, and one spelling is what keeps the two in step.
const KIT_ID_TRAPPING := "trapping"
const KIT_ID_GATHERING := "gathering"
const KIT_ID_NONE := "none"
## The two BAND-WIDE roles have a kit axis now — they had none while nothing in the roster was gear
## for them, and `LaborAssignment.kitId` published `""` on those rows. Each names its own default,
## exactly as `equipment.json`'s `default_kits` does.
const KIT_ID_WAYFINDING := "wayfinding"
const KIT_ID_WARRIOR := "warrior"
const KIT_DEFAULT_HUNT := KIT_ID_BIG_GAME
const KIT_DEFAULT_FORAGE := KIT_ID_GATHERING
const KIT_DEFAULT_SCOUT := KIT_ID_WAYFINDING
const KIT_DEFAULT_WARRIOR := KIT_ID_WARRIOR

## The `clubs` tier the warrior kit grants — well under the spear's 20, because a raid is people
## fighting animals at the camp with whatever is by the fire rather than a hunting party that chose
## its ground. It is the same `attack` stat the hunt reads; what keeps a club out of a hunt is the
## kit's `jobs` list, which is why this value can sit in the same roster without disturbing it.
const KIT_ATTACK_CLUBS := 6.0

## The world's kit roster, in `equipment.json` order — the picker's list, and the ONE roster both
## preview harnesses drive (`band_panel_preview` preloads this module for it, so the two cannot quote
## different tiers or a different default).
##
## **EVERY ENTRY STATES ALL FIVE TIERS, and the ones its kit does not use are the BARE ones.** That
## is the wire's own shape and it is what `KitRoster.unequipped_tier` reads the bare-handed tier off:
## the minimum across the roster on an axis IS that axis's unequipped tier, so a fixture that left an
## unused axis at its equipped value would make the client's step-down silently unreachable. The
## MAXIMUM is the twin claim `KitRoster.equipped_tier` reads — the rate every source row is published
## at — so an axis no entry equips reads bare at both ends, which is the honest answer for a roster
## with no kit supplying it rather than a hole.
##
## **THE TWO BAND-WIDE ROLES ARE IN THE ROSTER AND CHANGE NO EXISTING PICKER**: `wayfinding` lists
## `scout` and `warrior` lists `warrior`, so `kits_for_job` filters both out of every hunt and forage
## sheet. What they are here for is the AXES — the roster is what the bare-handed vantage tier is read
## off, and a roster missing them describes a world the sim does not ship.
##
## **`none` IS AN ORDINARY MEMBER AND IT IS AUTHORED LAST**, exactly as `equipment.json` authors it —
## which is the whole of why the picker renders it last. The client sorts nothing.
##
## **EVERY ENTRY STATES ITS `item_ids`**, the wire's own `uses` list, in config order — weapon first,
## haul aid after. `none` states an EMPTY one, which is a real answer rather than a missing field: it
## carries nothing, so the hint prints no condition clause for it.
static func kit_roster_fixture() -> Array:
	return [
		{
			"id": KIT_ID_BIG_GAME, "display_name": "Stalking kit", "jobs": ["hunt"],
			"attack": KIT_ATTACK_EQUIPPED,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_EQUIPPED,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE,
			"item_ids": [KIT_ITEM_SPEARS, KIT_ITEM_SLED],
		},
		{
			"id": KIT_ID_GATHERING, "display_name": "Gathering kit", "jobs": ["forage"],
			"attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_EQUIPPED,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE,
			"item_ids": [KIT_ITEM_BASKETS],
		},
		{
			"id": KIT_ID_WAYFINDING, "display_name": "Wayfinding kit", "jobs": ["scout"],
			"attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_EQUIPPED,
			"item_ids": [KIT_ITEM_WAYFINDING],
		},
		{
			"id": KIT_ID_WARRIOR, "display_name": "Warrior kit", "jobs": ["warrior"],
			"attack": KIT_ATTACK_CLUBS,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE,
			"item_ids": [KIT_ITEM_CLUBS],
		},
		{
			"id": KIT_ID_NONE, "display_name": "No kit",
			"jobs": ["hunt", "forage", "scout", "warrior"],
			"attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE,
			"item_ids": [],
		},
	]

## **THE BAND'S OWN RESOLVED TIER ROWS** (`PopulationCohortState.kitTiers`) — what EVERY offered kit
## would grant THIS band at its live wear, one row per kit id, authored the way the sim resolves them.
##
## **THEY ARE AUTHORED, NEVER DERIVED HERE FROM THE CONDITIONS BESIDE THEM.** The whole reason the field
## exists is that the step-down needs the per-kit axis→item mapping, which the wire does not carry and no
## client-side rule recovers; a fixture that re-derived the rows would be writing exactly the guess the
## field replaced, and would agree with a client that had put the guess back.
##
## **A ROW STATES ALL FIVE AXES `BandKitTiers` CARRIES**, the pen and the vantage included. They were
## absent here while the wire's table was, and a row that omits an axis exercises a fall-back rather
## than the real path: the client used to answer those two off the ROSTER's fresh tier, so a dry
## `husbandry_gear` band read `pen 40.0 per keeper` against a sim collecting 12. Stating them is what
## makes a worn fixture prove the step-down instead of hiding it.
##
## **EACH OF THE TWO IS SUPPLIED BY ONE ROSTER KIT, WHICH IS WHY ONLY THE VANTAGE TAKES AN ARGUMENT.**
## The wayfinding gear equips the vantage, so the `wayfinding` row moves with that item's condition and
## every other row reads the bare tier — a scout's reach is not a thing a sled or a basket can buy. The
## PEN is bare on every row here because **no kit `kit_roster_fixture()` offers equips it**: the
## handling gear rides the `husbandry` kit, which that roster does not carry (the one chapter that
## needs it builds its own roster and its own row). A table that let the hunt carry stand in for the
## pen would agree with a client that had put the roster fall-back back — a sled drags a carcass in off
## the range and a pen stands at the camp.
##
## `KIT_ID_TRAPPING` gets a row although `kit_roster_fixture()` does not offer it: the roster is the
## PICKER's list, this is the BAND's answer sheet, and the trapping kit is a shipped kit that one chapter
## stages against a roster of its own. A row for a kit no roster offers is never looked up.
##
## **THE WARRIOR'S ATTACK IS ITS OWN ARGUMENT, and it is not the hunter's.** One roster kit resolves
## `attack` off `clubs` rather than `spears`, so a table that reused the hunt tier would quote the camp's
## defenders a spear's 20 — the exact mis-pairing the per-kit rows exist to make impossible.
static func kit_tiers_rows(attack: float, hunt_carry: float, forage_carry: float,
		warrior_attack: float, scout_vantage: float) -> Array:
	return [
		{"kit_id": KIT_ID_BIG_GAME, "attack": attack,
			"hunt_carry_per_worker_biomass": hunt_carry,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE},
		{"kit_id": KIT_ID_TRAPPING, "attack": attack,
			"hunt_carry_per_worker_biomass": hunt_carry,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE},
		{"kit_id": KIT_ID_GATHERING, "attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": forage_carry,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE},
		{"kit_id": KIT_ID_WAYFINDING, "attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": scout_vantage},
		{"kit_id": KIT_ID_WARRIOR, "attack": warrior_attack,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE},
		{"kit_id": KIT_ID_NONE, "attack": KIT_ATTACK_BARE,
			"hunt_carry_per_worker_biomass": KIT_HUNT_CARRY_BARE,
			"forage_carry_per_worker_biomass": KIT_FORAGE_CARRY_BARE,
			"pen_carry_per_worker_biomass": KIT_PEN_CARRY_BARE,
			"scout_vantage_range": KIT_SCOUT_VANTAGE_BARE},
	]

## A band carrying EVERY item the roster ships, each at its own condition and each role at the tier
## this band's own job defaults resolve to.
##
## **THE PEN TIER IS THE BARE ONE, AND THAT IS THE FIXTURE BEING HONEST.** `kit_roster_fixture()`
## carries no husbandry kit, so the HUNT default (`big_game`) supplies no `husbandry_gear` and a
## keeper collects at 12 however healthy the item is — which is also what makes the pen row
## assertable against the sled's 40 rather than agreeing with it by construction. The per-kit rows
## beside it say the same thing kit by kit, which is what a picker reads.
static func with_equipped_kit(band: Dictionary) -> Dictionary:
	band["kit_item_conditions"] = kit_condition_rows()
	# The uniform case, and it is the NORMAL one — every hunter on the same gear, so the band publishes
	# ONE crew and no readout has a split to state. It is what makes the partly-armed fixture below a
	# contrast rather than the only shape the crew field is ever seen in.
	band["hunt_crews"] = hunt_crews_uniform(KIT_ATTACK_EQUIPPED,
		[KIT_ITEM_SPEARS, KIT_ITEM_SLED])
	band[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = KIT_ID_BIG_GAME
	band["kit_tiers"] = kit_tiers_rows(KIT_ATTACK_EQUIPPED, KIT_HUNT_CARRY_EQUIPPED,
		KIT_FORAGE_CARRY_EQUIPPED, KIT_ATTACK_CLUBS, KIT_SCOUT_VANTAGE_EQUIPPED)
	band["hunter_attack"] = KIT_ATTACK_EQUIPPED
	band["hunt_carry_per_worker_biomass"] = KIT_HUNT_CARRY_EQUIPPED
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_EQUIPPED
	band["pen_carry_per_worker_biomass"] = KIT_PEN_CARRY_BARE
	band["scout_vantage_range"] = KIT_SCOUT_VANTAGE_EQUIPPED
	band["warrior_attack"] = KIT_ATTACK_CLUBS
	return band

## **ONE KIT DRY, THE OTHER TWO INTACT** — the state that proves the three wear independently. The
## baskets have run out, so the FORAGE carry has stepped down to bare hands and the hunt's has not:
## a band that has gathered its baskets to pieces still drags carcasses home on an untouched sled.
## This is the frame a readout rendering one carry on the other's row fails.
static func with_baskets_dry(band: Dictionary) -> Dictionary:
	band = with_equipped_kit(band)
	# The baskets have run out — a DRY item is one the band owns none of, so nobody is holding one —
	# **and the forage job is still STAFFED**, which is what makes the loss sayable: four gatherers are
	# working with their hands. The dry face and the `0 of 4` are different facts about one item (the
	# tier stepped down; four people feel it), so the row states both.
	band["kit_item_conditions"] = kit_condition_rows(KIT_HUNT_HEADCOUNT, 0.0)
	band["kit_item_conditions"][2]["remaining"] = 0.0
	# The gathering kit's own row steps down and the two hunt kits' do not — the whole claim of this
	# fixture, stated where the client now reads it rather than left to be inferred from the conditions.
	# The WAYFINDING gear is untouched here, so the scout's reach is unmoved too: a band that has worn
	# its baskets out can still see as far as it ever could.
	band["kit_tiers"] = kit_tiers_rows(KIT_ATTACK_EQUIPPED, KIT_HUNT_CARRY_EQUIPPED,
		KIT_FORAGE_CARRY_BARE, KIT_ATTACK_CLUBS, KIT_SCOUT_VANTAGE_EQUIPPED)
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_BARE
	return band

## A band that has run EVERY kit dry — the bare-hands state, which is permanent: there is no
## replenishment path, so every role has stepped down and stays there. Its `hunter_attack` of 1 is
## what the combat gate refuses megafauna on.
static func with_bare_hands(band: Dictionary) -> Dictionary:
	# Every item spent and nobody holding anything — **but each job keeps its own head count**, which is
	# the whole reading: these are staffed people working bare-handed, not empty rosters. The keeper
	# row stays quietly at `0 of 0`, so even here the unstaffed job says nothing.
	var rows: Array = []
	for row in kit_condition_rows():
		rows.append(kit_condition_row(String(row["item_id"]), 0.0, 0.0,
			float(row["workers_on_quoted_job"])))
	band["kit_item_conditions"] = rows
	# **A WHOLLY BARE PARTY IS ONE CREW HOLDING NOTHING**, not an absent one — the sim's own rule, so
	# nothing here reads as a split and the gate's plain refusal is the whole of what renders.
	band["hunt_crews"] = hunt_crews_uniform(KIT_ATTACK_BARE, [])
	band[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = KIT_ID_BIG_GAME
	band["kit_tiers"] = kit_tiers_rows(KIT_ATTACK_BARE, KIT_HUNT_CARRY_BARE, KIT_FORAGE_CARRY_BARE,
		KIT_ATTACK_BARE, KIT_SCOUT_VANTAGE_BARE)
	band["hunter_attack"] = KIT_ATTACK_BARE
	band["hunt_carry_per_worker_biomass"] = KIT_HUNT_CARRY_BARE
	band["forage_carry_per_worker_biomass"] = KIT_FORAGE_CARRY_BARE
	band["pen_carry_per_worker_biomass"] = KIT_PEN_CARRY_BARE
	band["scout_vantage_range"] = KIT_SCOUT_VANTAGE_BARE
	# A camp with nothing left fights a raid with hands, i.e. the SAME creature `attack` a bare-handed
	# hunter has — one number, reached from two roles, and the row must still name which fight it is.
	band["warrior_attack"] = KIT_ATTACK_BARE
	return band


## **TEN SPEARS AMONG SEVENTEEN HUNTERS** (issue #520) — the band whose gear works perfectly and does
## not go round. Every kit is live and none is worn out, so it is INDISTINGUISHABLE from
## `with_equipped_kit` on every readout that reads a condition: the only thing that separates them is
## how far each item REACHES, which is the whole of what this arc put on the wire.
##
## **TWO CREWS, and the second holds the SLED.** A sled goes round and the spears do not, so the bare
## run is not empty-handed — it is under-equipped, which is the clause the split line has to pick over
## "bare-handed" and which a fixture with an empty second crew could not test. It also keeps the two
## crews holding DIFFERENT sets, so a reader that took the first crew's items for the party's fails.
##
## `hunter_attack` stays the EQUIPPED tier: the sim reads it off the best-equipped crew, so a band-level
## reading that spoke for everybody is exactly the reassuring half this fixture exists to catch.
## **BASKETS SHORT OF GATHERERS — the shortfall on a job that is not the hunt.** Two baskets among four
## gatherers, so two gather at the equipped rate and two with their hands.
##
## **IT IS THE FRAME THE FOUR-JOB DENOMINATOR EXISTS FOR.** While `Σ huntCrews.workers` was the only
## job head count on the wire this band was unreadable: its baskets are live and at full condition, its
## hunt is perfectly equipped, and every readout in the client called it fully geared. Nothing about the
## HUNT moves here, deliberately — a client that had quietly kept the hunt's head count as everybody's
## denominator states this band's spears as short and its baskets as fine, i.e. exactly backwards.
const KIT_SHORT_BASKETS_HOLDING := 2.0

static func with_short_baskets(band: Dictionary) -> Dictionary:
	band = with_equipped_kit(band)
	band["kit_item_conditions"] = kit_condition_rows(KIT_HUNT_HEADCOUNT,
		KIT_SHORT_BASKETS_HOLDING)
	return band

static func with_short_spears(band: Dictionary) -> Dictionary:
	band = with_equipped_kit(band)
	band["kit_item_conditions"] = kit_condition_rows(KIT_SHORT_SPEARS_ARMED)
	band["hunt_crews"] = [
		{"workers": KIT_SHORT_SPEARS_ARMED, "hunter_attack": KIT_ATTACK_EQUIPPED,
			"item_ids": [KIT_ITEM_SPEARS, KIT_ITEM_SLED]},
		{"workers": KIT_SHORT_SPEARS_BARE, "hunter_attack": KIT_ATTACK_BARE,
			"item_ids": [KIT_ITEM_SLED]},
	]
	return band
