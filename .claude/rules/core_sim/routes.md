---
paths:
  - "core_sim/src/routes.rs"
  - "core_sim/src/snapshot/routes.rs"
  - "core_sim/src/visibility_systems.rs"
  - "core_sim/tests/route_traffic.rs"
  - "core_sim/tests/route_sight.rs"
  - "core_sim/tests/route_wire.rs"
  - "sim_schema/src/state/routes.rs"
  - "sim_schema/src/codec/routes.rs"
---

# Roads — the intensification ladder's third branch

Authoritative design: `docs/plan_standing_upkeep.md` §4.13, and **§4.13b for the model this file
describes**; issue #532. The ladder engine itself is `.claude/rules/core_sim/intensification.md`; the
pooling this branch is worn in by and pays back to is `campaign.md` → "Supply Network", and the edge
it rides beside is `connections.md`.

## ⛔ A ROAD IS A **TILE** IMPROVEMENT, STRUCTURALLY IDENTICAL TO A FORAGE PATCH

Ray: *"a road is a single tile improvement, not the entire path, so one band could maintain 1/2 the
tile roads for the distance of a connection between two bands and another the other 1/2."*

**A path object cannot be half-maintained.** The model this replaced stored a road as one object
holding a `Vec<UVec2>`, one ladder position and one keeping bill for the whole run, and there was no
way to write down *these people look after this end and those people look after that end* — the
ordinary case the moment two camps sit at either end of a long road.

So each tile carries its own rung, its own meter, its own keeper and its own decay, held in
`RoadRegistry` — a `BTreeMap` keyed `(y, x)`, `forage::ForageRegistry`'s shape one branch over.
`routes::trace_path` **survives as a function** and is no longer state.

### Three objects were running together, and only the third was removed

Ray, on the risk: *"Connections between bands (i.e. supply depots) will probably be important later
in the game for logistics… just want to make sure you won't remove something that we need."*

| | what it is | status |
|---|---|---|
| **connection** | two bands know each other, this much (`connections.rs`) | **shipped, untouched** |
| **logistics link** | goods move between them; what pooling rides (`supply.rs`) | **shipped, untouched** |
| **road** | a built, maintained improvement **on one tile** (`routes.rs`) | the model here |
| **link quality** | how good the going is between two camps | **DERIVED each turn** by reading the road tiles along the way — never stored |

**A link already knows its two endpoints, so the tiles between them are computable.** Ray's phrasing
is the right one — *"a route projected onto the tiles"*: the **link** is the object, the **roads** are
the ground it runs over.

**A depot does not exist**, and is not folded into either. Neither a connection nor a road is *a place
goods sit*; that is a third object nothing here provides.

## ⛔ A ROAD IS IN THE GROUND. IT DOES NOT FOLLOW THE CAMP.

Ray: *"How would a road follow a camp? That makes no sense and is in fact a factor in moving vs
staying. Roads can't follow camps."*

**The fault is not the unphysicality. A road that follows its camp deletes a decision.** A road you
paid to build and pay to hold is one of the strongest reasons the game can give you to *stay*; one
that packs up and comes with you costs nothing to leave, so it can never weigh on move/stay/fork —
the pillar the project is built around.

## Who keeps a road: the band that built it, and nobody else

| | | seam |
|---|---|---|
| **1** | **A trail has no keeper**, and that is fine because there is nothing to keep. The free floor costs nothing, is formed by use, and is lost to disuse | `Road::keeper` is `None` across the whole free floor |
| **2** | **`grade` / `pave` make the tile that band's job** — the same act `cultivate` performs on a patch. **ONE KEEPER, NO SHARES** | `Road::take_keeper`, written by the command |
| **3** | **Distance raises the cost; it never forbids the road** | `remoteness_multiplier` |

**§4.13a's *"several bands each pay a share and the contributions add together"* is RETIRED** — Ray:
*"Don't agree with at all"*. Per tile it is **unrepresentable** rather than merely discouraged: there
is one `keeper` field, the verb refuses a tile another band keeps, and `RoadRegistry::kept_by` is what
the `Roadwork` pool pays against.

**When the keeping band is gone** the road has no keeper, owes its bill to nobody and decays like any
unkept improvement. **Re-issuing `grade` / `pave` is how another band picks it up — no new verb**,
because adoption is the same act as building. That is why the *"already at that rung"* refusal is
scoped to a road **this band already keeps**: a keeperless dirt road is a road to pick up, not a job
already done.

### ⛔ THE WORD **OWNERSHIP** IS RETIRED FROM THIS ARC

Ray: *"A road could have no owner, but that is an abstract term in this game so it really has no
meaning."* There is no owning. There is a **job** — *these people look after this tile's road* — and
the question *"who owns a road"* should not be asked again.

### DEFERRED, AND IT IS ITS OWN ISSUE: INFRASTRUCTURE ABOVE THE BAND

**Issue #598.** A faction-level authority maintaining roads beyond any single band's reach is more
realistic than a per-band keeper and is *"a whole entire mechanism in the game we don't have"* — a
central government. Explicitly out of this arc; the per-band keeper is this arc's answer rather than
the final one.

## ⛔ DISTANCE IS A COST, NEVER A WALL — and the range is READ THROUGH A FUNCTION

**There is no work-range rule on this branch, and the reason is not complexity.** Ray: *"already
forage and hunting have different work ranges, expeditions are even farther. I don't think it makes
sense to restrict it."* A fourth arbitrary radius would say nothing. What bounds a distant road is
that it is **dearer to hold and slower to build** — the argument `TradeExpeditionConfig` already makes
about friction, *"what a long haul costs is already paid, and paid in the right currency."*

**A THRESHOLD, NOT A CURVE.** Within `routes::road_keeping_range` a road costs what the rung says;
beyond it both the build pile and the standing upkeep are multiplied by
`route_range.remote_cost_multiplier`.

> ### ⛔ EVERY CALLER ASKS `road_keeping_range`, AND NOBODY READS `cfg.base_tiles`
>
> Ray: *"Be flexible on the threshold… make it a function that can expand over time, don't just
> create a hardcoded constant. You can have a configuration item for the 'base' range, but still make
> a function accessor for it so we can calculate it later."*
>
> So the config holds a **base** and the seam is the answer. The day the range grows with knowledge,
> faction size or a central authority (**#598**), that is *one function body* changing and **no call
> site moving** — where a `cfg.base_tiles` read scattered across the build cost, the upkeep, the
> command's quote and the wire is four places to find and three to miss. It is the same discipline
> `fauna::herd_ecology` and `forage::patch_land_capacity` are seams for.

**The quote is taken once, when the keeper takes the road on** (`Road::keeper_remoteness`), and held
for the whole job — `ForagePatch::field_cost_multiplier`'s discipline. Read live it would move the
rung boundaries under a half-built road every time the band took a step, which is a second producer of
a standing. **Re-issuing the verb re-prices it**, which is also how adoption works.

**The free floor is never re-priced** (`road_rung_cost` applies the multiplier only at
`FIRST_BUILT_RUNG` and above): traffic wears a trail in, and traffic does not care how far anybody's
camp is. That is also what keeps `traffic_ceiling` one number for every road on the map.

## ⛔ TRAFFIC PAYS FOR THE FLOOR, AND IT STOPS AT THE TOP OF IT

| | free floor, formed by use | built, and paid for |
|---|---|---|
| `plant` | wild | tended · field |
| `animal` | wild | pastoral · pen |
| `route` | **path · trail** | **dirt road · paved road** |

The two free rungs declare no `verb`, append no `BuildQueueEntry`, draw nothing from the builders'
pool and are billed nothing for holding. **Traffic banks work up to `routes::traffic_ceiling` — the
top of `FREE_FLOOR_TOP_RUNG` — and no further**, capped in `advance_roads`.

> ### ⛔ THE CAP IS THE LOAD-BEARING HALF, NOT WHERE THE LINE SITS
>
> 13a billed `route:trail`, so two camps sharing a larder wore a trail in by themselves and the band
> acquired **a standing labour bill it never opted into** — reported from play as a 7-worker nomadic
> band showing `Roadwork ⚠` with no road it had chosen.
>
> **Moving the line without capping the climb only relocates that fault.** Traffic would go on wearing
> a *dirt road* in for free and hand the player its bill anyway, one rung later and dearer.

**`Road::is_built()` is therefore a rung test** — `is_at_or_above(FIRST_BUILT_RUNG)`, not
`!= path`. Its one consumer, `Road::grants_sight`, reasons from *"paying the upkeep IS the
presence"*, so a free trail must light nothing however worn it is. `FREE_FLOOR_TOP_RUNG` and
`FIRST_BUILT_RUNG` are pinned adjacent by `the_free_floor_and_the_first_built_rung_are_adjacent`,
because `RungKey::above` is not `const`.

### ⛔ `RungBranch::is_crew_built` IS DELETED, AND ITS REPLACEMENT IS FINER

It answered *"does a band's `builders` pool raise this branch's rungs"* — `false` for `Route` alone.
With `grade` and `pave` on the builders' pool the branch is **no longer uniformly crew-free**, so a
branch-level answer is the wrong grain and would be wrong for half the rungs it covered.

***"Does this rung declare a `verb`"*** is the same question at the grain that can answer it, and
every former caller was asking about a **rung**: `RungDef::verb` / `RungKey::builder_verb`. The
`debug_assert` inside `BuildersGear::on` went with it, and the branch gained a real (deliberately
empty) gear reading — see "The build" below.

### Traffic is banked PER TILE, and under this model that is literal

`route_traffic.work_per_link_tile_per_turn` is banked on **each tile a journey crosses**
(`trace_path`), where the stored-path model banked `rate × path length` onto one object. A long haul
therefore wears **many tiles a little** rather than one object a lot — which is what makes *"one band
keeps half the tiles and another the other half"* a state the traffic can actually produce.

**The pace of a trail changed with the model and was deliberately not compensated.** At the shipped
`0.35`, two neighbouring camps used to put `0.70` a turn into a single road and reach the trail rung's
`40` in about 57 turns; each of their two tiles now banks `0.35` and takes about 114. That is §4.14's
number to move, and hiding a model change behind a compensating config edit is how the jump stops
being visible.

**So losing a free road is currently about 2.6× FASTER than making one**: a fully worn trail is gone
`disuse_grace_turns + 40 / disuse_loss_per_turn` ≈ **44** turns after the last traffic, against the
~114 turns of unbroken neighbourhood that wear it in. The pair was first written under the
stored-path model, where the two were ~44 against ~57 and losing was the slower of the two. **The
pace is settled in step 13e, not by a retune here** — and the ~114 is the *pooling-link* figure
alone: 13b added marching parties as a second source, so a tile that also carries traffic wears in
faster than it says by however much walks over it.

**Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in.
`RouteTrafficLog` is **drained** by the accrual (`std::mem::take`), so a turn with no traffic wears
nothing rather than re-wearing last turn's journeys.

### TWO KINDS OF TRAFFIC, TWO LEVERS, AND THEY STAY TWO

§4.13: *"two levers, not three: goods and people are the only two things that move, and a shipment is
people."*

| what moves | lever | recorded by |
|---|---|---|
| a **pooling link** — two camps sharing a larder | `work_per_link_tile_per_turn`, **per link per turn** | `RouteTrafficLog::walked`, from `supply::balance_supply_networks` |
| a **march** — anything travelling | `work_per_worker_tile`, **per worker** | `RouteTrafficLog::marched`, from `systems::advance_band_movement` |

**A link is not a headcount**: two camps pooling a larder are a *standing fact*, not a party of a
countable size — so its rate is per link per turn. **A march is people**, so its rate is per worker.
⛔ **No third lever for shipments, and no mass term**: `balance_supply_networks` drops
sub-`min_transfer` moves, so a mass-driven rate is the error §4.13a ① already corrected.

Each log entry carries **the work each tile of that journey earns** (`RouteJourney::work_per_tile`),
resolved where the journey was recorded — so there is still exactly one drain and one accrual loop,
rather than two logs and a third kind of traffic forgetting one of them.

> #### ⛔ ONE HOOK FILLS BOTH OF §4.13'S REMAINING TRAFFIC ROWS, AND THAT IS A FACT ABOUT THE CODE
>
> **Every travelling thing in the game is a `PopulationCohort` carrying a `BandTravel`** — a band
> under `move_band`, a scout, a hunt party, and a **trade shipment**
> (`handle_send_trade_expedition` spawns an `Expedition` + `BandTravel` like every other party) — and
> `systems::advance_band_movement` is the single system that steps all of them.
>
> So *a shipment walking a connection* and *ordinary band / expedition movement* are **one
> mechanism**. Building two paths to make the table look symmetrical would be a second producer of
> the same number. The head count is `components::available_workers`, the same seam the labour pass
> spends; the journey is `current → next`, where `next` is the position after the whole turn's
> movement.

> #### ⛔ A MARCH IS BANKED ONE TURN LATER THAN A LINK, AND THAT IS THE ARRANGEMENT
>
> `advance_band_movement` is in `TurnStage::Population`; `advance_roads` drains the log in
> `TurnStage::Logistics`. So a **march** is banked in the **next** turn's Logistics while a **pooling
> link** is banked in the same turn's. Each entry is banked **exactly once** — the log has one drain
> — so nothing is lost and nothing doubles. **Do not reorder a stage for it**; it is the same shape as
> every other lag in this arc.

## The scale term — `UpkeepScale::RouteSpan` COLLAPSED into `SourceLoad`

```text
measure = infrastructure_cost(this tile's terrain) × keeper_remoteness
```

`RouteSpan` existed to express a road's `length × terrain`. **Per tile there is no length term**, and
what remains is the same *shape* the plant web already reads: a patch scales its keeping on its tile's
own `K`, a road tile on its tile's own `infrastructure_cost`. Both arms of `UpkeepScale::factor` were
the same expression, so the variant was a second name for one measure — and collapsing it is §4.11's
stated preference, *"one primitive with a per-branch reading beat a second variant"*.

`UpkeepScale` is therefore a **one-variant enum** and every rung that owes anything declares
`scaled_by: "source_load"`. The key stays in the config because it is what a future primitive reading
a genuinely different quantity would be declared through, and because a rung stating its measure is
what makes the rung-monotonicity check's *"compare only adjacent rungs sharing a `scaled_by`"*
meaningful.

- **This is still the only reader `TerrainDefinition::infrastructure_cost` has** — authored for all 37
  terrains and dead data until this branch. `routes::road_upkeep_measure` is the one definition of the
  arithmetic and `road_measure` is the ECS wrapper over it.
- **The land is a SCALE term, not an offset** (§2.7): it *multiplies* the demand, never subtracts.
- **The remoteness rides the same measure**, so it prices the material half as well as the work half
  the day a route rung declares a material.

## What a rung buys — DERIVED FROM THE TILES, never stored on a link

`infrastructure_cost` had zero readers and nothing anywhere read a route rung to reduce a cost.
**Shipping the dearer half alone builds a tax, not a ladder** — §4.9 item 12's named trap.

| Term | Reading | Where |
|---|---|---|
| friction | **the MEAN over the tiles between two camps** | `routes::path_friction_multiplier`, over `trace_path`, read by `supply::component_friction` |
| reach (`holds_link_to_tiles`) | **the WEAKEST tile** | `routes::path_reach_tiles`, read by `supply::balance_supply_networks`' link test |
| the lesson a connection teaches | **the WEAKEST tile** | `routes::path_lesson_rung`, read by `routes::credit_route_lessons` |
| `Seen` along a kept road | a per-rung yes/no | `Road::grants_sight` → `visibility_systems::light_kept_routes` |

**Friction averages and reach takes the minimum, and the asymmetry is the point.** You genuinely lose
less over the roaded stretch of a haul, so **a partly-built road pays partly**; a link goods must get
*through* is not most-of-the-way-there, so **one gap breaks the run**. Both are monotone-improving in
the roads beneath them, which is the additive guarantee the branch rests on — *a rung can only widen
the set of links and lower a loss, never the reverse*.

> ### ⛔ THE *"BEST ROAD BINDING THE NETWORK"* RULE IS DEAD
>
> It was an artifact of choosing among independent path objects. Reading a path of **tiles**, *best*
> would call a thirty-tile dirt road with one paved tile a paved road.
>
> **What survives of it is one level up: a COMPONENT takes its best LINK**, and that is forced rather
> than generous. `balance_commodity` pools a whole component against one friction scalar and has no
> path model, so any per-component reading is an approximation; under a *worst*-link reading a
> component that gained a new unroaded neighbour would see its friction **rise**, punishing a band for
> having walked somewhere. A minimum of monotone-improving readings is monotone-improving.

> ### ⛔ THE PAYOFF FILTER IS `keeping_is_met`, AND IT IS NOT `grants_sight`
>
> The two predicates answer different questions and only one of them is a payoff's.
>
> - **`keeping_is_met()` asks *"is this road being held up?"*** — *an unmaintained road is not
>   carrying anything*, which is exactly what a payoff wants to exclude. **For the free floor the
>   answer is yes by arithmetic**: a path and a trail declare no `upkeep`, so nothing stamps a demand
>   and the shortfall is permanently zero. It is false only for a **built** road in shortfall.
> - **`grants_sight()` additionally asks *"is this a rung that lights ground at all?"*** — a
>   **visibility** question resting on *paying the upkeep IS the presence*. It is unchanged, and no
>   free rung lights anything.
>
> Both payoffs filtered on `grants_sight` until 13b, so **the entire free floor read as bare ground**:
> a fully worn trail held `0` tiles open and saved `0%` of the friction, against a config declaring
> `route:trail` worth `holds_link_to_tiles: 6` and `friction_multiplier: 0.85`.
>
> **That was load-bearing rather than untidy.** With a trail extending reach by nothing the branch is
> unclimbable: `grade` is gated on `roadbuilding`, which is learned from a standing connection that
> only a road holds open, so you would need a `dirt_road` to earn the lesson that unlocks the
> `dirt_road`. **The free floor is what breaks the circle** — *free is not worthless*.

### ⛔ NEW TRAFFIC PREFERS AN EXISTING ROAD, AND NEVER DETOURS TO REACH ONE

`trace_path` takes the `RoadRegistry` and, among the neighbours that **equally minimise** the
remaining hex distance, prefers the one carrying the **highest held rung** (no road < `path` <
`trail` < `dirt_road` < `paved_road`), breaking any remaining tie on the old lowest-direction-index
rule so the walk stays deterministic. Only steps already tied for best are compared, so the hex
distance bounds the walk exactly as before: it cannot get longer and it cannot loop. What it buys is
that the second journey between two camps runs over the road the first one wore, rather than beside
it.

**The named limitation**: a road only helps a link where it lies along a *shortest* hex path between
the two camps. That is self-consistent rather than a gap — roads are worn in by traced journeys in
the first place, so they form on shortest paths.

⛔ **It reads no terrain, and must not start.** Roads forming across a lake (#601) is blocked on #282;
two answers to *"can you cross this hex"* is worse than the bug.

⛔ **Anything that traces while holding the registry mutably collects every path first.**
`advance_roads` phase 4 does exactly that — trace all, then bank — because the trace reads the
registry the banking writes, and interleaving them would also let the first journey of a turn lay the
road the second one follows.

### THE REACH IS CONSUMED — a road is what lets two camps pool where they cannot

`balance_supply_networks`' link test is now

```text
hex_distance(a, b) <= max(cfg.reach_tiles, path_reach_tiles(roads, trace_path(a, b, ..)))
```

with `pools_freely` and `tie_is_live` **unchanged**. It is purely additive: a pair inside
`reach_tiles` pools exactly as it did, which is the no-early-game-regression guarantee. This is the
first consumer `holds_link_to_tiles` has ever had — the number the client was rendering in the future
tense — and it is what makes the top rungs a **capability** rather than a discount.

> #### ⛔ THE BINNING NEIGHBOURHOOD IS SIZED BY `routes::max_route_reach_tiles`
>
> The pooling pass bins nodes into `cell_size` cells and scans ±1 cell (±2 in x when wrapping), under
> the invariant it states itself: *"the neighbourhood must be a superset of what the distance test
> accepts, or pairs are dropped silently."* Accepting out to a paved road's `16` against a cell size
> of `reach_tiles` (`3`) breaks it — and it breaks as **some long roads just don't work**, with
> nothing erroring anywhere. So `cell_size = max(cfg.reach_tiles, max_route_reach_tiles(ladder))`.
>
> **Nobody reads the rung records for that number directly**, the same discipline `road_keeping_range`
> exists for: the day a rung is added or its reach retuned, the binning follows with no call site
> moving.
>
> **Cost**: the trace runs only once the free test has already failed *and* the pair is within
> `max_route_reach_tiles`, so a game with no roads — the shipped turn-1 state — traces nothing.

**`Road::payoff` is stamped** and re-stamped on every write to the position, exactly as
`Road::standing` is. A supply pass that resolved it would take a `LadderConfigHandle` to re-derive a
number the road already knows, and every harness that stands the pooling up would have to hand it one.

## The one-turn lag is the ledger's, and it is accepted for the ledger's reason

`balance_supply_networks` runs in `TurnStage::Logistics` and **`routes::advance_roads` runs after it**
— declared with `.after(supply::balance_supply_networks)`, not left to the ambiguity gate. So the
route pass sees **this turn's** links, and the *payoff* is read at each tile's standing as of the
**previous** turn — precisely the lag `balance_supply_networks` already accepts against
`ConnectionLedger`.

**Do not reorder a stage for it, and do not let the supply pass raise a road**: that would make a
second producer of a rung's position, the failure this arc has had three of.

## The four rungs

`path → trail → dirt road → paved road`, in `intensification_ladder.json`.

| rung | verb | `unlock_knowledge` | `earns_knowledge` | `build` | `build.materials` | `upkeep` |
|---|---|---|---|---|---|---|
| `path` | — | — | — | null | — | null |
| `trail` | **— (it forms from use)** | **—** | `roadbuilding` | 40 work | — | **null** |
| `dirt_road` | **`grade`** | `roadbuilding` | `paving` | 300 work | **none — cut earth is already there** | yes |
| `paved_road` | **`pave`** | `paving` | — | 800 work | **20 stone** | yes |

- **The floor is TWO rungs.** A path is what traffic wears in before anybody decides to make a road,
  and a trail is the floor's second storey. Neither costs anything to hold, neither takes a verb,
  neither has a keeper, and neither lights a tile.
- **`partial_credit: continuous` on all three built rungs.** A half-worn trail is genuinely half a
  trail, unlike `animal:pen` where half a fence is not a fence.
- **`site_requirement: null` on every route rung**, and that is the honest answer rather than a gap: a
  road asks nothing of the ground it crosses — what it asks is **priced, not gated**, and that is the
  `scaled_by` term. **Do not invent a `route_requirement` sibling**; a rung asking nothing of its site
  is what every animal rung already ships.
- **The grace direction is the ANIMAL branch's** — the highest rung is the most forgiving, because the
  roadbed does the holding.
- **The free floor still BUYS something.** `route_payoff` is required on every rung including the two
  free ones: a trail holds a link 6 tiles out and takes 15% off the friction. **Free is not
  worthless** — the shape `plant:wild` has, where a wild patch feeds you for nothing.

> #### ⛔ THE FLOOR RUNG IS `path` BECAUSE NOTHING IN THE SIM LETS AN ANIMAL WEAR A ROAD IN
>
> It shipped as `game_trail`, named for #215's *"the first roads are the ones the animals made"*.
> **Everything that banks route work is people**: `walked(..)` from the pooling-link pass, and
> `marched(..)` from the movement pass — two camps sharing a larder, and a band, scout, hunt party or
> shipment on the move. Herds move, but no herd has ever recorded a step as route traffic — so every
> path on the map is worn in by the **player's own** bands, and the floor rung was displaying them a
> trail the animals made. Ray hit it in play at tile (60,40).
>
> That is the second half of a correction begun in `96bf835d`, which deleted the tile card's flavour
> line *"a path the animals made"* for the same reason: it asserted a cause the model does not have.
>
> **#215 is unaffected and remains open.** Even were herds to bank route work later, a path worn in
> by the player's traders is not a game trail; the rung's name states what reaches it — traffic,
> whoever's — rather than an origin.

### ⛔ THE CHAIN LOST A LESSON, AND THE ONE IT LOST TAUGHT NOTHING

13a shipped **four**: `path` (then spelled `game_trail`) earned `trailcraft`, which gated `trail`.
**A lesson for something you cannot fail to do** — you wear a path by walking it, so there is no
knowing-how involved and no way to be refused. `trailcraft` is **deleted** from `discovery_id_for`, from the ladder's
`lesson_costs` and from `start_profile_knowledge_tags.json`.

**Discovery id 2011 is RETIRED, not reused**, and `roadbuilding` / `paving` keep 2012 / 2013 rather
than sliding down onto it — a gap is safer than a renumber, which silently re-points every start
profile that already names one. `routes.rs` carries the retirement note where the constant was.

**Both survivors gate something a player decides**: a trail carrying traffic teaches `roadbuilding`,
which opens `grade`; keeping a dirt road teaches `paving`, which opens `pave`.

### ⛔ WHAT CREDITS THE LESSON — `routes::credit_route_lessons`, AND THE UNIT IS THE CONNECTION

Until 13b **nothing in the sim credited a route lesson at all**: the only ladder credit was
`systems::labor::credit_rung_lesson`, whose reachable callers are all food-web arms. So `roadbuilding`
could not leave `0` by any means, `grade` was permanently refused, and both built rungs were
unreachable. This pass is what makes the branch climbable.

**A connection is two bands within reach of each other. Distance only** — nothing about goods, nothing
about factions, nothing about whether they have ever met. **A road is what makes that reach bigger**:
the same `max(reach_tiles, path_reach_tiles(..))` the pooling test uses.

Ray: *"only credit the 'connection', so if a trail (or other road type) makes an unbroken connection
between two bands … only then does road building get learned. That fits our distance model perfectly,
since the local connections are shorter, they would contribute less."* Three things follow, and each
is a reason:

- **It dissolves the *how often* question rather than answering it.** That question only bit while a
  caravan was modelled as an occasional *journey* against a neighbour link's every-turn pooling. A
  connection is a **standing** thing, so both are present every turn and there is no frequency term
  left.
- **It kills a scaling bug a per-tile credit would have shipped**: a per-tile lesson scales with tile
  count, so a wider map would teach roadbuilding faster for a reason no player caused.
- **Credit per turn while the connection stands, never once on completion.** A one-off is a step
  function and pays twice for a connection that breaks and reforms.

| | |
|---|---|
| **the lesson** | the connection's **WEAKEST tile** (`routes::path_lesson_rung`) — one path hex in the middle of a paved road means what you travel is the gap. A tile with **no road**, or one whose `keeping_is_met()` is false, breaks the run and teaches nothing |
| **the amount** | proportional to the connection's **length in tiles**, every turn it stands (`RungDef::route_knowledge_accrual`) |
| **who is credited** | the faction of **each endpoint band**, gathered into a set — so one people at both ends is one credit, without the code ever asking whether the two ends are the same people |
| **where** | `TurnStage::Logistics`, `.after(routes::advance_roads)` — **declared**, not left to the ambiguity gate, because it reads the road standing that pass just produced |

⛔ ***"A path does not count"* NEEDS NO SPECIAL CASE.** `route:path` declares `earns_knowledge: null`,
so the accrual answers `None` on its own — exactly as the free floor owing no upkeep falls out of the
arithmetic rather than an `is_built()` guard. **Do not write a `path` branch.**

⛔ **IT DOES ITS OWN O(n²) PASS AND MUST NOT BORROW THE SUPPLY LINK LIST.** This is the thing a future
reader will "simplify". `balance_supply_networks`' links carry a **same-people** gate
(`pools_freely`) and a **live-tie** gate (`tie_is_live`), and neither has anything to do with whether
two camps are connected by ground. Band counts are small; a pass of its own is what keeps *a
connection is distance* true.

#### `RungDef::route_knowledge_accrual` is a SIBLING of `knowledge_accrual`, not a faked floor

```text
practice = learn_rate × tiles        // per CONNECTION per turn, NOT per worker
amount   = practice / lesson_cost(this rung's knowledge)
```

Same seam, same currency, the branch's own reading: on the food webs `learn_multiplier` is *how hard
you are pressing the source*; on the route branch the multiplier is *how far the connection runs*.
Passing a floor this branch does not have would be inventing a number nobody chose — which is why
`credit_managed_rung_lesson` states its own reading rather than pretending to have a floor.

⛔ **It takes no worker count**, for exactly the reason `knowledge_accrual` refuses one: practice is
earned by *a turn the thing is used*, never by hands, or a faction would learn roadbuilding faster by
parking people on the road. `knowledge.learn_rate` and `lesson_costs["roadbuilding"]` / `["paving"]`
are the dials, and **13e tunes them** — the pass adds no lever of its own.

## The build — `grade` and `pave` are TILE commands on the builders' pool

`grade <faction> <band> <x> <y>` → `route:dirt_road`. `pave <faction> <band> <x> <y>` →
`route:paved_road`. `cultivate`/`sow`'s grammar **plus a band token**, and that token is the one
structural difference: a patch's keeper is whoever is already foraging it, and **a road has no take
row at all**, so who will keep the tile has to be said out loud.

Issuing one does two things at once, and they are the same act: it **declares the job** (a
`BuildQueueEntry` on that band's queue, raised by that band's `builders` pool at the head) and it
**names the keeper**.

### The refusals, each named

| | |
|---|---|
| no band by that id, or not yours | you cannot commit somebody else's people |
| no road on that tile at all | there is nothing to grade; walk it first |
| not yet a trail / not yet a dirt road | the rung beneath has to stand — asked through `rung_beneath`, off the coded climb rather than named per verb |
| the knowledge is not learned | `roadbuilding` gates `grade`, `paving` gates `pave` |
| another band keeps it | **one keeper per tile** |
| you already keep it at that rung or above | there is nothing left to raise — and the scoping to *"you"* is what makes adoption work |

**There is deliberately no range refusal.** Distance is priced (`remoteness_multiplier`) and refused
nowhere.

### ⛔ `BuildSource::Road(tile)` IS THE ONE SOURCE WITH NO LABOR ROW

Every other build source is named by a `Forage` / `Hunt` row, and `prune_build_queue` drops an entry
whose row is gone. A road is named by its **keeper**, recorded on the road, which `LaborAllocation`
cannot see — so **the keeper is passed in**: `prune_build_queue` takes a `keeps_road` predicate and
`systems::labor::band_keeps_road` answers it off `RoadRegistry`, at both of the turn's prunes. *An
entry raises a rung on a tile this band keeps; the moment it is not the keeper the job is not theirs
and the entry goes.* One rule covers every exit — `abandon`, adoption by another band, and decay or
disuse dropping the road below `traffic_ceiling`, which is `Road::set_position`'s own keeper release.

⛔ **`holds_build_source` used to answer `true` for a road unconditionally, and that stranded a
band's whole pool.** Every other exit was closed at the same moment: the road arm banks nothing once
the band is not the keeper, `retire_entries_already_built` reads the rung the *tile* holds (which a
decayed road no longer does), and `abandon` finds no keeper to release. So the entry sat at the
**head** for ever and — all hands on the head — every other build that band had queued was funded
zero work, silently, recoverable only through `unqueue`. `build_queue.rs`'s
`a_road_entry_dies_with_its_keeper_and_frees_the_pool_behind_it` drives the disuse path and asserts
both halves, the retirement and the pool moving on.

The two callers that cannot see the roads — `drop_source_row` and the role clear, both row-driven —
pass `components::road_holding_unchanged`, because clearing a labor row cannot change a keeper.
`enqueue_build` passes it too: `grade` / `pave` write the keeper immediately before declaring, and no
other path can produce a `BuildSource::Road`.

⛔ **`head_rung_gate` ANSWERS THE ROUTE BRANCH BEFORE ITS LABOR-ROW LOOKUP, AND THAT IS WHAT MADE
THE MATERIAL HALF REACHABLE AT ALL.** Every other source is found by matching the queue entry against
`assignments`, and `BuildSource::of` never yields a `Road` — so a road head fell through to
`BuildGate::Unworked`, `source_banking_its_first_work` filtered it out, `banking.source` was
permanently `None` for a road, and no pile was ever struck for a `pave`. `route_head_gate` answers it
from the same two terms the build arm resolves — *does this band still keep the tile* (`OwnedByOther`)
and *does the faction know the rung* (`Knowledge`) — so the claim side and the payment side cannot
disagree about whether a road banks this turn. It leaks nothing into the keeping claim:
`SourceBankingFirstWork::declared_on` also goes through `BuildSource::of` and still answers `None` for
every row.

**The build is its own arm in `advance_labor_allocation`, after the assignment loop**, because that
loop visits `assignments` and can never reach a road. It banks the whole `builders` pool at the
entry's own kit, capped at the destination rung's top, and **banks nothing unless the ordering band is
still the keeper** — a band whose `grade` was superseded (the tile decayed back into the free floor,
or another band adopted it) puts no work on ground that is no longer its job.

- **The route branch has its own builders kits, and they are bound PER RUNG.** `roadbuilding`
  (earthmoving tools) serves `route:dirt_road` and `paving` (stone-dressing tools) serves
  `route:paved_road`; each is worth its offset on its own rung and `NO_BUILD_GEAR` on the other, off
  `EquipmentEffect::rung`. `BuildersGear` therefore resolves the kit at the rung **in flight** rather
  than at the entry's destination — a `pave` on a road that has decayed below a dirt road is doing
  grading work, and must resolve and wear the grading tool.
- **The store scales the work, exactly as it scales the pile.** The arm's accrual is multiplied by
  the head entry's material coverage before it is banked, which is `animal:pen`'s stated rule — *a
  short store stalls the build proportionally and never refuses it*, and the unbanked remainder is
  wasted rather than carried.
- **A road publishes a chained countdown like any other source.** `RouteState.buildTurnsRemaining`
  carries **the same quantity with the same sentinels** a patch and a herd publish, through the same
  `published_build_countdown` seam — there is deliberately no route dialect, so a client renders a
  road through the identical fork. Only the **queue** can answer it (an entry is dated as everything
  above it plus its own span), so `publish_entry` stamps it and the decay pass clears it, exactly as
  the two food webs do. **Only a queued road has a number**: an unordered rung has no quote and reads
  the honest *no estimate*, never a `0` that renders as a finished build.

  ⛔ **It shipped stamping nothing, and the client filled the silence with a constant** — every road
  queue model hardcoded the `-5` *not yet estimated* sentinel, so a road read `Queued 97%` on turn 1
  and on turn 147 alike. The justification was *"a road has no source row for the sim to stamp one
  on"*, which was never true of `RouteState`. **No claims object arbitrates it**, unlike the two food
  webs': one keeper per tile, and each band's own `prune_build_queue` drops the entry for a road it
  does not keep *before* that band's queue is walked, so at most one band can hold an entry for a
  tile by the time the pass runs.

> #### ⛔ EVERY ROAD ENTRY RECORDS A `BuildQuote`, AND THE COST OF NOT DOING SO WAS NOT CONFINED TO ROADS
>
> `publish_build_chain` mints `BuildTurns::Blocked` for a **staffed head that recorded no quote**,
> with `blocked_reason(None)` — the cause `unworked` — and `carried` then hands that same `−4` to
> **every entry behind it and every unqueued source the band works**. A road pushed no quote, so a
> band that typed `grade` and staffed its builders published `⚠ Blocked` on its patches and its herds
> while the road was building perfectly well. It is the hole the pen ring fell into, and it is closed
> the same way: the arm records a quote for **every** road entry, head or waiting.
>
> The invariant is now stated where it is broken. A staffed head whose source `source_is_on_the_ground`
> can resolve must carry a quote, `debug_assert`ed in the chain pass. A source that is genuinely *not*
> there — a `sow` on bare ground the faction cannot seed, which places no patch — is the case the
> `unworked` cause is honest for, and it is the term that keeps the assertion true.

### ⛔ THE STONE: FLAT WHERE THE WORK IS REMOTENESS-SCALED

`route:paved_road` declares **both halves** — `build.materials { stone: 20 }` to lay it and
`upkeep.materials { stone: 0.1667 }` to hold it, which is `plan_standing_upkeep.md` §4.13's *"a paved
road declares stone on the pile **and on the rate**"* and correction ②'s *"the paved road was to
swallow stone on the pile and the rate"*. Every other rung on the branch declares neither: a graded
roadbed is cut earth, and the earth is already there.

> **The rate shipped missing for one slice**, on a stated default that paving owes no standing stone.
> The plan says the opposite twice. A road that holds for free is the failure that produced.

**The rate is the PEN's ratio, not a second arithmetic.** `animal:pen` sets a 6-hurdle pile against a
`0.05`/turn rate — it replaces its whole pile every **120 turns**. A paved road's pile is 20 stone, so
the same relationship gives `20 / 120 = 0.1667` a turn: one stone every six turns, a roadbed
re-dressed at exactly the pace a fence is re-panelled. The road's own internal ratio
(`20 stone / 800 work × 0.95 work/turn ≈ 0.024`) would have been **seven times lighter**, replacing
the pile every 840 turns; the pen is the worked precedent and one precedent beats a second sum.
Opening value — **§4.13e owns the retune**, as it owns every route number.

| | scaled by `keeper_remoteness` / `infrastructure_cost`? | tracks the position? |
|---|---|---|
| the **pile** (`build.materials`) | **no** — flat, see above | draws as the meter climbs |
| the **rate** (`upkeep.materials`) | **yes**, through the same `scaled_by` the work reads | owes in proportion to how much stands |

The rate's scaling is §2.7's *"the land is a SCALE term, not an offset"*: a road over a range costs
more stone to hold **and** more hands, which is the one thing this branch says about ground. The
pile's flatness is the deliberate exception, for the reason above it.

### A FRACTIONAL RATE IS NOT A ROUNDING PROBLEM, AND MUST NEVER BECOME ONE

`0.1667` a turn is far below one whole stone, and there is **no accumulator beside the stock because
the stock IS the accumulator**: a material is a **continuous** fixed-point quantity (`Scalar`,
micro-units), not a count of discrete items, so a draw of `0.1667` subtracts exactly `0.1667` and the
stock crosses whole units by itself. Rounding the per-turn draw would either lose every charge below
half a unit — a road held for nothing while the wire still reports a bill — or bill a whole stone
every turn. **The seam is exercised rather than new**: the pen has drawn `0.05`/turn since it shipped.

### ⛔ THE TWO SHORTFALLS COMPOSE AS A MAXIMUM, AND CANNOT DOUBLE-COUNT

`intensification::keeping_shortfall_fraction` takes the **worst** of the work fraction and each
material's own — never their sum — and `keeping_is_short` trips **one** `neglect_turns` on **one**
grace. So a road fully staffed with no stone rots at the stone's rate, one with stone and no hands at
the hands' rate, and one short of both rots **once**, at the worse of the two. Summing would let a
full store cover a band's missing hands, which is the papering-over §4.9 item 12 forbids. `advance_roads`
was work-only until this rung declared a rate, which was correct while nothing ate anything and became
*"a paved road holds for free"* the moment one did.

**And the shortfall must name the right thing.** §2.7: *"you cannot mend a road with no stone, so a
shortfall message that names the **pool** is wrong advice."* The row publishes both pairs so a client
can tell them apart — `upkeepDemand − upkeepSupplied` is **short of keepers**, `upkeepMaterialDemand −
upkeepMaterialSupplied` is **short of stone**, and only one of those two sentences helps at a time.

### ⛔ HOLDING WHAT YOU HAVE OUTRANKS EXPANDING

**A band's standing paved roads take their stone before a new paving build may touch the store.**
`bill_and_stock_roads` runs `.before(advance_labor_allocation)`; it strikes both of a road's bills and
spends the standing material, and only then do the builders draw their pile. While the build pile
settled *inside* the labour pass and the standing rate settled *after* it, the build simply got there
first — an ordering nobody chose, in which pushing a road out quietly stripped the stone from every
road the band was already holding.

**Both bills are stamped in one pass, and that is why the DRAW moved rather than the STAMP.** The
build arm moves a paving road's meter inside the same turn, so a work bill struck on one side of it
and a material bill on the other are two readings of two different roads, and
`demand − supplied == shortfall` goes false in whichever lagged. Moving the stamp *earlier* keeps the
pair together **and** puts the material draw ahead of the build's — and the pre-accrual position is
the right one anyway: both food webs bill there, and roads were the odd branch out.

**What stays behind is the WORK payment alone**, because that half needs the one thing the early
pass cannot have — the `roadwork` head count *the shedding order left*. So it runs as
`settle_bands_roadwork`, called from **inside** `advance_labor_allocation`'s band loop, immediately
after the shed and before the band's `continue`s — the same seat both food webs settle their keeping
from.

⛔ **THE PLANT AND ANIMAL WEBS ARE NOT REORDERED.** `settle_material_upkeep` still settles their
standing materials and the build pile in one call, ranked by the player's own `SourcePriority`. What
changed is that the **route** branch's standing draw happens before that call rather than after it, so
a road's keeping outranks a *build* — including a pen's — on any material they share. On the shipped
roster nothing is shared: a pen eats hurdles and a road eats stone.

**A starved build STALLS, it is not refused.** §2.7's rule holds unchanged: the covered fraction
scales both the work banked and the stone drawn, so a build the standing roads have left short banks
proportionally less. A build the store cannot cover *at all* publishes `materials` as its
`buildBlockedReason` — the rung's own gate is `Open` there, so a surface reading the gate alone would
show an unexplained freeze.

| | scaled by `keeper_remoteness`? | why |
|---|---|---|
| the **work** (`road_rung_span`) | **yes** | a road far from the band that keeps it is dearer to reach and dearer to hold |
| the **stone** (`build.materials`) | **no** | a tile of road needs the same twenty stone wherever it lies, and remoteness already taxes the getting there |

**The flatness falls out of the draw's own arithmetic, not out of a special case.** The pile is spread
over the leg's own priced width (`pile × accrual / width`) and a whole climb banks exactly that width,
so the remoteness in the denominator is cancelled by the remoteness in the work that fills it. A
remote road therefore draws its stone **more slowly, over more turns**, and swallows the same twenty.
`head_build_legs` quoting the leg at the *unscaled* span is the one-line mistake that would inflate
the pile by the remoteness; `labor.rs`'s
`a_remote_road_costs_more_work_and_exactly_the_same_stone` asserts the leg's width against
`road_rung_span` directly rather than leaving it to be inferred from a total.

**Drawn as the meter climbs, and decay refunds nothing.** A roadbed a third laid has swallowed seven
stone; if it then washes out, the stone is gone. That is what makes neglect self-limiting rather than
a store bleeding for ever. A store with nothing in it at all blocks the head with
`BuildGate::Materials` — the cause a rung whose *own* gate holds publishes, read off
`BuildQuote::blocking_gate` and never off the gate directly.

**`stone` has no producer.** It reaches the player through `materials.json`'s `start_stock` alone
(`12.5` per worker — about ten paved tiles to the shipped band), seeded by worldgen for every material
that declares one. Quarrying belongs to the minerals arc, issue #583.

## ⛔ TWO DECAY TRIGGERS, BECAUSE A FREE RUNG CANNOT BE SHORT

| trigger | region of the position | armed by | rate |
|---|---|---|---|
| **unpaid keeping** | strictly **above** `traffic_ceiling` | `Road::neglect_turns`, past the rung's own `upkeep.grace_turns` | `shortfall_fraction × meter_decay.per_turn` |
| **disuse** | **inside** the free floor's span | `Road::idle_turns`, past `route_traffic.disuse_grace_turns` | `route_traffic.disuse_loss_per_turn`, **flat** |

**They do not overlap.** The free floor declares no `upkeep`, so its demand is `NO_UPKEEP_DEMAND` and
its shortfall is permanently zero — the built rungs' path can never reach it, and 13a's collapsing of
both into that one path left a worn trail **immortal**. This is `plan_contact_and_logistics.md` §Q4's
own *"an unused road reverts"*, restored to its own trigger. A dirt road nobody walks is still lost
because nobody **pays** for it, which needs no traffic term.

**The disuse loss is FLAT rather than proportional**: a bill can be *partly* paid, but traffic is a
yes/no — a road either carried a journey this turn or it did not — so there is no fraction to scale by.

**`Road::idle_turns` is counted on EVERY road, built ones included**, so a road that decays back down
into the free floor arrives there with an honest reading rather than a zero that would buy it a second
grace it never earned. **It is also what keeps the registry bounded**: a road bled to `RUNG_UNSTARTED`
is pruned, and without a loss on the free floor an abandoned trail would sit there for ever.

### A road that falls back into the free floor LOSES ITS KEEPER

`Road::set_position` releases the keeper below `traffic_ceiling`: the floor declares no `upkeep`, so a
keeper held there would be a job with no work in it. The test is **strictly below** — a road sitting
exactly on the top of the trail is the state a fresh `grade` leaves (keeper set, first work not yet
banked), and clearing it there would undo the command on the turn it was typed.

⛔ **A FIXTURE THEREFORE WRITES THE POSITION BEFORE THE KEEPER.** Seating a keeper first and then
seating the position hands it straight back, silently.

## The keeping — the `Roadwork` POOL, and the decay it pays for

⛔ **THE POOL SURVIVED THE PER-TILE MODEL, AND THIS IS A REVERSAL WORTH READING.** An earlier cut of
§4.13b replaced it with per-tile work rows. **That was wrong.** Ray: *"a road isn't active like
hunting or foraging is so you don't need the tile workers. You just need to say you want a 'road' in
this tile and it builds (with builders) and then maintains."* There is no per-turn activity on a road
whose output scales with people standing there, so a work row with a stepper is the wrong instrument.

**The pool was never the problem — the automatic billing was.** With the free floor free and `grade`
the only way onto a paid rung, every road a band pays for is one it chose by typing a command. So
`Roadwork` covers **the roads that band is the keeper of**, exactly as `Agriculture` covers the patches
it cultivated, and the per-road choice is exercised with `abandon`.

`LaborTarget::Roadwork` is an ordinary band-wide standing role — `assign_labor <faction> <band>
roadwork <n>`, published as a `laborAssignments` row with `kind: "roadwork"`, shed by `normalize`,
checkpointed. Its TOE job is `KitJob::Roadwork`, and `default_kits.roadwork` is the bare `none` kit — but that
is the **fall-back and no longer the answer**: `roadbuilding` and `paving` both list `roadwork` among
their jobs, so `keeping_kit_for` derives a real kit **per road, at the rung that road stands on** —
`roadbuilding` for a dirt road, `paving` for a paved one.

### ⛔ THE BILL IS STAMPED ON EVERY ROAD, KEEPER OR NOT

The two halves run in **two** passes, both edges declared — see *"holding what you have outranks
expanding"* above for why the stamp moved earlier:

1. `systems::bill_and_stock_roads`, `.before(advance_labor_allocation)` — **stamps the interpolated
   bill on every road in the registry**, first-write-wins, and spends the standing material;
2. `settle_bands_roadwork`, called from **inside** `advance_labor_allocation`'s band loop — **pays
   the WORK half**, from every band whose `roadwork` row is staffed, against **the roads it keeps**
   (`RoadRegistry::kept_by`). ⛔ **It is not a system of its own, and that is the fix to a false
   countdown**: paid from one system later, the build quote read a `Road::upkeep_supplied` still at
   `0`, pinned the work shortfall at `1.0` and published the FULL rot for a road the player had just
   funded. It sits before the band's `continue`s so a band that sheds its whole allocation still
   clears the roll-up rather than republishing a stale bill.

**Step 1 is the load-bearing half, and its scope is the trap.** `Road::keeping_is_met` answers `true`
for a road with **no stamped bill** — an honest *"it has not been judged this turn"* — so a pass that
stamped only the roads somebody keeps would leave a **keeperless** road reading as kept **for ever**:
never arming its neglect counter, never decaying, never pruned. It fails as *no decay at all* rather
than as a slow one. `route_traffic::a_keeperless_road_decays_and_is_finally_pruned` is the pin.

**THE WHOLE FREE FLOOR falls out of the arithmetic rather than being branched around.** Neither free
rung declares an `upkeep`, so a road holding a path — **or a fully worn trail** — owes nothing.
An `is_built()` guard would be a second statement of *"nobody keeps the free floor"*, free to disagree
with the ladder that already says it.

### The catchment is the KEEPER, not who is standing there

`route_keeping_claims` walks `kept_by(band)` and **does not read the band's position at all**. A band
four tiles from a road it graded goes on paying for it: what distance costs is priced into the road's
own `keeper_remoteness`, never into whether the bill exists.

Everything downstream is the identical seam the two food webs use — `keeping_rates` →
`KeepingRate::worker_need` → `intensification::distribute_upkeep_pool`, under the band's own
`upkeep_fund_mode`. **There is deliberately no second supply expression.** The one structural
difference is that a claim carries no assignment index: a road has no labor row, so `KeepingClaim::index`
indexes the returned tile vector instead. `Priority` funds most-invested first on the road's
**position** — which *is* the accumulator on this branch — tie-broken on the tile coord.

**`upkeep_supplied` accumulates (`+=`)**, the §2.5 rule kept unchanged even though a tile now has
exactly one keeper: the split hands a keeper's pool out claim by claim, and the field is cleared once
per turn by `advance_roads`.

### `advance_roads` is five phases, and the order is the whole of it

1. **judge last turn's keeping** — `upkeep_shortfall_fraction` off the **stamped** basis arms or wipes
   `Road::neglect_turns` (consecutive turns, never a lifetime budget);
2. **bleed the rung at risk** at `shortfall_fraction × meter_decay.per_turn`, past that rung's own
   `grace_turns`. `RungDef::upkeep_decay` owns both the rate and the strictly-greater comparison;
3. **clear** `upkeep_demanded` / `upkeep_supplied` for the coming turn's stamp;
4. **bank this turn's traffic on every tile each journey crossed, capped at `traffic_ceiling`** — and
   count the idle turns. Each journey banks **its own** `RouteJourney::work_per_tile`, so a link and a
   march share one loop. **Every path is traced before any is banked**, because the trace reads the
   registry the banking writes. The cap takes a `max` against the tile's own position, so a road
   **above** the ceiling is untouched rather than dragged back to a trail every turn a link runs over
   it;
5. **bleed a free road nobody walked**, past `route_traffic.disuse_grace_turns`. **After the banking**,
   because whether a road was idle is only known once this turn's journeys have been drained onto it.

**Then the registry is PRUNED of every road back at `RUNG_UNSTARTED`, after the banking.** A path
with no work in it is indistinguishable from no road; pruning *before* phase 4 would delete every road
on the turn it formed. Remembering that animals once walked there is **#215's concern, not this
registry's**.

**`routes::road_at_risk_rung` is the one answer to *which rung is at risk*** — `standing.raising` where
anything is banked in it, else `standing.held` — because the bill interpolates through it, the grace
lookup asks it, and the decay bleeds it. Three readers that disagreed is what
`forage::patch_unwinding_key` exists to prevent one branch over.

**The one-turn carry is the arrangement and must not be "fixed".** Logistics runs before Population, so
the supply this pass judges was stamped by *last* turn's Population — the same lag
`forage::advance_cultivation` and `fauna::advance_husbandry` already run on.

### The shed takes a road keeper LAST of the three

`ShedStep::SpareKeeper` (step 3) and `ShedStep::NeededKeeper` (step 8) walk Agriculture, then
Husbandry, then Roadwork. **The reason is recoverability**: a road carries the longest graces on the
ladder, and its free floor is re-earned by **traffic alone**. `ShedFacts::spare_roadwork_keepers` is
struck in `advance_labor_allocation` off the **same** `route_keeping_claims` the payment uses, priced
through `routes::road_keeping_basis` — the stamp where one exists, the live demand where it does not,
because the shed runs a whole system *before* anything is stamped and a count struck against a bill of
zero would shed every road keeper as spare.

## ⛔ A MAINTAINED ROAD IS TRAFFIC, SO ITS TILE IS `Seen` — and the keystone is UNTOUCHED

Ray: *"If a road exists and is maintained, the assumption is that there is traffic on it and it is
seen."* **Maintenance is not free** — a kept road bills its keeper every turn out of that band's
`Roadwork` pool, and what those hands are doing is being on the road. **Paying the upkeep IS the
presence.**

**So the keystone does not bend.** `connections.rs` states it as inviolable — *"Only presence makes a
tile `Seen`. A connection can only ever grant `Discovered`."* — and names **logistics** as the first
rider that will be tempted. The sight is granted by the **road**, maintained presence on specific
ground, and **not by the connection**. `core_sim/tests/connections.rs` passes unchanged, and the grant
is written as its own visibility source rather than routed through the connection grant — plumbing it
through the ties would satisfy that test by accident rather than by the rule.

**The condition is the PAID BILL, not the held rung**, so a road in shortfall **goes dark before it
decays**. **And the rung gates too**: a trail lights nothing even with its bill trivially met, because
`grants_sight` is `is_built() && keeping_is_met()`.

### As built — `visibility_systems::light_kept_routes`

Its own system, chained in `TurnStage::Visibility` **after `calculate_visibility` and before
`apply_visibility_decay`**, writing the same `FactionVisibilityMap::mark_active` a band's own camp
writes. Deliberately *not* a `VisionSource`: a source carries an effective range, an elevation bonus,
LOS and a `ContactSink`, and a road grants none of those — it grants **exactly its own tile**.

⛔ **THE FOG IT LIFTS IS THE KEEPER'S, and the pass walks the REGISTRY rather than the bands.** A road
tile is one band's job, so the faction that sees it is that band's — *the people paying for this tile
are the people on it*. The keeper's own position is irrelevant, which is what makes the grant survive
the band walking away and stop the turn the bill does.

**NO CONTACT RIDES THIS REVEAL.** `ContactSink` hangs off the *sight sweep*, whose geometry this pass
has no part in. Crediting contact from a road would let a band meet a people it never looked at — the
second half of the keystone in a different coat.

## The wire — `RouteState`, one row per TILE

`RouteState` / `RouteSection`, appended after `connections` on **both** `WorldSnapshot` and
`WorldDelta`. A section with no delta twin is permanently stale on a delta-fed client — the defect
`campaign_profiles` actually was. Order is the registry's row-major key order, so the section is stable
frame to frame and diffs out when nothing moved.

**The fog gate is `Discovered`, and that is the OPPOSITE of the herd list's.** `herd_is_visible` demands
`Active` because ground you saw two hundred turns ago says nothing about where a herd is standing
today; a road does not wander off. **Per tile the gate stopped needing a rule of its own**: the
stored-path model had to decide how much of a path had to be explored, and the question is now *"have
you seen that tile"*. It **fails closed** on an absent faction map.

| Field | What it is |
|---|---|
| `tileX` / `tileY` | **the row's identity**, and what replaced the retired `RouteId` — with one record per tile there is nothing left for a separate id to name |
| `rung` | the rung it **holds**, `RungKey::wire_key`. **This string is the bool**; a rung is never inferred from the float beside it |
| `buildFraction` | the meter on the rung being **raised**, through `routes::road_build_fraction` → `intensification::rung_work_done` / `build_fraction` |
| `hasKeeper` / `keeperBandId` | **whose job this tile is.** Read the bool first — `0` is a real `BandId`. `false` across the whole free floor, which is the commonest road in the game |
| `keeperRemoteness` | what distance did to the price, as a multiple — the only way a client can explain a bill larger than the rung says |
| `upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` / `upkeepWorkersNeeded` | the standing bill in **work**, all four off the **stamped** `road_keeping_basis` |
| `upkeepMaterialDemand` / `upkeepMaterialSupplied` | the standing bill in **goods**, off the stamped material basis, on the same `demand − supplied == shortfall` rule. ⛔ **This is the pair that separates *short of stone* from *short of keepers*** — a surface reading only the work pair tells the player to staff `roadwork` for an empty shelf. The noun is `RouteRungState.buildMaterialId`. They do **not** add: the decay rides the worse fraction, never the sum |
| `hasNeglectGrace` / `neglectGraceRemaining` | the **countdown**, through `routes::road_neglect_grace_remaining` at the at-risk rung |
| `grantsSight` | the resolved *"is this road lighting its tile"* |
| `frictionMultiplier` / `holdsLinkToTiles` | what the rung is buying, off the tile's stamped `payoff()` — the first a MEAN along a journey, the second the journey's MINIMUM |
| `buildTurnsRemaining` | **the chained countdown** — everything above this entry in its band's queue plus its own span, with the two food webs' sentinel vocabulary verbatim (`-1` no estimate, `-2` holds, `-3` rots, `-4` blocked, `-5` not yet estimated, `>= 0` a real count). ⛔ **Only a QUEUED road has a number**; an unordered rung reads `-1`, never `0` |
| `buildBlockedReason` | **why the pool is stuck on this tile**, `""` when it is not — the same `BuildGate` vocabulary a patch row uses. A road can carry `knowledge`, `owned_by_other` (another band keeps the tile), `no_keeper` (**nobody** does — the tile is going begging, and re-issuing the verb adopts it) and `materials`. ⛔ The first two are **not one cause**: an unkept road reported as another band's sends the player after a rival that does not exist. Read off `BuildQuote::blocking_gate`, never off the rung gate, because a head the store emptied has an **open** rung gate |
| `buildMaterialDemand` / `buildMaterialSupplied` | this turn's share of the rung's pile and what the stores paid of it — the material twin of the four above, on the same rule: `demand − supplied` is the shortfall, verbatim |

### ⛔ THE FOUR RULES THE ROW IS WRITTEN UNDER

- **`demand − supplied == shortfall` holds VERBATIM**, because all three read one local struck from
  `road_keeping_basis` — never the live interpolated demand. `upkeepWorkersNeeded` is `ceil` of the
  **same** number.
- **`buildFraction` is never derived by subtraction.** `rung_work_done` answers a rung the standing
  already holds with the rung's full `width` by construction rather than with `fl(base + width) −
  base`, which is the rounding that published a completed Field at *"99%"*.
- **The countdown, not the counter.** `0` = reverting now; a road whose bill is met reads its rung's
  full `grace_turns + 1`. `hasNeglectGrace: false` means *nothing at risk* — anywhere on the free
  floor.
- **`grantsSight` is the resolved answer**, because a client cannot re-derive *"is the bill met"*: that
  is a comparison against the stamped basis with the sim's own `KEEPING_EPSILON`.

## The rung CATALOG — `RouteRungState`, once per world and carrying no tile

`RouteState` answers *where does this tile stand and what is that worth*. It says nothing about the
rungs **above** it, so no readout could state what a paved road would cost or what it would buy until
the tile already held one — which is exactly what a ladder of rungs nobody has built yet has to say.
`SubsistenceSection.routeRungs` is that ladder: **one row per rung of `intensification_ladder.json`'s
route branch, in climb order**.

**It rides the SECTION, beside `ladderKnowledge`, and not the tile row.** These are properties of the
**branch**, identical for every road in the world; on `RouteState` they would repeat the same four
rows on every road tile. Both are the same kind of thing — the *declaration* of what the ladder holds,
carrying no faction and no tile — and both are per-world constants, diffed whole like `kits`.

⛔ **EVERY FIELD BUT ONE IS DERIVED FROM THE RUNG'S OWN RECORD; NOTHING IS RESTATED.** That is the whole point
of publishing it at all: a rung added to the config appears in the client's ladder with **no client
edit and no schema edit**, the same promise `ladderKnowledge` makes for the knowledge screen.
`routes::route_rungs_in_climb_order` walks `ladder.rungs` — **not `RungKey::ALL`** — so a rung the
coded climb has never heard of is in the catalog, and `RungDef::wire_key` /
`RungDef::requires_rung_wire_key` answer the key off the **record** for the same reason
(`RungKey::wire_key` answers for the rungs a *system* names; the two agree by construction).

| Field | Reading |
|---|---|
| `rungKey` / `order` | the record's `branch:id` and its own climb order — **the row order IS that order**, bottom rung first. `rungKey` is the same spelling `RouteState.rung` carries, so a tile joins to its row here |
| `displayName` | the id read as a player reads it, through the ladder's own `knowledge_title_from_id` — one capitalization rule for underscored ladder ids, not two |
| `verb` | the rung's own, `""` where it declares none. **An empty verb is the free floor**: a path and a trail are formed by use, so there is no command to draw a button for |
| `unlockKnowledge` | the gate, joining to `LadderKnowledgeState.knowledgeId` for the faction's own progress; `""` where the rung waits on nothing |
| `earnsKnowledge` | **the remedy** — the lesson standing at this rung teaches; `""` at the floor and at the top of the branch, which has nothing above it to open |
| `requiresRung` | the rung beneath, **branch-qualified** (the config's `requires_rung` is a bare id within the branch); `""` at the floor, which ends the chain |
| `workCost` / `upkeepWorkPerTurn` | the rung's `build.work_cost` and `upkeep.work_per_turn`, **unscaled** — `0` where the record declares no block at all |
| `frictionMultiplier` / `holdsLinkToTiles` | the rung's `route_payoff`, which `validate` requires on every route rung — so the capture `expect`s it exactly as `road_payoff_at` does, rather than publishing a quieter neutral for a config that cannot load |
| `grantsSight` | **does a road at this rung light its tile while its keeping is met** — `routes::rung_grants_sight`, which asks whether the record declares an `upkeep` |
| `buildWorkPerWorkerTurn` | **what one bare-handed worker banks in a turn** — `intensification::build_work_per_worker_turn(NO_BUILD_GEAR)`, i.e. `PER_WORKER_OUTPUT` before gear and before any multiplier. The one field that is the **sim's** and not the rung's record |
| `buildMaterialCost` / `buildMaterialId` | the rung's declared pile **and the material it is counted in**, resolved from one lookup so they cannot disagree. ⛔ **Unlike `workCost` beside it the amount is FLAT** — the tile's own `keeperRemoteness` multiplies the work and not the stone, so the catalog figure is already the whole truth for every tile on the map and a client that scaled it would over-quote every remote road. ⛔ **And they are read as a PAIR**: an amount with no noun cannot be made into a sentence — the row shipped reading *"+ 20 to raise it"* — and the client must not supply `stone` itself, because which material a rung eats is a fact about the config and a transcribed noun is a second authority. One id rather than a `MaterialPayoff` list: one material per rung **is** the model, and a second would make the single-float amount meaningless before the name mattered |

⛔ **THE REMEDY IS PUBLISHED BECAUSE IT CANNOT BE INFERRED FROM `requiresRung`.** A gate states what a
player does not yet know; what they *do* about it is stand on the rung that **teaches** that lesson,
and that is a different fact from the rung directly beneath the gated one. The two coincide on the
shipped four — `trail` teaches `roadbuilding` and `dirt_road` requires `trail` — and the config is
free to break the pairing, at which point an inference names the wrong rung in the one place it is
telling the player what to go and do. `earnsKnowledge` is the rung's own `earns_knowledge`, in the
same vocabulary `unlockKnowledge` and `LadderKnowledgeState.knowledgeId` use, so the client joins
gate → teacher rather than guessing it.

**The two rates are the BRANCH's figures, and a road's real price is not.** The remoteness quote and
the tile's own `infrastructure_cost` are per-tile facts published on `RouteState`
(`keeperRemoteness`, `upkeepDemand`); a catalog row states the number that is the same for every road
in the world, which is the only one a ladder can quote.

⛔ **`buildWorkPerWorkerTurn` RIDES THE CATALOG FOR EXACTLY THAT REASON, AND IT IS THE SIM'S NUMBER
RATHER THAN THE CONFIG RUNG'S.** Worker output is written as a **sum of terms**
(`intensification::build_work_per_worker_turn` = `PER_WORKER_OUTPUT` + the kit's addend), so every
SOURCE row publishes its own resolved rate and its readers are told to *read it, never assume it* — a
transcribed `1.0` goes stale in silence the day a second bare term lands. A road's own row does not repeat it,
which is why `buildTurnsRemaining` is a no-op for roads sim-side, so the route ladder's turn estimate
and its *short N workers* clause had no published rate to divide by and the client held a constant
instead. The rate is identical for every rung and every road in the world, which is the
catalog's own definition of what belongs on it; on `RouteState` it would be one copy of one constant
per road on the map. It is published at `NO_BUILD_GEAR` — bare hands — because gear is the crew's
fact and the ladder prices a job, not a kit: the client adds the kit's addend itself through
`SourceForecast.pool_work_supply`, the same seam a Cultivate is paced by.

**A `0` here is *no estimate*, never a divisor.** Readers state no turns clause and no *short N*
clause rather than substituting a rate, which is the same discipline
`ForagePatchState.buildWorkPerWorkerTurn`'s readers follow — reintroducing a client-side default is
the transcription coming back through the side door.

**`grantsSight` asks the RECORD for its upkeep rather than comparing against `FIRST_BUILT_RUNG`**, and
the two answer identically: the free floor is exactly the rungs that cost nothing to hold, pinned by
`the_free_floor_and_the_first_built_rung_are_adjacent`. Asking the record is what keeps the reading
alive for a rung the coded climb does not name, and it is the same reasoning `Road::grants_sight`
runs on — *paying the upkeep IS the presence*, which is why the floor lights nothing however worn it
is. The per-tile field stays the **resolved** answer, because a rung that grants sight still goes
dark while its bill is unmet.

**Route branch only, deliberately.** The table is `RouteRungState` rather than a generic
`LadderRungState` because a generic table filled for one branch is a promise the code does not keep;
the plant and animal branches publishing the same is their own change.

`snapshot::subsistence::snapshot_route_rungs` is the one producer, called beside
`snapshot_ladder_knowledge` in the capture. `core_sim/tests/route_wire.rs` pins it on the **encoded
envelope**: one row per rung the config declares, in the coded climb's order, every value read back
off that rung's record — plus the shape a ladder renders differently, the floor requiring nothing and
the free floor naming no verb, owing nothing and lighting nothing, and every gate on the branch
answered by some *other* rung's lesson. The bare work rate is pinned against the sim's own
sum-of-terms seam rather than a literal, and against the branch reading **one** rate: a per-rung
figure would not be a catalog fact.

### The band roll-up — `roadworkDemand` / `roadworkSupplied` / `roadworkShortfall`

On `PopulationCohortState`, summed by `settle_bands_roadwork` over **the roads the band keeps**.
⛔ **THE SIM SUMS IT AND A CLIENT MUST NOT** — the identical rule `fodderNeed` is minted under: road
rows are fog-filtered, so a road out of sight would silently drop out of any client-side total while
the band certainly still owes its keeping.

- **The demand is summed BEFORE the head-count gate**, so a band with nobody on `roadwork` publishes
  the bill it is failing to pay rather than a reassuring zero. It is the alarm.
- **Both are cleared at the top of every band's iteration, ahead of the `continue`s** —
  `advance_labor_allocation`'s rule, so a band that abandons its last road stops republishing a bill it
  no longer owes.

## The client half, as built

`native/src/dict/routes.rs` publishes the row (`tile_x` / `tile_y` / `has_keeper` / `keeper_band_id` /
`keeper_remoteness`, and no path halves) and the GDScript side reads it: `MapView._ingest_road_network`
joins on the tile pair, `TerrainRenderer.rebuild_shader_maps` packs each row into the per-hex
`road_map` splatmap the terrain shader's road pass draws from, and the tile
card gained a `Kept by:` row naming the keeping band and the multiple distance put on its price.
`.claude/rules/client/roads.md` is that half.

**Two things the client half needed from this side and now has.** `grade` / `pave` are in
`sim_runtime::command_text` as `<faction> <band> <x> <y>`, and `SourceForecast.RUNG_KEY_IMPROVEMENTS`
carries the route ladder — the branch declares verbs now, so it goes into that table rather than
staying out of it.

⛔ **AND `roadwork` COULD NOT BE STAFFED AT ALL UNTIL SLICE 13's CLIENT PASS.** The role was in
`server.rs`'s `handle_assign_labor` dispatch and **missing from `command_text`'s own grammar** — and
the client's native bridge parses a line there before it sends, so every
`assign_labor … roadwork <n>` was refused inside the client with nothing failing anywhere. It is the
identical hole `builders` fell through one role earlier; `command_guard`'s role sweep found both.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/intensification_ladder.json` | `route_traffic.work_per_link_tile_per_turn` (**0.35**) | **How fast traffic wears a road in**, in work units, **per tile a journey crosses**, per turn. **The link, not the tonnage**. Under the per-tile model *per tile* is literal, so two neighbouring camps wear each of their two tiles in over ~114 turns where the stored-path model took ~57 for the pair. Validated finite and `> 0`. **PLAYTEST DIAL**, §4.14 owns the number |
| `src/data/intensification_ladder.json` | `route_traffic.work_per_worker_tile` (**0.05**) | **What people on the move wear in**, per tile a travelling party crosses, **per worker** — the *people* lever against the *link* lever above, and §4.13's *"two levers, not three"*. Every travelling thing reaches it through the one `advance_band_movement` hook, so there is no third lever for shipments and no mass term. Validated finite and `> 0`. Opening value chosen for **shape, not balance**: a 10-worker band's single pass puts `0.5` on a tile against a live pooling link's `0.35` a turn. **PLAYTEST DIAL**, step **13e** owns the number |
| `src/data/intensification_ladder.json` | `route_traffic.disuse_grace_turns` (**4**) | **How many consecutive idle turns a FREE road forgives** before it gives back what traffic put into it — the free floor's own `upkeep.grace_turns`. It lives on this block rather than on a rung because it is a fact about *traffic*, and the free rungs declare no `upkeep` to hang it on. Validated finite only: a grace of `0` is meaningful and must stay expressible. **PLAYTEST DIAL**, §4.14 |
| `src/data/intensification_ladder.json` | `route_traffic.disuse_loss_per_turn` (**1.0**) | **What an idle free road loses each turn past that grace**, in the same work units the position is banked in. **Flat, not proportional** — traffic is a yes/no. Validated finite and `> 0` (at zero the registry keeps every trail it ever laid). **PLAYTEST DIAL**, §4.14 |
| `src/data/intensification_ladder.json` | `route_range.base_tiles` (**4**) | **How far a band keeps a road at the rung's own price**, in tiles, measured keeper→tile at the moment the verb is issued. ⛔ **READ IT THROUGH `routes::road_keeping_range`, NEVER FROM THIS FIELD** — see the callout above. Validated `> 0`: a base of zero prices every road as remote, which is a threshold that has stopped being one. **PLAYTEST DIAL**, §4.14 |
| `src/data/intensification_ladder.json` | `route_range.remote_cost_multiplier` (**2.0**) | **What a road outside that range costs**, as a multiple of the rung's own — applied to **both** the build pile and the standing upkeep. A threshold, not a curve. Validated finite and `>= 1.0`: below one, distance would make a far road *cheaper*, which inverts the term. **PLAYTEST DIAL**, §4.14 |
| `src/data/intensification_ladder.json` | the four `route` rungs | The branch itself — see "The four rungs". The `route_payoff` block is **required on every route rung and rejected on every other**: a route rung with a standing cost and no payoff is the *tax, not a ladder* failure, so its absence is a load failure rather than a default |

## Tests

`core_sim/src/routes.rs`'s own module — the ladder liveness (which every other claim rests on), the
free floor's adjacency **and its verb-lessness** (what replaced `is_crew_built`), the measure as
`ground × distance`, the one-primitive scale claim, the bill, the remoteness seam moving **both** the
upkeep and the build pile, the three sight states, the keeper released back into the free floor, the
friction **mean**, the reach **minimum**, the traced path, and `kept_by`. **Each payoff carries a
trail sibling beside its dirt-road one** — a wholly trailed run holds the *trail rung's own* reach and
reads the *trail rung's own* friction, with one bare tile in it holding nothing — which is what pins
the `keeping_is_met` filter; and a built road in shortfall carries nothing on either term, so the
filter is not simply *everything passes*. The road-preferring walk is pinned by seating a road on a
tile the tie-break would have passed over and asserting the path takes it **at the same length**.

`core_sim/tests/route_traffic.rs` drives the route passes in **stage order** through real turns
(`balance_supply_networks` + `advance_roads` in Logistics; in Population a local `pay_road_keepers`
driver calling `settle_bands_roadwork`, **the same function production calls**, so the harness is a
driver and not a second arithmetic): a
run of tiles worn in by pooling nobody ordered, the traffic cap, the disuse fade, the friction payoff
paired against an unrouted run, the off-the-run negative control, the keeping (a road that holds
beside one that loses its rung, the proportional bleed, the grace, the keeperless road that is finally
pruned), the free floor owing nothing beside a dirt road that does, the remote road's dearer bill, and
**Ray's case** — `two_bands_each_keep_half_the_tiles_between_them_and_each_pays_only_for_its_own_half`,
in two phases so *"each pays only for its own"* is measured rather than assumed.

Its fixture turn runs the five passes in stage order — `balance_supply_networks`, `advance_roads`,
`credit_route_lessons` (Logistics), then `advance_band_movement` and `pay_road_keepers` (Population)
— so a **march's one-turn lag is visible rather than papered over**. On top of the above it carries:
the march banked on the **following** turn and **exactly once** (a third turn standing still moves
nothing); the reach payoff as a **capability** (two camps 8 tiles apart deliver nothing, and deliver
over an unbroken kept road) with its containment half (one bare tile closes the link) and its **free
floor** half (a wholly trailed run holds a link open at 5 tiles, which is the test that proves the
payoff filter); the no-regression half (a pair inside `reach_tiles` with no roads pools as before);
and the knowledge chain end to end — `roadbuilding` rising from `0` over a standing trail and
**stopping dead** when one tile drops to a `path`, length as the multiplier (a 6-tile connection is
worth twice a 3-tile one), the weakest tile picking the lesson (all-dirt teaches `paving`, one trail
tile in it teaches `roadbuilding`), a run of paths teaching nothing, and the `grade` gate opening —
asserted through the very `knows(..)` expression the command's refusal reads.

`core_sim/tests/route_sight.rs` and `core_sim/tests/route_wire.rs` both drive **whole turns** through
`build_test_app`, deliberately: the thing under test in the first is that something *hands*
`grants_sight` to the fog, and the second asserts on the **encoded envelope** through
`root_as_envelope`, because a field that never reached the codec still passes an in-process assertion.

`core_sim/src/bin/server.rs`'s test module owns the two verbs: the knowledge gate with its liveness
half, the rung-beneath refusals on both verbs, **one keeper per tile**, adoption of a keeperless road
paired with the already-at-that-rung refusal, `abandon` releasing the keeper *and* its entry, and the
builders' pool actually raising a graded tile (paired against an unstaffed pool banking nothing).

⛔ **EVERY KEEPING FIXTURE SEATS ITS ROAD AT THE DIRT ROAD, NOT THE TRAIL.** The trail is free, so a
fixture seated there has no bill to meet or miss and every claim about a shortfall, a grace, a payer or
a sight grant would be vacuous.

**Every live claim was falsified in isolation** — see the PR's falsification table. The breaks that
caught the most: **paying every band standing on a road instead of its keeper** fails Ray's case and
the one-keeper command test; **stamping the bill only where a keeper exists** fails the keeperless
decay; **taking the friction as the best tile instead of the mean** fails the averaging claim;
**taking the reach as the best tile** fails the weakest-tile claim; **dropping the remoteness from
`road_rung_cost`** fails the seam test's build half while its upkeep half still passes, which is what
proves the two really are one seam.

A harness that stands `balance_supply_networks` up must insert `RoadRegistry` and `RouteTrafficLog` —
an empty registry is the shipped turn-1 state, which is what makes those files' pooling numbers the
**unrouted** reading they have always been.

## See Also

- `intensification.md` — the ladder engine, the rung grammar, `UpkeepScale`, and the standing upkeep
  this branch inherits rather than rebuilds
- `cultivation.md` — the per-tile improvement this branch is structurally a copy of
- `campaign.md` → "Supply Network" — the pooling that wears these roads in and reads their payoff
- `connections.md` — the edge beside this one, and the keystone the sight grant must not route through
- `docs/plan_standing_upkeep.md` §4.13 / §4.13b — the design, and the corrections to #532 and
  `plan_contact_and_logistics.md` §Q4
