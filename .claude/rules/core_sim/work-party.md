---
paths:
  - "core_sim/src/work_party.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/supply.rs"
  - "core_sim/src/labor_config.rs"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/src/demographics_config.rs"
  - "core_sim/src/forecast_query.rs"
  - "core_sim/tests/labor_allocation.rs"
  - "core_sim/tests/work_party_caravan.rs"
---

# The work party — hunt, forage and extract are one model, and distance is a caravan

Design: `docs/plan_civilization_steps.md` §One work party. Engine: `core_sim/src/work_party.rs` (the
caravan's state and per-turn rule, the walk, the forecast, the pricing) plus the posting seam in
`core_sim/src/systems/labor.rs`, the assign-time seed in `bin/server.rs` and the compose-sheet query
in `forecast_query.rs`. **A deposit (wood, stone) posts a party by exactly the rule a patch does** —
see "The deposit web: the cargo is the material".

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
for every job**. Hunt gets no longer apron than forage: the retired `hunt_reach` was a patch over the
wrong model, and a party that follows its herd never roams out of range.

Pinned by `labor_allocation::a_local_row_takes_no_party_and_its_whole_take_reaches_the_larder`
(the published `actual` against the larder credit in **fixed point**, with a liveness assertion) and
`::a_hunt_inside_the_old_leash_posts_a_party_on_the_same_apron_as_forage`, which stages a herd
strictly between the two old thresholds — the only distance at which the two readings disagree.

> #### ⛔ `hunt_reach` IS DEAD, AND IT IS STILL ON THE WIRE
>
> `PopulationCohortState` still publishes `hunt_reach` (`band_work_range + hunt_leash_tiles`), and
> `hunt_leash_tiles` is still a validated config key — both survive only until the expedition path
> they also served is retired. **No reader may decide anything by either.** A client that routes a
> herd past `hunt_reach` to the expedition sheet, or refuses a forage source past the work range, is
> exactly the defect the first playtest of this arc found: the work party was unreachable from the
> map. The only threshold a caravan has is `band_work_range`.

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
> long walk is spoilage, which is the storage arc's term, not this one.

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
its own inline take, the forecast around a projected one (`WorkParty::step`). Order:

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
animal the pack cannot seat whole is still killed whole, and a resident band walks away from the rest.
**A party walks away from nothing**: a carcass too big for one pack goes a pack's worth at a time and
the remainder stays in the load for the next porter. So a far hunt's take is `take.killed_biomass()`,
not `take.carried`, and its row's `wasted` is `0`. An unbounded carry (a pen that is a larder) takes
the whole load in one pack.

**On the two food webs only the FOOD account travels.** Fodder and the hide, bone and fibre a take
yields are credited to the band as they always were: a batch carries a characteristic vector and a
band key, and a pipe over those is the storage arc's. Standing yield (milk) is food with no biomass:
it rides the load with the next pack. **On the deposit web the material IS the cargo** — see below.

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
the pass, the unassign, a lapsing row, the unposted sweep, the shed, `bring_the_dropped_party_home`.
**The row's food projections are not written for a material cargo** (`CargoHome::is_food` gates
`publish_caravan_projection`): its rate home rides `netRateHome` alone, in material units per turn.

## ⛔ One function, stepped — shared by the turn, the seed and the query

`work_party::forecast_caravan` steps a party forward `yield_average_horizon_turns` from a state,
through `WorkParty::step` and a projected take at whatever crew is present each turn. The projected
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
declines no far row on any web, and its Hunt gate no longer reads the retired `hunt_reach()`. A far
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
| `netRateHome` | cargo per turn arriving home — the number the row prints: food on a hunt or forage row, the material's own units on an extract row (the row's `kind` says which) |

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
hands it spends keeping), appended (proto 7 / 8). They are what the compose sheet previews a crew of
`n` on every web, rather than pricing every worker as a taker; pinned on a patch by
`forage_cultivation::a_kept_patchs_next_turn_take_is_quoted_on_the_hands_its_keeping_leaves`.

**`ForageCrewTake`** (proto query **9**, reply **12**) is the patch's whole crew curve in one round
trip — the hunt and deposit curves' shape: one `ForageCrewTakeRow{workers, take, keep_hands}` per
crew `1..=max_workers`, each row **the work-party forecast's own `take_next_turn` / `keep_hands` at
that crew** (`forecast_query::answer_forage_crew_take` asks it per crew), so the stepper and a
one-crew quote are one arithmetic. Seat-gated like the other faction-bearing questions. Pinned by
`forage_cultivation::a_patchs_crew_curve_is_the_single_crew_answer_at_every_size`.

## Every exit brings everything home, through ONE settle step

**A caravan that ends early must not lose what is on the road.** `systems::stand_down_party` is the
one settle step — `WorkParty::hand_over_everything` into `bring_the_party_home` — and every path that
ends a posting routes through it:

- **A lapsing row** — a holding with nothing left to hold; the row ends with `status=lapsed` and its
  caravan comes home before the row is removed.
- **A source that stopped posting** — the herd drifted back inside `band_work_range` (or the band
  moved up to it), or the herd **left the registry** (`status=lapsed reason=herd_gone`). Neither turn
  posts a party, so neither reaches the per-posting settlement; the foot of the pass sweeps every row
  still carrying a party that posted nothing this turn, settles it, and clears `party` — **before**
  the `lapsed` removal, which would otherwise drop the row with its caravan on the road. A re-entered
  row is plainly local afterwards and publishes no party field.
- **Unassign** — a row held at zero hands has nobody at the source and nobody to send, so the caravan
  is brought home and the party stood down (the row survives as a holding if it holds anything). A
  zero-crew row that posts nothing is caught by the sweep above.
- **Abandon / a zero-crew drop** — `LaborAllocation::drop_source_row` returns the row it removed, and
  `systems::bring_the_dropped_party_home` settles its party.
- **The starvation shed** — `LaborAllocation::normalize` holds no larder, so the labor pass reads the
  parties off the rows before the walk and settles the party of every row the shed drops outright.
- **`cancel_order`** — `clear_kinds` holds no larder either, so `handle_cancel_order` reads the rows it
  is about to clear and brings each one's party home through `bring_the_dropped_party_home`.

> #### ⛔ CARGO HANDED OVER ON THE WAY OUT GOES ON THE LEDGER'S ROUTE ARM
>
> It is not this turn's income — a row that is ending publishes no telemetry to count it in — and food
> that reached the larder through neither `food_income` nor a transfer would break the pinned identity
> `larder_delta == food_income − food_consumption − raid_forfeit + transfer_received − transfer_sent`.
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

Pinned by `work_party_caravan::unassigning_a_caravan_mid_walk_brings_every_pack_home`,
`::a_herd_back_inside_the_apron_brings_its_caravan_home_once` and
`::a_vanished_herd_brings_its_caravan_home_as_the_row_lapses`, each on both the larder and the route
arm.

## `BandReach` no longer asks about distance

It used to carry the band's position and the two lapse distances, because the arms abandoned an
out-of-reach row on the same `continue` every keeping draw and material spend sat beneath. Nothing
lapses for distance now, so the only thing left that makes an arm skip is a herd the registry no
longer carries. The type survives rather than collapsing into a bare `registry.find`, because *"will
the arm reach this row"* is the question the settlements must go on asking.

## The tests that carry the arc

| test | claim |
|---|---|
| `work_party::tests::an_eight_hex_source_walks_six_each_way`, `work_party_caravan::a_source_eight_hexes_out_walks_six_each_way` | Ray's formula, in the arithmetic and on the wire |
| `work_party::tests::the_steady_share_at_the_source_is_pack_over_pack_plus_the_round_trip` | `L / (L + 2·w·r)` within tolerance, with a liveness conjunct |
| `work_party::tests::more_hunters_land_the_first_load_sooner` | the first pack fills at the whole party's rate |
| `work_party::tests::a_road_shortens_the_walk_and_one_covering_the_run_takes_it_to_zero` | over a real road registry, through `free_pooling_reach_tiles` |
| `work_party::tests::departures_never_exceed_the_hunters_present` | a take wanting more packs than hunters leaves the rest in the load |
| `work_party::tests::a_carcass_heavier_than_one_pack_goes_home_over_several_porters` | the big carcass: nothing wasted that the resident take would waste |
| `work_party_caravan::unassigning_a_caravan_mid_walk_brings_every_pack_home` | the road comes home, on the larder and the route arm |
| `work_party_caravan::a_herd_back_inside_the_apron_brings_its_caravan_home_once` | a re-entered source settles its caravan once, clears `party` and publishes none |
| `work_party_caravan::a_vanished_herd_brings_its_caravan_home_as_the_row_lapses` | a vanished herd's caravan comes home once, before the row lapses |
| `work_party_caravan::the_query_quotes_exactly_the_rate_the_row_publishes` | forecast == actual on the encoded snapshot |
| `work_party_caravan::a_far_kept_herds_caravan_forecast_is_what_its_party_lands_after_keeping` | a far kept herd: query == `netRateHome`, what lands over the horizon is that rate to within one landing, and the unkept price overshoots |
| `work_party_caravan::a_deposit_eight_hexes_out_posts_a_party_that_walks_six_each_way` | a far working posts a party on the wire, and nothing lands while it walks out |
| `work_party_caravan::a_local_working_takes_no_party_and_its_numbers_are_unchanged` | the deposit web's local identity: the take seam's own figure |
| `work_party_caravan::the_query_quotes_exactly_the_rate_an_extract_row_publishes` | forecast == actual on the deposit web, in material units |
| `work_party_caravan::unassigning_a_deposit_caravan_mid_walk_brings_every_pack_home_as_material` | every pack lands in the store as wood, booked `PartyHome`, and nothing on the larder or the food route arm |
| `work_party_caravan::a_woodcutting_crew_on_the_deadfall_floor_carries_a_larger_pack_than_bare_hands` | the first porter's pack is the pricing's haul carry over the weight — larger than bare hands |
| `work_party_caravan::a_far_felling_crew_with_the_woodcutting_kit_carries_the_sled_pack` | the kit is claimed whole, so a felling party's pack is the sled's haul over the weight, larger than bare |
| `work_party_caravan::a_local_extract_take_is_capped_by_carry_over_weight` | on a material heavy enough, a bare crew's cut is exactly its carry over the weight, and the sled raises it |
| `work_party_caravan::a_pack_is_the_haul_carry_over_the_materials_weight` | twice the weight, half the units per pack — the material's one number is the whole difference |
