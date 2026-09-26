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

# The work party — hunt and forage are one model, and distance is a caravan

Design: `docs/plan_civilization_steps.md` §One work party. Engine: `core_sim/src/work_party.rs` (the
caravan's state and per-turn rule, the walk, the forecast, the pricing) plus the posting seam in
`core_sim/src/systems/labor.rs`, the assign-time seed in `bin/server.rs` and the compose-sheet query
in `forecast_query.rs`.

## Config files

| File | Purpose |
|------|---------|
| `src/data/labor_config.json` | `band_work_range` (**2**) is the apron: past it every job posts a party, and the walk is measured from it. `band_move_tiles_per_turn` (**1**) is read a second time as the party's walking speed — the walk out and every porter's walk home — and is validated `>= 1` for that reason. **No lever of this arc's own exists**: the share of a party on the road falls out of carry, take rate and distance |
| `src/data/supply_network_config.json` | Read, not written: `reach_tiles` is subtracted from `supply::free_pooling_reach_tiles` to give the **road bonus** — how much of a walk a road takes away. `friction` is **not** read: distance is paid in walking |
| `src/data/demographics_config.json` | Read, not written: `consumption.per_capita_draw × consumption.working_factor` is the party's upkeep, through the named seam `DemographicsConsumption::worker_draw` |

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
3. **Take with the hunters PRESENT** (`workers − on the road`) through the **ordinary take path** —
   every arm the resident take runs. There is no caravan-specific take formula.
4. **The party eats first, from the take, and the eaten share is credited HOME at once.** Only the
   surplus goes into the load, so a party on thin game eats most of its take and walks little home.
5. **Fill and dispatch.** While the load holds one pack and a hunter is present, one hunter leaves
   with one pack. Departures therefore never exceed the hunters present.

> ### ⛔ THE EATEN SHARE IS CREDITED HOME — it is not a second meal
>
> The band's population consumption already feeds these workers: they never left the cohort, and
> `simulate_population` charges the whole `working` bracket. Food the party ate **at the source** is
> food the band did not have to carry out, not food that vanished. Netting it out again would bill the
> band twice for one meal. What the caravan changes is only its consequence: the eaten share never
> enters the load, so it is never walked.

**One pack is one hunter's carry, seated by the web's own rule** — `fauna::one_pack_biomass` on the
animal web (whole animals, **rounded down**), continuous on the plant web. It is deliberately **not**
`fauna::animals_the_pack_seats`, which rounds **up** because it answers the kill-stop question — the
animal the pack cannot seat whole is still killed whole, and a resident band walks away from the rest.
**A party walks away from nothing**: a carcass too big for one pack goes a pack's worth at a time and
the remainder stays in the load for the next porter. So a far hunt's take is `take.killed_biomass()`,
not `take.carried`, and its row's `wasted` is `0`. An unbounded carry (a pen that is a larder) takes
the whole load in one pack.

**Only the FOOD account travels.** Fodder and material batches are credited to the band as they
always were: a batch carries a characteristic vector and a band key, and a pipe over those is the
storage arc's. Standing yield (milk) is food with no biomass: it rides the load with the next pack.

**The deficit is stamped when the party is posted**, and the take site settles it down. An arm that
returns before its take site still closes the turn on a zero take, so the party still eats and a
deficit is never left at zero — the one reading that must never be assumed.

**A crew at the source is what earns a lesson** (`crew_at_the_source`, the hunters present), while
the holding test asks what the player **staffed** (`take_crew_present`). Read the holding test off the
hunters present and a party with every hand on the road would retire its own row silently; read the
lesson off the staffed crew and a party walking out would learn at a source it has not reached.

### ⛔ The take flows HOME, always — and the feeding is the home band's too

To the band that owns the row, **never** to whichever band the party is standing beside. A party is an
*extension of its home band*, not a peer node in the supply network, in **either** direction:
`balance_supply_networks` has no party awareness at all, and a party's position never enters the
union-find. The home band feeds it, because the party's people never left the home band's cohort —
cost and benefit have one owner. Making the party a network node "so it can be fed" re-introduces the
bug the take's rule forbids, one direction over.

Pinned by `labor_allocation::a_partys_take_is_credited_to_its_home_band_not_to_the_band_beside_it`.

## ⛔ One function, stepped — shared by the turn, the seed and the query

`work_party::forecast_caravan` steps a party forward `yield_average_horizon_turns` from a state,
through `WorkParty::step` and a projected take at whatever crew is present each turn. The projected
take is the smooth headline's own step — `fauna::HuntProjection::step` and
`forage::ForageProjection::step`, which `project_realized_hunt` / `project_realized_forage` are loops
over — so the forecast runs the hunt's and the gather's own projection, not a second copy.
`forecast_hunt_caravan` / `forecast_forage_caravan` are the two web adapters. The run stops once the
source is spent **and** nothing is on the road or in the load, and averages over the turns stepped.

It is read in three places, and must stay one function:

| reader | where | what it publishes |
|---|---|---|
| the turn | the take site, from the state the turn leaves | the row's `netRateHome`, `realized` and arrival schedule (`publish_caravan_projection`) |
| the assign-time seed | `bin/server.rs::seed_source_yield` | the same three, plus `actual` = what lands next turn (`0` while walking out) |
| the compose-sheet query | `forecast_query::answer_work_party_forecast` | `rate_home`, the walk, the mean hunters on the road, the first landing, the mean `deficit` |

**The row's projections are what arrives home, not what is taken.** `realized` is the headline the
food runway and the work board read, so a far row publishing its gross take would promise a larder
food still being eaten at the source or walking home. The published split `meat + standing == actual`
is kept by scaling the two parts onto the row's `actual` in their own proportion.

**Priced at the row's STAFFED crew** (`work_party::CaravanPricing`), not at the hunters present this
turn: the forecast steps a crew that moves every turn, and neither the seed nor the query knows who is
on the road now, so the row's own head count off the band's share of its gear
(`BandItemBudget::with_prospective_row` beside its other rows) is the one input all three resolve
identically. The turn's *take* is still priced at the hunters present.

**The seed and the query step the band's STANDING party when it has one**, restamped for the crew
asked about — a stepper press on a live posting re-seeds that posting, not a fresh one that would
re-promise a walk out. With none, they step a party posted now. **Inside the apron the query answers
the ordinary local row's steady rate** with `posts_a_party: false` and every walk field `0`. The seed
no longer declines a far Hunt or Forage row, and its Hunt gate no longer reads the retired
`hunt_reach()`; **Extract keeps its range gate**, because a working still lapses past range.

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
| `partyAte` / `partyDeficit` | the eaten share (credited home) and the upkeep still wanted |
| `netRateHome` | food per turn arriving home — the number the row prints |

The query is `QueryPayload::WorkPartyForecast` (`sim_runtime`, proto query field 7, reply field 10),
seat-gated like the other faction-bearing questions: band, `Hunt { herd_id }` or `Forage { x, y,
take_species }`, kit (named, never defaulted), crew and floor. It is refused with a `query_error`
token for an unknown band, herd (`unknown_herd`) or patch (`unknown_patch`), an unknown or wrong-job
kit, an invalid floor or an oversized crew.

**The reply** is `posts_a_party`, `rate_home`, `walk_tiles`, `walk_turns`, `hunters_on_the_road` (a
mean, so a float), `first_load_turn` (1-based, `0` = none within the horizon) and **`deficit`** — every
walk field and the deficit read `0` inside the apron.

> #### ⛔ `deficit` IS WHAT LETS THE SHEET WARN BEFORE THE ROW DOES
>
> A small party on thin game eats its whole take: no pack ever fills, and the committed row prints
> its `partyDeficit` as food the home larder must send every turn. Without the figure on the reply the
> compose sheet had no way to say so before the player committed — reported from play on a
> three-hunter boar sheet (`0.17` food a turn taken against `0.48` of upkeep).
>
> It is `CaravanForecast::mean_deficit` — `WorkParty::deficit` averaged over the **same** turns and
> the **same** stepping `rate_home` is, never recomputed from a rate. It is **not** bit-equal to the
> row's published `partyDeficit`, and cannot be: the row's figure is *this turn's* shortfall against
> the whole-animal take, the reply's is the horizon mean through the smooth projection every forecast
> uses. At a steady footing the two differ by the float noise between a quantised take and its
> expectation (measured `2.7e-7` food), which `work_party_caravan`'s `DEFICIT_TOLERANCE` bounds. A
> posting still walking out averages its walk-out turns in, where the party eats and takes nothing,
> so its mean reads **higher** than the row's first figures — the honest reading for a sheet quoting
> a posting that has not yet reached its source.

Pinned by `work_party_caravan::a_thin_take_quotes_the_deficit_the_row_will_publish` (a genuinely
positive deficit, at the shipped draw, with nothing on the road and liveness on both sides) and, for
the surplus case, the same comparison inside `::the_query_quotes_exactly_the_rate_the_row_publishes`.

## Fold-back, unassign, abandon — everything comes home

**A caravan that ends early must not lose what is on the road.** `WorkParty::hand_over_everything`
settles the load and every walker's pack into the band on all three exits:

- **Unsupplied fold-back** — the deficit must be coverable from the home larder as the pass opened
  (`work_party::larder_supplies`, `larder >= deficit`; **not** grossed up by any transit loss). Failing
  it folds the row back and says so on the source's own feed channel:
  `status=recalled reason=unsupplied {x= y=|fauna=} walk= deficit= band=` — **Notable, not Alert**:
  nothing was destroyed and the pack came home.
- **Unassign** — a row held at zero hands has nobody at the source and nobody to send, so the caravan
  is brought home and the party stood down (the row survives as a holding if it holds anything).
- **Abandon / a zero-crew drop** — `LaborAllocation::drop_source_row` returns the row it removed, and
  `systems::bring_the_dropped_party_home` settles its party.

> #### ⛔ FOOD HANDED OVER ON THE WAY OUT GOES ON THE LEDGER'S ROUTE ARM
>
> It is not this turn's income — a row that is ending publishes no telemetry to count it in — and food
> that reached the larder through neither `food_income` nor a transfer would break the pinned identity
> `larder_delta == food_income − food_consumption − raid_forfeit + transfer_received − transfer_sent`.
> A party carrying goods home is exactly what `TransferLink::Route` is for, so `bring_the_party_home`
> credits it there. Food a **live** posting lands (the eaten share, a delivered pack) goes through the
> row's `actual` like any other take.

Pinned by `work_party_caravan::unassigning_a_caravan_mid_walk_brings_every_pack_home`, on both the
larder and the route arm.

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
| `work_party_caravan::the_query_quotes_exactly_the_rate_the_row_publishes` | forecast == actual on the encoded snapshot |
| `work_party_caravan::a_thin_take_quotes_the_deficit_the_row_will_publish` | the sheet's `deficit` is the row's `partyDeficit`, genuinely positive |
