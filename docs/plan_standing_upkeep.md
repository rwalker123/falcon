# Plan: Standing Upkeep — what it costs to HOLD an improvement

**Status:** design. Filed under issue #532 (the route ladder), because the route ladder is what
exposed the gap — but the mechanism is general and the routes are its last consumer, not its first.

## 0. The gap, in one line

**Every cost on the intensification ladder today is a *job* — a fixed pile of work you finish once.
Nothing on it is a *rate* — work you must supply every turn to keep what you built.**

`docs/plan_unit_costed_work.md` priced improvements in work units and made turns the output. It
priced **building**. It did not price **holding**, and without that term the ladder is a straight
upgrade path: every rung is strictly better than the one below it once paid for, so the only
question a player ever faces is *"can I afford the build?"* — never *"is this worth keeping?"*

The route ladder cannot be built on that. Its whole design is that **each rung is cheaper to travel
and dearer to keep**, so you pave where the traffic pays for the upkeep and a trail is the right
answer everywhere else. Remove the standing cost and the ladder collapses into *"pave everything,
eventually"*.

---

## 1. It is not a new idea — it exists three times, in three vocabularies

The shape is already in the sim. What is missing is that it is **one** thing.

| Where | How it is expressed | Currency |
|---|---|---|
| A penned or tamed herd needs keepers | `herders_needed`, from `animals_per_herder` — a **headcount**. Fall short and the flock **sheds animals** | people |
| A pen's animals must eat | `pen.upkeep_per_biomass × biomass`, offset by the footprint's grazing; hay and then the larder pay the remainder; underfed herds **shrink** | collected food |
| A plant improvement rots | `decay_fraction_per_turn × work_cost` bled off the meter every turn nobody works it, past `grace_turns` | **work units per turn** |

The third row is the tell. **The plant bleed is already denominated in work units per turn** — it is
numerically the exact quantity a standing upkeep would be. The only difference is which direction it
points: today it is *rot that happens when the crew leaves*, rather than *a bill the crew pays to
stay*.

Three consequences of leaving it as three mechanisms:

- **"What does it cost to hold this?" has three different answers** depending on which rung you ask.
- **Only the animal web's answer is legible.** The plant web's cost is invisible until you walk away
  and find the meter drained — the player is never shown a price, only a punishment.
- **Neglect is binary.** `tended_this_turn` is a flag: a source is worked or it is not. A crew half
  the size the source needs counts as fully worked, so under-crewing has no cost at all until it
  reaches zero, at which point it has the full cost.

---

## 2. The model

### 2.1 Three costs, one currency

An improvement may declare up to three things. All are in **work units**
(`intensification::PER_WORKER_OUTPUT` = one worker-turn at the food peak with no gear).

| Cost | Shape | What it buys |
|---|---|---|
| **Build** | a **pile**, drawn down once | the rung transition. Ships today as `RungBuild::work_cost` |
| **Upkeep** | a **rate**, per turn, forever | the improvement stays at its rung |
| **Production** | a **rate**, per turn | the yield actually taken |

**Every one of them is optional.** A route declares upkeep and no production — that is the point of
building the term against routes, because an architecture that assumes every improvement produces
something breaks on the first one that does not.

### 2.2 The PLAYER allocates workers per activity — TWO on the source, ONE on the band

**A source carries two independent worker allocations from a band, and the player states each.**
There is no split rule to derive, because there is nothing to derive.

| Activity | Set by | Was |
|---|---|---|
| **take** | `assign_labor` — unchanged; its `workers` is the take crew | the only allocation |
| **build** | the improvement verb, which now takes a worker count | rode the take crew |

**The KEEPING is not one of them — it left the tile.** It is a band-level standing role, one per food
web; §2.5 is now that decision's home.

`extend_pen` takes a worker count on the same grammar and routes through the **build** allocation: a
ring rides the same `animal:pen` rung as the pen it widens, so it cannot be the one build in the game
that is free.

They draw on the same finite band, so **competition between them is the opportunity cost** — and it
is visible in the numbers the player typed rather than buried in a fraction or in an ordering they
cannot see. **No cap** means no cap on any *one* activity — fifty hands may widen a pen and fifty more
gather beside it, and the constraint is what those hands are not doing elsewhere — but the whole
allocation can never exceed the band. Two rules hold that line, and they answer different questions:

- **A command that asks for more hands than the band has idle is REFUSED**, naming what is available
  (*"Cultivating needs 9 workers — the band has 4 idle."*). It refuses where `assign_labor` clamps,
  because a smaller *gathering* crew is a coherent version of the same order while a quietly-smaller
  *build* crew is a commitment the player believes they made.
- **A band that SHRANK sheds**, tail-first and **build → take** within the row: a band that has just
  lost people keeps gathering longest, because the build is an investment and the food is not. A
  keeping role is a row of its own, so it is shed with the tail like any other.

**The gear a build's crew carries is quoted at that crew, not at the band's crew on the source.** A
tool's contribution is a rate per worker, so it is summed over the *builders*; the coverage behind it
is resolved over the builders too.

The arithmetic is then trivial:

```text
supply           = the build crew below the meter's cost, the band's keeping POOL at it (§2.4/§2.5)
net              = supply − upkeep_demand
build_work       = max(0, net)                                  // − what the crew's gear takes off the job
take             = min(take_workers × per_worker_capacity, source_offer)
```

> #### `work_cost / crew` IS NOT THE BUILD PACE, and this arc is what changed that
>
> The maintenance rate is owed **every turn, while building and while held alike**, so a build crew is
> paying it too and only its **surplus** is progress: `turns = work_cost / (crew − rate)`. Earlier
> prose in this file asserted `work_cost / crew`; that identity is gone, deliberately.
>
> **A crew at or below the rate never finishes.** It holds the meter exactly where it is, or takes it
> backwards. That is a real minimum-viable-crew threshold rather than a slow build, and it is sharper
> than anything else in the game — so `build_turns_remaining` answers **no estimate** at a non-positive
> net rather than a large number, and `upkeepDemand` / `upkeepWorkersNeeded` publish the threshold on
> **both** sides of completion so a compose sheet can say *"this crew is below it"* before the player
> commits.

#### This is what dissolved the dip

`yield_fraction_while_building` (0.50 on all four shipped rungs) said *"this crew is preparing ground,
not gathering"* — which is true of a **shared** crew and of nothing else. Separate the allocations and
it is not a dial, it is a fact about where the hands are: the gatherers on a patch carry what
gatherers carry, and what a Cultivate costs is the people who are clearing instead. Four magic numbers
retire, along with a term nobody ever chose (the plant web sat at 0.25 for years purely because that
was the pre-move constant's value).

**The cost no longer depends on a regime the player cannot see.** Under the dip, a crew big enough to
saturate the source's standing stock paid *nothing* for its build, because the ceiling bound it
either way; a thin crew paid the full fraction. The price is now the same statement at every
staffing.

#### And it retired `crew_needed`

A rung declared a **staffing floor** under the source's published `workers_needed`, because that count
was inverted out of the *take* and a building crew was paid a dipped take — so committing to a
25-turn Cultivate made the panel ask for **one** forager where the same wild patch asks for two. With
each activity stating its own crew there is no blended count for a floor to raise, and
`workers_needed` is answered **per activity**: hands to meet the upkeep, hands to haul the offer.

#### The floor comes off the build rate

`learn_multiplier(floor)` no longer scales `build_accrual`. The shipped rule — *a crew pulling hard on
the source it is improving builds slowly* — was written when one crew did both jobs. **With separate
crews the build crew is not pulling anything**, and worse: a build crew on a source nobody is
harvesting has **no floor to read at all**, so the term would have to be invented from a default
nobody chose.

- `learn_multiplier` keeps scaling **knowledge accrual**, where *how much you leave standing shapes
  what you learn* still holds.
- **`MANAGED_SOURCE_FLOOR` disappears with it** — it existed only because rung-3 builds had no real
  floor to pass. The one place it still meant something (a managed rung's *lesson*) is now stated by
  a seam of its own.
- **Upkeep never reads the floor either.** It is charged against raw worker-turns, because a route has
  no escapement floor at all and an upkeep that read one could not be applied to this arc's actual
  target.
- **Pacing is unchanged**: `learn_multiplier` is exactly `×1.0` at the food peak, which is the floor a
  fresh assignment carries. Only sub-peak floors build faster now.

### 2.3 Build completion hands the crew to the KEEPING ROLE

The turn a build completes, its crew has finished the thing it was staffed for. **If the finished rung
declares an upkeep, those hands move onto that web's standing role on the band**; if it declares none,
they free up.

Without the carry-over a brand-new pen starts decaying on turn one because nobody noticed it had begun
costing something — a punishment for arithmetic the player cannot see, which is the same failure §2.5
exists to prevent.

**The head count does not move**, so no refusal is owed: the crew comes off the source's build
allocation and lands on the band's role. **Added, never assigned** — a band already keeping other
sources on that web keeps them.

**Either way it is announced.** The completion already pushes a feed line; the hand-off rides the same
channel, because a crew moving is a thing the player has to re-task around and a silent re-allocation
is only ever discovered later.

### 2.4 ONE FORMULA: net = supply − rate. And a rung is not lost on the first dip

Meet the rate and the net is zero and the improvement holds. Go over and the surplus is progress. Go
under and it rots, in proportion to how short you are.

```text
net = supply − maintenance_rate
  net > 0  →  the surplus is BUILD PROGRESS
  net = 0  →  the meter HOLDS exactly where it is
  net < 0  →  it ROTS: (shortfall / demand) × the rung's own decay rate, past the grace
```

**The rate is owed ALWAYS.** What the meter's state decides is **only who supplies it**:

| the meter | state | who supplies the rate |
|---|---|---|
| **below its cost** | *building* | the **build crew** — surplus above the rate is progress |
| **at its cost** | *maintaining* | the band's **keeping pool** — surplus does nothing, shortfall rots |

#### AND THE VERB IS DERIVED FROM THE METER TOO

The same state test answers *which rung is being built*, so nothing needs to be stored or restated:

| meter | state | who declares |
|---|---|---|
| **zero** | nothing in flight | **the player** — a wild patch could climb to tended *or* be sown, and the sim cannot guess |
| **between zero and its cost** | building that rung, **implied** | nobody — the progress banked on it *is* the answer |
| **at its cost** | maintaining | nobody |

**Per METER, not per source**: a completed tended patch the player wants to sow is still a
declaration, because its field meter is at zero. **Newest meter first**, so a Field with progress on
it governs the tended ground beneath — a `Cultivate` on a Field is dead rather than stalled.

**What it fixed.** A build banks nothing unless the rung's verb is in flight, and completion freed the
declaration — so a completed rung that eroded back below its cost re-entered the *building* state with
nothing set and could not be repaired until the player re-issued `cultivate`. They never withdrew that
intent. **A player who has paid for a rung and watched it slip adds hands, not a command.**

**`abandon_improvement` is RETIRED**, not arbitrated. It existed to let a player walk away from a
25-turn commitment while the *verb* was the commitment; the commitment is the **hands** now, so you
walk away by unstaffing the builders (`cultivate <faction> <x> <y> 0`). A command that cleared a
derived value would either do nothing or fight the derivation. Its proto field is reserved, never
reused — and the "nothing left to build" test went with it, since a stale declaration on a finished
meter derives to `None` on its own.

**A meter at exactly zero clears back to "the player must declare"** together with everything else
that empties: `reconcile_owner` drops the owner and the committed crop on the same edge, and the
stamped cost goes with them. One notion of empty, not three.

That is one state test and two costs. There is no third concept: an earlier cut of this arc gave an
unfinished meter its *own* demand (`meter_raising_demand`), which was redundant — it is the same rate
throughout — and carried a per-web exception with no fact under it. Both are deleted. *You cannot be
billed to hold something you have not finished building* is answered by **who pays**, never by
discounting the bill.

**Both webs answer identically.** An unfinished rung on either web is owed the same rate from its
builders; an under-supplied one decays, which is a **meter bleed** on plants and a **shed** on
animals. There is no "the animals are standing there whether or not the fence is up" exception.

#### THREE DIALS, because the decay must decouple from the demand

| | question | dial |
|---|---|---|
| **demand** | how much work per turn does holding this want | `upkeep.work_per_turn` |
| **decay rate** | once rotting, *how fast* | `upkeep.meter_decay.per_turn` |
| **grace** | how long under-supplied before rot begins | `upkeep.grace_turns` |

**`shortfall` USED TO BE the decay**, which welded the first two: raising a demand made the
improvement rot faster in exact proportion, so neither number could be retuned. Splitting them is what
let the plant demands become whole numbers a player can staff exactly — **`plant:tended` 2,
`plant:field` 4** — while the rot rates stayed precisely where they were (**0.5** and **0.75**, the
retired `decay_fraction_per_turn`'s own product). The demands moved; the rotting did not.

**The animal web already had the rate half**, which is why neither animal rung declares a
`meter_decay`: its shed is `shortfall_fraction × head count` at the species' own
`pen_escape_fraction` / `pastoral_escape_fraction`, and those fractions **are** the rate. A second one
on the rung would be two numbers for one mechanic. Only the shortfall *fraction* is shared.

This retires the binary flag. `tended_this_turn` / `tamed_this_turn` and the "is this source worked"
question go away, and with them the whole class of ruling about whether a lightly-crewed source counts
as worked.

#### A RUNG IS NOT LOST THE INSTANT ITS METER DIPS

**A completed meter sits *exactly* at its own cost**, so a `progress >= cost` predicate made the first
bleed of any size revoke the rung: finish a Cultivate and the patch could be out of *tended* before
its keepers were assigned. No grace and no rate could fix that, because the loss was a **threshold
test rather than a rate**.

- The rung's **achieved** state and the meter's **fullness** are two facts. The predicates
  (`is_cultivated()` / `is_field()`) compare against a **stamped retention bar**, which is where the
  loss point lives — one seam rather than a hundred call sites.
- The bar is `retain_fraction × cost`, stamped **at completion** and cleared the turn the meter falls
  below it. A fraction, so it survives a cost retune; stamped, so the predicate needs no config.
- **The rung is still EARNED at `progress >= cost`.** Only losing it moves.
- Shipped at **0.75** on both plant rungs: a completed tended patch survives **28** wholly
  unmaintained turns and a Field **27**, against `grace + 1` — three and two — before, and re-earning
  the rung then costs only the work that rotted.
- **KEEP IT ORTHOGONAL TO THE STATE TEST.** *Building vs maintaining* is the meter's fullness and
  decides who pays the rate; *is the rung still achieved* is the retention bar and decides what the
  ground pays out. A patch at 99% is **building** (a repair, which its build crew may run) and
  **still tended**. Folding the two would make a rung's loss and a rung's repair the same edge.
- **No animal rung needs a bar**: `domestication_progress` is monotone-up and a pen is held by a
  stored flag, so no animal rung can be lost by a meter dipping.
- **A rung's BENEFIT stays binary on the achieved state.** Scaling a rung's payout with its meter is a
  real proposal and a much larger one; it is deliberately not this.

`grace_turns` survives unchanged in meaning: consecutive turns of shortfall forgiven before decay
begins.

### 2.5 MAINTENANCE IS A BAND-LEVEL ROLE, and the shortfall split is the player's

**The keeping is not a crew on the tile.** It is a **standing role on the band**, in the same family
as the local scout and warrior dials: `agriculture` keeps the plant web, `husbandry` the animal one,
each staffed with `assign_labor <faction> <band> agriculture|husbandry <workers>`.

- **One role per WEB, because the two webs are already separate ladders.** This is their existing
  split, not a new axis.
- **The band's demand is the SUM** over everything it holds on that web, and the pool supplies against
  that total. Only a **built** rung draws: a meter still being raised is owed its builders.
- **`0` is still how you say "stop maintaining"** — for a whole web rather than for one source.

#### WHY IT LEFT THE TILE: an indivisible supplier WASTES what it does not spend

A per-source keeper crew has to round a fractional demand up to whole workers and throws the remainder
away, **once per source** — and the waste grows as gear makes a hand worth more. A pool has no
leftover by construction: every unit either meets a demand or is still in the pool.

#### The shortfall split is a per-band PLAYER OPTION, and both modes ship

When the pool cannot cover the sum, there are two defensible answers and the choice is the player's
(`upkeep_mode <faction> <band> spread|priority`):

- **Spread** — proportional to demand, so everything degrades a little. The **default**: it is what an
  unstated policy means, since nobody is singled out.
- **Priority** — fund sources completely until the pool runs out, **most-invested first**, so the
  biggest investments stay safe and the marginal ones rot. Ordered on the at-risk meter's **stored
  cost** (not its live progress, which would slide a source down the order exactly as it started to
  need the hands), tie-broken on a stable per-source key so the ordering is **total and
  deterministic** — a checkpoint restores the same allocation.

The mode rides the band's allocation, so it is `SimState` and survives a rollback.

#### The per-source readouts STAY, and they answer a better question

`upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` remain per patch and per herd, with `supplied`
becoming that source's **share of the pool**. They stop answering *"did you staff this one"* and start
answering *"where is my pooled shortfall landing"*, which is more useful, not less.

#### What this replaced, and the trap it removed

The predecessor was a per-source `maintain` command with a hard-coded priority *inside* one crew's
turn: a pen needing 5 work a turn, staffed for 2, spent both on upkeep, was still 3 short, decayed
anyway, and the crew had spent itself for nothing. **The pool removes that by construction** — a band
that cannot cover its web decides *how* it falls short rather than paying into a losing position it
could not see.

**Routes need the "stop" more than anything else does.** The ladder's central claim is that you pave
where traffic pays the upkeep and let it be a trail elsewhere. Without the ability to unstaff the
keeping, the only way to stop maintaining a paved road is to stop *using* it — which also stops the
traffic that made it worth having.

### 2.6 Upkeep has a scale term, and that is the generic piece

Upkeep cannot be a flat per-rung number, because what makes a thing expensive to hold differs by what
it is:

| Improvement | Scaled by |
|---|---|
| a pen | the herd it holds (biomass / head) |
| a farm | its area, or the capacity it works |
| a route | its length **and the terrain it crosses** |

So a rung declares an upkeep **rate** plus **what scales it**, chosen from a bounded set — the same
"config over coded primitives" idiom the ladder already uses for `behavior`
(`movement`/`feeding`/`harvest`). Adding a scale primitive is coding one thing once; using an
existing one is a config edit.

Build already has this hook in miniature (`taming_cost_multiplier` scales a Tame by species; pen
extension prices per ring), so this generalizes an established pattern rather than inventing one.

### 2.7 The resource half — COLLECTED goods, never the land

**Upkeep is work, and optionally a draw on stored goods your people gathered.** A road wants hands and
stone; a pen wants keepers and **hay**.

**The line is collected versus growing there.** Hay is cut, carried, stored and fed — a good that
exists because somebody made it. Grazing is the land feeding the animals for free. Stone and timber
for a road are quarried, hauled and laid; the rock in the hillside is not a road input until someone
moves it. So hay and concrete are the same kind of thing, and grazing and the standing hillside are
both **not upkeep at all**.

That makes the land's contribution an **offset**, which is exactly what the shipped pen already does:
the footprint's grazing covers what it can, hay covers some, and the larder pays only the remainder
(`pen_pasture_fraction`, `penHayFood`, `penLarderBill` — three terms of one demand, already asserted
to sum to it). A pen on lush pasture is cheap because the land is doing the work; a pen on barren
ground pays in full. **A route down a river valley versus one over a range is the same sentence**, and
`infrastructure_cost` is where that per-terrain answer is already written.

**What is NOT upkeep**, and both were reached for during design:

- **Inputs to a production activity** — seed for sowing, fuel at a drying rack, materials at a bench.
  Those are consumed when the activity *runs*, not by the thing *existing*.
- **The animals eating, considered on its own.** A herd eats because it is a herd; a pastoral herd
  with no fence at all eats the same amount. What penning changes is *where they can eat*, so the
  cost that appears is the hay and larder needed to cover what the land no longer can — a
  land-access consequence, and it vanishes the moment the land can cover it again.

**Labor that always travels with the keeping is folded into the keeping, not split out.** Haying is
genuinely work, but the keepers are already there and the pen's upkeep already scales with the herd —
a separate feeding-work line would scale off the same term and would never be staffed independently.
A number with no decision attached is not a lever. The same disposes of a road's quarrying and
hauling: that work is the upkeep, not a second line beside it.

---

## 3. What this replaces

| Shipped today | Becomes |
|---|---|
| `herders_needed` as a standing headcount | an upkeep **rate** in work, scaled by herd size |
| the shed (`shed_uncontained_animals`) | the animal web's **shortfall penalty** |
| `decay_fraction_per_turn` as an independent dial | `upkeep.meter_decay.per_turn` — the rung's own rot rate, scaled by **how short** you are rather than being the shortfall itself |
| `tended_this_turn` / `tamed_this_turn` binary flags | retired — shortfall is continuous |
| `RungBuild::crew_needed` (a staffing floor) | retired — the player states the build's crew |
| the `maintain` command + `LaborAssignment::maintain_workers` | the **band-level** `agriculture` / `husbandry` roles and `upkeep_mode` (§2.5) |
| `progress >= cost` as the LOSS test | `upkeep.meter_decay.retain_fraction` — a rung is earned at its cost and held to a stated fraction of it (§2.4) |
| `abandon_improvement` + the "nothing left to build" test | retired — the build verb is **derived from the meter** (§2.4), so there is no stored authority to clear |
| `work_cost / crew` as the build pace | `work_cost / (crew − maintenance_rate)` — the rate is a tax on building, and a crew at or below it never finishes (§2.4) |
| `learn_multiplier(floor)` on the build rate | retired — a build crew is not pulling on the source |
| `yield_fraction_while_building` (×4 rungs) | retired — the build has its own crew, so what it costs is the hands on it |
| `pen.upkeep_per_biomass` + the pasture/hay/larder split | the **resource half** of the pen rung's upkeep (hay and larder), with pasture as its **offset** — unchanged in behavior |
| `TerrainDefinition::infrastructure_cost` (never read) | the route rung's **scale term** |

---

## 4. Sequencing

Seven slices. The ordering constraint that is **not** negotiable is that the client readout lands
before any tuning — `plan_unit_costed_work.md` §11 learned this: *a cost spread with no readout
change is invisible*.

1. **This design doc.**
2. **The mechanism.** The upkeep term on the ladder engine — config shape, the per-turn demand, the
   one-budget priority order, shortfall→decay, the maintain toggle. **No rung declares an upkeep
   yet**, so the upkeep half is dormant.
   > **It is NOT pacing-neutral, and the one change it makes is the arc's headline.** The dip and the
   > budget cannot both be live without double-counting work, so `yield_fraction_while_building`
   > retires here. That makes slice 2 a single reviewable claim — *the dip is now emergent* — with a
   > measurable before/after, and leaves slices 3 and 4 as config plus penalty-rewiring on a seam that
   > already exists.
3. **The plant web onto it.** `plant:tended` and `plant:field` declare upkeep; the binary bleed
   retires; the dip dissolves.
4. **The animal web onto it.** `herders_needed` becomes an upkeep rate; the shed becomes its
   shortfall penalty; the feed becomes the resource half.
5. **The client readouts.** What an improvement demands, what the crew supplied, the shortfall, and
   the maintain toggle. Before any tuning.
6. **Gear as productivity.** A kit raises what a supplier delivers **per turn** rather than
   subtracting from the job. Decided because a job is a pile and an upkeep is a rate: subtraction has
   nothing to subtract from, so the shipped build model needs a second mechanism for upkeep, while one
   supply expression feeds both — a build divides a pile by it, an upkeep compares a demand against
   it. What it gives up is the shipped arc's scale-sensitivity (a flat turn saving, so a tool nearly
   frees a small job); what it gains is that a tool can no longer drive a job to zero, and a hoe fades
   on a farm by being *insufficient* rather than by arithmetic. **This changes the shipped build
   model**, not only upkeep.
7. **Priority as a GENERAL per-source property.** Player-ordered, drag-and-drop, its own column — and
   deliberately not a maintenance-funding list. The auto-assigner sketch (§6) wants tile priority for
   its own reasons, and two orderings meaning almost the same thing would drift apart. Pooled
   maintenance is its **first consumer**, not its owner; the shipped most-invested-first ordering
   survives as the **tie-break** beneath an explicit rank, which matters because most sources will sit
   at the default.
8. **Symmetric partial credit.** A rung's benefit scales with its meter in **both** directions — a
   half-built field pays half a field, as a decayed one does. Wanted; it is what makes the model
   coherent both ways, and it removes the last discontinuity: today a build pays nothing for its whole
   span and then everything at once. **The blast radius is wide but shallow** — the ~100 binary
   predicates stay (they answer *"has this rung been achieved"*, which still gates the verbs and the
   knowledge); every *payout* branching on them becomes an interpolation on the meter-over-cost
   fraction already published. **What to watch:** the total work is unchanged, so the arithmetic of
   *"is this worth it"* does not move — but the payoff starts on turn one, which softens the
   commitment considerably. That may be right, given this arc has been about removing cliffs; it
   should be a deliberate smoothing rather than a discovered one.
9. **The route branch (#532 proper).** Routes as the ladder's third branch, `infrastructure_cost`
   wired for the first time, traversal-driven progress from supply links, shipments and movement.
10. **The tuning spread.** Config-only, once the readouts can show it.

Slices 3 and 4 are separable but adjacent; they are kept apart because the two webs' penalties are
genuinely different code paths (a meter bleed versus a flock shedding), which is where the mechanism
will actually be tested. Merging them is a merge-time call.

## 5. The allocation layer this arc kept running into

**Leftover work units are a symptom, not the disease.** Every fix for them — bank the surplus, spill
it into the build, round the demands — patches a mismatch that should not exist: an **indivisible
supplier** meeting a **per-source demand** wastes whatever it does not spend, and the waste grows as
gear improves.

Pooling maintenance (§2) fixes it for one activity. The general form is to stop allocating at the
tile at all: the player states **role pools** and **tile priorities**, and the sim assigns. That is
not a rewrite — the sim already resolves everything off per-source assignments, so an assigner that
*produces* those each turn leaves accrual, take, upkeep, decay, forecasts and the wire untouched. The
assignment layer stops being **authored** and becomes **derived**, which is *"turns are the output"*
applied one level up.

Three things decide whether it is good: **legibility** (*"why are these suppliers here"*, and the
more actionable *"these had nothing to do"*); **role-to-kit typing** — roles want to be job-typed
because kits are, which is why spilling a surplus between activities gets strange; and **churn**,
which is deliberately **not** being engineered against — it is a problem nobody has observed, and the
cheap answer is to make reassignment observable so it would be noticed rather than inferred.

---

## 6. Open items

- **`retain_fraction` is a playtest dial, and it is the one to argue with.** `0.75` on both plant
  rungs puts a wholly unmaintained improvement at ~27 turns before it is revoked — about a season.
  `0.5` would give ~53, which reads as *"never lost"* while a rung's benefit is still binary. **It
  becomes much less load-bearing once §4's symmetric partial credit lands**, because a rung sliding
  back turns into a fading payout rather than a status you lose — so it is not worth over-tuning now.
- **The scale primitives' bounded set.** `Flat` and `SourceLoad` ship; a route wants length × terrain
  (`infrastructure_cost`), which is slice 9's to add. Whether that is a third primitive or a
  parameterisation of one is a question for the code, not for this doc.
- **Whether the two keeping pools should split further.** Agriculture and husbandry split because the
  webs do. A finer split — a herd keeper's kit versus a field tender's — is only meaningful once a kit
  declares a maintenance contribution, which none does today, so splitting now would invent a
  distinction nothing can express. It becomes a config-shaped change the moment §4's gear-as-
  productivity lands.

> **Two items were retired rather than answered, and both are worth recording as such.** *"What the
> maintain toggle does to a build in flight"* went with the toggle itself — keeping is a band-level
> pool, so *"stop maintaining this one thing"* is no longer expressible, and that is deliberate. And
> *"a building crew takes nothing"*, which this section flagged as the first thing to check in play,
> was true only of the one-budget model it was written under: with the crews stated separately, the
> gatherers beside a build carry exactly what gatherers carry, and what a build costs is the hands on
> it. Neither warning survives its mechanism.

---

## See Also

- `docs/plan_unit_costed_work.md` — the arc that priced **building** in work units; this prices
  **holding** in the same currency.
- `docs/plan_contact_and_logistics.md` §Q4 — the route ladder, which needs this term to exist.
- `.claude/rules/core_sim/intensification.md` — the ladder engine as built.
- `.claude/rules/core_sim/husbandry.md` — the pen's feed and the shed, the two shipped mechanisms
  §2.7 and §3 generalize.
