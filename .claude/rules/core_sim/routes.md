---
paths:
  - "core_sim/src/routes.rs"
  - "core_sim/tests/route_traffic.rs"
---

# Roads — the intensification ladder's third branch

Authoritative design: `docs/plan_standing_upkeep.md` §4.13, issue #532. The ladder engine itself is
`.claude/rules/core_sim/intensification.md`; the pooling this branch is worn in by and pays back to is
`campaign.md` → "Supply Network", and the edge it rides beside is `connections.md`.

## ⛔ A ROAD IS IN THE GROUND. IT DOES NOT FOLLOW THE CAMP.

**Both #532 and `plan_contact_and_logistics.md` §Q4 say a route "is an edge" that "belongs to a
connection". That is wrong, and both were corrected.** The shape it describes defines a route as
*"the road between band A and band B"* and re-derives its path each turn from wherever those two
bands currently stand — so the road **moves when the camp moves**.

Ray: *"How would a road follow a camp? That makes no sense and is in fact a factor in moving vs
staying. Roads can't follow camps."*

**The fault is not the unphysicality. A road that follows its camp deletes a decision.** A road you
paid to build and pay to hold is one of the strongest reasons the game can give you to *stay*; one
that packs up and comes with you costs nothing to leave, so it can never weigh on move/stay/fork —
the pillar the project is built around. The simpler implementation was buying its simplicity with the
decision the feature exists to create.

**So a `Route` is a world object with a fixed tile path and its own `RouteId`.** The band pair is who
*uses* it, never what it *is*. There is deliberately **no `RouteKey { low, high }`** and no band-pair
keying anywhere in `routes.rs`.

### The four rules, and each replaces a rule the band-pair shape needed

| | Rule | Seam | What it dissolves |
|---|---|---|---|
| **1** | A road's tiles are **stamped once**, from the path the first traffic walked, and never re-derived | `Route::path`, written by `RouteLedger::insert` | the re-stamp rule |
| **2** | A band is served by a road while **standing on one of its tiles** | `RouteLedger::routes_on_tile` | the *"close enough"* tolerance constant — the road's own path is the catchment, so **there is no radius** |
| **3** | A road nobody stands on earns no traffic, is claimed by no band, and reverts | the rungs' own `meter_decay` / `grace_turns` | the orphan case, and *"who pays for a road to nowhere"* — nobody does, and nobody needs to |
| **4** | New traffic **prefers an existing road** that already joins both ends | `RouteLedger::road_joining`, read by `advance_routes` | the near-duplicate-road swarm. It is also why real networks consolidate |

**A road is therefore a SHARED PUBLIC GOOD, which the band-pair shape could not express.** Camp A
leaves and camp C settles on the same ground: **C inherits the road**, because the road never belonged
to A.

**The accepted cost, named rather than softened: a band that steps one tile off its own road loses
it.** No radius is added to cushion that — a radius is precisely the constant rule 2 exists to avoid,
and *stay on your road* is the legible half of the same pillar.

## The build is paid by TRAFFIC, so a route takes no builder and no queue entry

*"A crew clears ground | **traffic wears the route in**."* Traffic **is** the crew, so:

- **no route rung declares a `verb`** — the branch adds no `Improvement` variant, and `RungKey::built_by`
  needed no new arm;
- **no `BuildQueueEntry`, and no draw on the `builders` pool.** `RungBranch::is_crew_built` is the one
  predicate that says so, and it is `false` for `Route` alone.

**That is what lets a road be owned by nobody**: the queue is the most band-shaped thing in the engine
(an entry names a destination on a source **that a band holds**, and the head takes every builder that
band has), so a road sitting in one band's queue would become that band's property.

**Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in, so *"what
does it cost to raise this"* has one answer in one unit whichever branch is asked.
`progress_per_turn` was retired arc-wide and does not come back for routes.

> ### ⛔ IT IS THE LINK, NOT THE TONNAGE — a correction to §4.13's own spec
>
> The design first specified the traffic rate in **mass-tiles** (quantity moved × distance). That is
> wrong for the commonest road in the game: `balance_supply_networks` drops sub-`min_transfer` moves,
> so a **balanced** network ships nothing — and a mass-driven rate would leave two neighbouring camps
> who have shared a larder for thirty turns with **no path at all**, which is exactly the case #532
> says must not be the one that produces no trail. A trail between two camps forms because they are
> neighbours who walk to each other, not because of what they happened to be carrying.
>
> So the lever is `intensification_ladder.json`'s `route_traffic.work_per_link_tile_per_turn` —
> **one** number, banked **per tile of road**, for every turn a link is live. Per tile, so a longer
> link banks proportionally more into the longer road it needs, which keeps a road's *pace* roughly
> independent of its length in the same way the span keeps its *cost* proportional to it.

**The lever lives on the LADDER rather than in a config file of its own**, for the reason the
knowledge pacing does: *"every knowledge pace in the game is tuned in ONE file"*, and this is the same
kind of number one branch over.

## The scale term — `length × terrain`, as ONE SUM

`UpkeepScale::RouteSpan` is the second variant, and the one the `scaled_by` key was held open for when
`Flat` was deleted.

```text
span = Σ over the road's tiles of infrastructure_cost(that tile's terrain)
```

**This is the first reader `TerrainDefinition::infrastructure_cost` has ever had** — authored for all
37 terrains and dead data until this branch. `routes::span_of_terrains` is the one definition of the
arithmetic and `route_span` is the ECS wrapper over it, so the bill, the quote and the decay cannot
read three different geometries.

- **A sum, not a product.** Length falls out of the *number* of terms and terrain out of each term's
  *value*, so *"length × terrain"* is a single `f32` through the same `RungUpkeep::scaled_by` seam both
  currencies already read, rather than two factors a caller multiplies in the right order.
- **Summed per tile crossed, NEVER averaged.** Three tiles of marsh cost three tiles of marsh;
  averaging prices a long road and a short one through the same country identically, which deletes the
  length half of the term outright. Pinned against **both** failure modes by
  `routes::tests::the_span_is_a_sum_over_the_tiles_crossed_and_never_an_average_or_a_tile_count`,
  because an average is right about terrain and blind to length while a bare tile count is the reverse
  — each passes the other's test.
- **The land is a SCALE term, not an offset** (§2.7): `infrastructure_cost` *multiplies* the demand.
- **The rung-monotonicity check needed no change, and that is the point.** It compares only adjacent
  rungs *sharing* a `scaled_by` — a rule §4.11 kept stated rather than simplifying away *"because it
  is what makes adding the route branch's scale safe"*.

## What a rung buys — and why it had to buy something

`infrastructure_cost` had zero readers and nothing anywhere read a route rung to reduce a cost.
**Shipping the dearer half alone builds a tax, not a ladder** — §4.9 item 12's named trap. The
consumer was already chosen and never wired: `SupplyNetworkConfig::reach_tiles`' own shipped doc
comment says *"beyond it a link needs a route to hold it open."*

| Term | What it answers | Where it is read |
|---|---|---|
| `RungRoutePayoff::friction_multiplier` | *how much of what is sent arrives?* Multiplies `SupplyNetworkConfig::friction` on a routed component | `balance_supply_networks`, via `routes::component_friction_multiplier` |
| `RungRoutePayoff::holds_link_to_tiles` | *may these two bands pool at all?* — the **capability**, and what makes the top rungs about distance | **not yet read** — it is 13b's, and the branch's own config states it |
| `Seen` along a kept road | *what can I watch?* | `Route::grants_sight` — **not yet wired to the visibility sweep**; 13a resolves the predicate, 13b's sibling grants on it |

**All are purely additive**, which is what preserves §Q4's *"no early-game regression, by
construction"*: an unrouted pair pools exactly as it does today, at exactly today's friction.
`supply_network::a_connected_network_moves_exactly_what_the_proximity_network_moved` is the pin, and
`route_traffic::a_kept_road_delivers_more_and_an_unrouted_network_is_untouched` is the paired one.

### ⛔ THE COMPONENT'S FRICTION IS THE BEST ROAD BINDING IT, AND THAT IS FORCED

`balance_commodity` pools a whole component against **one** friction scalar — there is no path model —
so the per-component reading is an approximation either way. **Best-road is the only approximation
that cannot regress.** Under a worst-road reading, wearing a *new* poor trail into an existing network
would **raise** that network's friction: a road would make things worse, and a band would be punished
for having walked somewhere. That breaks *"a rung can only widen the set of links and lower a loss,
never the reverse"* outright.

- **A road counts only if at least `MEMBERS_TO_CARRY_A_LINK` (2) members stand on it** (rule 2). A
  road one band happens to be camped on carries none of that component's pooling, and crediting it
  would pay a network for a road nobody is using to reach anyone. Pinned by
  `route_traffic::a_road_only_one_camp_stands_on_buys_that_network_nothing` — **a negative control
  that passes with the whole feature ripped out**, so it is only worth anything paired with the test
  above it.
- **And only if it is BUILT and KEPT** — the same `Route::grants_sight` condition, for the same
  reason: an unmaintained road is not carrying anything.

### ⛔ THE PAYOFF IS STAMPED ON THE ROAD, WHICH IS WHAT KEEPS THE LADDER OUT OF `supply.rs`

`Route::payoff` is derived and re-stamped on every write to the position, exactly as `Route::standing`
is. A supply pass that resolved it instead would take a `LadderConfigHandle` to re-derive a number the
road already knows — and **every harness that stands the pooling up would have to hand it one**. A
stamped reading has one producer, which is the rule the standing beside it is stamped under.

## The one-turn lag is the ledger's, and it is accepted for the ledger's reason

`balance_supply_networks` runs in `TurnStage::Logistics` and **`routes::advance_routes` runs after
it** — declared with `.after(supply::balance_supply_networks)`, not left to the ambiguity gate. Two
consequences, both deliberate:

- the route pass sees **this turn's** links, which is why the supply pass writes them to
  `RouteTrafficLog` rather than laying roads itself;
- the *payoff* is therefore read at each road's standing as of the **previous** turn — precisely the
  lag `balance_supply_networks` already accepts against `ConnectionLedger`.

**Do not reorder a stage for it, and do not let the supply pass raise a road**: that would make a
second producer of a rung's position, the failure this arc has had three of.

`RouteTrafficLog` is **drained** by the accrual (`std::mem::take`), so a turn with no pooling wears
nothing rather than re-wearing last turn's links.

## The four rungs

`game trail → trail → dirt road → paved road`, in `intensification_ladder.json`.

- **The game trail is the FLOOR** — `build: null`, `upkeep: null`, and a position of `RUNG_UNSTARTED`
  already holds it, exactly as `plant:wild` and `animal:wild` are floors. **Nobody maintains a game
  trail**, which is the whole of what makes the rung free — and why it lights no tiles, since
  `grants_sight` reads the *paid bill*. It is #215's origin: the first roads are the ones the animals
  made.
- **`partial_credit: continuous` on all three built rungs.** A half-worn trail is genuinely half a
  trail, unlike `animal:pen` where half a fence is not a fence.
- **`site_requirement: null` on every route rung**, and that is the honest answer rather than a gap: a
  `site_requirement` asks what the land *under this tile* must be, and a route does not stand on a
  tile. What a route asks of the land is **priced, not gated** — it is the `RouteSpan` term. **Do not
  invent a `route_requirement` sibling**; a rung asking nothing of its site is what every animal rung
  already ships.
- **The grace direction is the ANIMAL branch's** — the highest rung is the most forgiving, because the
  roadbed does the holding.
- **The three lessons** are `trailcraft` → `roadbuilding` → `paving` (discovery ids 2011–2013,
  `routes.rs`), priced in the ladder's own `lesson_costs` at the same 20 as every other.

### ⛔ NO ROUTE RUNG DECLARES A MATERIAL, AND THE REASON IS THAT THE MATERIAL DOES NOT EXIST

§4.13 has the paved road swallowing **stone** on both the pile and the rate — the material half of
§2.7 applied unchanged. It is not declared, because `stone` is not in `materials.json` (the roster is
bone / fibre / grape / hide / hurdles / tea / tobacco / wood) and the ladder's own load-time check
rightly rejects a rung naming a material the table does not carry: *"a rung that eats a material
nobody defines draws nothing, for ever, with no fault reported anywhere."*

**Adding a stone material with no way to obtain one would be worse than declaring none** — it ships a
rung that can **never be held**, which is a harder failure than one that is cheap to hold. Quarrying
is the crafting arc's. When a stone material lands, the rung takes `build.materials {stone: 30}` and
`upkeep.materials {stone: 0.08}` and nothing else changes: the engine half already ships.

## The keeping — the `Roadwork` pool, and the decay it pays for

They landed **together**, and had to: a decay with no pool to fund it reverts every road on the map,
and a pool with nothing to fund is a dial nothing reads.

`LaborTarget::Roadwork` is an ordinary band-wide standing role — `assign_labor <faction> <band>
roadwork <n>`, published as a `laborAssignments` row with `kind: "roadwork"`, shed by `normalize`,
checkpointed. Its TOE job is `KitJob::Roadwork` and `default_kits.roadwork` is the bare `none` kit,
so **road keepers work bare today**; that is intended, and the day a barrow declares a `build_work`
stat serving `route` the existing seam picks it up with no code change.

### ⛔ THE BILL IS STAMPED ON EVERY ROAD, NOT ONLY THE ONES A BAND STANDS ON

`systems::settle_route_keeping` runs in `TurnStage::Population`, `.after(advance_labor_allocation)`
and `.before(advance_crafting)` — both edges declared, not left to the ambiguity gate — and it does
two things in order:

1. **stamps the interpolated bill on every road in the ledger**, first-write-wins
   (`routes::route_upkeep_demand`, scaled by `route_span`);
2. **pays**, from every band whose `roadwork` row is staffed, against the roads under **its own
   tile** (`routes_on_tile` — rule 2, and there is no radius).

**Step 1 is the load-bearing half, and its scope is the trap.** `Route::keeping_is_met` answers
`true` for a road with **no stamped bill** — an honest *"it has not been judged this turn"* — so a
pass that stamped only where a band stands would leave an abandoned road reading as kept **for
ever**: never arming its neglect counter, never decaying, never pruned. That is rule 3 deleted, and
it fails as *no decay at all* rather than as a slow one. `route_traffic::a_road_no_band_stands_on_
decays_and_is_finally_forgotten` is the pin.

**The game trail falls out of the arithmetic rather than being branched around.** The floor declares
no `upkeep`, so `RungDef::upkeep_demand` answers `NO_UPKEEP_DEMAND` for it and a road holding only
the trail owes nothing. An `is_built()` guard would be a second statement of *"nobody maintains a
game trail"*, free to disagree with the ladder that already says it.

### A route keeper is funded exactly as a field or a flock keeper is

`route_keeping_claims` builds ordinary `KeepingClaim`s and hands them to the same
`keeping_rates` → `KeepingRate::worker_need` → `intensification::distribute_upkeep_pool` chain the
two food webs use, under the band's own `upkeep_fund_mode`. **There is deliberately no second supply
expression.** The one structural difference is that a claim carries no assignment index: a road is a
shared public good that no row of `assignments` names, so the index points at the claim's own id
vector instead. `Priority` funds most-invested first on the road's **position** — which *is* the
accumulator on this branch, so there is no separate stored cost to read — tie-broken on `RouteId`.

**`upkeep_supplied` accumulates (`+=`), never assigns**, because several bands may stand on one road
and each pays a part. Pinned by `route_traffic::two_bands_on_one_road_each_pay_a_part_of_its_bill`,
which is the only fixture in the file with two payers — every single-band fixture cannot tell the two
spellings apart.

### `advance_routes` is four phases, and the order is the whole of it

1. **judge last turn's keeping** — `upkeep_shortfall_fraction` off the **stamped** basis arms or
   wipes `Route::neglect_turns` (consecutive turns, never a lifetime budget);
2. **bleed the rung at risk** at `shortfall_fraction x meter_decay.per_turn`, past that rung's own
   `grace_turns`. `RungDef::upkeep_decay` owns both the rate and the strictly-greater comparison, so
   this system restates neither;
3. **clear** `upkeep_demanded` / `upkeep_supplied` for the coming turn's stamp;
4. **bank this turn's traffic**.

**`routes::route_at_risk_rung` is the one answer to *which rung is at risk*** — `standing.raising`
where anything is banked in it, else `standing.held` — because the bill interpolates through it, the
grace lookup asks it, and the decay bleeds it. Three readers that disagreed is exactly what
`forage::patch_unwinding_key` exists to prevent one branch over. It returns a rung rather than an
`Option`, unlike the plant web's: a route position always holds *something*, and that something
declares no upkeep at the floor.

**Then the ledger is PRUNED of every road back at `RUNG_UNSTARTED`, and the prune runs AFTER the
banking.** A game trail with no work in it is indistinguishable from no road — it buys nothing,
lights nothing and owes nothing — and an unbounded ledger of empty trails is a leak whose entries
still answer `routes_on_tile`. Pruning *before* phase 4 would delete every road on the turn it
formed, which is the whole feature. Remembering that animals once walked there is **#215's concern,
not this ledger's**.

**The one-turn carry is the arrangement and must not be "fixed".** Logistics runs before Population,
so the supply this pass judges was stamped by *last* turn's Population — the same lag
`forage::advance_cultivation` and `fauna::advance_husbandry` already run on.

### The shed takes a road keeper LAST of the three

`ShedStep::SpareKeeper` (step 3) and `ShedStep::NeededKeeper` (step 8) walk Agriculture, then
Husbandry, then Roadwork. **The reason is recoverability**: a road carries the longest graces on the
ladder, and a lost road is re-earned by **traffic alone** — the bands that walk it wear it back in
with no command typed and no crew staffed. A feral patch wants a `Cultivate` and the builders behind
it; a shed flock is gone.

`ShedFacts::spare_roadwork_keepers` is struck in `advance_labor_allocation` off the **same**
`route_keeping_claims` the payment uses, which is why that system now takes `Res<RouteLedger>`
read-only. Its bill is priced through `routes::route_keeping_basis` — the stamp where one exists,
the live demand where it does not — because the shed runs a whole system *before* anything is
stamped, and a count struck against a bill of zero would shed every road keeper as spare.

## What is NOT wired yet

Stated because a reader will otherwise look for it, and because `Route` carries the fields:

- **The visibility grant.** `Route::grants_sight` is resolved and read by the friction payoff; nothing
  hands it to the visibility sweep. When it is wired it must be **its own visibility source beside a
  band's own presence, never routed through the connection grant** — see the callout below.
- **The wire and the client.** No route field is published and the client has no route surface.
  `map_preview.gd`'s existing `"routes"` annotation state draws **order paths** and is a different
  thing; **do not reuse that name.**

## ⛔ A MAINTAINED ROAD IS TRAFFIC, SO ITS TILES ARE `Seen` — and the keystone is UNTOUCHED

`plan_contact_and_logistics.md` §Q5's rider row says a logistics rider's *"tiles stay `Seen` while
held"*, and §Q3 states the keystone as inviolable — *"Only presence makes a tile `Seen`. A connection
can only ever grant `Discovered`."* — while naming **logistics** as the first rider that will be
tempted to break it.

**The design pass first read this row as that temptation and proposed granting nothing. That was
wrong, and the inference ran backwards.** Ray: *"If a road exists and is maintained, the assumption is
that there is traffic on it and it is seen."* **Maintenance is not free** — a kept road bills a band
every turn out of the `Roadwork` pool, and what those hands are doing is being on the road. **Paying
the upkeep IS the presence.** A road nobody walks is a road nobody pays for, and rule 3 has it
reverting.

**So the keystone does not bend.** The sight is granted by the **road** — maintained presence on
specific ground — and **not by the connection**, which still grants `Discovered` and nothing else. A
band with a live tie to a people it has never travelled to sees exactly what it sees today, and
`core_sim/tests/connections.rs` passes unchanged.

**The condition is the PAID BILL, not the held rung**, so a road in shortfall **goes dark before it
decays** — the honest early warning that the road is being lost.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/intensification_ladder.json` | `route_traffic.work_per_link_tile_per_turn` (**0.35**) | **How fast traffic wears a road in**, in work units, per tile of road, per turn a link is live. **The link, not the tonnage** — see the callout above. At `0.35` a two-tile link between neighbouring camps banks `0.7` a turn and reaches the trail rung's 40 in about 57 turns of unbroken neighbourhood: a road you notice having made rather than one you decide to make. Validated finite and `> 0` (a rate of zero freezes the whole branch at its floor while reading like a live dial). **PLAYTEST DIAL**, §4.14 owns the number |
| `src/data/intensification_ladder.json` | the four `route` rungs | The branch itself — see "The four rungs". The `route_payoff` block is **required on every route rung and rejected on every other**: a route rung with a standing cost and no payoff is the *tax, not a ladder* failure, so its absence is a load failure rather than a default |

## Tests

`core_sim/src/routes.rs`'s own module — the ladder liveness (which every other claim in the file rests
on), the span's two failure modes, the bill, rules 2 and 4, the three sight states, and the traced
path. `core_sim/tests/route_traffic.rs` drives the three systems in **stage order** through real
turns (`balance_supply_networks` + `advance_routes` in Logistics, `settle_route_keeping` in
Population): a road forming under pooling nobody ordered, the friction payoff paired against an
unrouted run, the one-camp negative control, and the keeping — a road that holds beside the same road
that loses its rung, the proportional bleed, the grace, the abandoned road, the two payers, and the
free floor.

**Every live claim was falsified in isolation**: making the span an average, and ungating the sight
from the bill, each fail their own test; removing the friction term and removing the traffic recording
each fail theirs. On the keeping half — ignoring the grace, stamping the bill only where a band
stands, dropping the `+=` to an assignment, taking the decay at the flat rate instead of the shortfall
fraction, removing the prune, and reading the at-risk rung without the banked-work test — each fails a
different one, and the last fails six. A harness that stands `balance_supply_networks` up must insert `RouteLedger` and
`RouteTrafficLog` — five test files do, and an empty ledger is the shipped turn-1 state, which is what
makes those files' pooling numbers the **unrouted** reading they have always been.

## See Also

- `intensification.md` — the ladder engine, the rung grammar, `UpkeepScale`, and the standing upkeep
  this branch inherits rather than rebuilds
- `campaign.md` → "Supply Network" — the pooling that wears these roads in and reads their payoff
- `connections.md` — the edge beside this one, and the keystone the sight grant must not route through
- `docs/plan_standing_upkeep.md` §4.13 — the design, the three-step split, and the corrections to #532
  and `plan_contact_and_logistics.md` §Q4
