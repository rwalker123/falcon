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

### 2.2 The PLAYER allocates workers per activity

**A source carries up to three independent worker allocations from a band, and the player states
each one.** There is no split rule to derive, because there is nothing to derive.

| Activity | Set by | Was |
|---|---|---|
| **take** | `assign_labor` — unchanged; its `workers` is the take crew | the only allocation |
| **build** | the improvement verb, which now takes a worker count | rode the take crew |
| **maintain** | the `maintain` command | did not exist |

`extend_pen` takes a worker count on the same grammar and routes through the **build** allocation: a
ring rides the same `animal:pen` rung as the pen it widens, so it cannot be the one build in the game
that is free.

They draw on the same finite band, so **competition between them is the opportunity cost** — and it
is visible in the numbers the player typed rather than buried in a fraction or in an ordering they
cannot see. **No cap** means no cap on any *one* activity — fifty hands may keep a pen and fifty more
widen it, and the constraint is what those hands are not doing elsewhere — but the three together can
never exceed the band. Two rules hold that line, and they answer different questions:

- **A command that asks for more hands than the band has idle is REFUSED**, naming what is available
  (*"Cultivating needs 9 workers — the band has 4 idle."*). It refuses where `assign_labor` clamps,
  because a smaller *gathering* crew is a coherent version of the same order while a quietly-smaller
  *build* crew is a commitment the player believes they made.
- **A band that SHRANK sheds**, tail-first and **maintain → build → take** within the row: a band
  that has just lost people keeps gathering longest, because the keeping and the build are
  investments and the food is not.

**The gear a build's crew carries is quoted at that crew, not at the band's crew on the source.** A
tool's contribution is a rate per worker, so it is summed over the *builders*; the coverage behind it
is resolved over the builders too.

The arithmetic is then trivial:

```text
upkeep_supplied  = maintain_workers × PER_WORKER_OUTPUT
upkeep_shortfall = max(0, upkeep_demand − upkeep_supplied)      // → decay, past grace
build_work       = build_workers × PER_WORKER_OUTPUT            // − what the crew's gear takes off the job
take             = min(take_workers × per_worker_capacity, source_offer)
```

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

### 2.3 Build completion hands the crew to maintenance

The turn a build completes, its crew has finished the thing it was staffed for. **If the finished rung
declares an upkeep, those hands carry onto that rung's maintain allocation**; if it declares none,
they free up.

Without the carry-over a brand-new pen starts decaying on turn one because nobody noticed it had begun
costing something — a punishment for arithmetic the player cannot see, which is the same failure §2.5
exists to prevent.

**Either way it is announced.** The completion already pushes a feed line; the hand-off rides the same
channel, because a crew moving is a thing the player has to re-task around and a silent re-allocation
is only ever discovered later.

### 2.4 Shortfall IS the decay — continuously

Meet the demand and the net is zero and the improvement holds. Fall short and **the shortfall is the
decay**: half the hands a pen needs means it slides at half rate, not at the full neglect rate and
not at nothing.

This retires the binary flag. `tended_this_turn` / `tamed_this_turn` and the "is this source worked"
question go away, and with them the whole class of ruling about whether a lightly-crewed source
counts as worked. `decay_fraction_per_turn` stops being an independent dial and becomes *what happens
when the demand goes unmet*.

`grace_turns` survives unchanged in meaning: consecutive turns of shortfall forgiven before decay
begins.

### 2.5 "Stop maintaining this" is a crew of ZERO

**The player must be able to say "stop maintaining this, take everything, let it go."** With the
maintain allocation that is not a separate control at all — it is `maintain <faction> <source…> 0`.

Without it, a hard-coded priority creates a trap. A pen needing 5 work a turn, staffed for 2: all 2 go
to upkeep, it is still 3 short so it decays anyway, and the crew has spent itself for nothing. The
player has paid into a losing position as a penalty for arithmetic they cannot see.

With the crew as the control the same position is a real decision: **hold it and spend the hands, or
write it off and put them somewhere else.** That is the principle the pen's starve mechanic already
states — *starving your animals should be a decision, not an accident*.

**There is no boolean beside the number**, and that is deliberate: a toggle would be a second way to
say what the count already says, and the two could disagree. A source maintained by nobody and a
source deliberately written off are the same state.

**Routes need this more than anything else does.** The ladder's central claim is that you pave where
traffic pays the upkeep and let it be a trail elsewhere. Without the ability to unstaff the keeping,
the only way to stop maintaining a paved road is to stop *using* it — which also stops the traffic that
made it worth having. *"Keep the traffic, drop the road back to dirt"* is unreachable otherwise.

It maps onto grammar that already exists: `abandon_improvement` is the "I am done building this"
command, and `maintain` is its standing-cost sibling, sharing its source grammar exactly.

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
| `decay_fraction_per_turn` as an independent dial | what happens when the demand goes unmet |
| `tended_this_turn` / `tamed_this_turn` binary flags | retired — shortfall is continuous |
| `RungBuild::crew_needed` (a staffing floor) | retired — the player states the build's crew |
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
6. **The route branch (#532 proper).** Routes as the ladder's third branch, `infrastructure_cost`
   wired for the first time, traversal-driven progress from supply links, shipments and movement.
7. **The tuning spread.** Config-only, once the readouts can show it.

Slices 3 and 4 are separable but adjacent; they are kept apart because the two webs' penalties are
genuinely different code paths (a meter bleed versus a flock shedding), which is where the mechanism
will actually be tested. Merging them is a merge-time call.

---

## 5. Open items

- **The scale primitives' bounded set.** §2.6 names three (herd size, area/capacity, route
  length×terrain). Whether those are three primitives or one parameterized one is slice 2's to
  settle against the code.
- ~~**Whether production stays a *capacity* or becomes work.**~~ **Settled: it joins the budget, and
  the capacity survives as a second cap.** Ray: *"it will fall out of the worker count for sure — as
  you are building, you need to allocate more workers, otherwise your take will be less."* A take
  cannot fall while building unless building and taking draw on the same pool. What the budget
  produces is an **effective worker count**, which then meets the existing
  `per_worker_biomass_capacity` exactly as it does today — so hauling stays a carrying limit and the
  work model decides how many hands are left to haul with.
  > **The consequence to watch in playtest:** the retired dip was a flat 0.50, so a building crew
  > took half. Under the budget a building crew takes **nothing**, and a band that wants both must
  > staff both. That is a materially harsher early game, and it is the first thing to check.
- **What the maintain toggle does to a build in flight.** Turning maintenance off while building the
  next rung is expressible; whether it is meaningful (you are letting the thing you stand on decay
  while climbing off it) is a play question.

---

## See Also

- `docs/plan_unit_costed_work.md` — the arc that priced **building** in work units; this prices
  **holding** in the same currency.
- `docs/plan_contact_and_logistics.md` §Q4 — the route ladder, which needs this term to exist.
- `.claude/rules/core_sim/intensification.md` — the ladder engine as built.
- `.claude/rules/core_sim/husbandry.md` — the pen's feed and the shed, the two shipped mechanisms
  §2.7 and §3 generalize.
