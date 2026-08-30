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

**Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in.
`RouteTrafficLog` is **drained** by the accrual (`std::mem::take`), so a turn with no pooling wears
nothing rather than re-wearing last turn's links.

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
| reach (`holds_link_to_tiles`) | **the WEAKEST tile** | `routes::path_reach_tiles` — **not yet consumed by the sim**; that is 13b's |
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

**Only a BUILT and KEPT tile counts** on either term — the same `grants_sight` condition, for the same
reason: an unmaintained road is not carrying anything.

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

| rung | verb | `unlock_knowledge` | `earns_knowledge` | `build` | `upkeep` |
|---|---|---|---|---|---|
| `path` | — | — | — | null | null |
| `trail` | **— (it forms from use)** | **—** | `roadbuilding` | 40 work | **null** |
| `dirt_road` | **`grade`** | `roadbuilding` | `paving` | 110 work | yes |
| `paved_road` | **`pave`** | `paving` | — | 260 work | yes |

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
> **Exactly one thing in the whole simulation banks route work**: `route_traffic.walked(..)`, called
> from `supply.rs`'s pooling-link pass where two camps sharing a larder walk between them. Herds
> move, but no herd has ever recorded a step as route traffic — so every path on the map is worn in
> by the **player's own** trade-pooling bands, and the floor rung was displaying them a trail the
> animals made. Ray hit it in play at tile (60,40).
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
whose row is gone. A road is named by its **keeper**, recorded on the road, which
`LaborAllocation` cannot see — so `holds_build_source` answers `true` for `Road` unconditionally and a
road entry is retired by three explicit paths instead: **arriving** at its destination,
`retire_entries_already_built` (which reads the rung the tile holds), and `abandon` / `unqueue`.

**The build is its own arm in `advance_labor_allocation`, after the assignment loop**, because that
loop visits `assignments` and can never reach a road. It banks the whole `builders` pool at the
entry's own kit, capped at the destination rung's top, and **banks nothing unless the ordering band is
still the keeper** — a band whose `grade` was superseded (the tile decayed back into the free floor,
or another band adopted it) puts no work on ground that is no longer its job.

- **No shipped tool declares a `build_work` serving `route`**, so `BuildersGear`'s route reading lands
  on `default_kits.builders` and road builders work **bare-handed** — the same intended emptiness
  `default_kits.roadwork` ships for their keepers. `FOOD_WEB_BRANCHES` still excludes `Route` for that
  reason, and is what gets widened the day a barrow declares the stat.
- **A road publishes no build estimate.** `buildTurnsRemaining` and its four siblings are per-patch and
  per-herd scratch, and a road has no source row to stamp them on; the arms are stated as no-ops so a
  future row cannot be forgotten.
- **No material pile.** No route rung declares a material (see below), so `head_build_legs` answers
  `None` for a road and the draw is exact rather than defaulted.

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

### ⛔ NO ROUTE RUNG DECLARES A MATERIAL, AND THE REASON IS THAT THE MATERIAL DOES NOT EXIST

§4.13 has the paved road swallowing **stone** on both the pile and the rate. It is not declared,
because `stone` is not in `materials.json` and the ladder's load-time check rightly rejects a rung
naming a material the table does not carry.

**Adding a stone with no way to obtain one would be worse than declaring none** — it ships a rung that
can **never be held**, which is a harder failure than one that is cheap to hold. Quarrying is the
crafting arc's. When a stone material lands, the rung takes `build.materials {stone: 30}` and
`upkeep.materials {stone: 0.08}` and nothing else changes.

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
checkpointed. Its TOE job is `KitJob::Roadwork` and `default_kits.roadwork` is the bare `none` kit, so
**road keepers work bare today**.

### ⛔ THE BILL IS STAMPED ON EVERY ROAD, KEEPER OR NOT

`systems::settle_route_keeping` runs in `TurnStage::Population`, `.after(advance_labor_allocation)`
and `.before(advance_crafting)` — both edges declared — and does two things in order:

1. **stamps the interpolated bill on every road in the registry**, first-write-wins;
2. **pays**, from every band whose `roadwork` row is staffed, against **the roads it keeps**
   (`RoadRegistry::kept_by`).

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
   count the idle turns. The cap takes a `max` against the tile's own position, so a road **above** the
   ceiling is untouched rather than dragged back to a trail every turn a link runs over it;
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
| `upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` / `upkeepWorkersNeeded` | the standing bill, all four off the **stamped** `road_keeping_basis` |
| `hasNeglectGrace` / `neglectGraceRemaining` | the **countdown**, through `routes::road_neglect_grace_remaining` at the at-risk rung |
| `grantsSight` | the resolved *"is this road lighting its tile"* |
| `frictionMultiplier` / `holdsLinkToTiles` | what the rung is buying, off the tile's stamped `payoff()` — the first a MEAN along a journey, the second the journey's MINIMUM |

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

### The band roll-up — `roadworkDemand` / `roadworkSupplied` / `roadworkShortfall`

On `PopulationCohortState`, summed by `settle_route_keeping` over **the roads the band keeps**.
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
joins on the tile pair, `AnnotationRenderer.draw_road_network` stamps one HEX per road, and the tile
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
friction **mean**, the reach **minimum**, the traced path, and `kept_by`.

`core_sim/tests/route_traffic.rs` drives the three systems in **stage order** through real turns
(`balance_supply_networks` + `advance_roads` in Logistics, `settle_route_keeping` in Population): a
run of tiles worn in by pooling nobody ordered, the traffic cap, the disuse fade, the friction payoff
paired against an unrouted run, the off-the-run negative control, the keeping (a road that holds
beside one that loses its rung, the proportional bleed, the grace, the keeperless road that is finally
pruned), the free floor owing nothing beside a dirt road that does, the remote road's dearer bill, and
**Ray's case** — `two_bands_each_keep_half_the_tiles_between_them_and_each_pays_only_for_its_own_half`,
in two phases so *"each pays only for its own"* is measured rather than assumed.

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
