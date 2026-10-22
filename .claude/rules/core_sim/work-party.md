---
paths:
  - "core_sim/src/work_party.rs"
  - "core_sim/src/staffing.rs"
  - "core_sim/src/take_claims.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/supply.rs"
  - "core_sim/src/labor_config.rs"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/src/demographics_config.rs"
  - "core_sim/src/forecast_query.rs"
  - "core_sim/tests/labor_allocation.rs"
  - "core_sim/tests/work_party_caravan.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/tests/migration_mode.rs"
  - "core_sim/src/hunt_by_need.rs"
  - "core_sim/tests/hunt_by_need.rs"
  - "core_sim/src/roaming_skills.rs"
  - "core_sim/tests/roaming_skills.rs"
---

# The work party — hunt, forage and extract are one model, and distance is a caravan

Design: `docs/plan_civilization_steps.md` §One work party. Engine: `core_sim/src/work_party.rs` (the
caravan's state and per-turn rule, the walk, the forecast, the pricing) plus the posting seam in
`core_sim/src/systems/labor.rs`, the assign-time seed in `bin/server.rs` and the compose-sheet query
in `forecast_query.rs`. **A deposit (wood, stone) posts a party by exactly the rule a patch does** —
see "The deposit web: the cargo is the material".

**A tile improvement's crew (`LaborTarget::Tile`, `intensification.md` -> "The tile kind") is the
fourth row kind and posts a party by the same rule**: `party_source_position` names its tile, a far
crew is a party, and the party carries no cargo (the row takes nothing; it only keeps). Nothing forks
for it.

## The sim staffs every source from the player's floor (#871)

Design: `docs/plan_labor_at_scale.md` ("Decided"). Engine: `core_sim/src/staffing.rs`, applied at the
head of each band's labor pass in `systems/labor.rs`, restated by the command path through
`staffing::restaff_band`.

**A forage, hunt (not a need row), extraction, tile-improvement or Roadwork row's `workers` is the
sim's answer**, not the player's. The player states a **Max** (`LaborAssignment::max_workers`;
`None` = no Max) and the **floor** the row already carries; each turn the sim zeroes those rows'
hands and refills them. `LaborTarget::is_staffed` names the kinds. Scout, Warrior, Builders and a
need row keep the player's count.

**The pool** is `staffable_pool`: the working people less the bench, the hands still walking home and
every crew the player set by hand (the rows that are not staffed). Rows are grouped by
`SourcePriority`, High to Low.

**Within a priority group every pass deals ONE head at a time, round-robin**, across the group's rows
that still want a head in that pass, until no row in the group wants one, the pool is empty or (for a
kitted head) the shelf cannot arm one more; only then does the next lower group get that pass. Pass 1
runs over every group before pass 2 starts, and pass 2 before pass 3, so kitted heads still come
before bare ones across priorities. A round deals **nearest row to camp first** (hex distance from
the band's tile to the row's site, a hunt row's herd at its current tile, `StaffRowInput::distance`;
Roadwork has no single site and reads 0), ties to the earlier row; distance orders a round and never
excludes or demotes a row, so a far row is dealt every round just after the nearer ones. Two
equal-priority bare rows with 4 heads staff 2 / 2, and with 3 heads the nearer row gets the odd one.
Each step re-reads the row's want against the live shelf and armed count, so a row stops taking once
one more head would pass its target; keepers are dealt round-robin before that group's kitted hold
heads, and pass 3 deals the group's unspent-kit heads before its bare heads.

**Sim-staffed crews never block a player-set order.** They are what is left after the player's own
orders, so every ceiling a player-set crew clamps against counts them as available:
`BandWorkforce::benchable` (the bench crew and `bench_enqueue`) is `pool − (assigned − sim_staffed) −
walking_home`, and `LaborAllocation::set_assignment` nets only the other **player-set** rows (Scout,
Warrior, Builders, a need row) out of the headroom. `assignable` (the pool less the bench and the
walkers) and the published `idleWorkers` (what pass 3 left over) keep their meaning. `bench_crew`,
`bench_enqueue` and every `assign_labor` restaff the band, so the sources give up the hands at once.
The party verbs (`send_scout`, `send_expedition`, `send_denial_raid`, `station_expedition`,
`send_trade_expedition`) draw from `BandWorkforce::partyable` (pool less the player-set crews, the
bench and the walkers) and restaff the band after the launch. A hunting-by-need muster is unchanged.

**Three passes**, each capped by the row's Max:

| pass | what it hands out |
|---|---|
| 1 | the site's **keepers** (`planned_keep_hands`, ceiled), then **kitted** hands up to the row's **hold crew** |
| 2 | **kitted** hands up to the row's **reach crew** |
| 3 | every row up to its **useful crew** (the room above the floor, clear-it-now; the crew past which the take stops rising): the kit's units still unspent first, then **bare** hands, only on a web where a bare hand takes something (never on a hunt that needs a weapon) |

**A hunt row's three targets (hold, reach, useful) are sized by `fauna::hunt_crew_for_take`**, the
one crew function `workers_needed` also reads: haul, reach and fight against a take RATE, capped at
what the take can reach (`room x stay`), never against a whole animal felled in one turn
(`fauna.md` -> "A hunt crew is sized on a RATE"). A Wild Aurochs herd regrowing a third of an animal
a turn holds with four hunters, where the whole-animal sizing named eleven and starved the forage and
stone rows of hands.

A kitted hand holds the whole of its row's live kit items, drawn from one shelf per item shared by
every row (so two rows cannot arm one spear twice). **Hold crew** is the hands that take the
regrowth at the floor; **reach crew** is the hands whose take is strictly above the peak regrowth
between the floor and today's stock (`REACH_STRICTLY_ABOVE_PEAK`), so they draw the stock down
toward the floor. Stone has no regrowth and takes `ReachTarget::Heads(Max - keepers)`. A source
already at or under its floor staffs its keepers only. Each web builds its own take-per-hand
(`forage_input`, `hunt_input`, `extract_input`; a tile improvement and Roadwork size by their
keeping bill alone) and at least one hand is asked of a take that rounds to nothing
(`AT_LEAST_ONE_HAND`).

**What is written on the row** is `workers` plus `StaffedCrew { keepers, kitted, bare, hold_crew,
reach_crew, useful_crew }` (`staffing`), applied by `LaborAllocation::apply_staffing(staffed, lost_people)`. A far
row that loses hands hands them to its party (`cut_crew` / `walk_home`); hands lost to people the band
no longer has are discarded where they stand (`discard_lost_hands`), not walked home, so the loss is
not counted against next turn's pool and cut again.

**Kit claims follow the staffing.** `take_claims::row_claims` settles a staffed row on exactly the
hands the passes armed (plus the keeping remainder: the leftover fraction of a keeper the bill asks
for), so the band's item budget grants a staffed row its `kitted` units and no more. `RowClaims::asks`
keeps the planned claim (what the row would ask for if armed as planned) beside the granted
`claims`; a row's published `kitToe` states `required` from the ask and `filled` from the grant, so a
shortfall the staffing left stays visible by item.

**Commands.** For a staffed kind the count on `assign_labor` is the **Max** (`0` = no Max, never an
unassign); `stop_labor <faction> <band> <source>` ends the row
(`x y` | `herd_id` | `x y material` | `branch x y` | `roadwork`, `StopLaborCommand`, proto field 95).
Both restaff the band immediately (`staffing::restaff_band`, then the assign-time seed), so the row
reads its staffed crew before the next turn.

**Wire** (`LaborAssignment`, appended): `maxWorkers` (`0` = no Max), `holdCrew`, `reachCrew`,
`usefulCrew` (pass 3's ceiling, keepers included, after the Max), `keepersStaffed`, `kittedStaffed`, `bareStaffed` (the last three sum to `workers` on a staffed row).
`SAVE_FORMAT_VERSION` 49.

**Approximations to know.** The command-time restaff reads the world before the coming turn's
regrowth, while the turn staffs after it, so a row's crew can move by a hand between the seed and the
turn. Several bands working one source each staff their share of the keeping bill. A band's gear that
the turn settles after staffing (the pools' and the keeping's issue) is not in the staffing's shelf.

Pinned by `staffing::tests` (the passes, in order, against a shelf), `labor_allocation` (the
re-staff on a smaller pool and the turn's staffed rows), `shedding_order::a_smaller_pool_restaffs_the_rows_and_sheds_nothing`,
`take_kit_claims` (granted claims against asks) and `server::tests` (`stop_labor`, the Max on
`assign_labor`).

## Config files

| File | Purpose |
|------|---------|
| `src/data/labor_config.json` | `band_work_range` (**2**) is the apron: past it every job posts a party, and the walk is measured from it. `band_move_tiles_per_turn` (**1**) is read a second time as the party's walking speed — the walk out and every porter's walk home — and is validated `>= 1` for that reason. **No lever of this arc's own exists**: the share of a party on the road falls out of carry, take rate and distance |
| `src/data/supply_network_config.json` | Read, not written: `reach_tiles` is subtracted from `supply::free_pooling_reach_tiles` to give the **road bonus** — how much of a walk a road takes away. `friction` is **not** read: distance is paid in walking |
| `src/data/materials.json` | Read, not written: every material's required **`weight`** — biomass-equivalent mass per unit, validated positive and finite — is what a deposit's pack is divided by (wood **2.4**, stone **3.0**; every value PROVISIONAL, stated in the file's `_comment_weight`) |
| `src/data/equipment.json` | Read, not written: the extract kits `sledding` (`sled`), `woodcutting` (`sled` + `axe`) and `stonework` (`sled` + `wedges`), claimed whole like every job's kit, so a far working's porters haul on the sled on every rung |

## A party is state ON the labor assignment, not an entity

`LaborAssignment::party: Option<WorkParty>`. There is no second cohort, no `ResidentBand`-less
entity and no merge verb, because **the workers never stopped being the band's** — the whole
architectural difference from an expedition (`expeditions.md`). What falls out of it:

- **The row that staffed the party is the row that reports it**, so a work board has one place to
  look for every work item, near or far. The capture needs nothing handed in.
- **`None` is what every command constructs.** A party is posted by the *turn*, from geometry — it
  is never ordered, and there is no haul command, no chosen destination and no launch gate.
- **It is outside `LaborAssignment`'s equality** (a hand-written `PartialEq`): a rank is intent, a
  party is a fact about the world restamped every turn, and a derived `PartialEq` would have made the
  command no-op guard and the rollback record report *nothing changed* as a change every turn.
- **`set_assignment` carries the whole caravan across the re-push** — walk out, load and road —
  because a `−`/`+` press is not an order to teleport the walkers home or restart the walk out.

## ⛔ The local identity is the point

A source the band's own hands reach takes **no party at all**, and every number on that row is what
it was before any of this existed. Far work *falls out of* one model instead of sitting beside it
only while that holds.

**A work party IS its row — no near/far distinction beyond the physics of the walk** (carry, porters
on the road, the keeping paid only by the hands present). Every feature a local crew has, a party
has, **Priority included**: its take-kit claim is settled at its row's own rank
(`BandItemBudget`, `docs/plan_site_crews.md` §2.3), and every forecast that prices a party — the
caravan (`CaravanPricing`), the trip and denial sheets, the compose query — ranks it at that row's
Priority, or at the default a new row is given where no row exists yet. Pinned by
`kit_selection::a_high_party_beats_a_normal_local_row_for_a_scarce_kit`.

`work_party::party_begins_past` is the one place the threshold lives, and it is **`band_work_range`
for every job**. Hunt gets no longer apron than forage: a party that follows its herd never roams out
of range, so there is no hunt-only reach, on the wire or in config.

Pinned by `labor_allocation::a_local_row_takes_no_party_and_its_whole_take_reaches_the_larder`
(the published `actual` against the larder credit in **fixed point**, with a liveness assertion) and
`::a_hunt_just_past_the_apron_posts_a_party_on_the_same_apron_as_forage`, which stages a herd two
tiles past `band_work_range` — where a hunt-only threshold would have called it an ordinary local row.

> **The only threshold a caravan has is `band_work_range`.** A client that refuses a source past the
> work range, or routes a far herd anywhere but the ordinary `assign_labor … hunt` row, is exactly
> the defect the first playtest of this arc found: the work party was unreachable from the map.

## What distance costs — walking, and nothing else

**The party hunts as any hunt does. When the take fills one hunter's pack, that hunter walks it
home, delivers it, walks back and rejoins.** The others keep working meanwhile. So the share of the
party on the road *falls out* of carry, take rate and distance: with a per-hunter take rate `r`, one
pack `L` and a one-way walk of `w` turns, the steady fraction working is

```text
L / (L + 2·w·r)
```

A slow-filling hunt loses almost nobody to walking; a fast-filling one loses a lot; more hunters land
the first load sooner, because the first pack fills at the whole party's rate.

> ### ⛔ THIS REPLACED A PORTER FRACTION AND A FRICTION TERM, AND NEITHER MAY COME BACK
>
> The first cut charged distance as `porter_fraction_per_travel_tile` (**0.12**, a number somebody
> picked) of the party carrying, plus the supply network's `friction` compounded over the tiles
> walked. The porter share got the economics wrong on the axis that matters — it charged a boar hunt
> that fills a pack every thirteen turns the same share as one that fills a pack every turn — and the
> friction then charged the walk **a second time**: distance paid in walking already is the cost, and
> a loss in transit on top counts it twice. Both are deleted, and the lever with them. **Friction
> still governs band-to-band pooling in `balance_supply_networks`, untouched.** Meat going off on a
> long walk is a different cost — a property of the *food*, not a fraction of the walk — and it is
> the one below.

### A pack rots by its walk (#706)

**Every pack carries the keeping classes of the food in it, and a class whose shelf life is no longer
than the porter's walk is lost on the way — entirely, not as a share** (`spoilage::rots_in_transit`;
the classes and their shelf lives are `campaign.md` → "Food spoils by keeping class"). So the same
walk costs a meat hunt everything and a nut gather nothing: a boar is `flesh`, which keeps four
turns, so a herd eight hexes out (a six-turn walk) lands nothing it keeps, and one five hexes out
(three turns) loses nothing; a walk of exactly four turns is lost too, the same as a camp kill
of that age (the larder expires a lot at `age >= shelf`). That is the natural range a far hunt has, and a longer shelf life —
drying — is what extends it.

- **The composition rides the caravan beside its scalar cargo.** `WorkParty::load_classes` and each
  `Walker::classes` are cargo per class (`work_party::CargoClasses`), split off the load in the same
  proportion as the cargo every time a pack leaves; the caravan's arithmetic still runs on the scalar
  `cargo`. A take site hands its take's classes
  in with `WorkParty::close_turn_classed`; `open_turn` hands each landed pack back as a
  `LandedPack { walk_turns, cargo, classes }`. The deposit web's cargo is a material and carries no
  classes.
- **One spoilage term, booked at the landing.** `systems::labor`'s `land_food_home` is the one place a
  take's food enters the larder: the whole delivery is **credited as income** (by class), then every
  class of every pack whose walk exceeded that class's shelf life is **debited the same turn** and
  added to `PopulationCohort::last_food_spoiled`. Income therefore stays the row's `actual` (what
  *lands*), and the loss is the ledger identity's single `spoiled` term. A pack landed without a walk
  (a local row) never rots. **A pack that survives its walk lands aged by it**: the delivery is
  split across (class, walk) lots (`FoodMix::from_aged_weights`), so its shelf life counts from the
  kill and a walk-*W* pack expires *W* turns sooner than a camp kill.
- **The forecast strikes the same rot on every landing pack, so `netRateHome` is what arrives AND
  keeps.** `forecast_caravan` takes a `TransitRot` — the row's cargo shares by class (a herd's one
  class; a basket's `forage::patch_food_mix` shares) plus the keeping table — and each projected pack
  loses `TransitRot::rotten_share(walk)` of its cargo. `rate_home`, the arrival schedule and
  `realized` are therefore the larder's real gain, and the loss rides `spoiled_rate_home`; the
  shortest shelf life that rots on the walk is `transit_keeps_turns` (`0` when nothing does). The
  deposit web passes no rot (`NO_TRANSIT_ROT`). The turn, the seed and the query all step this one
  function, so the row and the compose-sheet quote state the same three numbers — the hunt panel can
  say *before the order is committed* that a far take will spoil.
- **A posting that ENDS rots by the same rule.** Its packs and load walk home
  (`WorkParty::walk_home`, below) and land through `bring_the_party_home`, which credits the route
  arm and then strikes every class whose shelf life is no longer than the walk the cargo was carried
  over — the porter's own walk, or the whole walk for the load the source hands carry.

Pinned by `work_party::tests::a_pack_carries_its_loads_classes_home_with_its_walk` (the composition
and the walk ride the pack), `systems::labor::transit_rot_tests` (the landing rule), and
`work_party_caravan::a_walk_longer_than_flesh_keeps_loses_the_pack_and_a_shorter_one_does_not` (a real
far hunt at both distances, each with a liveness assertion that food landed).

### No individual hunters

Nobody is simulated as a unit and nothing moves on the map. It is the codebase's ordinary pattern
for a fractional flow: **a running total that fires an event each time it crosses a whole unit** (the
load, crossing a pack), plus **a short queue of the hunters currently on the road**, bounded by the
party. Each walker carries its own walk length, fixed when it left: a herd that drifts further
changes the *next* porter's walk, never the one already on the road.

### The walk

```text
walk_tiles = max(0, hex_distance − band_work_range − road_bonus)
road_bonus = supply::free_pooling_reach_tiles(..) − supply_network_config.reach_tiles
walk_turns = ceil(walk_tiles / band_move_tiles_per_turn)
```

**Measured from the apron, never from `reach_tiles`.** An 8-hex source walks `8 − 2 = 6` each way, 12
for the round trip — Ray's `(hexes × 2) − (localRange × 2)`. The hunt he playtested was quoted 16 by
the old expedition sheet, which is the defect this formula replaces.

**A road shortens the walk through the seam two camps pool through**, so what a road does for a
caravan and what it does for pooling are one reading of one road — the weakest-tile rule included. A
road covering the whole run takes the walk to zero: every pack lands the turn it fills and nobody is
ever absent. That is the promotion path the design turns on. A road raising a porter's *carry* is not
modelled.

`work_party::resolve_walk` is the one resolver; the turn, the seed and the query all read it.

## The caravan, per turn — one rule, two halves

`WorkParty::open_turn` is steps 1–2 and `WorkParty::close_turn` steps 4–5; the turn runs them around
its own inline take, the forecast around a projected one (`forecast_caravan`). Order:

1. **Advance the road.** Each walker takes a step; one reaching home hands over its pack; one back
   from the whole round trip rejoins (absent for exactly `2 · w` takes).
2. **Walking out → no take.** A new party walks out once, `walk_turns` turns with nobody at the
   source, never re-raised.
3. **Keep, then take, with the hunters PRESENT** (`workers − on the road`) through the **ordinary
   take path** — every arm the resident take runs. There is no caravan-specific take formula. A kept
   site's keeping comes out of the hands present first and is capped at them
   (`SiteKeeping::at_the_source`, `docs/plan_site_crews.md`): a party still walking out keeps nothing.
   The forecast steps the same split (`work_party::take_hands_present`) off the keeping planned at
   the staffed crew, so a far kept site's `netRateHome` is what lands after keeping.
4. **The whole take goes into the load.** Nothing is eaten out of it at the source — see below.
5. **Fill and dispatch.** While the load holds one pack and a hunter is present, one hunter leaves
   with one pack. Departures therefore never exceed the hunters present.

### ⛔ The home band feeds its party — nothing about feeding is modelled at the source

**The home band feeds its party through its ordinary consumption; supplies ride the porters' return
leg, so nothing about feeding is modelled at the source.** The party's people never left the home
band's cohort, and `simulate_population` already charges the whole `working` bracket wherever those
workers stand. A porter walking a pack home walks supplies back out on the way to rejoin, so the
feeding needs no mechanism of its own. The whole take walks home, and a party cannot be short of food
separately from its band: if the larder runs dry, the band starves by the ordinary demographic rules,
and its party with it.

A **stationed party** (`expeditions.md` → "A stationed party") is the same rule for a detached party:
it carries no larder, and its workers are charged to the home band's meal from launch until fold-back;
if the larder cannot cover them the band bears the shortfall and the post stands.

> #### RETIRED: an eat-first rule
>
> The first caravan had the party eat its upkeep out of its own take before anything was loaded,
> crediting the eaten share home. It was bookkeeping — it changed nothing physical except *which*
> food walked — and it produced a deficit, a supply gate that folded an "unsupplied" posting back
> (`status=recalled reason=unsupplied`), and four readouts (`partyAte`, `partyDeficit` on the row,
> `deficit` on the query reply, the recalled feed line) to explain the gate. All of it is deleted.
> Do not reintroduce a per-party food account: the band's consumption is the one place its people
> eat.

**One pack is one hunter's carry, seated by the web's own rule** — `fauna::one_pack_biomass` on the
animal web (whole animals, **rounded down**), continuous on the plant web. It is deliberately **not**
`fauna::animals_the_pack_seats`, which rounds **up** because it answers the kill-stop question — the
animal the pack cannot seat whole is still killed whole.
**Nothing is walked away from**: a carcass too big for one pack goes a pack's worth at a time and
the remainder stays in the load for the next porter. Every hunt — a far posting and a camp kill
within `band_work_range` alike — keeps `take.killed_biomass()`, not `take.carried`, and its row's
`wasted` is `0`; a camp kill differs in that its walk is zero, it charges no sled wear and its kill count has no carry cap (only a posted kill is stopped by what its pack seats). An unbounded carry (a pen that is a larder) takes
the whole load in one pack.

**A pack is filled by BULK — everything the crew cut — and lands whatever it is worth as food.** A
basket of a cash or fodder crop (tobacco, cotton, hay: `provisions_per_biomass == 0`) cuts biomass
every turn and walks it home in packs exactly as a food basket does; its packs carry zero food
cargo. Two readings had to say so: `forage::ForageProjection::step` calls the stand spent only when
**no biomass** was taken (`REALIZED_PROJECTION_BIOMASS_EPSILON`), not when the take was worth no
food — read off provisions alone it ended every no-food projection on its first turn — and
`forecast_caravan`'s `first_load_turn` is the first turn a **pack** lands, not the first turn food
does. Together they made a far no-food basket quote the `0` sentinel, and the compose sheet drop its
*first load home in N turns* clause. Pinned by
`forage_cultivation::a_far_basket_with_no_food_quotes_its_first_load_turn` (one harvester, six
tiles out: first load turn 14).

**A far FORAGE row's fodder and materials ride the packs with its food** (#706). The take site packs
them instead of crediting them: the fodder at the credit's own gate (`fodder_permitted`) and each
material through `materials_config::material_yield_batches` — the batches `credit_material_yield`
deposits, so a far pack carries exactly what a local take would have credited. They join the load
as `WorkParty::load_goods` (`work_party::CarriedGoods`: fodder plus `CarriedMaterial`s at their exact
reading and band key), leave in every pack in the bulk's proportion (`CarriedGoods::take_share`, the
food classes' split), and land when the pack does — fodder into the `FODDER` store, each material
through `LocalStore::deposit_material`, the one deposit a caravan's material makes. **They do not
rot**; the transit rot strikes food classes only.

- **On a live row they are that turn's income at landing** (`land_delivered_goods`): the row's
  `fodder` and `materials` read what landed, and `fodderInflow` is the hay that arrived — so
  `Δ FODDER == fodderInflow + Δ received − draws` closes with the delay.
- **Off a live row they land on the route arm** (`land_goods_off_the_row`): a stood-down walk, a
  lapsing row's last packs and an arm that never reached its take site book the fodder on the fodder
  ledger and each material as its own `PartyHome` crossing.
- A homeward walk carries the same mix, and **a load abandoned with nobody at the source abandons
  all of it**.
- **The forecast carries them as bulk**: `CaravanForecast::bulk_rate_home` is the bulk landing per
  turn, and `forecast_forage_caravan` turns it into `CaravanForecast::fodder_rate_home` /
  `materials_rate_home` through the basket's own per-biomass rates, behind the credit's Foddering gate
  (passed in as `fodder_credited`; `forecast_query::forage_fodder_credited` for the seed and the
  query). The turn writes them onto the party beside `net_rate_home`, so the row publishes
  `fodderRateHome` / `materialsRateHome` — **the smoothed rates the work row prints**, because its
  per-turn `fodderYield` / `materialYield` are lumpy on a far row. The work-party reply carries the
  same two as `fodder_rate_home` / `materials_rate_home` (proto 11 / 12). `0` / empty inside the
  apron, where the take-site figures apply. Pinned on the encoded row by
  `work_party_caravan::a_far_hay_row_publishes_smoothed_fodder_and_materials_rates_home` (non-zero on
  turns no pack landed).
- **A local row is unchanged**: it credits its fodder and materials the turn it cuts them.

**A far HUNT's or PEN's by-products ride the packs by the same rule** (#706). At the kill a far row
packs every material its carcass yields — off `loaded`, the whole carcass the party keeps, through
the species' `hunt_materials_for` rows — plus a pen's standing rows (fleece) off the head count
(`systems::labor::hunt_goods_cut`, the same `material_yield_batches` → `CarriedGoods` path the forage
arm takes), instead of crediting them; they land with the pack through the same landings above. A
hunt yields no fodder. A pen can be far: it stands where its herd does, so it posts a party like any
other row. A local hunt or pen still credits at the kill, off the whole kill (`killed_biomass`). The deposit web's
material is not a side good but the cargo itself (below).

**A far pen's standing yield walks home and is in its forecast.** Its milk or eggs ride the pack's
food cargo (the pen arm loads meat and standing food as one delivery, in the herd's one keeping
class, so they rot by the walk as its flesh does) and its fleece rides the goods. Both now carry
**bulk** (`work_party::standing_stream`): the milk's biomass-equivalent at the herd's own meat rate,
each fleece at its material's `weight`. The pen and hunt arms load it beside the carcass, so a pen
that culls nothing still fills packs — without it, its milk and fleece sat in the load and never
left. `forecast_hunt_caravan` steps the same thing through `forecast_caravan_carrying`: each
projected turn's food is `HuntProjection::step`'s (which already counts the standing food at the
projected head count), its bulk adds the stream's, and its goods are the carcass rows off the cull
plus the fleece — the stream read off `HuntProjection::herd`, so fleece follows the projected head
count as milk does. So `netRateHome` / `rate_home` carry the milk net of the walk's rot (and
`spoiledRateHome` / `transitKeepsTurns` when the walk outlasts its class), and `materialsRateHome` /
`materials_rate_home` carry the hides and the fleece, averaged over what lands. Pinned by
`pen_standing_yield::a_far_pens_milk_and_fleece_are_in_its_caravan_forecast` (a culling-free sheep
pen at 5 and 8 hexes, on the encoded row and the compose reply: the milk carried is at least half the
standing stream and spoils on the long walk, the fleece likewise, and the reply quotes the row).
Pinned for the carcass by
`work_party_caravan::a_far_hunt_lands_its_hides_with_its_packs_and_prints_a_smoothed_rate_home`
(on the encoded row: hide lands exactly on the turns food does, none before the first pack, and a
no-landing turn prints a non-zero hide `materialsRateHome` the compose reply quotes too), and for
the forage web by `forage_cultivation::a_far_hay_row_lands_its_fodder_and_fibre_only_when_a_pack_lands` (nothing lands
before the walk allows; the fodder ledger closes every turn) and
`work_party::tests::a_pack_carries_its_share_of_the_loads_goods`.

**On the deposit web the material IS the cargo** — see below.

**A crew at the source is what earns a lesson** (`crew_at_the_source`, the hunters present), while
the holding test asks what the player **staffed** (`take_crew_present`). Read the holding test off the
hunters present and a party with every hand on the road would retire its own row silently; read the
lesson off the staffed crew and a party walking out would learn at a source it has not reached.

### ⛔ The take flows HOME, always

To the band that owns the row, **never** to whichever band the party is standing beside. A party is an
*extension of its home band*, not a peer node in the supply network, in **either** direction:
`balance_supply_networks` has no party awareness at all, and a party's position never enters the
union-find. Cost and benefit have one owner — the home band feeds its party and receives its take.
Making the party a network node "so it can be fed" re-introduces the bug the take's rule forbids, one
direction over.

Pinned by `labor_allocation::a_partys_take_is_credited_to_its_home_band_not_to_the_band_beside_it`.

## The deposit web: the cargo is the material

**A far working posts a party exactly as a far patch does** — `party_source_position` answers the
deposit's own tile, and the Extract arm's out-of-range lapse (`status=lapsed reason=out_of_range`) is
deleted. There is **nothing special about wood or stone**: no code path in the arc names a material.
What a quarry the band has moved away from costs it is paid in walking, as a far patch's is.

**Cargo and bulk.** The caravan does not know what it carries. `WorkParty`'s load and every walker's
pack are **cargo** (what lands at home) measured in **bulk** (what a pack is measured in): food and
biomass on the two food webs, and on the deposit web **both are the material's own units** — one
number, so the caravan arithmetic is unchanged.

**⛔ ONE PACK IS THE HUNT'S OWN HAUL CARRY OVER THE MATERIAL'S WEIGHT.** `work_party::material_pack`
is `CaravanPricing::haul_carry / weight`: the bare `labor.hunt.per_worker_biomass_capacity` (**12**),
or the sled's `hunt_carry` (**40**) under the coverage and wear of the kit the row **claims** — the
same carry a hunter's pack is struck from. So a bare porter carries `12 / 2.4 = 5` wood and a sledded one
`40 / 2.4 ≈ 16.7`. The whole difference between two materials is the one `weight` on each; a
per-material branch, or an extraction-only carry lever, would be the defect.

**A slow far working lands nothing until its first pack fills, and the stone is in the load, not
lost.** A porter leaves only with a full pack (`WorkParty::close_turn`), so a lone hand at 0.8 a turn
against a sledded stone pack of `40 / 3 = 13.3` digs for 17 turns before the first landing, with the
deposit's stock falling, the store holding no stone, `materialYield` empty and `nextLoadHomeIn` `0`.
The load is published on an extract row: `loadUnits` (`WorkParty::load_cargo`, the material's
units cut and not yet sent), `packUnits` (the pack in the same units, stamped by the extract arm; `0`
with no party) and `loadHomeIn` (the caravan forecast's `first_load_turn` - turns until the next
landing, the wait for a pack to fill included; `0` = none within the horizon, `NO_LOAD_WITHIN_HORIZON`).
`nextLoadHomeIn` keeps its meaning (a pack already on the road). `loadHomeIn` is published on every
party row; `loadUnits` / `packUnits` only on an extract row, the one web whose load is in the row's
own units (a food web's is biomass). Pinned by
`work_party_caravan::a_slow_stone_dig_is_conserved_and_lands_only_when_a_pack_fills` (stock removed ==
store + load + porters' cargo every turn; the wire's load climbs, the pack matches, and
`loadHomeIn` counts down to the turn the stone actually lands).

**The haul carry is read off the row's kit, claimed whole like every job's.** `CaravanPricing` is
resolved over the row's stored kit — the same claim `LaborAllocation::item_budget` rations the band's
gear with — and every take kit (`sledding`, `woodcutting`, `stonework`) carries the sled, so a
kitted crew hauls on its sleds on every rung, felling and quarry included. The pricing's crew-weighted `deposit_take` per worker
(`CaravanPricing::deposit_gear_per_worker`) is the tool term the forecast adds to each turn's cut,
`present ×` that rate, and one pack per present hand is its carry: **the same carry caps the cut,
near and far** (`extraction::CrewLift`), as a hunter's haul bounds a kill. The sled is **charged
`biomass_hauled` over every extract take, in the carry's unit** (units × weight), against the row's
kit, at the take and after it — the hunt's own haul quantum and ordering.

**The take site** routes `outcome.taken` through `deliver_take_home` like every other arm. What
lands this turn — walkers' deliveries plus any pack landed now — is what is deposited through
`LocalStore::deposit_material` at the ground's characteristics and reported in the row's
`materials`; while the party walks out nothing is deposited. `actual` stays zero. **A local working
never enters the seam**: `deliver_take_home` speaks the food ledger's fixed-point `Scalar`, and the
round trip moves an `f32` take by an ulp, which is the local identity broken on the one row with no
party to explain it.

**One projection.** `extraction::DepositProjection` is regrow-then-take on a clone
(`renew_deposit` → `take_from_deposit`), and it is the only forward take formula on the deposit web:
`forecast_extract_caravan` steps it for a far working, the seed's local arm reads its first step, and
`extraction::project_realized_deposit` averages it for the query's local answer. A working is **spent**
only when nothing is reachable on ground that never renews; a crew of nobody takes nothing while the
stand regrows.

**Where it lands is `systems::CargoHome`**, resolved from the row's target in one place
(`CargoHome::of`): the larder for a food web, the material store — at the deposit ground's
characteristics and `materials_cfg.band_key` — for a deposit. Every settlement reads it: the foot of
the pass and every homeward walk's arrival (`bring_the_party_home`, off the walk's own `target`).
**The row's food projections are not written for a material cargo** (`CargoHome::is_food` gates
`publish_caravan_projection`): its rate home rides `netRateHome` alone, in material units per turn.

## ⛔ One function, stepped — shared by the turn, the seed and the query

`work_party::forecast_caravan` steps a party forward `yield_average_horizon_turns` from a state,
through `open_turn` → a projected take at whatever crew is present → `close_turn` each turn. The projected
take is the smooth headline's own step — `fauna::HuntProjection::step` and
`forage::ForageProjection::step`, which `project_realized_hunt` / `project_realized_forage` are loops
over — so the forecast runs the hunt's and the gather's own projection, not a second copy.
`forecast_hunt_caravan` / `forecast_forage_caravan` / `forecast_extract_caravan` are the three web
adapters, the last over `extraction::DepositProjection`. The run stops once the source is spent
**and** nothing is on the road or in the load, and averages over the turns stepped.

It is read in three places, and must stay one function:

| reader | where | what it publishes |
|---|---|---|
| the turn | the take site, from the state the turn leaves | the row's `netRateHome`, `realized` and arrival schedule (`publish_caravan_projection`) |
| the assign-time seed | `bin/server.rs::seed_source_yield` | the same three, plus `actual` = what lands next turn (`0` while walking out) |
| the compose-sheet query | `forecast_query::answer_work_party_forecast` | `rate_home`, the walk, the mean hunters on the road, the first landing |

**The row's projections are what arrives home, not what is taken.** `realized` is the headline the
food runway and the work board read, so a far row publishing what it takes this turn would promise
a larder food that is still walking home. The published split `meat + standing == actual`
is kept by scaling the two parts onto the row's `actual` in their own proportion.

**Priced at the row's STAFFED crew** (`work_party::CaravanPricing`), not at the hunters present this
turn: the forecast steps a crew that moves every turn, and neither the seed nor the query knows who is
on the road now, so the row's own head count off the band's share of its gear
(`BandItemBudget::with_prospective_row` beside its other rows) is the one input all three resolve
identically. The turn's *take* is still priced at the hunters present. **The kit is spread over the
staffed crew's take hands and settled on its claim** (`CaravanPricing::resolve(take_hands, claim,
…)`, `equipment.md` → "A ROW CLAIMS ONLY THE TAKE HANDS"), struck by the same `take_claims` function
in all three, so a party's carry is what its cutters and gatherers hold.

**The seed and the query step the band's STANDING party when it has one**, restamped for the crew
asked about — a stepper press on a live posting re-seeds that posting, not a fresh one that would
re-promise a walk out. With none, they step a party posted now. **Inside the apron the query answers
the ordinary local row's steady rate** with `posts_a_party: false` and every walk field `0`. The seed
declines no far row on any web, and its Hunt gate reads no hunt-only distance. A far
Extract row seeds `materials` with what lands next turn (nothing while walking out) and no food field.

Pinned by `work_party_caravan::the_query_quotes_exactly_the_rate_the_row_publishes`, which asks the
socket after a turn that has put somebody on the road and compares with the row's `netRateHome` read
off the **encoded** snapshot — exact equality, because both are one function from one state.

## The wire

Every party field on `LaborAssignment` (`snapshot.fbs`) reads `0` on a local row, and `partyWorkers ==
0` is the test for *"no party"*:

| field | what it is |
|---|---|
| `partyX` / `partyY` | the tile the workers stand on — the source's own position |
| `partyWorkers` | every hand the posting holds |
| `huntersOnTheRoad` | **live**, this turn: out with a pack or walking back. It moves `0, 1, 1, 0, 2…` and that is honest |
| `walkTiles` | the one-way walk, from the apron, shortened by any road |
| `walkOutRemaining` | `> 0` while the whole party is still walking out; `0` for the rest of the posting |
| `nextLoadHomeIn` | turns until the soonest pack lands; `0` = nobody is carrying a load home |
| `loadHomeIn` | turns until the next landing counting the wait for a pack to fill; `0` = none within the forecast horizon |
| `loadUnits` / `packUnits` | extract rows only: material units in the load at the source / units one pack carries |
| `netRateHome` | cargo per turn arriving home **and keeping** — net of transit rot — the number the row prints: food on a hunt or forage row, the material's own units on an extract row (the row's `kind` says which) |
| `spoiledRateHome` | cargo per turn lost on the walk home to transit rot, over the same forecast (#706); `netRateHome + spoiledRateHome` is what the porters carry in. `0` on a local row, an extract row and any walk every class survives |
| `transitKeepsTurns` | the shortest shelf life among the row's cargo classes that rot on this walk, in turns; `0` when nothing rots |
| `fodderRateHome` | a far forage row's fodder per turn arriving home (a hunt yields none) — `netRateHome`'s twin, smoothed off the same forecast, where the row's `fodderYield` reads only what landed that turn; the credited figure (Foddering gate). `0` on a local row, a hunt and an extract row |
| `materialsRateHome` | a far forage row's, hunt's or pen's materials per turn arriving home (a hunt's hide, bone and sinew), one `MaterialPayoff` per material id, never summed; absent on a local row and an extract row. `materialYield` beside it reads only what landed |

The query is `QueryPayload::WorkPartyForecast` (`sim_runtime`, proto query field 7, reply field 10),
seat-gated like the other faction-bearing questions: band, `Hunt { herd_id }`, `Forage { x, y,
take_species }` or `Extract { x, y, material }` (proto oneof field 8), kit (named, never defaulted),
crew and floor. It is refused with a `query_error` token for an unknown band, herd (`unknown_herd`),
patch (`unknown_patch`) or deposit (`unknown_deposit` — ground holding none of that material), an
unknown or wrong-job kit, an invalid floor or an oversized crew. A deposit's `rate_home` is material
units per turn.

**The reply** is `posts_a_party`, `rate_home`, `walk_tiles`, `walk_turns`, `hunters_on_the_road` (a
mean, so a float) and `first_load_turn` (1-based, `0` = none within the horizon) — every walk field
reads `0` inside the apron — plus **`take_next_turn`** (next turn's take AT THE SOURCE by the asked
crew, struck on the hands its keeping leaves, before any walk) and **`keep_hands`** (the fractional
hands it spends keeping), appended (proto 7 / 8), and **`spoiled_rate_home`** / **`transit_keeps_turns`**
(proto 9 / 10) — the row's `spoiledRateHome` / `transitKeepsTurns`, answered by the same forecast, so
`rate_home` is net of transit rot. Pinned on the encoded row and the answer together by
`work_party_caravan::a_far_hunt_publishes_and_quotes_the_take_its_walk_spoils`. They are what the compose sheet previews a crew of
`n` on every web, rather than pricing every worker as a taker; pinned on a patch by
`forage_cultivation::a_kept_patchs_next_turn_take_is_quoted_on_the_hands_its_keeping_leaves`.

**`ForageCrewTake`** (proto query **9**, reply **12**) is the patch's whole crew curve in one round
trip — the hunt and deposit curves' shape: one `ForageCrewTakeRow{workers, take, keep_hands}` per
crew `1..=max_workers`, each row **the work-party forecast's own `take_next_turn` / `keep_hands` at
that crew** (`forecast_query::answer_forage_crew_take` asks it per crew), so the stepper and a
one-crew quote are one arithmetic. Seat-gated like the other faction-bearing questions. Pinned by
`forage_cultivation::a_patchs_crew_curve_is_the_single_crew_answer_at_every_size`.

**Each row also carries `next_rung_take` / `next_rung_keep_hands`** (proto 4 / 5): the same crew on
the patch once the rung in flight is finished — the next rung up where nothing is in flight — with
**that rung's keeping netted** (`forecast_query::patch_once_raised`: the meter seated at the rung's
top on a clone, `K` re-struck at the rung's gain, the bill re-struck at the finished rung). `0` at the
top of the branch. It is the compose sheet's *once sown / once tended* figure.
`rungQuotes[plant:farm].foodYield` / `rungQuotes[plant:tended].foodYield` are **not** that figure and must not stand in for it:
they are the rung's crew-blind payoff (`forage::rung_payoff`), so a crew too small to keep a Field
read `12.48` beside a sheet whose own keeping left it nothing. Pinned beside the next-turn quote by
`server::tests::a_lapsed_fields_quote_is_what_the_turn_pays_and_once_sown_nets_the_fields_keeping`.

**The next-rung half states every account and is priced for a CROP.** Each row also carries
`next_rung_fodder` (proto 6) and `next_rung_materials:[MaterialPayoff]` (proto 7) — the same take's
fodder and per-material vector, off one projected turn's biomass through the patch's own rates, as
the labor arm credits them. Fodder is the **credited** figure: `0` without Foddering unless the
commitment is to a fodder-bearing plant (`systems::committed_to_a_fodder_crop`, the credit site's own
gate). Materials are one row per material and never summed. The ask carries `crop` (proto 9): an
**uncommitted** patch is priced as committed to it when it grows in the tile's basket, else to
`forage::default_species_for_rung` for that rung; a committed patch ignores it. Pinned by
`server::tests::an_uncommitted_patchs_once_tended_is_priced_for_the_picked_crop_in_every_account`.

## Every exit walks everything home, through ONE stand-down step

**A caravan that ends early loses nothing on the road, and hands nothing over at once** (#706).
`systems::stand_down_party` is the one stand-down step: it turns the party into the band's
`HomewardWalk`s (`WorkParty::walk_home`) on `LaborAllocation::homeward`, which outlives the row —

- **a porter carrying a pack** finishes its walk (`walk_turns − turns_out` turns) and lands it;
- **a porter heading back out empty** turns round and walks back the way it came
  (`turns_out − walk_turns` turns);
- **the hands at the source** carry the load over the whole walk. **When nobody is at the source**
  (every porter on the road) **the leftover load is left behind** — no group walks without a hand, so
  it never lands. It was never income nor in the larder, so no ledger term or spoilage answers for it;
- **a party still walking out** walks back the turns it has covered, carrying nothing;
- a group already home (a porter that landed that very turn, a party stood down before its first
  step) is not listed.

**Each walk takes one step at the top of its band's labor pass** (before the starvation shed, so a
group arriving this turn is back in the pool the shed reads), and lands when `turns_left` reaches
zero through `bring_the_party_home` — the route arm, then the transit rot by its walk, added to
`last_food_spoiled`. A stand-down in the command window (unassign, abandon, `cancel_order`) is
stepped first on the next turn, so a porter one turn out lands then, as it would have on the live
row. A stand-down the labor pass makes **before** the party's turn opened (the shed, the unposted
sweep) goes through `stand_down_unopened_party`, which takes the party's step for this turn first
(`open_turn`, landing whatever it lands) — without it those walkers would lose a turn the
command-window paths do not cost.

**A player's cut walks its dropped hands home FROM THE COMMAND** — `LaborAllocation::set_assignment`,
the one place a crew is cut: a far row cut short of zero through `WorkParty::cut_crew`, one held at
zero through `WorkParty::walk_home`, onto `homeward` in the command itself (as `abandon`'s
`bring_the_dropped_party_home` already did). `walking_home` therefore covers them at once, so
`idleWorkers` and `BandWorkforce::assignable` never offer a hand still days from home, and a second
`assign_labor` cannot give them to another row. In `cut_crew` the hands at the source go first and
walk the whole walk carrying nothing — the load stays with those who remain; past them, porters on
the road nearest home first: one carrying a pack finishes its walk and lands it (rotting by its walk)
but does not walk back out, one heading back out empty turns round. A party still walking out sends
the dropped hands back the turns it has covered. A raise cuts nothing: new hands are at the source
at once, as they always were. Pinned by
`work_party_caravan::cutting_a_far_crew_walks_the_dropped_hands_home` (before any turn: the four on
the wire as walking home, `idleWorkers` unchanged, `assignable` without them, a second row refused
them; they rejoin only as they arrive) and `work_party::tests::a_deep_cut_turns_porters_round_nearest_home_first`.

> #### ⛔ THE STARVATION SHED'S HANDS LEAVE WITHOUT WALKING HOME
>
> The shed (`LaborAllocation::normalize`) drives the band's committed hands — `walking_home`
> included — down to the people it still has, so every hand it removes is one the band **no longer
> has**. Sending those hands home would list them on `homeward`, count them against the pool next
> turn and fire the shed again on the hands it had just shed: a far row of six losing two people
> would go 6 → 4 → 2 → 0. **A staffed row is not shed at all** (#871): its hands are zeroed before the
> shed walks and refilled from the smaller pool after it, so only the hand-set crews (Scout, Warrior,
> Builders, a need row's muster) are trimmed; the hands a staffed row's party loses to the smaller pool
> are discarded the same way (`apply_staffing`'s `lost_people`). So a row the shed **trims** reaches `post_a_party` below its party's crew
> (the only way it can, a player's cut having already matched them), and the party sheds the
> difference through `cut_crew`'s own order — the hands at the source first, then porters nearest
> home, whose packs go with them — with its walks discarded. A row the shed **drops** goes through
> `shed_unopened_party`: the party takes this turn's step (what lands now lands) and nothing walks
> home. Pinned by `work_party_caravan::a_far_row_the_shed_trims_sheds_once_and_nobody_walks_home`.

**The hands are away until they arrive.** `LaborAllocation::walking_home` sums the **plain** walks'
workers (a walk that [rejoins its row](#a-re-entered-rows-walkers-rejoin-it-and-are-counted-once) is
inside that row's `workers` already); `BandWorkforce::walking_home` nets it out of `idle`, `assignable` and `benchable`, and
`LaborAllocation::normalize` counts it beside the bench in the total it drives down (nothing in the
shed can shed a walker, so a pool below them strips every row and stops). It rides `SimState` with
the allocation (save v18) and sits outside the allocation's intent `PartialEq`, as a row's party
does.

Every path that ends a posting routes through the one step:

- **A lapsing row** — a holding with nothing left to hold; the row ends with `status=lapsed`. The
  packs its turn already landed land off the row (rotting by their walk), and its caravan is stood
  down before the row is removed.
- **A source that stopped posting** — the herd drifted back inside `band_work_range` (or the band
  moved up to it), or the herd **left the registry** (`status=lapsed reason=herd_gone`). Neither turn
  posts a party, so neither reaches the per-posting settlement; the foot of the pass sweeps every row
  still carrying a party that posted nothing this turn, stands it down, and clears `party` — **before**
  the `lapsed` removal, which would otherwise drop the row with its caravan on the road. A re-entered
  row is plainly local afterwards and publishes no party field. **Its walkers stay on the row**: the
  sweep marks every walk `rejoins_row` (below); a row the same pass lapses (`herd_gone`) flips them to
  plain walks as it goes.
- **Unassign** — `set_assignment` at zero stands the party down in the command (above); the row
  survives as a holding if it holds anything, with no party. A row the shed trims to zero walks
  home whatever its shed left.
- **Abandon / a zero-crew drop** — `LaborAllocation::drop_source_row` returns the row it removed, and
  `systems::bring_the_dropped_party_home` stands its party down.
- **The starvation shed** is the one exit that does **not** walk its hands home — see the callout
  above.
- **`cancel_order`** — `clear_kinds` drops rows with their parties too, so `handle_cancel_order` reads
  the rows it is about to clear and stands each one's party down through
  `bring_the_dropped_party_home`.

**The band carries the total; each row carries its own share.** A walk outlives its row, so the
band's fields (`PopulationCohortState`, appended) count every walk:

| field | what it is |
|---|---|
| `homewardWorkers` | **plain** walkers only — hands walking home, not idle, on no row; `idleWorkers` already excludes them. Hands walking back to a surviving row are in that row's `workers` and are not counted here |
| `homewardFood` | the food they carry, gross of the walk's rot (a deposit's material is not counted) |
| `homewardFoodSpoils` | of `homewardFood`, what rots before it lands (`HomewardWalk::food_that_rots`) |
| `homewardNextLoadIn` | turns until the soonest homeward load lands; `0` = none — the row's `nextLoadHomeIn`, carried past the row's end |
| `homewardAllHomeIn` | turns until the last plain walker is back; `0` = nobody walking home |

A row still on the board (`LaborAssignment`, appended) carries the walks whose `target` is its own
source, by `LaborTarget::same_source` — the identity a row keeps across a floor or crew edit — so a
cut or unassigned far row can say *"3 walking home"* under itself:

| field | what it is |
|---|---|
| `homewardWorkers` | **plain** walkers from this row — hands that LEFT the row (a cut, an unassign) walking home |
| `homewardAllHomeIn` | turns until the last of those hands is back; `0` = none |
| `homewardFood` | the food they carry, gross of the walk's rot |
| `rejoiningWorkers` | hands of this row walking back to it — already inside `workers`, not at the source until they land |
| `rejoiningAllHomeIn` | turns until the last of those hands is back; `0` = none |
| `rejoiningFood` | the food they carry, gross of the walk's rot (a deposit's material is not counted) |

**Both readings are one summation** (`work_party::HomewardTotals::of`, the row's through
`HomewardTotals::for_source`; the rejoining fields through `HomewardTotals::rejoining_for_source`), so
they cannot disagree. A walk whose row is gone (abandoned, lapsed,
cancelled) matches no row and appears on the band only, so the rows' `homewardWorkers` sum to at most
the band's. Pinned off the encoded row by
`work_party_caravan::{a_far_row_unassigned_to_zero_publishes_the_hands_walking_home_from_it,
a_partly_cut_far_row_shows_its_crew_and_its_walkers_until_they_arrive,
an_abandoned_rows_walkers_appear_on_the_band_only}`.

> #### ⛔ CARGO A STOOD-DOWN PARTY BRINGS HOME GOES ON THE LEDGER'S ROUTE ARM
>
> It is not any live row's income — a walk outlives its row, so no telemetry counts it in — and food
> that reached the larder through neither `food_income` nor a transfer would break the pinned identity
> `larder_delta == food_income − food_consumption − raid_forfeit − spoiled − left_behind + transfer_received −
> transfer_sent`.
> A party carrying goods home is exactly what `TransferLink::Route` is for, so `bring_the_party_home`
> books it there — through `LaborAllocation::book_crossing` as `TransferCause::PartyHome`, the one way
> a crossing is written, so the route arm and the cause-keyed crossings row agree and the band's own
> caravan never reads as trade (`campaign.md` → "The cause key and the crossings list"). Food a
> **live** posting lands (a delivered pack) goes through the row's `actual` like any other take.
>
> **A deposit party's cargo is MATERIAL and never touches the food ledger.** It is deposited into the
> store at the ground's characteristics and booked as a `TransferCrossing::material` crossing with
> the same `PartyHome` cause — the material route arm, which is the crossings list itself. Which of
> the two a party's cargo is comes from `CargoHome::of`, never from a check at the settle site.

Pinned by `work_party_caravan::a_cancelled_far_hunt_walks_every_pack_home_and_rots_it_by_its_walk`,
`::a_herd_back_inside_the_apron_walks_its_caravan_home_once` and
`::a_vanished_herd_walks_its_caravan_home_as_the_row_lapses`, each on the route arm.

## A re-entered row's walkers REJOIN it, and are counted ONCE

A herd that drifts back inside `band_work_range` sends the party home (above), and the row survives as
a local row at the crew the player set. Its walkers are **that crew's hands on the road**, not a
second set of hands, so they are counted on the row and nowhere else:

- **`HomewardWalk::rejoins_row`** is set on every walk the unposted sweep makes for a row that
  survives the pass (`stand_down_party(.., WALKS_BACK_TO_ITS_ROW)`). Every other stand-down
  (`bring_the_dropped_party_home`, a row held at zero, a row lapsing under its party) walks **plain**
  (`WALKS_PLAIN`).
- **Counting.** `LaborAllocation::walking_home` — and so `BandWorkforce::idle / assignable /
  benchable` and `normalize`'s total — counts **plain walks only**; `assigned_total` counts the row's
  `workers`, rejoining hands included. A band of eleven holding rows of four and four with three
  hands on the road home reads idle **3**, not 0: forage 4 + hunt 4 are committed and nothing is
  counted twice.
- **Hands present** (`LaborAllocation::hands_present_on` / `hands_present`) = the row's `workers`
  less its rejoining walkers (`rejoining_to`, matched by `LaborTarget::same_source`). A local row with
  three of four away takes with one. The crew a row's **take, keeping, kit claims, lessons and
  caravan posting** are priced on is the hands present; `assignment.workers` still answers "is this
  row held". The readers of it:
  - `systems::labor::advance_labor_allocation` — the row walk's `present_hands` (the party's crew,
    the take crew, the lesson test, the caravan forecasts' keeping hands);
  - `SiteCrews::of`, `site_keeping_claims` and the keeping asks — a site's keeping is paid by the
    hands there;
  - `take_claims::row_claims` (and so `LaborAllocation::item_budget`, the capture's kit lines and the
    craft suggestions' `take_row_gear`), `LaborAllocation::workers_on_job`;
  - `extraction::deposit_crew_needed` — the useful-cutters count;
  - the need-row muster — `MusterKit`, `muster_donors`, `musterable_pool`, `donor_order` and
    `MusterPricing::other_rows_at` lend only hands that are present.
- **Landing.** A rejoining walk lands exactly as a plain one does (the route arm, the transit rot by
  its walk) and changes no head count: the hand is on the row already and is present from then on.
- **The row goes far again while hands are still walking back.** The party posts with the hands
  present or arriving as its crew. A rejoining hand that lands on a turn the row posts a party joins
  it as a `Walker` heading back out empty with the whole walk ahead (`turns_out == walk_turns`, the
  state a porter is in the moment its pack lands): it is in the party's `workers` and not among
  `hunters_present` until its walk ends (`post_a_party`'s `returning`).
- **Every exit routes through ONE helper**, `LaborAllocation::settle_rejoining(ExcessRejoining)`,
  which restores the invariant *a source's rejoining hands never exceed its row's `workers`* (a source
  with no row holds none), settling the excess the most `turns_left` first:
  - `WalkHomePlain` — an unassign, a cut below the rejoining count (`set_assignment` cuts the present
    hands first, then flips the excess), `abandon` / a zero-crew drop (`drop_source_row`),
    `cancel_order` (`clear`, `clear_kinds`), a lapse (the removal at the foot of the labor pass) and
    the sweep's own zero-crew rows: the excess hands become ordinary walkers going home, counted in
    `walking_home` and landing into idle (`HomewardWalk::split_off` divides a group's cargo with its
    hands);
  - `Discard` — the starvation shed (`normalize`) trims a row's `workers` present-first and then
    rejoining, discarding the excess without walking home, as every hand the shed removes leaves.
- **Which turn's take is priced on the present hands.** The sweep runs at the foot of the pass, so the
  turn the herd comes back in is still priced as a local row at the full crew (as it always was); the
  next pass is the first at which the row's hands present are fewer than its crew.

Pinned off the encoded frame by `work_party_caravan::{a_herd_back_inside_the_apron_does_not_count_its_walkers_twice,
a_rejoining_rows_hands_are_counted_once_every_turn_until_they_land,
a_re_entered_rows_take_is_priced_on_the_hands_present,
a_cut_below_the_rejoining_count_flips_the_excess_to_plain_walkers,
an_unassign_flips_every_rejoining_walk_to_plain, a_lapse_flips_every_rejoining_walk_to_plain,
the_shed_discards_the_rejoining_walkers_it_trims,
a_rejoining_hand_landing_on_a_re_posted_party_walks_back_out_the_whole_walk}`. `SAVE_FORMAT_VERSION`
37 (the flag rides `LaborAllocation::homeward`).

## `BandReach` no longer asks about distance

It used to carry the band's position and the two lapse distances, because the arms abandoned an
out-of-reach row on the same `continue` every keeping draw and material spend sat beneath. Nothing
lapses for distance now, so the only thing left that makes an arm skip is a herd the registry no
longer carries. The type survives rather than collapsing into a bare `registry.find`, because *"will
the arm reach this row"* is the question the settlements must go on asking.

## Migration mode — the band's camp moves with a migratory herd (#797)

Design: `docs/plan_roaming_bands.md` §Migration mode. This section is the as-built rationale.

**It is a box on the hunt row, not an order.** `LaborTarget::Hunt::move_with_herd` (wire:
`AssignLaborCommand.move_with_herd`, text token `follow`, snapshot `LaborAssignment.moveWithHerd`).
`same_source` keys a hunt row on the herd id alone, so the flag is a mutable property of the row, as
the floor is: re-sending the row without `follow` turns it off.

**`follow_hunted_herds` issues the movement the band already has.** It lives beside
`advance_band_movement` (`systems/expeditions.rs`) and is registered in the Population chain
**immediately before it**. The herds have already moved this turn (`advance_herds`, Logistics), so
the band re-aims at where its herd now stands and steps right after it: a band camped in the herd
stays on the herd's tile through loiter and migration, and every kill is a camp kill. No work-party,
porter or kill code changed. A band that turns the mode on while far away hunts nothing until it has
caught up (see "Hunting by need" below: the row is a need row and only a camp kill is ever mustered).
A resident band only (never a detached party). `LaborAllocation::followed_herd` follows a row with
the flag **whatever its head count** — a need row holds no standing workers. If the band already
stands on the herd's tile any `BandTravel` is removed; the herd gone from the registry does nothing.

### ⛔ Re-aiming keeps `departed`

A band that already carries a `BandTravel` has its `target` updated **in place**. A fresh
`BandTravel::to()` is `departed: false`, and `advance_band_movement` re-runs the long-move carry shed
(`.claude/rules/core_sim/band-carry.md`) on the first step of a not-yet-departed order that starts
beyond `carry::move_ferry_reach_tiles` — so replacing the order every turn would shed the packs again
on every step of a far catch-up. Only a band with no order gets a fresh one, and a far start sheds
once, as any long move does. `migration_mode::a_far_start_…` refills the larder each turn and counts
the `left_behind` lines; swapping in a fresh `BandTravel::to()` makes it fail on turn 2.

### Two rules clear the flag

- **One herd at a time.** `handle_assign_labor` setting `move_with_herd` on a hunt row clears it on
  the band's other hunt rows (`LaborAllocation::clear_move_with_herd_except`).
- **A `move_band` order ends it.** `handle_move_band` clears it on every hunt row of that band;
  otherwise next turn's follow would pull the band back to the herd it was told to leave.

### Only a migratory herd can be followed

`validate_labor_policy`'s hunt arm (which runs only when workers > 0, so an unassign is never
refused) rejects the flag unless the herd's `size_class` is `SizeClass::Migratory`, with a reason on
the hunt channel. A resident herd never leaves its few tiles, so there is nothing to relocate for.

Tests: `tests/migration_mode.rs` (loiter and migrate camps, the far start, the wire) and, for the
command boundary, `bin/server.rs` `migration_mode_*`; the `follow` token round-trips in
`sim_runtime::command_text`.

### Hunting by need (#798)

Design: `docs/plan_roaming_bands.md` §Hunting by need. Engine: `core_sim/src/hunt_by_need.rs`
(`assess_need`), the muster in `components.rs` (`LaborAllocation::muster_donors`) and its
application in `systems/labor.rs` (`apply_muster` / `restore_muster`), the wire in
`snapshot/population.rs`.

**A hunt row with `move_with_herd` is a *need row*, and it holds no standing workers** (`workers ==
0` always). `set_assignment` forces it to zero whatever count a command names (the hands go back to
idle) and keeps it by its flag: `handle_assign_labor` never drops it at zero, the Hunt arm's holding
lapse (`!row_is_held && !is_need_row()`) passes it, and `validate_labor_policy`'s migratory check
runs at zero too. `assign_labor … hunt <n>` **without** `follow` on a need row is an ordinary hunt of
`n` again (zero drops the row as it always did); `move_band` clears the flag and the next labor pass
retires the empty row.

**The plan is `LaborAssignment::muster_crew`** — the hands the row sends next turn, `0` = waiting.
Like `party` it is outside `LaborAssignment`'s `PartialEq` (a fact the turn restamps), and it rides
the save (`SAVE_FORMAT_VERSION` 34). The donors are never stored. **Beside it rides
`LaborAssignment::planned_crew`** - the meal-sized crew `assess_need` priced at the same decision,
whether or not the row sends (`0` when no crew can be raised, the herd is gone or out of camp
range): a waiting row has `muster_crew == 0` and a planned crew, and the craft suggestions equip that
crew ahead of the hunt (`crafting.md` -> "Craft suggestions"). Also outside the `PartialEq`, and on the
save.

**`planned_crew` is the crew the hunt WANTS** (`NeedAssessment::wanted_crew`): the meal-sized crew
before the equipment cap, so where bare hands cannot wound the quarry a band short of spears sends the
few it can equip, or nobody, and where they can it sends the whole wanted crew; the craft suggestions
ask for the spears the wanted crew needs. **A band holding none of the kit on such a quarry** raises
no useful crew over its own gear, so nothing is sent (`muster_crew` 0, `turnsToKill` 0,
`turnsUntilHunt` `NO_HUNT_NEEDED`) - but `wanted_crew` (and so `planned_crew`) is sized from the
curve priced as if every hand held the whole take kit (`hunt_by_need::unarmed_assessment`: a
`fully_equipped_ledger` of the kit's items unworn at their default tier, no competing rows, no
muster), by the same meal sizing; `0` when even that curve finds no useful crew. `muster_crew` is
the capped crew that goes out; the wire's `musterCrew`, `turnsToKill` and `turnsUntilHunt` are read
off the same capped assessment the turn makes.

**The decision, at the foot of the band's labor pass for next turn** (`assess_need`):

| step | rule |
|---|---|
| herd gone, or beyond `band_work_range` of the band | `muster_crew = 0` — **only a camp kill is mustered**; a one-turn crew cannot run a caravan, so a need row never posts a party and a band still catching up hunts nothing |
| crew | **sized to the meal** (below): the smallest row of `hunt_crew_take_curve(..)` landing the animals the meal calls for, never above `hunt_useful_crew`, priced with `max_workers` = the musterable pool (idle + every donor row's hands) |
| no crew | `0` |
| equipment cap | the crew is capped at **the hands the band can fully equip with the kit items that bind a camp kill** (`MusterPricing::equippable_crew`): per such item, the units no row claims (`unclaimed_units x workers_per_unit`, floored) plus the hands the lendable donor rows' settled units would arm (each row capped at the hands it has present), and the least over those items. **The items that bind a camp kill** (`hunt_by_need::kill_bearing_items`, one list for the cap and for `muster_kit`'s shortfall) are the kit's items declaring a stat the kill reads - `attack`, `dispersion` or `exposure` (`EquipmentStat::bears_on_a_kill`), on the item or any tier - never an item id; the sled (`hunt_carry`, `deposit_take`) is not one, because a need crew is only mustered inside `band_work_range`, where nothing is hauled and the sled wears nothing. **Never caps (`NO_KIT_CAP`)** when no item of the kit binds a camp kill, or when the bare hunter already wounds the quarry (`combat::strike_damage(inputs.intrinsic.attack, defense) > 0` against the quarry the hunt curve prices): the gate is cleared bare-handed, so a kit shortage does not stop the hunt. `crew = min(meal-sized, cap)`; a cap of `0` sends nobody (`muster_crew` 0, `turnsToKill` 0, `turnsUntilHunt` `NO_HUNT_NEEDED`, even mid-hunt) |
| a hunt in progress | the crew was out this turn, the herd has `wounds.pending() > 0` and **no kill landed** → send, whatever the larder says |
| otherwise | send iff `runway <= turns_to_kill + follow.need_margin_turns` (default **1**); `turns_to_kill` is the first non-zero slot of `fauna::project_arrivals_hunt` at that crew (`fauna::hunt_crew_turns_to_kill`), `runway` is the **need clock's** (`snapshot::kill_turns_of_food`, below) and the `NOT_FOOD_LIMITED_TURNS` sentinel never triggers |

### The clock counts only the food the kill replaces

`snapshot::band_turns_of_food` (the wire's `turnsOfFood`) counts the whole larder, and a trigger read
off it let a band burn a dried reserve to its last turn before its first hunt (a band with 66 food,
mostly 60-turn `dry`, reading "hunt goes out in 22 turns · food lasts 25"), and then, after each
kill, eat the meat and go back to burning the reserve. So the need clock reads the larder the kill
stands in for:

- **The larder it counts is the classes that keep no longer than this herd's kill does**
  (`spoilage::keeps_no_longer_than`, through `band_shelf_life` for the kill's class and for each stored
  class). A class that keeps longer is a reserve. `snapshot::kill_food_terms` /
  `kill_turns_of_food` are `band_food_terms` / `band_turns_of_food` over that subset
  (`FoodMix::retaining`), with `rot_ahead` applied to the subset; **net eating (demand less steady
  income) is unchanged**, and `turnsOfFood` is untouched.
- **Both the trigger's runway and the meal's usable larder read it** (`hunt_by_need::need_clock`,
  one call per herd for the turn and the capture). Today only `flesh` counts, so a band arriving with
  no meat has a runway near nothing and the first hunt goes out at once; after a kill the next goes
  out as the meat runs low. When drying (#708) lengthens the kill's shelf life the longer classes
  join the count through `band_shelf_life` — no lever.
- **On the wire:** `LaborAssignment.needRunway` (decode key `need_runway`) — on a need row, the turns
  the meat on hand lasts, the figure `turnsUntilHunt` counts down from (`999` = not food-limited,
  `0` off a need row). A row reading "food lasts N" beside "goes out in M" reads this, not the
  band-wide `turnsOfFood`.

Pinned by `hunt_by_need::{a_dried_reserve_with_no_meat_sends_a_crew_at_once,
with_meat_on_hand_the_band_waits_and_hunts_as_the_meat_runs_low,
a_kill_that_keeps_as_long_as_the_reserve_counts_the_reserve}`.

### The crew is sized to the meal

The useful crew is where the take curve stops rising, and for small easy game that is enormous:
Marsh Grazers (47 a head, ≈ 2.8 food an animal, 37 animals of room) priced a crew of 131 on a camp of
200, which would kill dozens of animals in a turn that mostly rot within `flesh`'s four. So
`assess_need` sizes the crew to **what the band can eat of one kill before it spoils**:

```text
net_eat = max(0, food_demand − Σ realized)          // band_turns_of_food's own terms
keeps   = spoilage::band_shelf_life(keeping, class) // the class the herd's meat lands in
meal    = max(0, net_eat × keeps − usable_larder)   // usable = larder − rot_ahead, as the runway
animals = max(1, ceil(meal / food_per_animal))      // food_per_animal = body_mass × provisions_per_biomass
crew    = smallest curve row with likely ≥ animals  // CREW_TAKE_RISE_TOLERANCE slack; else the useful crew
```

- **One whole animal is the floor** (an animal is indivisible), and the crew never exceeds
  `hunt_useful_crew`. A mammoth is unchanged: no row of its curve lands one in a turn, so the crew is
  the useful one.
- **The terms are shared, not copied.** `snapshot::band_food_terms` (`BandFoodTerms`: demand, steady
  income, usable larder) is what `band_turns_of_food` and the meal both read;
  `hunt_by_need::herd_meal` builds the `NeedMeal` for one herd (class via `FaunaConfig::keeping_for`,
  falling back to `kill_fallback_class`; food per animal via `fauna::herd_food_per_animal`, the
  figure behind the wire's `foodPerAnimal`). The turn (labor pass) and the capture both call it, so
  the wire's `musterCrew` is what the turn musters. The `finishing` path sizes the crew the same way.
- **Preservation moves the meal through `spoilage::band_shelf_life`** — the one place a shelf life is
  read (`campaign.md` → "Food spoils by keeping class"). A longer `flesh` shelf life (drying, #708)
  lengthens the rot rule and the meal together: a bigger meal, a bigger crew, a bigger haul.

The meal's `usable_larder` is the same kill-class larder as the clock's (above), not the whole store.

**Wire** (appended): `PopulationCohortState.huntMeals:[KeepingMeal{keeping, food}]` — the meal for
every configured keeping class, in the keeping table's order, each at that class's shelf life — and
`HerdTelemetryState.keeping` — the class the herd's meat lands in (empty only for a species the table
cannot resolve). A compose sheet sizing a crew before the row exists reads the `huntMeals` entry
named by the herd's `keeping`, divides by the herd's `foodPerAnimal`, floors at one animal, and takes
the first crew-take row reaching it (never above the useful crew). Native decode keys: `hunt_meals`
(array of `{keeping, food}`) on the band dict, `keeping` on the herd dict. Pinned by
`tests/hunt_by_need.rs` (`a_crew_is_the_smallest_that_lands_the_meal_far_below_the_useful_crew`,
`a_mammoth_is_still_sent_the_useful_crew`, `a_bigger_larder_sends_a_smaller_crew`,
`a_meal_of_nothing_still_calls_for_one_whole_animal`, `a_longer_shelf_life_sends_a_bigger_crew`,
`the_wire_carries_the_meal_per_keeping_class_and_the_herds_class`), each against an oracle built from
the fixture's own numbers and the compose sheet's crew-take query.

### ⛔ "Wounds pending" is a hunt in progress, not `pending > 0`

A crew that kills a mammoth deals more damage than the body holds and the excess banks toward the
**next** animal (`DamageLedger::strike`), so a herd carries a small `pending` after almost every
kill. Read as *"the animal is partly down"* that remainder sent a full crew after the next mammoth
every time, whatever the larder said, for ever. The rule is therefore *the crew was out this turn,
the animal took damage and is not down*; Pinned by
`hunt_by_need::wounds_keep_the_crew_going_until_the_animal_is_down_and_the_carcass_lands_whole`,
which refills the larder every turn so only the wound can keep the crew out, and asserts the remainder
after the kill sends nobody.

**Applied in `advance_labor_allocation`, after the starvation shed and before the pools, the site
keeping and the row claims**: for each need row with `muster_crew > 0` whose herd is inside
`band_work_range`, `muster_donors(idle, need_idx, crew, ..)` is clamped to what the donors hold now,
the donor rows' `workers` (and the bench's crew) are cut and the need row's set to the crew. Everything downstream reads those
numbers; kits come through the ordinary claims at the need row's own Priority. After the row walk
`restore_muster` hands every donor its hands back and the need row returns to zero.

- **Donors**: idle first, then source rows (forage, hunt, extract) with no party posted, not a need
  row and **whose source is within `band_work_range` of the band** — decided by geometry
  (`hunt_by_need::source_is_local`, on `work_party::party_begins_past`, the threshold `post_a_party`
  posts past), never by "a party exists yet", so a brand-new far row can't lend. Order is `Low` →
  `Normal` → `High`, the later row first within a tier. Role rows and road keepers never lend. It
  may come up short. The turn and the capture pass the same predicate to `muster_donors`.
- **Idle hands give way to the kit when it is short.** With the take kit short
  (`MusterKit::shortfall > 0`) `muster_donors` takes idle hands only for `crew - shortfall` first,
  then the kit-carrying donors up to the shortfall, then the rest of the idle hands, then the ordinary
  donor order - so a bare idle hand never takes the place of a hand carrying a spear. With no
  shortfall the order is idle, then Priority alone. `MusterPlan::from_idle` is the total idle taken.
  The shortfall is struck over the same kill-binding items as the cap (a sled shortage reorders no
  donor). On a quarry bare hands cannot wound the curve prices only the hands the kit arms, so the
  crew is the free kit's hands and the crafting bench is reached only by a crew the kit fully arms - a
  band short of spears does not strip its bench of the crafters who would make them; on a quarry
  bare hands wound the shortage caps nothing and the whole wanted crew goes, bare hands included.
  Pinned by
  `hunt_by_need::{a_band_short_of_the_kit_sends_only_the_hands_it_can_equip_and_leaves_the_bench_alone,
  a_band_with_no_kit_sends_nobody_on_a_quarry_bare_hands_cannot_wound,
  a_band_with_no_kit_hunts_bare_where_bare_hands_wound_and_still_asks_for_spears,
  a_band_with_spears_and_no_sleds_sends_the_speared_crew,
  a_kit_shortage_binds_only_where_bare_hands_cannot_wound,
  a_short_kit_is_lent_by_its_donor_before_any_idle_hand_goes}` and
  `components::tests::a_short_kit_holds_the_idle_back_until_the_donors_holding_it_have_lent`.
- **The crafting bench lends like a row.** It is not a `LaborTarget`, so `muster_donors` takes it as
  a `MusterBench { priority: BandBench::priority, hands: BandBench::workers }` beside the allocation
  (`MusterBench::of`; `None` for a bench nobody is at). It joins the donor order as one candidate
  ranked by its own mark among the source rows, and **within a tier it lends before that tier's
  rows** (a bench pays into no account — the famine shed's own tie-break, `yield-forecast.md` →
  "The crafting bench is a candidate in the walk"). It stands at camp, so it is always local, and it
  carries no hunt take kit, so it only joins the ordinary-order pass, never the kit-carrying one.
  `MusterPlan::from_bench` is its share (`total()` includes it) and `musterable_pool` counts its
  crew. `MusterPricing::bench`, `HuntCrewGear::bench` and the forecast query's `MusterScene` carry
  it to every reader, so the board, the turn, the crew pricing and the craft suggestions agree.
- **Applying the bench's loan is non-destructive** (`apply_muster` / `restore_muster`): the bench's
  `workers` are cut for the row walk and handed back with the rows' — never `remove_order`, so the
  queue, every order's progress and made count, the drawn pile and the last grade all stay. The
  labor pass hands the crew back **before** `advance_crafting` runs (it follows the labor pass in
  the Population chain), so the loan is also written to `BandBench::lent_to_hunt`, and
  `advance_crafting` works `workers − lent_to_hunt` and clears it. That is what makes this turn's
  craft progress reflect the lent hands, and a fully lent bench bank nothing and draw nothing (the
  `AN_IDLE_BENCH` gate reads the reduced crew). The field is `0` at every turn boundary.
- **Kit-aware order** (`MusterKit`, struck by `hunt_by_need::muster_kit` from the band's own
  `BandItemBudget` on the rows' standing claims): an item of the need row's kit is *short* when the
  units no row claims (`BandItemBudget::unclaimed_units`) arm fewer hands than the crew. When one is,
  the donors that carry a short item and were settled units of it lend **first** (Priority order
  among them), only as far as the shortfall in kitted hands; then everyone in the ordinary order from
  what each still holds. Otherwise the order is Priority alone. Without it a harvest donor lent its
  hands and the crew claimed stalking kits off the shared budget, stripping boar hunters who kept
  their hands but lost their gear. `muster_donors` is still the one function the turn
  (`apply_muster`, claims struck before any hand is lent), the capture and the craft suggestions
  call. Pinned by `hunt_by_need::{when_the_hunts_kit_is_short_the_donor_holding_it_lends_first,
  when_the_kit_is_not_short_the_lowest_priority_lends_first, the_turn_lends_the_donors_the_board_named}`
  and `components::tests::a_short_kit_puts_the_donors_holding_it_first_up_to_the_shortfall`.
- **A going-out crew's kit shortage shows on the rows.** `apply_muster` crews the need row ahead of
  the turn's kit settlement, so while `muster_crew > 0` each take row's published `kitToe` is read
  off the mustered gear (`mustered_take_gear`, shared with the hunt-turn craft lines —
  `crafting.md` → "Craft suggestions"), and so is `kitWorkersHolding` (the same pass's coverage), so
  the two agree; a waiting row keeps the standing reading for both. Pinned by
  `hunt_by_need::a_crew_going_out_publishes_the_shortage_on_the_rows_and_a_waiting_row_does_not`.
- **A donor emptied for the turn is not lapsed**: the holding test asks `row_is_held` (hands before
  the loan), while the lesson test still asks who is actually there.
- **Priced over the gear the muster brings** (`hunt_by_need::MusterPricing`): a need row holds no
  hands, so a curve priced over the **free** units alone arms nobody on a band whose stalking kits
  are all claimed by its boar-hunt rows - and a quarry bare hands cannot wound (Marsh Grazers,
  defense 3) prices at useful crew 0, the hunt never goes out, and the kit-aware donor order is
  moot. A hunter lent from a boar row brings his kit, so `HuntCrewCurveInputs::muster` re-strikes the
  competing rows **at every crew size**: `MusterPricing::plan_at(c)` is `muster_kit` +
  `muster_donors` (idle first, kit-carrying donors first up to the shortfall), and
  `other_rows_at(c)` scales each lent row's claim by the share of its hands that stayed. The plan is a
  function of the crew and the *standing* claims alone - the reduced claims are derived from it, not
  fed back - so there is no fixed point. **One function, four readers**: the labor decision
  (`assess_need`: useful crew, meal-sized crew, `turns_to_kill`), the capture (`with_hunt_curve` for
  a need row, so `huntUsefulWorkers`/`musterCrew`/`turnsToKill` are the turn's), the hunt-turn craft
  pass (`hunt_turn_shortfall_lines`) and `apply_muster` (the donors actually lent) all call
  `MusterPricing`, so they cannot disagree about the gear. Ordinary rows leave `muster` `None` and
  price over their own claims as before. Pinned by
  `hunt_by_need::{a_quarry_bare_hands_cannot_wound_is_hunted_with_the_kit_the_donors_bring,
  the_sheet_prices_a_need_row_over_the_muster_when_it_asks_with_follow}` (both fail with the muster
  pricing switched off).
- **The Assign hunters sheet asks with `follow`** (`HuntCrewTakeQuery.follow`, proto field 7):
  the same curve for the same row. Without it the query prices an *ordinary* hunt row, which
  competes for gear at its Priority; a client composing a `follow` (need) row sends `follow = true`
  and `max_workers` = the musterable pool (`huntUsefulWorkers` is already priced over it), and the
  "Sends N" figure is then the row's own `musterCrew`.

**The wire** (`LaborAssignment`, appended after `moveWithHerd`; `PopulationCohortState`):

| field | meaning |
|---|---|
| `musterCrew` | the hands this need row sends next turn (`0` = waiting) |
| `lentToHunt` | on a donor row, the hands it gives up next turn (same `muster_donors` call) |
| `BenchState.lentToHunt` | the hands the crafting bench gives up next turn — the bench's share of that same call (`MusterPlan::from_bench`); `BenchState.workers` stays the crew the player set. Pinned on the encoded frame by `hunt_by_need::{the_bench_lends_by_its_mark_and_before_the_rows_of_its_tier, this_turns_craft_progress_reflects_the_lent_hands, a_lent_bench_keeps_its_order_progress_and_drawn_pile}` |
| `needRunway` | on a need row, the turns the meat on hand lasts (the need clock above); `0` elsewhere |
| `turnsUntilHunt` | on a waiting need row, `ceil(runway - turns_to_kill - margin)`, floor 1; `NO_HUNT_NEEDED` (`u32::MAX`) when the larder is not food-limited, no crew can be raised or the herd is out of camp range; `0` on a sending row |
| `turnsToKill` | turns until the animal is down at the planned crew, counting wounds; `0` when no crew can be raised |
| `killProgress` | `wounds.pending() / durability`, in `[0, 1)` |
| `PopulationCohortState.idleMustered` | the idle hands the muster takes next turn |

For a need row `huntUsefulWorkers` is priced over the musterable pool, not idle only. The native
decode keys are the snake_case of each (`muster_crew`, `lent_to_hunt`, `need_runway`, `turns_until_hunt`,
`turns_to_kill`, `kill_progress`, `idle_mustered`).

Tests: `tests/hunt_by_need.rs` (the full and the lean larder, the muster order and exactly what the
turn took against the published plan, the small band, the wound rule and the whole carcass, the far
herd, the save and the wire both ways), `components::tests` (the donor order and exclusions), and
`bin/server.rs` `a_follow_order_creates_the_need_row_at_zero_…` / `unfollowing_…` /
`a_move_order_ends_a_need_row`.

## The roaming skills — herd lore, travois, drives (#799)

Design: `docs/plan_roaming_bands.md` §"The roaming skills — decided". Three **faction knowledges** on
the discovery ledger, owned by `core_sim/src/roaming_skills.rs`. They are not ladder rungs — nothing
is built or held — so each has its **own practice signal**, the way `storage` is taught by rot, and
each is priced in `intensification_ladder.json` → `knowledge.lesson_costs` (all **20**, like every
lesson) and credited through `LadderKnowledge::ledger_credit` (practice ÷ cost), the one seam that
turns practice into ledger progress. Ids continue the sequence (2011 is retired, 2018 is storage):
`herd_lore` **2019**, `travois` **2020**, `drives` **2021**, resolved by `discovery_id_for` and
mirrored in `start_profile_knowledge_tags.json` (which grants none of them at the start).

| Skill | Taught by (quantum) | Effect |
|---|---|---|
| `herd_lore` | each hex a **resident** band steps in `advance_band_movement` while the herd its hunt row follows (`LaborAllocation::followed_herd`) is on a `RoamState::Migrate` leg — `learn_rate` per hex. A herd that loiters teaches nothing; a band under a plain move order is not following | `HerdTelemetryState.nextGroundX/Y` — see below |
| `travois` | a departure's long-move shed: `learn_rate × shed share`, share = shed load ÷ held load (`carry::shed_share`, both measured by `CarryLoad::load`), clamped `0..=1`, **once per departure** (the `!departed` branch). A move that sheds nothing teaches nothing | `carry.travois_carry_multiplier` on `per_worker_carry` — see `band-carry.md` |
| `drives` | each turn a crew works the herd its band follows (`move_with_herd`) **as a camp kill** — the same `crew_at_the_source && working_the_herd` gate as the hunt lesson, and the herd inside `band_work_range` (`work_party::party_begins_past`), not a posting. `learn_rate` per source per turn, never scaled by the crew | `hunt.drive_engage_gain` / `hunt.drive_full_head_count` on `herd_engage_rate` |

**Herd lore's field is the grounds the herd goes to next, in both phases.** `step_index` is the anchor
the herd is AT while it loiters and the anchor it LEFT while it migrates (arrival advances it), so
`route[(step_index + 1) % len]` ([`roaming_skills::next_ground`]) is the ground it leaves for after
the current loiter and, mid-leg, the destination it is already walking to. `nextX`/`nextY` stay the
leg's next single hex. The pair is `-1` unless the viewer's people know herd lore, and always `-1` for
a graze-wandering herd or a route of fewer than two anchors. It rides the herd row, so the row's own
fog gate (`herd_is_visible`) is the visibility gate; **the ground tile is deliberately not
fog-checked** — seeing past the fog to where a herd is going is the lesson. `HerdSnapshotInputs
.herd_lore_known` is resolved once per capture off the ledger.

**Drives is one multiplier on one seam.** `fauna::herd_engage_rate(herd, fauna, drives_known)` takes a
wild herd's rate × `1 + drive_engage_gain × min(head_count / drive_full_head_count, 1)`
(`roaming_skills::drive_engage_multiplier`; head count = `biomass / body_mass`). It is the wild arm
alone — a pen and a halter have their own reach gains. The flag travels on the party:
`HuntingParty::drives_known`, set from `PartyResolution::drives_known` (a **required** field, so a new
construction site must answer), and on `HuntCrewCurveInputs` / `HuntCrewLevers` / `ClaimSources` where
no party exists yet. `resolve_hunt_engagement`, `engagement_reach`, `hunt_useful_take_hands`,
`hunt_crew_needed`, the projections and `forecast_source_yield`'s `engage_rate` therefore read one
value; every construction site resolves it for ITS faction (`labor.rs` per band, `advance_expeditions`
off the home people, `forecast_query` / `seed_source_yield` off the asking band,
`take_claims::with_world_sources(world, faction, ..)`). The shipped `drive_full_head_count` **100** is
in heads: a migratory herd is 4,000–12,000 biomass, which at the light roster's body masses
(reindeer 20 … steppe runner 53) is roughly 75–600 head, while a mammoth herd (5–15 head) gains almost
nothing.

**The roster carries them as a branch of their own.** `LadderConfig::standalone_knowledges` (required)
files a knowledge no rung teaches under a wire `branch` token and an `area` (here `roam` / `reach`,
orders 1–3); `knowledge_roster()` appends them after the rung-taught ones with `is_step: false`, so
`ladderKnowledge` and `intensificationKnowledge` publish them with no client list. See
`intensification.md` → "Knowledge no rung teaches".

## The tests that carry the arc

| test | claim |
|---|---|
| `work_party::tests::an_eight_hex_source_walks_six_each_way`, `work_party_caravan::a_source_eight_hexes_out_walks_six_each_way` | Ray's formula, in the arithmetic and on the wire |
| `work_party::tests::the_steady_share_at_the_source_is_pack_over_pack_plus_the_round_trip` | `L / (L + 2·w·r)` within tolerance, with a liveness conjunct |
| `work_party::tests::more_hunters_land_the_first_load_sooner` | the first pack fills at the whole party's rate |
| `work_party::tests::a_road_shortens_the_walk_and_one_covering_the_run_takes_it_to_zero` | over a real road registry, through `free_pooling_reach_tiles` |
| `work_party::tests::departures_never_exceed_the_hunters_present` | a take wanting more packs than hunters leaves the rest in the load |
| `work_party::tests::a_carcass_heavier_than_one_pack_goes_home_over_several_porters` | the big carcass: nothing wasted |
| `work_party::tests::a_stood_down_party_walks_every_pack_and_the_load_home`, `…::a_party_walking_out_walks_back_what_it_covered` | `walk_home`'s groups: each porter's remaining walk, the load on the whole walk, and a load nobody is at the source to carry left behind |
| `work_party_caravan::a_cancelled_far_hunt_walks_every_pack_home_and_rots_it_by_its_walk` | the cancel lands nothing; the wire's homeward fields; hands rejoin `idleWorkers` only as their group arrives; last home after the whole walk; a 3-turn walk keeps, a 6-turn walk rots every flesh pack into `spoiled` |
| `work_party_caravan::a_herd_back_inside_the_apron_walks_its_caravan_home_once` | a re-entered source stands its caravan down once, clears `party`, publishes none on the row and the walk on the band |
| `work_party_caravan::a_vanished_herd_walks_its_caravan_home_as_the_row_lapses` | the row lapses that turn; its caravan outlives it and lands over the walk, booked `PartyHome`, once |
| `work_party_caravan::the_query_quotes_exactly_the_rate_the_row_publishes` | forecast == actual on the encoded snapshot |
| `work_party_caravan::a_far_kept_herds_caravan_forecast_is_what_its_party_lands_after_keeping` | a far kept herd: query == `netRateHome`, what lands over the horizon is that rate to within one landing, and the unkept price overshoots |
| `work_party_caravan::a_deposit_eight_hexes_out_posts_a_party_that_walks_six_each_way` | a far working posts a party on the wire, and nothing lands while it walks out |
| `work_party_caravan::a_local_working_takes_no_party_and_its_numbers_are_unchanged` | the deposit web's local identity: the take seam's own figure |
| `work_party_caravan::the_query_quotes_exactly_the_rate_an_extract_row_publishes` | forecast == actual on the deposit web, in material units |
| `work_party_caravan::unassigning_a_deposit_caravan_mid_walk_walks_every_pack_home_as_material` | every pack walks home into the store as wood, booked `PartyHome`; nothing on the larder, the food route arm or `homewardFood` |
| `work_party_caravan::a_woodcutting_crew_on_the_deadfall_floor_carries_a_larger_pack_than_bare_hands` | the first porter's pack is the pricing's haul carry over the weight — larger than bare hands |
| `work_party_caravan::a_far_felling_crew_with_the_woodcutting_kit_carries_the_sled_pack` | the kit is claimed whole, so a felling party's pack is the sled's haul over the weight, larger than bare |
| `work_party_caravan::a_local_extract_take_is_capped_by_carry_over_weight` | on a material heavy enough, a bare crew's cut is exactly its carry over the weight, and the sled raises it |
| `work_party_caravan::a_pack_is_the_haul_carry_over_the_materials_weight` | twice the weight, half the units per pack — the material's one number is the whole difference |
| `roaming_skills::walking_beside_a_migrating_herd_teaches_herd_lore_a_hex_at_a_time` and its two negatives (`following_a_loitering_herd_…`, `a_band_that_is_not_following_the_herd_…`) | herd lore: one `learn_rate` per hex stepped beside a `Migrate` herd, nothing for a loitering herd or an unfollowing band |
| `roaming_skills::a_departure_that_sheds_teaches_travois_once`, `a_long_move_that_sheds_nothing_teaches_no_travois`, `knowing_travois_sheds_less_and_lifts_the_published_pack` | travois: credited once per departure in proportion to the share shed, not at all when nothing is shed; a knowing band walks off with `travois_carry_multiplier` times the pack and the wire's `carryPerWorker` echoes it |
| `roaming_skills::camp_kills_on_a_followed_herd_teach_drives_a_turn_at_a_time` and its two negatives (`a_camp_kill_on_a_herd_nobody_follows_…`, `a_band_still_catching_up_to_the_herd_…`) | drives: one `learn_rate` per worked camp-kill turn on a followed herd; not on an unfollowed herd, not beyond `band_work_range` |
| `roaming_skills::the_next_ground_is_published_only_with_herd_lore_and_is_the_next_anchor_in_both_phases`, `…_wraps_the_route`, `a_graze_wandering_herd_…`, `a_herd_in_the_fog_…` | the encoded `nextGroundX/Y`: `-1` without the lesson, the next anchor with it in both phases, wrapping, absent for a graze-wanderer, no row in the fog |
| `roaming_skills::the_roaming_skills_ride_the_roster_as_a_branch_in_the_reach_area` | the three on the encoded roster as branch `roam`, area `reach`, `is_step` false, progress riding `intensificationKnowledge` |
| `fauna::tests::a_drive_scales_the_wild_reach_with_head_count_and_caps` | the engage gain is the identity unknown, scales with head count, caps at `drive_full_head_count`, and reaches `resolve_hunt_engagement` through the party |
