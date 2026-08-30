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
| A pen's animals must eat | `fodder_per_biomass × biomass`, offset by the footprint's grazing; **fodder pays the rest, and nothing else does** — the larder term was retired in #578, human food not being animal feed. Underfed herds **shrink** | collected fodder |
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

An improvement may declare up to three things. The **work** term of each is in work units
(`intensification::PER_WORKER_OUTPUT` = one worker-turn at the food peak with no gear).

| Cost | Shape | What it buys |
|---|---|---|
| **Build** | a **pile**, drawn down once | the rung transition. Ships today as `RungBuild::work_cost` |
| **Upkeep** | a **rate**, per turn, forever | the improvement stays at its rung |
| **Production** | a **rate**, per turn | the yield actually taken |

> **BUILD AND UPKEEP EACH HAVE A SECOND TERM, AND IT IS MATERIALS** (§2.7, §4.9 item 12). Work is
> never the whole price: raising a fence swallows hurdles, holding a road swallows stone. So a rung
> declares a **material pile** beside `work_cost` and a **material rate** beside its upkeep's
> `work_per_turn`, and **both track the position** exactly as the work terms do — the pile draws in
> proportion to the work banked, the rate scales with how much of the rung stands. Production keeps
> no material term: what a source *consumes to run* is not upkeep, and §2.7 says why.

**Every one of them is optional.** A route declares upkeep and no production — that is the point of
building the term against routes, because an architecture that assumes every improvement produces
something breaks on the first one that does not.

### 2.2 The PLAYER allocates workers per activity — ONE on the source, THREE on the band

**A source carries exactly one worker allocation from a band: the crew that TAKES from it.** The
other two activities are band-level standing roles, because neither of them divides sensibly at the
tile (§2.5).

| Activity | Where the hands stand | Set by |
|---|---|---|
| **take** | on the **source** | `assign_labor` — unchanged; its `workers` is the take crew |
| **keeping** | a **band** role, one per food web | `assign_labor <faction> <band> agriculture\|husbandry <workers>` |
| **building** | a **band** role, one for the whole band | `assign_labor <faction> <band> builders <workers>` |

**A verb therefore names no crew.** `cultivate` / `sow` / `tame` / `corral` — and `extend_pen` — say
*what* to build and put it in the band's build queue; *how fast* is the Builders pool, and *when* is
where the entry sits in the queue.

They all draw on the same finite band, so **competition between them is the opportunity cost**. **No
cap** means no cap on any *one* activity — fifty hands may build and fifty more gather beside them,
and the constraint is what those hands are not doing elsewhere — but the whole allocation can never
exceed the band. Two rules hold that line, and they answer different questions:

- **A role's stepper clamps on the band's idle count**, exactly as scout's and warrior's do. The
  refusal the four verbs used to make retires with the crew they used to name: an order that states
  no head count cannot ask for hands the band has not got.
- **A band that SHRANK sheds**, and a role is a row like any other — the builders have their own two
  steps in the decided shedding order (§2.9), one for a spare hand and one for the last.

**The gear the builders carry is the Builders role's own**, read off that row like every other role's,
and the coverage behind it is resolved over the pool.

The arithmetic is then trivial:

```text
keeping_supply   = this source's share of the band's keeping POOL for its web (§2.5)
rot              = the shortfall against upkeep_demand, at the rung's own rate (§2.4)
build_work       = the band's BUILDER pool — on the HEAD of its queue and nowhere else (§2.5)
net              = build_work − rot
take             = min(take_workers × per_worker_capacity, source_offer)
```

> #### THE RATE IS NOT A TAX ON BUILDING, and reversing that is what made the pools coherent
>
> An earlier cut of this arc had the **build** crew supply the maintenance rate while a meter was
> being raised, so only its surplus was progress and the pace was `work_cost / (crew − rate)`. That
> was sound while the build crew stood **on the tile** — the crew was already there, so letting it
> cover the rate cost nothing. **It stops being sound the moment both crews are pools**, because the
> boundary between them then moves a whole band's builders around under the player. §2.4 is where
> that is worked through; the consequence here is that **the pace is `work_cost / builders` again**,
> and the two pools are stated separately because they are separately staffed.
>
> **A build can still fail to finish, and it is still worth its own published answer** — but the term
> that eats it is the **rot**, not the rate. A meter whose keeping is short loses ground every turn,
> and builders raising it more slowly than that are losing work already bought. So the three answers
> stand with a new denominator: `buildTurnsRemaining` reads a count while `build_work > rot`, `-2`
> (**the meter holds**) at equality, and `-3` (**the meter rots**) below it.
>
> > #### ⛔ AND THE NO-ANSWER BOUNDARY IS **WORK BANKED**, NOT *IS ANYONE STAFFED*
> >
> > `-1` used to mean, among other things, *this source is unstaffed, and nobody has promised
> > anything*. **A meter carrying work has promised something — the player paid for it** — so the
> > test is now whether anything is banked (or anyone is building), and a rung merely *declared* on an
> > empty meter is the one that still answers `-1`.
> >
> > **Without that move both sentinels are dead on the shipped ladder.** The rot is capped at the
> > rung's own decay — `0.5` and `0.75` on the plant rungs, structurally **zero** on both animal ones —
> > while a single builder banks a whole worker-turn, so no staffed crew can ever net `<= 0`. The
> > states did not vanish with the rate; they moved to **zero builders**, which is exactly where the
> > model now expects to find them.
> >
> > **So `-2` is no longer only a failure.** With no builders and the keeping met it is the player
> > **parking a half-built improvement** — §2.4's own case — held indefinitely at no risk, and a
> > surface that marks it with a hazard teaches the player to ignore the mark everywhere else. `-3`
> > stays unambiguously bad: an abandoned meter, bleeding.
> >
> > **The animal web can still stall in a way no countdown term sees**, and that is an *eligibility*
> > stall rather than a balance one — `husbandry.md` → "THE REGROWTH SUPPRESSION CLOSES A LOOP" owns
> > it. §4.6b's queue is where a permanently-stalled head entry has to be answered.

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

### 2.3 RETIRED — build completion no longer hands a crew anywhere

This section used to move a finished build's crew onto its web's keeping role, so that a brand-new pen
did not start decaying on turn one because nobody noticed it had begun costing something.

**It has nothing left to move.** The builders are a band-level pool (§2.5), so completion does not free
a per-source crew — it frees the *queue's head*, and the same pool starts on the next entry. And the
failure it guarded against cannot happen either: under §2.4 the keeping bill starts at the **first work
banked**, not at completion, so a player who built the thing at all was already paying to hold it. A
carry-over would now be the sim quietly moving hands between two roles the player staffs by hand.

### 2.4 KEEPING HOLDS, BUILDING ADDS — and a rung is not lost on the first dip

Two pools, two questions, and **no test that moves a source from one to the other**:

| pool | owes | for how long |
|---|---|---|
| the web's **keeping** (`agriculture` / `husbandry`) | the rung's rate, to hold what is on the meter | from the **first work banked** until the last |
| the band's **builders** | nothing — its whole output is **progress** | while the source is the **head of the queue** |

```text
rot = (shortfall / demand) × the rung's own decay rate, past the grace   // keeping came up short
net = build_work − rot
  net > 0  →  the meter CLIMBS
  net = 0  →  the meter HOLDS exactly where it is
  net < 0  →  the meter LOSES GROUND — work already bought, bleeding
```

**A source at 10% and a source at 100% are billed the same and billed to the same pool.** Nothing about
how full the meter is decides who pays.

> #### WHY THE FULLNESS TEST HAD TO GO
>
> It used to: the **build crew** supplied the rate below the meter's cost and the **keeping pool** at
> it. That was defensible while the build crew stood on the tile. Pooling both crews broke it in two
> directions at once, and both were reported as ordinary play:
>
> - **A half-built meter nobody was building could not be held.** Take the builders off a Cultivate at
>   50% and the patch is billed to a crew that is not there, so it bleeds its full rate — with keepers
>   standing idle in the role and **no command that can aim them at it**. That is the exact defect this
>   arc already fixed once for a finished Field, wearing the other half's clothes.
> - **A held rung that dipped commandeered the band's builders.** A completed rung eroding to 99% is
>   *below its cost*, so it flipped into *building* and jumped a queue funded all-hands-on-the-head — a
>   one-percent repair displacing the Cultivate the player actually ordered. Topped back up, it
>   returned to a keeping pool that was still short, and dipped again. **It oscillates**, and the
>   player's real build stands still through every cycle.
>
> **It costs the player nothing to delete.** Four builders against a rate of two used to bank two turns
> of progress a turn; two builders and two work of keeping bank the same two. Same hands, same pace —
> the player states which row each hand stands in, and the sim stops moving the line between them.

#### AND THE QUEUE IS THE DECLARATION — the meter no longer makes one

The build verb stays **derived from the meters** in the sense that matters — *which rung* a queue entry
names is the newest meter with room on it, so an entry on ground that has moved on is **dead rather
than stalled**. What the meters no longer do is *create* an entry nobody asked for.

| meter | what it means |
|---|---|
| **zero** | nothing banked; the entry names the rung the player picked |
| **between zero and its cost** | that rung is what this entry is raising |
| **at its cost** | there is nothing left to raise — **the entry leaves the queue** |

**A rung that erodes back below its cost is NOT re-adopted.** It is held — or, unstaffed, lost — by the
keeping pool, and repairing it is a fresh decision the player makes by putting it back in the queue.
The earlier cut adopted it automatically, which was right when adoption cost nothing and is wrong now
that it costs the head of a queue.

**`abandon_improvement` stays retired; what came back is disposal, not arbitration.** §2.5's
`abandon` drops a band's *holding* of a source outright — the row, its declaration and its queue entry
— because a half-built meter the player has lost interest in otherwise draws keepers forever. It is one
bit per source, never a number, so it smuggles no per-source staffing back in.

**A meter at exactly zero clears back to "the player must declare"** together with everything else
that empties: `reconcile_owner` drops the owner and the committed crop on the same edge, and the
stamped cost goes with them. One notion of empty, not three.

There is no third concept: an earlier cut gave an unfinished meter its *own* demand
(`meter_raising_demand`), which was the same rate under a second name. Deleted then, and the rule that
replaced it — *you cannot be billed to hold something you have not finished building* — is deleted now
too. **You can.** From the first work banked, holding it is what the keeping pool is for; what you
cannot be billed for is ground with nothing on it at all.

**Both webs answer identically.** An under-supplied meter decays, which is a **meter bleed** on plants
and a **shed** on animals. There is no "the animals are standing there whether or not the fence is up"
exception.

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
- **KEEP IT ORTHOGONAL TO THE METER.** *How full is the meter* decides what a repair would cost and
  whether there is anything left to raise; *is the rung still achieved* is the retention bar and
  decides what the ground pays out. A patch at 99% is **still tended** and is **also** short of its
  cost. Folding the two would make a rung's loss and a rung's repair the same edge.
  > **It used to be orthogonal to a third thing — the who-pays test — and that test is gone** (§2.4).
  > Note what does *not* follow from a dipped meter any more: it does not change which pool is
  > billed, and it does not put the source back in the build queue. It is a fact about the ground, and
  > acting on it is the player's.
- **No animal rung needs a bar**: `domestication_progress` is monotone-up and a pen is held by a
  stored flag, so no animal rung can be lost by a meter dipping.
- **A rung's BENEFIT stays binary on the achieved state.** Scaling a rung's payout with its meter is a
  real proposal and a much larger one; it is deliberately not this.

`grace_turns` survives unchanged in meaning: consecutive turns of shortfall forgiven before decay
begins.

### 2.5 KEEPING AND BUILDING ARE BOTH BAND-LEVEL ROLES, and neither is a crew on the tile

**Three standing roles on the band**, in the same family as the local scout and warrior dials:
`agriculture` keeps the plant web, `husbandry` the animal one, and `builders` raises whatever the band
has queued — each staffed with `assign_labor <faction> <band> agriculture|husbandry|builders <workers>`.

- **One keeping role per WEB, because the two webs are already separate ladders.** This is their
  existing split, not a new axis. **The builders are ONE pool for both**, because a build is a job
  rather than a standing charge and the queue already says which one is being worked.
- **The keeping demand is the SUM** over everything the band holds on that web that carries work on a
  meter — at any fullness (§2.4).
- **`0` is still how you say "stop"** — for a whole role rather than for one source.

#### THE BUILDERS FUND ONE ENTRY: the HEAD of the band's queue

A band holds an **ordered queue** of the builds it has declared, and the **whole** Builders pool goes
on the first entry until its meter fills, then on the next. **Spread is not offered here, and the
asymmetry with keeping is honest rather than an omission:** *keeping has something to ride out and
building does not.* An under-kept improvement degrades toward a threshold you can stay above, so
spreading a short keeping pool loses nothing while you recover; splitting a builder pool across three
jobs just means nothing finishes. A queue removes the choice rather than offering a bad one.

- **A verb declares; it does not staff.** `cultivate` / `sow` / `tame` / `corral` / `extend_pen` append
  an entry. Where it sits is the player's, and re-ordering is the one input a list can carry that a
  stepper cannot.
- **Membership is the player's too** (§2.4) — nothing enrols itself, and completion retires the entry.
- **An entry that is waiting costs nothing and loses nothing**, because its meter is held by the
  keeping pool like everything else. That is what makes a queue safe to fill.
- **"Builders with nothing to do" needs no warning.** A build demand ends when its meter fills, unlike
  a keeping demand, so an empty queue beside a staffed pool says that by itself.

#### AND A SOURCE CAN BE PUT DOWN: `abandon`

**`abandon <faction> <source>` drops the band's holding of it** — the row, its declaration and its
queue entry go together. The ground keeps whatever is on its meter and, with nobody holding it, rots
back down at the rung's own rate over the following turns exactly as an unkept improvement already
does.

It exists because §2.4 bills a meter from the first work banked, so a half-built patch the player has
lost interest in otherwise draws keepers forever. **It is one bit per source, never a number** — the
per-source *funding* lever stays deleted, and this is a disposal rather than a smaller share. Nothing
is destroyed on the spot, so it needs no confirmation and no second destruction path: the player stops
paying and the land goes back to what it was.

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

#### THE POOL IS WORKERS AND GOODS; THE TOOL IS THE WORK SITE'S

The band is what a site draws **from** — it does not decide what the hands it lends carry. So the
keeping **kit** is a property of the worked row (`upkeep_kit <faction> <source…> [kit <id>]`,
`LaborAssignment::upkeep_kit`), while the keepers and the store stay one pool per band. There is no
tension between the two: the pool decides how many hands each site gets, the site decides what those
hands carry.

It was one stored id on the band's `agriculture` / `husbandry` **role row** until this note, which
could not say *hoes on the Field, bare hands on the scrub patch beside it* — one pick put the same
tool on every site that band kept, with no way back, and the wear of that one tool was charged
against the work of all of them. It is the identical argument §4.7a ② makes for the builders' kit
being per **queue entry**, and the take kit was already per row; keeping was the last one still
band-scoped.

**What that changes in the split**: with one rate per web, dividing the work pool in proportion to
demand and dividing the worker pool in proportion to worker-need are the same arithmetic. With a tool
per site they are not — two sites owing the same work but worked with different tools ask for
**different numbers of hands** — so what is split is the **head count**, in units of `demand ÷ what
one of that site's own keepers delivers`, under the same two modes above. Where the rates agree, which
is every branch on the shipped roster, the answer does not move by a bit.

**And sites naming the same kit share its scarcity.** A band's three hoes cannot arm two keepers on
one patch *and* three on another, so the coverage read is taken once per distinct kit rather than once
per site. Sites naming different kits do not compete; sites naming the same one degrade together,
exactly as the single band-wide pool did.

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

### 2.7 The material half — WHAT YOU LEAVE BEHIND, never what you carry home

**A job spends three things, and they are not interchangeable.** **Workers**, the hands. **Kit**, the
crafted tools a worker carries — and carries on to the next job. **Materials**, the goods that go
*into* the thing and stay there.

> ⛔ **THE WORD `resource` IS RETIRED, AND SO IS THE MODEL IT NAMED.** This section read *"the
> RESOURCE half — collected goods"* and made **hay** its reference: the fenced footprint grazed what
> it could and hay covered the rest, so upkeep was a draw on whatever your people had gathered.
> That conflated two different things under one word. **Hay is FEED** — an animal eats because it is
> an animal, penned or not — and feeding is not holding. The pen feed settlement (`settle_pen_hay`,
> `.claude/rules/core_sim/graze.md`) keeps doing its own job, untouched, and it is **not** an
> instance of this section. What upkeep draws is **materials**, and nothing else.

**The test is whether it comes home with you.** A hoe goes to the next field, so it is kit: it wears
with use and its absence makes the work dearer, never impossible. A fence stays in the ground, so it
is material: it is spent, and no number of hands replaces it.

**That makes hurdles the reference implementation, and they are currently misfiled.** A hurdle is a
woven fence panel — it *is* the fence — and it ships as an equipment item with a durability pool. It
should be a **material**, crafted from wood, drawn on the pen's build pile and again on its upkeep
rate. §4.9 item 12 is that reclassification and the mechanism it needs.

**Materials track the position, both ways.** Raising a rung draws its pile as the meter climbs;
holding a partial rung owes its rate in proportion to how much of it stands. A road 30% raised has
swallowed 12 of its 40 stone and owes 0.6 of the 2.0 a turn. This is the work half's own behaviour,
restated in a second currency, so there is one rule rather than two.

**And decay refunds nothing.** Material goes in proportionally and does not come back out — the road
washes away and the stone is spent. What that buys is that **neglect is self-limiting**: position
falls, the rate falls with it, and an abandoned thing decays toward costing nothing rather than
bleeding a store forever.

**The two failures are not the same failure, and must not read alike.** A dead kit makes the same job
want more hands and takes nothing away. A missing material stops the work outright: twelve keepers do
not mend a road with no stone. So a shortfall message that names the *pool* is wrong advice the
moment the missing thing is a good — it points the player at a stepper that cannot help.

**The land is a SCALE term, not an offset.** A route down a river valley is cheaper to hold than one
over a range, and `infrastructure_cost` is where that per-terrain answer is already written — it
*multiplies* the demand. That is a different mechanism from grazing, where the land supplies the same
good the animal would otherwise be fed; grazing's offset lives inside the feed settlement and
generalises to nothing.

> ⛔ **IT WAS THREE TERMS UNTIL #578, AND THE THIRD ONE WAS A DEFECT.** The pen also drew **human
> food** from the keeper's larder for whatever grazing and hay left uncovered — the corral arm's
> *"the keeper must bring it food"* read as the food the people eat. It is fodder. The larder term
> was short-circuiting the starvation path this section's own model depends on: when the land and the
> hay fall short the answer is an underfed herd, never people going hungry to feed livestock. Retired
> with `pen.upkeep_per_biomass`, the food-unit lever that expressed it; `penFeedUpkeep`, `penUpkeep`,
> `penLarderBill` and `penHayFood` are deprecated slots. **A material upkeep must not reintroduce a
> human-food path** — and under the model above it cannot, because feed is not upkeep at all.

**What is NOT upkeep**, and all three were reached for during design:

- **Inputs to a production activity** — seed for sowing, fuel at a drying rack, materials at a bench.
  Those are consumed when the activity *runs*, not by the thing *existing*.
- **Feeding the animals.** A herd eats because it is a herd; a pastoral herd with no fence at all eats
  the same amount. Penning changes *where* they eat, not *whether* — so the grass and hay that feed
  them are the feed mechanism's business (`settle_pen_hay`) and appear nowhere in a rung's upkeep.
- **Kit wear.** A worn tool is a real cost and it is **not a term here**. It is charged where the work
  is done, per unit of work supplied (`WearQuantum::UpkeepWork`), and it is replaced by crafting. A
  rung declaring a kit cost would bill the same tool twice.

**Labor that always travels with the keeping is folded into the keeping, not split out.** Mending a
fence is genuinely work, but the keepers are already there and the pen's upkeep already scales with
the herd — a separate mending-work line would scale off the same term and would never be staffed
independently. A number with no decision attached is not a lever. The same disposes of a road's
quarrying and hauling: that work is the upkeep, not a second line beside it.

### 2.8 ONE POSITION ON THE LADDER — the rung stops being a status and becomes a place

**A source has ONE number: how far up its own ladder it has been worked, in cumulative work units.**
`plant:tended` runs 0 → 50, `plant:field` 50 → 125. Everything a rung means is read off that one
position: what the ground pays, what it costs to hold, what is offered next, and what a decay takes
away.

This replaces **two independent meters per source** (`cultivation_progress`, `field_progress`), and it
is what makes §4.10's partial credit expressible at all — partial credit needs a single fraction to
scale by, and the position **is** that fraction.

> #### WHY, IN ONE PLAYTEST REPORT
>
> Ray built a Field on a Tended patch and ended up with **Field > 0% while Cultivation read 99%**, and
> the board offered Cultivate again on ground that was already a Field. With two meters that state is
> *representable*, so rules have to police it; with one position it **cannot be written down.** The
> arc has produced three "two seams disagree" defects in one session — this is the same shape, one
> level down, in the data rather than the code.

#### THE FOUR RULES, AND THREE OF THEM ARE ARITHMETIC

1. **A RUNG IS OFFERED ONLY WHEN THE ONE BELOW IS AT 100%.** Not "achieved", not "past its retention
   bar" — **complete**. This is the one rule that is a *gate* rather than a consequence, and it is
   Ray's, with the deciding argument: **the kits differ per job.** If a Sow's work implicitly finished
   the Tended meter under it, the player would be doing Cultivate's work with Sow's tool — a plough
   finishing the clearing that wanted a digging stick. Gating on completion keeps every unit of work
   priced at the kit that actually did it, and it gets worse the further up the knowledge graph you go.
2. **ONE JOB IS OFFERED AT A TIME, and which one is a pure function of the position.** The `⌃` mark
   stops needing to choose between a repair and an upgrade: below 50 the offer is Cultivate, at exactly
   50 it is Sow. The repair path landed in §4.7 as a special case; here it stops being one.
3. **DECAY EATS FROM THE TOP, for free.** A Field at 10% decaying is the number falling 60 → 50; it
   consumes the Field's progress and reaches Cultivate's range only once the Field is gone. That **is**
   Ray's *"if Sow is > 0%, cultivation can never decrease"* — as arithmetic, not as a rule somebody has
   to enforce.
4. **A LOWER RUNG IS NEVER BELOW FULL WHILE A HIGHER ONE HAS PROGRESS.** A corollary of (3), stated
   because it is the invariant the bug violated, and because it is the one a reader will look for.

#### THE PAYOUT IS A DELTA ON THE RUNG BELOW

**A Field at 100% pays a Tended patch's full output PLUS the Field's own extra.** At 40% it pays a
Tended patch in full plus 40% of that extra. So **an upgrade in progress can never pay less than the
rung under it**, which is the invariant the whole shape exists to guarantee.

- **Config keeps stating ABSOLUTES** — *a Field pays 3.50, a Tended patch 1.20* — and the delta is
  derived. The numbers stay readable, and nothing has to be restated in a second form.
- **THIS BUYS A VALIDATE RULE NOTHING CHECKS TODAY**: each rung's payout must be **≥ the one below**,
  or the derived delta is negative and a half-built Field pays *less* than the patch under it.
- **The ~100 binary predicates STAY.** `is_cultivated()` / `is_field()` still answer *"has this rung
  been achieved"*, which still gates the verbs and the knowledge. What changes is every **payout** that
  branches on them: it becomes an interpolation on the position.

#### AND THE COST SIDE MOVES WITH IT, ON THE SAME SHAPE

**This is the half the arc keeps deferring, and it is the one reported from play three times in one
session.** Today a Tame at 3% owes the same 8.27 work/turn as a finished one; a patch 1% into a
Cultivate owes the whole rung's rate to hold a hundredth of a thing.

**The upkeep demand interpolates exactly as the payout does** — at Field 50% you are holding a full
tended patch plus half a field, so you owe Tended's upkeep in full plus half the extra a Field demands.
At the first rung above wild the delta form and a flat percentage agree, so a Tame at 3% owes **3% of
8.27 ≈ 0.25**, which is the reported case.

> **⛔ ONE THING TO CONFIRM BEFORE BUILDING.** Ray's words were *"if at 100% it is 10 units work to
> keep, at 50% it should be 5 units"* — a flat fraction of the rung's own demand. The delta form above
> is what the payout's shape implies and what he agreed to a message earlier. **They are identical at
> the first rung and differ above it**: at Field 50% with Tended demanding 2.0 and Field 6.0, the delta
> form owes `2.0 + 0.5 × 4.0 = 4.0` and the flat form owes `3.0`. Get this answered before writing code
> — do not pick one on the grounds that it is easier.

- **Benefit and cost move TOGETHER or not at all.** §4.6a's note stands: an interim with one scaled and
  the other flat is a worse asymmetry than the flat rate is.
- **A queued upgrade raises the keeping bill before it pays anything back**, and keeps raising it as
  the meter climbs. That is correct — it is what makes an upgrade a decision rather than a free
  ratchet — and it is worth surfacing, not hiding.

#### `retain_fraction` DISSOLVES

Losing a rung stops being a status flip at a stamped bar and becomes **the payout fading as the
position falls**. §6's open item said this would happen — *"a rung sliding back turns into a fading
payout rather than a status you lose"* — so `retain_fraction`, the retention bar and the stamped-bar
machinery retire with it. **Do not tune `retain_fraction` in an earlier slice**; it is being deleted.

#### THE ANIMAL WEB IS ALREADY HALF OF THIS, AND THAT IS THE ARGUMENT

`domestication_progress` is **monotone-up** and neither animal rung declares a `meter_decay` — an
unkept flock loses **animals**, never taming progress. So the animal web cannot reach the bug at all,
and the plant web is the odd one out. **Making plants match animals is a consistency fix, not a new
idea.** What the animal web still needs from this section is the **cost** side: its demand is flat at
any fullness exactly as the plant web's is.

### 2.9 WHEN A BAND CANNOT AFFORD ITS ROWS, THE ORDER IS DECIDED

**It fires only at zero slack.** Idle hands absorb a shrinking pool by themselves, so this reaches
only a band that is **fully committed** when it loses someone — a famine, a fission, a raid, an
elder. It is an edge-case handler, and that is why the order is decided here rather than exposed as a
policy: a config lever would be a second answer competing with a settled one.

`LaborAllocation::normalize` walks the list top to bottom and gives **one** hand off the first step
that names a staffed row, then re-runs for the next hand.

**Nothing is lost**

1. A **scout**.
2. A **warrior**, if nothing threatens the band.
3. A **keeper above the keeping demand** — Agriculture first, then Husbandry.
4. A **builder the pool is not spending** — with something queued, every builder **above the
   last one**; with **nothing queued, every builder there is**, the last included, because an
   idle pool builds nothing. The queue decides *how many* builders are spare, never *whether*
   any are: gating the step on a non-empty queue put idle builders below steps 6, 9 and 10, so
   a band with three builders and an empty queue answered a lost hand by dropping its only
   food row.

**Output falls, nothing ends**

5. **Thin the least-productive worked source that has two or more hands** — least yield **per
   worker**, passing over a source still accruing knowledge if another candidate exists. This never
   empties a row.

**Something ends**

6. **Empty the least-productive source carrying no improvement and no queued build.**
7. A **warrior, unconditionally.**
8. A **keeper below the demand** — improvements begin to rot.
9. **Empty the least-productive improved source with no queued build.**
10. **Empty a source carrying a queued build** — the row drops and the declaration goes with it
    (§3.2: an entry requires a row).
11. **The last builder** — every queued build stalls. Reached only while a build *is* queued;
    with an empty queue step 4 has already taken the pool.

**Terminal:** a single worker on a single row. Take it; the row ends.

**"Least productive" is two levels, and the first one is a presence test.** Steps 5, 6, 9, 10 and the
terminal all rank candidate rows by:

1. **Does this row pay into any account at all** — food, fodder or materials. A row paying nothing
   ranks below one that pays something, so the dead row is shed first.
2. **Then food per worker** (`last_yields[i].realized ÷ crew`), ties to the earliest row.

Level 1 exists because a hay Field and the five cash crops pay **zero food by design** and are paid
entirely by their materials rows, so a productive tobacco Field and a genuinely dead row tie at zero
provisions and list position decided between them.

**It is a PRESENCE test and must never become a combined score.** Ranking the two by amount would mean
comparing a food rate against a material rate, and `labor_config.json`'s `_comment_weeding` refuses
exactly that — *"an exchange rate this codebase does not have and should not invent"*. Asking only
*whether* a row pays invents no exchange rate. The levels are in this order so the standing intent
cannot invert: a food row pays **and** carries a positive per-worker yield, so a band short of hands
still keeps its people on food and drops the tobacco. Level 1 decides only the tie beneath that.

**Thinning beats emptying, and that is the sharp line.** Since §2.5 the builders are a band-level
pool, so taking a hand off a source mid-build does not slow the build at all — only **emptying** the
row does, because an entry requires a row and dropping the row drops the entry. The cliff is
emptying, never building. **9 is worse than 8** because an improved source with no take crew still
owes its upkeep and now pays nothing, where rot is gradual and recoverable; **7 sits after 6** because
pulling the guard under a real threat can cost people, which is worse than losing a row that had
nothing invested in it.

**What it replaced was the edit order.** `set_assignment` re-pushes an edited row to the end of the
list and the pass used to trim from the end, so the row the player had just touched was always first
to be cut — a Field's tenders raised `2 → 3` came straight back down on the turn an elder died.
Nothing in the eleven steps is positional, and list position must not become the shedding order
again.

---

---

## 3. What this replaces

| Shipped today | Becomes |
|---|---|
| `herders_needed` as a standing headcount | an upkeep **rate** in work, scaled by herd size |
| the shed (`shed_uncontained_animals`) | the animal web's **shortfall penalty** |
| `decay_fraction_per_turn` as an independent dial | `upkeep.meter_decay.per_turn` — the rung's own rot rate, scaled by **how short** you are rather than being the shortfall itself |
| `tended_this_turn` / `tamed_this_turn` binary flags | retired — shortfall is continuous |
| `RungBuild::crew_needed` (a staffing floor) | retired — a build is staffed by the band's Builders pool, and there is no per-source crew for a floor to raise |
| the `maintain` command + `LaborAssignment::maintain_workers` | the **band-level** `agriculture` / `husbandry` roles and `upkeep_mode` (§2.5) |
| `LaborAssignment::improvement_workers` (the build's own crew) | the **band-level** `builders` role and its **ordered queue** (§2.5) |
| the meter's FULLNESS as the who-pays test | retired — the keeping pool holds every meter carrying work, at any fullness; the builders only ever add (§2.4) |
| build completion handing its crew to the keeping role | retired — there is no per-source crew to hand, and the keeping bill already started at the first work banked (§2.3) |
| `progress >= cost` as the LOSS test | `upkeep.meter_decay.retain_fraction` — a rung is earned at its cost and held to a stated fraction of it (§2.4) |
| `abandon_improvement` + the "nothing left to build" test | retired — completion retires the queue entry, so there is no stale authority to clear. **`abandon` is a different command**: it puts a whole source down (§2.5) |
| `work_cost / crew` as the build pace | **restored**, with the rot as the term that eats it: `work_cost / (builders − rot)` (§2.2/§2.4) |
| `learn_multiplier(floor)` on the build rate | retired — a build crew is not pulling on the source |
| `yield_fraction_while_building` (×4 rungs) | retired — the build has its own hands, so what it costs is the pool it draws on |
| `pen.upkeep_per_biomass` + the pasture/hay/larder split | **retired outright** (#578, and §2.7's callout). Hay is FEED, not upkeep: the pasture/hay split stays in `settle_pen_hay` and is not an instance of the material half. The pen's upkeep material is **hurdles** (§4.9 item 12) |
| `TerrainDefinition::infrastructure_cost` (never read) | the route rung's **scale term** |

---

## 4. Sequencing

The ordering constraint that is **not** negotiable is that the client readout lands before any
tuning — `plan_unit_costed_work.md` §11 learned this: *a cost spread with no readout change is
invisible*. Tuning is therefore **last**, and after §4.10, which changes what the numbers do.

### Landed (PR #557, branch `worktree-route-ladder`)

1. **This design doc.**
2. **The mechanism** — the upkeep term on the ladder engine; `yield_fraction_while_building` retired,
   because the dip and a work budget cannot both be live without double-counting.
3. **The plant web onto it** — `plant:tended` / `plant:field` declare upkeep; the binary
   `tended_this_turn` bleed retires.
4. **The animal web onto it** — `herders_needed` becomes an upkeep rate; the shed becomes its
   shortfall penalty.
5. **The client readouts**, then, after playtest, three rounds of correction: the per-activity worker
   allocation, the retention bar, pooled keeping, the derived verb, the ∞/holding/rotting estimates,
   and the hazard rule of §2.6.

### Next, in order

6. **THE BUILDER POOL AND THE BUILD QUEUE — in TWO steps, one PR.** The first is a model correction
   the second cannot be built on the seam of; they land together because neither is playable alone.

   **6a — KEEPING HOLDS EVERY METER, AT ANY FULLNESS.** The who-pays test — the meter's own fullness
   — is deleted (§2.4). The keeping pool owes the rate from the first work banked; a build crew
   supplies nothing and only adds. `patch_is_maintaining` / `herd_is_maintaining` go, the build's net
   stops netting the rate off, and the ∞ pair re-aims at the **rot** as the term that eats a build.
   > **It is stated separately because it is worth having regardless, not because it ships alone.**
   > Two states reported from ordinary play are wrong today and neither needs a pool to reach: a
   > half-built meter whose builders left cannot be held by idle keepers, and a held rung that dips
   > below its cost stops being the keeping pool's business at the moment it starts needing it. §2.4
   > carries both autopsies.
   >
   > **The hands do not move.** Four builders against a rate of two banked two turns of progress a
   > turn; two builders and two work of keeping bank the same two. What changes is which row the
   > player states them in — which is the whole reason 6b can then pool them.

   **6b — THE POOL AND THE QUEUE.** The per-source build crew retires the way the keeping crew did —
   off the wire, the commands, `staffed_total` and the sheet's clamps — for a band-level **Builders**
   pool and a per-band **ordered queue** of declared builds. Funding is **all hands on the head entry
   until it completes, then the next**; §2.5 carries the asymmetry with keeping and why "builders
   with nothing to do" needs no warning.

   > **A WAITING ENTRY GETS A REAL FINISH DATE, not a queued badge.** The queue is deterministic —
   > the head takes the whole pool until it fills, then the next one does — so the sim can chain the
   > answer down the list: an entry's turns are the sum of everything above it plus its own span at
   > the full pool. Under 6a a waiting entry does not rot (the keeping pool holds it), so that
   > chained number is **exact** rather than an estimate that drifts. It also propagates the bad news
   > for free: if the head never finishes, nothing below it does either, so the head's `-2` / `-3`
   > is what every entry below it publishes. **Playtest is what decides whether all-hands-on-one is
   > right**; the chained date is what makes that judgeable.

   **One live defect folds in here rather than being fixed twice:**
   - **A declaration cannot be cleared.** `cultivate <f> <x> <y> 0` *sets* the improvement with zero
     builders; it does not clear it, so an unwanted declaration is stuck. (The claim that this was the
     walk-away path was made when `abandon_improvement` was retired, and was wrong.) Pooling makes it
     sharper: a declaration carries no crew at all, so unticking — dropping the queue entry — is the
     *only* undo, and `abandon` (§2.5) is how a source with work already on it is put down.

   > **`SourceForecast.build_pace` mapping the rotting sentinel to holding was the second defect
   > listed here, and it is FIXED** — `build_pace` forks `BUILD_TURNS_ROTS → BUILD_PACE_LOSING`, and
   > `labor-ui.md` → "THE SECOND `∞` IS RED" is the autopsy. It landed in PR #557's later corrections
   > after this line was written.

7. **The WORK TAB.** A pool header (Agriculture · Husbandry · Builders, with keeping's spread/priority
   mode), the **build queue** with drag-to-reorder, and the source list carrying each source's
   improvement state as a chip. **Keeping moves here from the Band tab** — the pool was on one tab and
   its consequences on another, which is why it went unnoticed in playtest entirely.
   > **Take and "what is built" are ONE row.** A tile you gather from is usually the tile you
   > cultivated; two lists would print it twice and make the player cross-reference.
   >
   > **Per source, show COVERED or NOT — never a maintenance number.** Keeping is pooled, so a
   > per-source work figure would re-imply the per-source allocation §6 deletes, and the player would
   > start tuning something that is not a lever. The hazard is the per-source fact; the quantities
   > belong to the pool card, which is the thing that turns.
   >
   > The queue sits **above** the sources because it is the one list whose *order is an input*.

   > **LANDED — the tab's STRUCTURE, plus ① and ③'s crop half.** The POOLS block (Agriculture ·
   > Husbandry · Builders, with the spread/priority mode) moved off the Band tab to the top of the WORK
   > zone; the queue states a **completion turn** rather than a chained countdown; the `⌃` mark is the
   > control that declares; the compose sheet keeps the forecast and stops being the commit; the crop
   > moved to the queue row's settings expansion. **What remains of §4.7 is ② (the per-entry kit
   > override, which needs a field on `BuildQueueEntry`, a command and a wire field), drag-to-reorder,
   > and the optimistic unqueue.** The rationale is in `.claude/rules/client/band-city-panel.md` →
   > "THE POOLS BLOCK" and `labor-ui.md` → "THERE IS NO CHECKBOX ANY MORE".
   >
   > **Four decisions taken on the 7 playtest, each closing a question this section left open:**
   >
   > - **The unworked-ground limit is STATED, not relaxed** — the alternative needed a different queue
   >   membership rule *and* a per-entry band id on the wire, to buy one saved click on ground the player
   >   is about to staff anyway. The sheet says *"Send gatherers here first, then Cultivate this patch
   >   from the Work tab."*
   > - **The queue's dates are ABSOLUTE turns.** The chained `≈42` / `≈61` / `≈98` read as each job's own
   >   span when they are cumulative.
   > - **The compose sheet quotes NO queue position and NO date** — the schedule is the Work tab's
   >   business — and no price either: *"That information should be on the work tab. No need to have it
   >   here, it is useless."*
   > - **The dock grew rather than the zone starving.** The Work zone could not hold a pool header, a
   >   queue and a board in 300px. **`PANEL_HEIGHT_WIDE` ships at 456 and the zone box at 396** — it
   >   went to 440 first, and was raised again inside the same slice once the work inspector's own
   >   height was found never to have been budgeted at all. **Both budgets are now FULL**: the zone
   >   reads 396 of 396 in height and 354 of 356 in width, so anything added to the Work tab overflows
   >   it. Both have assertions that fail loudly rather than clipping silently; the levers — a taller
   >   strip, a wider panel, two-abreast pool cards — are Ray's.

   **7a — BUILDING IS A BAND ACTIVITY, so its whole loop belongs on ONE tab.**
   **This slice is what makes 6b playable, and 6b's playtest is what decided its shape.** The pool,
   the queue and the funding rule are all band-level since §4.6b — a build is *a job on the band's
   list that happens to name a tile* — but the loop the player walks is still spread across the tile's
   compose sheet, the Band tab and a turn boundary. Three decisions, taken with Ray on the 6b
   playtest, close it:

   **① THE DECLARATION MOVES TO THE WORK TAB, and the `⌃` ready mark is the control.** A work row
   already carries `⌃<verb glyph>` meaning *this source can climb its next rung* — the same
   `RungGates.next_rung_ready` answer the compose sheet's checkbox is gated on — so clicking it queues
   the job. One click, on the tab that owns the pool and the queue.
   > **The compose sheet keeps the FORECAST and stops being the commit.** *"Should I cultivate this?"*
   > is answered by the patch's own `ONCE TENDED 1.20 food`, its crop basket and its refusals, and a
   > 28px work row cannot hold any of that — so the sheet remains where a rung is JUDGED. What leaves
   > it is the committing act, and with it the trap 6b shipped: the `🌱 Cultivate this patch` checkbox
   > is not the action, and the only thing that commits it is a button reading **`Forage`**. Ticking
   > the box and closing the sheet does nothing at all. Reported from play, repeatedly, as *"I just
   > click cultivate and not the Forage button — that seems completely unnatural."*
   >
   > **The sim's ordering constraint is what welded them, and it does not survive the move.**
   > `cultivate` / `sow` / `tame` / `corral` reach only bands **already working the source**, which is
   > why one press has to send `assign_labor` first and the verb second. Declaring from a work row is
   > declaring on a source the band demonstrably works, so the constraint is satisfied by
   > construction. **A source the band does NOT work has no row, and therefore no way to declare** —
   > which is a real limit and this slice's to answer: either the ready mark reaches un-worked sources
   > (needing the sim to relax that rule) or declaring stays gated on working the ground, which is
   > defensible and should then be SAID rather than left as a missing control.

   **② THE KIT MOVES TO THE QUEUE ROW, one per job.** The sim already resolves a builders kit **per
   queue entry** from that entry's own web; the Builders card forced a per-BAND answer onto it, and
   that mismatch was not a rendering bug but the whole defect — naming a kit on the `builders` row is
   the one thing the derivation cannot express, so the sim treats it as an **override that wins
   permanently**. Measured in play: one click pinned `kit hurdling` onto every later builders command,
   locking a band raising a *plant* Cultivate to the *animal* web's tool with no way back (`none`
   means bare-handed, which is a different statement). **§4.6b deleted that picker rather than leave
   it harmful**; this slice gives the override its correct home, where each entry derives its own kit,
   marks it `(default)` as hunting does, and a pick changes **that job alone**.
   > **A GLOBAL "default builders kit" was considered and REJECTED.** The per-entry derivation is
   > already the better answer — it is right per web automatically — so a configurable default would
   > be a coarser second answer competing with a working one. `default_kits.builders` already exists
   > in config for anyone who wants the blunt version.

   **③ THE CROP MOVES THERE TOO, which is what makes ① and ② one design.** `Sow` needs a species and
   `Cultivate` needs the committed one, so the queue row becomes **the job's settings — kit and
   crop** — and the loop reads *declare from the source list, configure on the queue row, fund from
   the pool header above it*. Without this, ① would have to open the compose sheet anyway for any rung
   that needs a crop, and the move would buy nothing.

   **7b — WHAT §4.6b LEFT STANDING, and this slice is where it lands.**
   Each was reachable in play and none was a defect in the model. **All four have LANDED** — ① with
   §4.7a, the other three together; the middle two were one shape, a band-level act with no
   band-level surface, and the last was an asymmetry the pending rows introduced.

   - **Nothing on the Work tab declares a build** (①), so the tile sheet is still the only door.
     > **LANDED, with §4.7a's ①.** The `⌃` ready mark on a work row is the control that declares, and
     > the compose sheet keeps the forecast while ceasing to be the commit. **The limit that move left
     > is STATED rather than closed**, which §4.7a called the defensible answer: a source the band
     > does not work has no row and therefore no way to declare, and the sheet says *"Send gatherers
     > here first, then Cultivate this patch from the Work tab."*
   - **The kit override has no home** (②) — the card's picker is deleted, so the derivation currently
     stands alone and cannot be overridden at all. **Still open, and it is the cross-cutting one**: a
     kit per queue entry needs a field on `BuildQueueEntry`, a command and a wire field, where the
     other two are client-only.
   - **The queue cannot be reordered from the UI.** `build_order` is command-line only, as is
     `abandon`; the block caps at three rows plus a `+N more` overflow, chosen so the board keeps
     legible rows in a height-capped horizontal dock. Drag-to-reorder is this slice's, and the cap
     should be re-measured against whatever the pool header costs the zone.
   - **UNQUEUE IS NOT OPTIMISTIC.** A declaration made this turn shows immediately (a pending row at
     the tail, `○`, no date), but unticking a **confirmed** entry does not leave the block until the
     turn resolves — the queue's positions are wire state and the optimistic overlay carries
     additions only. The asymmetry is visible and should be closed with the row's own controls.

   > **⛔ THE ZONE HAS NO ROOM LEFT, AND THAT IS THIS SLICE'S FIRST DECISION — NOT ITS LAST.** All
   > three surviving items want pixels in the Work zone, and §4.7's landing spent the last of them:
   > `PANEL_HEIGHT_WIDE` ships at **456** for a **396px** box, and the zone reads **396 of 396 in
   > height and 354 of 356 in width**, with assertions that fail loudly rather than clipping silently.
   > So each item's cost has to be **measured before it is designed**, and the lever that pays for it
   > — a taller strip, a wider panel, two-abreast pool cards — is **Ray's to pick**. Designing all
   > three and discovering the overflow at the end is the failure this note exists to prevent.

   > **LANDED — all three, and the measurement is what shrank the problem.** Only ② ever wanted
   > pixels: ③'s grab handle is the **marker column already reserved on every row** (10px, an empty
   > `MOUSE_FILTER_IGNORE` label on every row that is not the head) and its drop indicator draws
   > inside a row's own 28px, and ④ is client state with nothing new drawn. **The cap needed no
   > re-measure either**: on the wide dock `build_queue_rows_max` already answers **1 row plus
   > `+N more`** from the box, and on the tall LEFT dock it affords **14–27** against a cap of 3 —
   > so `BUILD_QUEUE_ROWS_MAX` binds on exactly one dock and is dead arithmetic on the other. It
   > **stays at 3**: raising it to 4 is free at four entries (the overflow row already holds that
   > slot) and first bites at five, where the block grows 28px and the tall dock's board goes 9 rows
   > to 7 — one to the row and a second to the pager it then needs.
   >
   > **THE KIT PICKER FLOWS, AND THE WRAP IS COMPUTED RATHER THAN DISCOVERED.** Ray's call, against
   > both drawn alternatives: shrinking the pickers to share a line truncates long names, and a fixed
   > second line spends a lever on docks that do not need one. So the strip lays the pair on one line
   > where the width allows and stacks them where it does not — but **not** through a flow container.
   > This zone `clip_contents` and `build_queue_settings_height` is reserved *before* the strip is
   > drawn, so a container that wrapped at layout time would leave the reservation unable to know how
   > many lines drew, and the difference comes silently off the bottom of the board.
   > `queue_settings_one_line(line_width)` is the one predicate both the reservation and the builder
   > read. **No shipped dock reaches the one-line state today** — 342px of strip on the tall LEFT
   > dock and 368 on a 1920 BOTTOM one, against the 408 the pair needs — because the work zone is one
   > board column wide at every dock; one line arrives when the board earns a **second column**, which
   > is a source-count answer and not a monitor one.
   >
   > **AND THE 30px WAS PAID BY A RULE, NOT BY A LEVER: ONE EXPANSION AT A TIME IN THE WORK ZONE.**
   > `_queue_open_key` and `_work_open_key` are mutually exclusive now — the one-at-a-time rule both
   > lists already followed internally, read one level up. The wide dock reads **396 of 396** with the
   > strip flowed to two lines, so `PANEL_HEIGHT_WIDE` did not move and the height stays out of
   > travel. **It also closed a live defect this arc shipped**: a settings strip and a work inspector
   > open together — one click each — drew **460 into the 396 box**, sabotage-verified by disabling
   > the exclusion. No harness frame could catch it, every strip-open frame having had no inspector
   > and every inspector-open frame no strip: two disjoint frame families with the defect in the gap,
   > which is the same shape as the inspector-height defect §4.7 found.
   >
   > **Two constants read lower than they drew and are corrected** — `BUILD_QUEUE_UNQUEUE_WIDTH`
   > 22 → **32** (`HudWidgets.compact` leaves the ghost button's horizontal padding) and
   > `BUILD_QUEUE_SETTINGS_HEIGHT` 30 → **34** (22px picker + 12px `ROLE_CARD_PADDING`). The second
   > was a live under-reserve, and correcting it is what makes the flow arithmetic honest.
   >
   > **② RETIRED THE ROW KIT RATHER THAN LAYERING ON IT.** `assign_labor` **refuses** a `kit` token on
   > the `builders` role by name; a stored id per band is the one thing the per-entry derivation
   > cannot express, and leaving it beneath the new field would have kept the pinning defect reachable
   > from the command line. `build_kit <faction> <source…> [kit <id>]` is the fourth member of the
   > `abandon` / `unqueue` / `build_order` family and shares their `BuildSourceRef`; **an absent `kit`
   > token clears the override**, which is *"an absent `kitId` means the job's default"* read as an
   > edit. `buildKitId` is captured **live** from the band's allocation rather than from a turn-written
   > cache, so a pick shows on the recapture the command triggers and needs no overlay at all.
   >
   > **④'s TOMBSTONE KEYS ON THE TURN, NOT ON THE NEXT SNAPSHOT**, and that is the whole trap. The
   > server re-captures and broadcasts after **every** dispatched command, and that snapshot still
   > carries the stale turn-written `buildQueuePosition` — so *"hide until the next snapshot"* flickers
   > the row straight back. `reconcile_pending` already keys additions on a snapshot with a NEWER turn;
   > the withdrawal and ③'s optimistic ordering take the identical rule, in the same per-band record.
   > **The withdrawal clears the improvement rather than dropping the record**, because `unqueue`
   > leaves the take crew standing and the same record may hold a pending crew edit.
   >
   > **THE PLAY REPORT WAS THE COSMETIC HALF ALONE — §4.6b's clearing defect was NOT live.** It was
   > closed when `unqueue` became its own verb, and the `✕` was already wired to it; the sim now
   > carries a test for the exact reported sequence (declare and withdraw within one turn, then
   > resolve) and the entry never reaches the queue. What survived its own withdrawal was the overlay
   > row, `_on_hud_unqueue` having only sent.

8. **Gear as productivity.** A kit raises what a supplier delivers **per turn** rather than
   subtracting from the job. Decided because a job is a pile and an upkeep is a rate: subtraction has
   nothing to subtract from, so the shipped build model needs a second mechanism for upkeep, while one
   supply expression feeds both — a build divides a pile by it, an upkeep compares a demand against
   it. What it gives up is the shipped arc's scale-sensitivity (a flat turn saving, so a tool nearly
   frees a small job); what it gains is that a tool can no longer drive a job to zero, and a hoe fades
   on a farm by being *insufficient* rather than by arithmetic. **This changes the shipped build
   model**, not only upkeep.

   > **LANDED — in full, BOTH accounts, alongside §4.7.** `effective_build_cost` and the whole
   > subtraction path are retired; `pool_work_supply(workers, gear) = workers × (PER_WORKER_OUTPUT +
   > gear)` is the one supply expression, and a build divides its pile by it while an **upkeep compares
   > its demand against it** — so an equipped keeper covers more of a rung's demand. `tillage` gained
   > the `agriculture` job and `hurdling` `husbandry` so the keeping pools have something to derive.
   >
   > **THE CONSTANT WAS A UNIT CONVERSION AND AN EXACT ROUND TRIP, NOT A TUNING CHOICE.** `build_work`
   > shipped at 8.5 meaning *units off the job, per worker* — and that 8.5 was itself minted from a
   > still earlier `build_rate` **×1.5** on the crew's output. Inverting the mint needs no reference
   > crew and no reference job: `PER_WORKER_OUTPUT + build_work = 1.5`. **Hoes are +0.5 build work per
   > worker per turn; hurdles are +0.5** — the same tools they always were. Carrying 8.5 across would
   > have meant a worker delivering nine and a half times a bare one. Provisional until §4.14.
   >
   > **The argument is the one-time lump versus the per-turn rate, and nothing else.** The subtraction
   > granted the kit's help once against the target where a tool is used every turn it is held, and it
   > has nothing to subtract from on an upkeep. **Do NOT argue it from "a tool could drive a job to
   > zero"** — a gear value large enough to swamp a job is a config problem, and the sentence above
   > that reaches for it is the arc's own flourish, not its reasoning.
   >
   > **KNOWN HOLE:** a keeping tool wears on the work it supplied (`WearQuantum::UpkeepWork`), added
   > with the upkeep half — but the rate is an opening value with no conversion to invert, since the
   > quantum never existed. §4.14 owns it.
9. **Priority as a GENERAL per-source property.** Player-ordered, drag-and-drop, its own column — and
   deliberately not a maintenance-funding list. The auto-assigner sketch (§5) wants tile priority for
   its own reasons, and two orderings meaning almost the same thing would drift apart. Pooled
   maintenance is its **first consumer**, not its owner; the shipped most-invested-first ordering
   survives as the **tie-break** beneath an explicit rank, which matters because most sources will sit
   at the default.
   > **The build queue of §4.6b IS this property's storage**, not a second ordering beside it. If the
   > queue ships with a rank of its own, they will drift — so whichever lands first owns the rank and
   > the other reads it.

   > **HALF OF THIS HAS LANDED — the QUEUE's half, in PR #570 (§4.7b ③).** Drag-to-reorder ships on
   > the build queue block, the drop sends `build_order <faction> <band> <source…> <position>`, and
   > by the note above **that queue IS the storage**: the client keeps no rank beside it. So the
   > property exists, on the one list whose order is already an input.
   >
   > **What remains is the WORKED ROWS**, and it is stated here rather than left implicit: the player
   > orders their worked jobs, and a band that runs short **sheds from the bottom of that order,
   > saying which row it took**. The shape is the one this item already names — an **explicit rank on
   > top, with the shipped ordering surviving as the tie-break beneath it**. §2.9's eleven coded steps
   > **are** that tie-break; they are not replaced, and nothing in them becomes positional (see §2.9's
   > own closing paragraph, which is the reason list position must never be the shedding order).

   **9a — THE QUEUE'S ORDER BELONGS TO A BAND, AND ON THE WIRE IT DOES NOT. This is 9's foundation
   and it comes first**, because 9b cannot be built on a queue whose ordering belongs to the wrong
   band. Found in PR #570's own review and fixed nowhere until this slice.

   **The defect.** `buildQueuePosition` is published **per SOURCE** — one `int` on `ForagePatchState`
   and one on the herd state — and it "rides the same winner as `buildTurnsRemaining`"
   (`BuildEstimateClaims::publish_running`, `core_sim/src/systems/labor.rs`): among the bands working
   a source, the one with the **soonest estimate** writes the field. Two bands holding one source is
   ordinary, so the number a source publishes is routinely **another band's place in another band's
   line**.

   What that does to the gesture: band B's queue is `[X, Y, Z]`; band C also holds Y and has the
   sooner estimate, so Y publishes `position = 0`. B's block ties X and Y at `0`, breaks the tie on
   the key string, and draws **`[Y, X, Z]`**. Dragging Z above X computes `insert = 1` from that wrong
   list and sends `build_order B Z 1`, and `move_build_entry` yields **`[X, Z, Y]`** — Z lands
   *behind* X. The optimistic overlay then paints the requested order until the turn resolves, so the
   list **silently jumps** a turn later.

   > **IT IS NOT FIXABLE ON THE CLIENT, and no tie-break papers over it.** There is no band-keyed
   > queue anywhere on the wire, and the chained date rides the same winner — so the client holds no
   > second signal to recover the true order from. Established in #570's review; it is not to be
   > re-derived, and a cleverer tie-break would only pick a different wrong order.

   **The fix is a per-band queue on the wire, and it is the BAND-SIDE ORDERED LIST.**
   `PopulationCohortState` gains `buildQueue` — the band's own entries, in the band's own order, each
   naming only its source. **The rank is the vector INDEX**: there is no second integer to disagree
   with anything, which is what this item's own note demands of whichever ordering lands first.
   > **Why not per-source `[{band, position}]` rows**, the other shape considered: it answers a
   > band-shaped question in source-shaped storage, so building one band's queue means scanning every
   > source, and its explicit `position` ints carry an invariant — dense, per band, spread across N
   > sources — that no codec can enforce. The band-side list has neither problem, and it mirrors what
   > the sim already holds (`LaborAllocation::build_queue`), so it is **captured live** rather than
   > turn-written.
   >
   > **AND THE LIVE CAPTURE RETIRES THE OPTIMISTIC ORDERING.** `build_order` mutates the allocation
   > at command time and the server re-captures after every command, so the new order arrives on the
   > command's own recapture — exactly the reasoning `buildKitId` already ships on. The client's
   > pending-order overlay was a **second ordering beside the wire's**, which is the drift this item
   > exists to forbid, so it goes with the defect.
   >
   > **WHAT IT DELIBERATELY DOES NOT FIX: the per-entry READOUTS still ride the winner.** The date,
   > the legs, the gear and the blocked cause are source-addressed fields and keep the
   > sooner-estimate rule they were designed with, so a shared entry in B's queue can still quote a
   > countdown chained down C's. That is a stated rule (`snapshot.fbs`, "IT RIDES THE SAME WINNER"),
   > not this defect, and the ordering fix is strictly an improvement over it: the list is B's, the
   > date is the best answer anyone has. Per-band estimates are a bigger change than the gesture
   > warrants and would want their own slice.

   **AND THE CONTROL THAT SPENDS THE RANK WAS DEAD, which the playtest found before it found
   anything else.** Drag-to-reorder shipped in §4.7b ③ and **did not work at all** in the real
   client: the handle tooltipped and wore the move cursor, and the gesture degraded to a click that
   opened the row's settings strip.

   > **THE CAUSE WAS THE ROW'S OWN PRESS HANDLER, and the fix is a general rule.** Click-to-open
   > fired on the **press**; `_toggle_queue_settings` ends in `_repage_work_zone`, which frees every
   > node in the zone — including the marker Label the Viewport had just latched `mouse_focus` onto.
   > `_gui_remove_control` nulled the focus before the pointer had travelled far enough for Godot to
   > ask for drag data, so no drag was ever attempted. It fires on the **release** now, inside the
   > row's own rect, which is `BaseButton`'s rule for the same reason. **Any press handler that
   > rebuilds its own subtree kills every drag beneath it** — that is the class, not the instance.
   >
   > ⛔ **NO TEST COULD HAVE CAUGHT IT, AND THE RULE FILE HAD WRITTEN DOWN WHY.** The harness drove
   > `_queue_drag_data` / `_queue_can_drop` / `_queue_drop` **directly** and never pressed a mouse
   > button, under an explicit note that *"Godot exposes no public getter for the callables
   > `set_drag_forwarding` installs"*. That is true and beside the point: a harness can push real
   > `InputEventMouseButton` / `InputEventMouseMotion` through the Viewport and let the engine's own
   > drag machinery run. Rationalising the gap is what let a completely dead gesture ship green, and
   > the reproduction — three assertions, one of them *"the reorder also opened the row's settings
   > strip"* — is Ray's report in the harness's own words.

   **THE ARROWS ARE THE PRIMARY CONTROL NOW, and the drag survives beside them.** Ray, on the
   playtest: *"There should also be little up/down buttons I think to order, that is more obvious
   than dragging anyways."* A 10px handle that appears only on hover is a poor way to state the one
   list whose order is an input.

   > **THE PLACEMENT WAS DECIDED FROM THE WIDTH ARITHMETIC, on a prototype rather than in code** —
   > Ray's precondition: *"if you are going to do that, I need to see a UX prototype before changing
   > the UX."* The row's ~356px is fully spoken for (marker 10 · face ~126, already ellipsised · date
   > 168 · `✕` 32 · four separations), so an arrow pair has to come out of a column that exists. Four
   > placements were drawn and priced: widening the marker slot (−6px off a face that already
   > truncates, and 16 × 13 targets), taking the `✕`'s column (free, full-height targets), the
   > settings strip (free in width, **+22px of height** in a box reading 396 of 396), and a
   > hover-only control (free, and invisible — the very failing being fixed).
   >
   > **THE `✕`'s COLUMN WON**: `▲`/`▼` at 15 × 24 in the 32px it held, so the face, the row and both
   > zone budgets are **bit-identical** — 126px face, 354 of 356 wide, strip 56 of 56 in two lines.
   > What it spends is the one-click withdrawal, which moves right-aligned onto the strip's last
   > line; Ray took that trade explicitly, reordering being the frequent act and withdrawing the rare
   > one. `queue_settings_one_line` — the one width predicate the reservation and the builder share —
   > counts the `✕` now, so its threshold moves 408 → 444 and the two cannot disagree on a dock wide
   > enough to put crop and kit on one line.
   >
   > **NEITHER CONTROL NEEDS AN OPTIMISTIC OVERLAY**, which is 9a paying for itself: `buildQueue` is
   > captured live, so an arrow press and a drop both land on the command's own recapture.

   **9b — THE WORKED ROWS TAKE AN EXPLICIT PRIORITY, and every scarcity reads it. LANDED.** The
   player marks a row **High / Normal / Low**; §2.9's walk consults that mark where it consults list
   position nowhere, and a shed **names the row it took** (`announce_shed_crew`, already shipped).
   The mark sits **on top** of the shipped ordering, which survives as the tie-break — most sources
   sit at Normal, so a rule that fires only on an explicit pick keeps the default behaviour exactly
   where it is.

   > **IT IS A THREE-TIER VALUE, NOT A RANKED LIST, AND §2.9 IS THE REASON.** Item 9 sketched
   > "player-ordered, drag-and-drop" before the eleven steps existed, and a total order is a promise
   > the walk cannot keep: it sheds from **four disjoint pools** selected on structure — step 5 thins
   > a row holding two or more hands, 6 empties an unimproved unqueued row, 9 an improved one, 10 one
   > carrying a build — so a wild patch ranked first is emptied at step 6 while an improved row
   > ranked last is not a candidate until step 9. The player would build a nine-row ordering and
   > watch it honoured in four unrelated pieces. A tier claims only a preference **within** a step,
   > which is exactly what the walk can deliver, and "I am done with this source" already has a verb
   > (`abandon`).
   >
   > **AND A TIER IS SELF-CANCELLING AT THE EXTREMES**, which is why it needs no guard: marking every
   > row High leaves the level constant and the comparator falls straight through to what it did
   > before. There is no degenerate config to defend against.

   > **THE COMPARATOR GAINS A LEXICOGRAPHIC LEVEL, NEVER A COMBINED SCORE.** `least_productive_row`
   > is three levels now — **priority → `pays_any_account` → `yield_per_worker`**, ties still to the
   > earliest row. A tier is a sentence the player typed rather than a measured quantity, so ranking
   > one above the presence test invents no exchange rate between food and materials;
   > `labor_config.json`'s `_comment_weeding` still refuses exactly what it always refused. Only
   > `least_productive_row` changed: the role steps (scout, warrior, keepers, builders) select by
   > ROLE and are not ranked.
   >
   > **A PRIORITY NEVER MAKES A ROW INELIGIBLE.** It orders candidates and does nothing else, so the
   > terminal step still takes the band's last worker off its last row. Pinned by test, because the
   > tempting reading of "High" is a veto.

   **THE PROPERTY IS GENERAL, AND WORKERS ARE ONLY ITS FIRST CONSUMER** — which is item 9's own
   sentence ("pooled maintenance is its *first consumer*, not its owner"), now literal. Every verb
   considered for the face — *shed*, *cut*, *keep running* — named the labor consumer and would have
   lied the moment a second scarcity read the same field, so the face is a bare importance word and
   each consumer states its own consequence. The picker's one line says it without naming a resource:
   *"When something runs short, the band spends it on high priority first."*

   > **SO THE PEN FEED WAS SETTLED IN THE SAME SLICE, because it was positional and nobody had
   > noticed.** A band's pens drew hay and larder food inside
   > `for (idx, assignment) in allocation.assignments.iter().enumerate()`, so when the `FODDER` store
   > or the larder could not cover every pen, **the pen earliest in the vector ate and the last one
   > starved** — and `set_assignment` re-pushes an edited row to the end, so the pen the player had
   > just adjusted was the one fed last. That is §2.9's own defect living in a system that never got
   > §2.9's fix.
   >
   > The settlement sees **every pen at once**: **High served whole, then Normal, then Low, and
   > proportional to demand within a tier.** Proportional needs no new ordering rule to invent and
   > cannot depend on vector position, which is the property that was missing. `last_pen_feed_upkeep`
   > still sums the real debits and the pinned identity
   > `larder_delta == food_income − food_consumption − pen_feed_upkeep − raid_forfeit` still holds.
   >
   > **AND IT IS TWO PASSES, BECAUSE THE TWO STORES ARE NOT THE SAME KIND OF THING.** `settle_pen_hay`
   > runs **before** the assignment loop and `settle_pen_larder` **after** it; the corral arm bids its
   > bread bill rather than drawing it.
   >
   > - **Hay is a STOCK** — the buffer the overwintering carry rides — so it is settled off the store
   >   standing at the top of the pass, and **a pen eats hay harvested on a previous turn.** What that
   >   replaced was not same-turn hay as a rule: it was same-turn hay **iff** the pen's row happened
   >   to sit after the hay Field's, which is the same accident being removed.
   > - **The larder is a FLOW** — `FOOD` is credited *inside* the loop by every gather, hunt and pen
   >   harvest — so settling it ahead of the loop would have meant **a band with an empty larder never
   >   feeding its pens again**, however much it gathered that turn. It settles late, off the income
   >   the turn actually produced. Found in review; the one-turn lag on starvation is what makes late
   >   payment safe, since `pen_fed_fraction` is read in Logistics, a stage and a turn later.
   >
   > Rationale in `.claude/rules/core_sim/graze.md`.
   >
   > **A COMPARATOR TEST WOULD HAVE PROVED NOTHING**, so both consumers are driven end to end: a band
   > at zero slack losing a worker with one Low row and one Normal, asserting which row gave; and two
   > pens on a thin store asserting the same outcome across **three vector arrangements** — natural
   > order, reversed, and after an edit re-pushed the fed row to the end. The first arrangement passes
   > with the defect restored, which is why there are three.

   > **THE FACE, AND WHY IT IS WORDS.** A glyph in line two's free 20px indent was drawn first and
   > rejected in prototype: a symbol nobody was taught, on a line that is otherwise plain text. A
   > ranked row prefixes line two with **`High priority ·`** / **`Low priority ·`** in the tier's
   > colour, and a Normal row prints nothing at all. **LEADING, not trailing** — the four-cash-crop
   > worst case already elides onto the floor clause, so a trailing mark would vanish exactly when a
   > famine made it matter. The control is a fourth inspector link opening a three-button picker
   > **mutually exclusive with the floor picker**, so the strip's tallest state is the one it already
   > reserves in a zone reading 396 of 396.

   **9c — AND THE RANK NEEDS A SURFACE THAT REACHES EVERY ENTRY. LANDED.** The block draws at most
   `BUILD_QUEUE_ROWS_MAX` (3) rows plus `+N more`, and **the queue itself has no cap** — the sim
   holds no length limit anywhere — so a fourth job was queued and funded with no row, and nothing
   past the third could be seen, reordered or withdrawn from the UI at all. 9a gave the order to the
   right band and gave it controls; this is what lets those controls reach the whole list.

   **THE 3-ROW BLOCK IS UNTOUCHED, AND THAT IS THE DESIGN.** Ray's call: it is a SUMMARY — what the
   pool is funding, and what is next — so the cap did not rise, the reservation did not move, and
   what it draws is bit-identical. The full list is a **MODE** over the same zone, which is what lets
   it spend **nothing permanent** in a zone reading 396 of 396: `_queue_expanded` is one bool, and
   every pixel the mode uses is one the collapsed zone was already spending on the board.

   > **TWO DOORS IN, ONE DOOR OUT.** The `BUILD QUEUE` header toggles both ways; `+N more` is a
   > second door IN only, the expanded view having no overflow row left to press. **The pools header
   > stays** — §4.7 moved keeping onto this tab precisely because a pool on one tab and its
   > consequences on another went unnoticed in playtest, and one zone down is the same mistake. **The
   > board GOES rather than shrinking**, with the chips, the pager and the work inspector that serve
   > it; a stub board is neither usable nor free.
   >
   > **THE ONE PIECE OF REAL ENGINEERING WAS EDGE AUTO-SCROLL**, because a 1920 BOTTOM dock affords
   > about nine rows of a list with no cap: the arrows do not care, but a drag that cannot reach past
   > the viewport is a control that silently stops working on exactly the queues this slice exists
   > for. Three mechanisms, each of which fails **silently and alone**: the pump is **per-frame**
   > (a pointer parked at the edge emits no motion events), the direction reads the **physical**
   > pointer (`Viewport.get_mouse_position()`, the same quantity `_drive_drag` warps), and the hover
   > is **re-resolved after every step** (Godot picks the drag-over control on MOTION, so scrolling
   > under a stationary pointer leaves the drop naming the row that used to be there). 6 rows/s ×
   > 28px = 168 px/s, one row of hot band, one row per tick maximum, with a float accumulator
   > because `scroll_vertical` is an int and 2.8px a frame truncates to nothing.
   >
   > **AND THE CLOCK IS WALL TIME, NOT A FRAME DELTA** — every render harness pins
   > `Engine.time_scale = 0.0`, so a delta-driven pump advances by exactly zero under the only thing
   > that can test it (measured: 0px over 45 frames). A general trap for any future rate in this
   > client, recorded in `band-city-panel.md`.
   >
   > **WHAT THE ONE-EXPANSION-AT-A-TIME RULE MEANS HERE**, since the mode breaks its premise: with no
   > board drawn there is no host for a work inspector, so at most one expansion is drawn
   > **structurally** rather than by enforcement. The enforcement stays because collapsing returns to
   > the mixed layout, and entering the mode clears `_work_open_key` — without which a stale
   > inspector springs back on collapse beside an open settings strip, which is the 460-into-396
   > defect §4.7b closed.
   >
   > **The rationale is in `.claude/rules/client/band-city-panel.md` → "THE EXPANSION" and
   > `harness-band-panel.md` → "The EXPANSION's frames".** Both docks fit exactly and no lever of
   > Ray's was needed: the list declares **625px of a 759px box** on the tall LEFT dock and **260 of
   > 394** on the 1920 BOTTOM one, where the scrollbar costs the job face 8px against a face already
   > ellipsised.
10. **Symmetric partial credit — AND the one-position ladder it needs.** **The model is §2.8; this is
   only its place in the order.** A rung's benefit and its cost both scale with how far up the ladder
   the source has been worked, and the two independent meters per source collapse into one cumulative
   position. It removes the last discontinuity — today a build pays nothing for its whole span and then
   everything at once — and it makes the rung-ordering bug (`Field > 0%` while `Cultivation < 100%`)
   unrepresentable rather than merely forbidden.
   > **THE LADDER RESTRUCTURE AND PARTIAL CREDIT ARE ONE CHANGE, not two slices.** Partial credit needs
   > a single fraction to scale by, and the position *is* that fraction — building them separately
   > means building the fraction twice. Decided with Ray on the §4.7 playtest.
   > **THE COST SIDE IS PART OF THIS, and §4.6a is what put it there.** Once the keeping pool holds a
   > meter at any fullness, a patch 10% into a Cultivate is billed the whole rung's rate to hold a
   > tenth of a thing. The natural pairing is that it owes a tenth — the same fraction, applied to
   > what it costs rather than to what it pays. It is deliberately **not** in 6a: the benefit and the
   > cost should move together or the interim is a worse asymmetry than the flat rate is.
   **The blast radius is wide but shallow** — the ~100 binary
   predicates stay (they answer *"has this rung been achieved"*, which still gates the verbs and the
   knowledge); every *payout* branching on them becomes an interpolation on the meter-over-cost
   fraction already published. **What to watch:** the total work is unchanged, so the arithmetic of
   *"is this worth it"* does not move — but the payoff starts on turn one, which softens the
   commitment considerably. That may be right, given this arc has been about removing cliffs; it
   should be a deliberate smoothing rather than a discovered one.
   > **LANDED ON THE PLANT WEB ONLY — this block said "in full" and that was wrong.** The animal web
   > kept its two unconnected meters (`domestication_progress`, `corral_progress`), and both its
   > payouts plus its keeping bill stayed **step functions on a completion predicate**. The claim went
   > unchecked for a whole slice and was found in §4.11's playtest, so **the failure was the claim, not
   > the code**: "in full" is a thing to write once both branches are measured, never once one is.
   > §4.11 completed it — see its own LANDED block. What follows describes the plant half, accurately.
   >
   > **It diverged from this plan in three ways worth recording.**
   > A source carries one `ladder_position` in cumulative work units; `RungStanding` is the one
   > producer of "where is this source", stamped on every write so no call site re-derives it;
   > `interpolate` states the delta form once, for the payout and the keeping demand alike. The
   > **~100 binary predicates figure was wrong** — the real split is **24 payout branch sites** and
   > about 20 verb/knowledge gates, and the plant half funnels almost entirely through the one rate
   > seam. `retain_fraction`, the retention bar and its four stamp sites are **deleted**.
   >
   > **① `partial_credit` IS A RUNG PROPERTY, not the pen special case it was designed as.** Ray:
   > *"make sure it is a configuration of the rung and not something hardcoded for pen."* It is
   > honoured in exactly one place — `RungStanding::credit` is already zeroed for an `on_completion`
   > rung — so no call site tests it. **`animal:pen` is its only member**, and deliberately so: a
   > half-sown field genuinely has half a crop in the ground, while half a fence is not half a pen.
   >
   > **② RUNG 3 ON BOTH WEBS WAS CHANGING THE DRAW, WHICH IS WHY ITS PAYOUT COULD NOT INTERPOLATE.**
   > Found by Ray in play: *"a field can be drawn down and its main goal is to increase the output of
   > the tile. The production draw and the production of a tile are two totally separate concerns."*
   > A Field and a pen were each switching the harvest itself to a flat managed rate with no
   > drawdown, no escapement floor and no engagement bound — which is also why the harvest floor, the
   > one pressure lever the player holds, did nothing on the ground they had spent the most work
   > reaching. **A rung may change production; no rung changes the draw.** A Field now holds ~2.5× the
   > standing crop and regrows ~2.5× faster and is foraged by the ordinary path; a penned herd is
   > drawn down with a real engagement bound in place of the infinite one. **Both are
   > re-expressions, measured against the pinned references**: the Field reads 6.2409 where it read
   > 6.2400 (0.014% off, rungs 1–2 bit-identical), the pen 0.9990 where it read 0.9990, with no
   > existing gain retuned. Rung 3 on both webs can now be over-farmed, which is the point.
   >
   > **③ A QUEUE ENTRY NAMES A DESTINATION, not a rung.** `sow` means *take it to Field*, so on
   > untended ground it lays two legs and costs **125 rather than 75** — the tended rung's work was
   > previously skipped rather than paid. That is a model change, not a tuning edit; if the combined
   > climb is too steep the answer is moving the rung spans in §4.14, never exempting a rung. Each
   > leg's work is what remains **from where the source stands**, which is what makes an existing
   > improvement a receipt rather than a discount. The client's `⌃` opens a "take it to…" ladder
   > track as an overlay, so neither Work-zone budget moved.
   >
   > **Three defects fixed on the way, each reachable in play:** a herd paid the **pen** rung's
   > keeping from the first corral work banked while still getting only pastoral benefits (the
   > benefit/cost asymmetry §2.8 forbids); `decay_ladder` reported only the top rung crossed, so a
   > bleed spanning two boundaries took two rungs and announced one; and an interpolated demand is a
   > moving target across the Population→Logistics carry, so a fully-staffed band bled ~0.03
   > work/turn forever while re-arming its neglect grace every turn.
   >
   > **⛔ THE SUITE WAS REPEATEDLY GREEN BECAUSE NO FIXTURE REACHED THE STATE — four times.** The
   > overdraw ⚠, the hunt byproducts, the material-only work row and the two-leg queue entry each had
   > passing tests that could not distinguish the defect from the fix. Every fix in this slice was
   > therefore **falsified** — the defect restored, the failing assertions counted and named — and
   > that is the practice to keep, not the fixtures.
   > **⛔ AND IT LANDED ON THE PLANT WEB'S *RATES* AND NOT ITS *MIX* — the unfinished half, closed
   > later.** The composition seam (`forage::patch_composition`) went on resolving at the rung
   > **achieved**, under an explicit note that a basket "is the one thing that cannot be
   > interpolated": mixing two baskets would invent shares of plants that are not growing there. The
   > smoothing, it said, was carried by the rates.
   >
   > **Both halves of that were wrong, and the second is what play found.** Reported by Ray: a tile
   > paid `+0.35 food · +0.07 fibre` one turn and `+0.00` with no material clause the next — the turn
   > its Sow completed. It had been committed to **tobacco**, a cash crop paying no food and no
   > fodder, and completion forced the tile's mix to 100% tobacco in a single step.
   >
   > - **The rates it delegated to do not move across a Sow.** `favored_conversion_gain` is flat by
   >   design (`tended_conversion_gain` 2.0 → `field_conversion_gain` 2.0), and
   >   `field_capacity_gain` / `field_regrowth_gain` land on the take **ceiling**, which sits above
   >   the worker cap on any normally-staffed row. Every smoothed term was inert, and the one term
   >   that decides what the ground pays — the mix — was the one that cliffed. **The discontinuity
   >   §4.10 exists to remove survived in the place nobody looked.**
   > - **The "invents plants" objection does not hold for this pair.** `planted` is a reweighting of
   >   `weeded`, which is a reweighting of the tile's own realized mix: every species in the later
   >   basket is already in the earlier one, so a blend only raises the favored share and lowers the
   >   others, and the shares still sum to one. It names no plant the ground was not already growing.
   >   That is ① above stated on the mix rather than on the meter — *"a half-sown field genuinely has
   >   half a crop in the ground"* — so refusing to interpolate the basket contradicted the very
   >   reason `animal:pen` is `partial_credit`'s only member. **The objection is retained where it is
   >   still true**: a material's *characteristic vector* cannot be averaged, which is why the
   >   material account is decomposed per species rather than blended.
   >
   > **So the mix interpolates on BOTH plant rungs**, through `intensification::interpolate_composition`
   > — [`interpolate`]'s vector twin, blending the held basket with the raising one per species at
   > `RungStanding::credit`. Reading `credit` is the *only* test of `RungPartialCredit`, so an
   > `on_completion` rung's basket still steps at completion for free. Weeding was smoothed with
   > sowing: leaving Cultivate stepped would make the ladder smooth on one rung and cliffed on the one
   > below it. **The blend is re-sorted into the wire's total order**, because
   > `default_species_for_rung` reads a basket's first entry as its dominant plant.
   >
   > **AND THE TAKE SELECTION HAD NO REPAIR PATH, which is what turned the cliff into a zero.**
   > `LaborTarget::Forage::take_species` has one writer (`assign_labor`) and nothing pruned it, so a
   > crew that had named the plants a Sow displaced held a selection summing to **zero share** — and
   > a zero selected share is a zero take *ceiling*, in food and materials alike. Interpolation makes
   > that a fade rather than a cliff, but it still arrives at zero, so the commitment now **prunes**
   > the selection and adds the crop it committed to. **Prunes, never overwrites**: a `planted`
   > basket keeps whatever stands outside the worked ground, so a sown tile with a fishery still has
   > fish in it, and a blanket reset would re-tick plants the player had deliberately unticked.
   > Nothing surviving the prune falls back to the whole basket rather than to the crop alone.
   >
   > **The command boundary was judging the wrong basket, and freshly, not stalely.** The take
   > selection resolved against the tile's raw wild realization while the take path narrows against
   > the rung-reweighted mix, so on any tended or sown patch it accepted a selection the very next
   > turn valued at zero. It judges the same mix the take will narrow.
   >
   > **⛔ AND WHAT THAT MIX DECIDES IS WHAT IS PRUNED, NOT WHAT IS REFUSED.** Judging the patch's
   > own mix is right. Hard-refusing on it is not, because the mix **moves under a stored
   > selection** — that is what a Cultivate or a Sow *is* — so the names found absent are typically
   > ones that were legal when the player made them and that the player's own crop then weeded out.
   > Refusing them refused the whole `assign_labor`, **worker count and all**: a Field standing at
   > `Wild Emmer 100%` whose row still named Wild Pulses could not have its tenders raised at all,
   > turn after turn, and the only thing said was *"Harvest failed — Wild Pulses does not grow at
   > (13, 10)"*.
   > The panel could not clear it either — a chip is drawn only for a plant the **current** mix
   > carries, so the stale key had no control attached to it.
   >
   > So the command runs the **same narrowing the commitment above it runs**
   > (`TakeSelection::pruned_to`, which `pruned_for_commitment` is a wrapper over): absent names are
   > dropped, the rest is kept, nothing surviving falls back to the whole basket, and it lands. One
   > feed line says so, because a selection the sim narrowed is a change the player did not ask for.
   > A key **no roster carries** is still refused by name — that is a typo, nothing can be inferred
   > from it, and one bad key spoils the whole selection rather than being filtered out of it.
   >
   > The general shape: **a gate belongs where the player's input can be wrong, a repair where the
   > WORLD can move underneath it.** These two failure modes arrive at one validator looking alike,
   > and answering both with a refusal makes the player's own investment into a lock.
11. **Plant upkeep SCALES WITH THE SOURCE.** Both plant rungs ship `scaled_by: flat`, so a rich
    alluvial patch and a thin one cost the same to hold. Ray: *"the flora track should scale by size,
    just like animals."* The whole-number demands were an explicit short-term step, not the model —
    §2.6 already says a flat per-rung number cannot be right, because what makes a thing expensive to
    hold differs by what it is. The animal web has had this since slice 4 (`SourceLoad`, the herd's
    own keeper load); the plant web needs its own measure, most likely the patch's capacity.
    **Mechanism, not tuning** — it is a scale primitive, and the numbers move in §4.14 regardless.
    > **LANDED.** The plant's measure is `forage::patch_tender_loads` =
    > `tile forage capacity / capacity_per_tender`, the exact twin of `head count /
    > animals_per_herder`, so one rate says *a tender minds this much standing crop*. Both plant
    > rungs declare `scaled_by: source_load` at unchanged rates of `2.0` and `4.0`.
    >
    > **① THE MEASURE READS THE TILE'S K, NOT THE PATCH'S — and the alternative was measured, not
    > argued.** `ForagePatch::carrying_capacity` is the tile's K *already multiplied* by an
    > interpolated `field_capacity_gain`, and the demand interpolates on the same position, so the
    > two compound: under sabotage a Field on `RiverDelta` billed **10.898** work/turn against the
    > **4.308** it owes — the 2.53× landing on top of the rate's own climb, exactly as predicted.
    > The tile's K is the size of the *place*; the gain is the rung's *payout*. `labor_config.json`
    > already stated the first half — *the land owns K and no rung may lower it* (#433) — and this is
    > the second: **no rung may be billed for the K it raised.**
    >
    > **② `capacity_per_tender` IS 195.0, the reference tile's own K**, so the conversion is provably
    > pacing-neutral on `AlluvialPlain` and nowhere else, which is the point. Measured, work/turn:
    >
    > | tile | K | tender-loads | tended | half-raised | Field |
    > |---|---|---|---|---|---|
    > | `PrairieSteppe` | 70 | 0.359 | 0.718 | 1.077 | 1.436 |
    > | `AlluvialPlain` | 195 | 1.000 | **2.000** | **3.000** | **4.000** |
    > | `RiverDelta` | 210 | 1.077 | 2.154 | 3.231 | 4.308 |
    >
    > **The plant demands stop being whole numbers a player can staff on the nose**, which they were
    > chosen to be. That is the cost of scaling and it was taken deliberately: *"two hands hold a
    > tended patch"* is now true of one biome rather than of the ladder.
    >
    > **③ `UpkeepScale::Flat` AND `UNSCALED_UPKEEP` ARE DELETED — this answers §6's open item.** With
    > both plant rungs scaled, nothing declared `flat`, and the rung-monotonicity check compares only
    > adjacent rungs *sharing* a `scaled_by` — so the unused variant was a way for a rung to opt out
    > of that check silently. With one variant every adjacent pair is now compared. **The `scaled_by`
    > key stays**: §4.13's `length × terrain` is the next primitive to land in it. One primitive with
    > a per-branch reading beat a second variant, because the rung already declares its `branch` and
    > a variant would have restated it.
    >
    > **④ THE PRICE QUOTE HAD TO MOVE WITH THE BILL.** `cultivationUpkeepDemand` /
    > `fieldUpkeepDemand` — what the compose sheet shows *before* you commit — were the bare ladder
    > rates, identical on every patch. Left alone they would have quoted `4.0` for a Field the
    > keeping pool bills `4.31`: two producers of one verdict, the failure this arc keeps repeating.
    > Both now strike off the same patch's tile as the bill, pinned on a **non-reference** tile since
    > on `AlluvialPlain` the right and wrong answers agree.
    >
    > **Every fix was falsified** — the defect restored, the failing assertion named, the fix put
    > back — including the interpolation test, which had to catch the measure being applied twice
    > (`3.479` against the correct `3.231`).
    >
    > **⑤ AND IT FINISHED §4.10, WHICH HAD ONLY LANDED ON THE PLANT WEB.** A herd now carries one
    > `ladder_position` like a patch; `herd_density_gain`, `herd_ecology`'s `regrowth_rate` and
    > `herd_upkeep_demand` interpolate on it; `herd_keeping_meter` is retired and the demand takes no
    > verb. **The asymmetry this removed was the inverted one:** `owner` is set by the *first* `Tame`
    > accrual, so a herd owed the **whole** pastoral keeping rate from turn one while `is_domesticated()`
    > withheld **every** payout until the last — 100% of the cost on day one against 0% of the benefit.
    > The pen keeps its step through `partial_credit: on_completion` rather than a hand-written
    > predicate.
    >
    > **⛔ AND IT MEASURED THE FLOOR TRAP, WHICH IS WORSE THAN "TAKES NOTHING".** The escapement floor is
    > `floor_fraction × K` against the density-boosted ceiling, so a rung raises the floor while the herd
    > stays put. On aurochs starting **exactly on** its floor: the room reaches zero at turn **6** with
    > one herder, **3** with four, **2** with eight — *building faster starves you sooner* — and because
    > `eligible` reads that same room, **the tame then never completes at any crew size**. It is the `-4`
    > escapement stall reached by the floor climbing rather than by over-hunting. **Five of the eleven
    > tameable species** are on the losing side of that race (aurochs, marsh grazer, reindeer, steppe
    > runner, wild horse); three clear it only barely; only the fast breeders are safe. Interpolating
    > turned the cliff into a slide without removing it, which is why the floor gets its own fix: **the
    > take is the room above the floor OR a share of the turn's growth, whichever is larger**, and the
    > build's eligibility gate moves with it.
12. **The MATERIAL HALF of build and upkeep** (§2.7). Today a rung costs **work and nothing else**,
    on both the pile and the rate. This adds the second term to both, and gives it a consumer on the
    day it ships by reclassifying **hurdles**: a fence panel is a material you build in and leave
    behind, not a tool you carry to the next job.
    > **Routes are what force it, but routes are not what proves it.** A road wants hands *and*
    > quarried stone, and §4.13 cannot be written without this term. But a mechanism whose only
    > consumer is the next slice is the failure mode — so the pen is re-expressed through the general
    > path here, and §4.13 inherits something already load-bearing.
    >
    > **THE MECHANISM.** `RungBuild` gains a material pile beside `work_cost`; `RungUpkeep` gains a
    > material rate beside `work_per_turn`. **Both track the position** (§2.7): the pile draws in
    > proportion to the work banked, the rate scales with how much of the rung stands, and decay
    > refunds nothing. A short draw is a shortfall like any other and drives the decay and shed paths
    > that already exist — **no new penalty**.
    >
    > **THE MATERIALS.** `wood` is added and supplied through worldgen's existing
    > `StartKit { equipment, recipes, materials }` — produced by nothing yet, which is deliberate and
    > has its own tracker item. `hurdles` stops being equipment and becomes a **material crafted from
    > wood** (`RecipeOutput::material`, the weaving craft); `animal:pen` declares it on both terms.
    >
    > **AND THE KIT ROSTER MOVES WITH IT, because removing hurdles-as-equipment empties two kits.**
    > `hurdling` takes a **crook** (bone + fibre) — one animal kit serving both husbandry keeping and
    > animal builds, the shape `tillage` already has for plants — without which every tame, pen build
    > and turn of herd keeping drops to bare-handed speed. `husbandry` loses hurdles and is left
    > **sled-only**, which is correct while a penned herd resolves no fight; §4.9 item 12b is what
    > deletes it. `pen_carry`'s bare-handed reading moves to the **sled**, which already owns the
    > equipped side of that pair, and the `biomass_collected` wear goes with the item.
    >
    > **FOUR DECISIONS, SETTLED, so they are not re-derived:** a store that cannot cover a build
    > **queues and stalls** rather than refusing — a build whose builders leave already stalls, and
    > the five verbs' affordability gate was deliberately retired in §2.5. A shortfall of *either*
    > kind trips the rung's **existing `grace_turns`** — the amounts stay separate so a full store
    > cannot paper over missing hands, but a second counter is a second dial free to disagree. A short
    > store splits by **`SourcePriority` tiers alone** (`settle_scarce_store`), ignoring
    > `upkeep_mode`: the rank is the player's per-row answer and the mode exists for a pool that has
    > none, so reading both would let a row marked `High` starve with nothing on screen saying why.
    > And the demand **interpolates on position** — the general rule, which the plant rungs exercise.
    > ⛔ **The pen does NOT test it, and this clause claimed the opposite.** `partial_credit:
    > on_completion` gates the **cost as well as the payout**: `RungStanding::at` pins `credit` to
    > zero for such a rung, so `interpolate` returns the rung *below* for the whole climb, and
    > `animal:pen` is the ladder's only member. That is right rather than a gap — **you spend hurdles
    > from the BUILD PILE while raising a fence, and owe the upkeep rate only once it stands**, there
    > being no fence to mend mid-climb. The tamed herd's own `animal:pastoral` upkeep continues
    > throughout, which is why the work account is non-zero there and the material one is not. See
    > `.claude/rules/core_sim/husbandry.md` → the pen's hurdle bill.
    >
    > **THE READOUTS, and the surface is the `⌃` TRACK — not the compose sheet.** Foraging and
    > hunting have no hold cost; the improvement is chosen from the work row's `⌃`, which opens
    > `RungLadder.build_track`. Its rows already carry an **asides array** (a locked rung's reason, a
    > crop row's price face), so a selectable rung takes two more: *what it eats to raise* and *what
    > it costs to hold*. Beside that: a work-row shortfall note that **names the missing good**, since
    > `WORK_ROW_UNDER_KEPT_NOTE`'s *"raise this band's Agriculture role"* is wrong advice when the
    > missing thing is stone; and a standing-bill disclosure row on the **band and faction pages**,
    > beside Food and Fodder, stating wants-against-comes-in per good.
    >
    > **THE `Gear` ROW COMES OFF BOTH PAGES.** It does not compress to a line and the crafting panel's
    > kit ledger already owns it — the Builders card's own gear line was retired in §4.7 for exactly
    > that reason. What replaces it is **notification**: `equipment.json`'s `life_readout` already
    > ships `warn_fraction` 0.34 and `danger_fraction` 0.10 with *"nothing in the sim branches on
    > either"*, and the danger seam was deliberately set inside a kit item's rebuild time. Wire them
    > to the event dock — warn → Notable, danger → Alert — plus an Alert, **naming the band**, for a
    > material the standing bills eat faster than it arrives. That last one is what replaces the
    > faction `Gear` row's `⚠ 1 band` discovery path.
    >
    > ⛔ **THREE THINGS THIS SLICE DELIBERATELY DOES NOT DO.** It does not put a kit line on the
    > Agriculture and Husbandry cards: those cards are the band's **head count** for a web, and the
    > keeping kit is a property of the **work site** (§2.5) — so the kit belongs where the rung is
    > known, on the source, and never on a band-wide role card. It does not make the pen's containment gains (`pen_gain`, `pen_density`,
    > `herd_engage_rate`) **scale** with how well the fence is kept; that is a real idea, it is
    > tuning-shaped, and it would make this slice's failure mode impossible to falsify against the one
    > that already exists. And it does not touch the feed settlement (§2.7).
    >
    > **UX prototype**, drawn against the shipped surfaces and their real metrics — the `⌃` track at
    > its true 292px, the work row, the band bill:
    > `https://claude.ai/code/artifact/a7c6333d-b510-4cc1-a9e1-9829b288b49b`

**12b — THE TAKE AT EVERY RUNG: wariness, and the pen's missing fight.** Independent of item 12 and
lands after it, sharing only the kit roster that item 12 has to rewrite anyway.

> **A tamed herd fights exactly as a wild one does**, because `herd_quarry_fight` reads the
> species and its wounds and consults no rung. **A penned herd does not fight at all** —
> `fight: NO_FIGHT_STAGE` when `is_corralled()`. So the ladder is a mode switch rather than a
> slide, and taming buys nothing at the kill.
>
> **The take should run its three stages at every rung, with the rung tuning the first two.**
> *Engage* — how many you can get hold of; the pen already raises this through
> `herd_engage_rate`. *Retreat* — how many stay rather than bolt; this is `wariness`, an existing
> species term a kit's `dispersion` already multiplies, so `pastoral_wariness` / `pen_wariness`
> are the same per-species multipliers `pastoral_gain` / `pen_gain` already are. *Fight* — how
> many go down; **species defense against the party's attack, unchanged at every rung.**
>
> **Containment solves catching; weapons solve killing.** A pen makes the take *reliable*, not
> *safe* — a contained bull can still gore you. The consequence to state plainly is **no weapons,
> no beef**: a bare-handed band can pen an aurochs and never butcher one.
>
> ⛔ **AND THE ROSTER SAYS THAT MORE SHARPLY THAN THIS CLAUSE ONCE DID.** It read *"while goats
> and sheep stay killable by hand"*, which is false by one point of `defense` and was never a
> decision anyone took. The bare hand's `attack` is `1.0` (`creatures.json` `person.combat.attack`)
> and the gate is `max(0, attack − defense)`, so of the seven species that can be penned
> (`husbandry_ceiling: "pen"`) only the three `defense 0` rows — **rabbit, fowl, snow hare** — fall
> to a bare hand at all. `crag_goat` and `wild_sheep` are `defense 1` and yield exactly zero;
> `boar` is 2 and `aurochs` is 6. **No roster value moves for this** — the fight is the one stage
> the rung may not tune, and retuning it to rescue an illustration would be tuning the wild hunt
> too. What the pen's default kit resolves to is `big_game` (spears, `attack 20`), so the bare hand
> is a choice the player makes, not the state they start in.
>
> **Deleting `NO_FIGHT_STAGE` FIXES what it was papering over.** It exists because a bare-handed
> band was quoted nothing and then paid a take — forecast and take disagreeing. With one
> mechanism they agree by construction: quoted almost nothing, gets almost nothing.
>
> **And the `husbandry` kit dies here.** Item 12 leaves it sled-only, which is right while a pen
> resolves no fight; the moment one does, a weaponless kit is obviously wrong and it collapses
> into `big_game` — the hunters who took the herd wild are the hunters who take it penned, with
> the gear they already carry.

**12c — ONE WORD FOR THE TAKE CREW, AND THE WORK STRIP STATES BOTH KITS.** The readout half of the
per-site upkeep kit (§2.5), and **the next slice after item 12**. Rides on it: the strip's Upkeep
picker only means what it looks like once the kit belongs to the site.

> **THE PLANT WEB'S TAKE CREW HAD TWO NAMES AND THE SECOND ONE WAS TAKEN.** The noun follows the
> standing rung — *Foragers* on wild ground, *Tenders* on a Tended Patch or a Field — so on a Field
> the sheet reads `ASSIGN TENDERS` and then offers the Gathering kit, which looks like a bug and is
> not: the tending is the **Agriculture pool's**, and a hoe does nothing for a harvest. Reported
> from play by Ray, who knows how it works and was still caught by it in the moment.
>
> **`Harvest` REPLACES BOTH, AT EVERY RUNG.** It is already this repo's cross-web word for taking
> from a source (a *pen harvest*, the *harvest floor*), it is neutral between wild and cultivated —
> which is the whole defect — and it survives the tech ladder, where *gatherers* would not. The
> rung mark on the row still says which ground it is; what stops changing is the crew's name,
> because the crew never changed. **The hunt sheet is NOT renamed**: `Hunters` is specific and
> collides with nothing.
>
> **THE WORD LANDS IN TWO GRAMMATICAL SLOTS AND TAKES TWO FORMS: the crew NOUN is `Harvesters`, the
> VERB is `Harvest`.** *"Replaces both"* above names the noun pair `Foragers`/`Tenders`; the row label
> and the commit button are a **verb** slot (`Forage (27, 26)` / `Tend`), and one string cannot fill
> both — `Assign harvest` and `♻ 3 harvest` are the readings that proves it. The existing noun→verb
> tables already do exactly this job (`WORK_ROW_PLANT_FORMATS`, `PLANT_ASSIGN_BUTTONS` are keyed off
> the resolved noun), so they simply collapse from two entries to one. So: `Harvest (28, 16)` on the
> board and the strip head line as drawn in the prototype, `Harvesters [Gathering kit ▾]` on the kit
> line beside the animal web's `Hunters`, `Assign harvesters` on the sheet, `♻ 3 harvesters` on the
> standing summary, and *"send at least one harvester"* on the dead button. Ray's call, taken against
> the prototype, which drew only the verb slot and so left the noun open.
>
> **`HudFormat.plant_crew_label` is the seam** — the one place the Foragers/Tenders fork is spelled.
> The one mismatch the choice costs is harvesters holding a *Gathering kit*; the kit's **id** stays
> `gathering` and only its `display_name` is player-facing, so closing it is a one-field config edit
> with no id or save churn.
>
> **THE STRIP IS THE SURFACE, because it is the one place the RUNG is known.** That is item 12's own
> phrase, used when it declined a kit line on the Agriculture and Husbandry cards. The work board's
> inspector strip already knows which source is selected and states none of it — not the rung, and
> neither kit. It gains: the **rung state on the head line** (`Harvest (28, 16) · ▦ Field 100%`,
> through `DetailFormat.rung_row_value`, so the strip and the tile card cannot word it differently),
> and **one line carrying both kits** — `Harvest [Gathering kit ▾]  Upkeep [Tillage kit ▾]`.
>
> **BOTH ARE PICKERS, and both reach exactly this row.** `agriculture` offers what `forage` offers —
> one real kit plus `none` — and `none` is a real choice, being how a site is worked bare-handed to
> conserve the tool. Reasoning from today's thin roster is what nearly made the upkeep one read-only;
> the roster is early and will not stay thin.
>
> **AND AN ANIMAL ROW GETS BOTH PICKERS TOO — the paragraphs above are written in plant vocabulary,
> which is not the same as being plant-only.** The strip is the one place the rung is known on
> **either** web, and `upkeepKitId` is already published for herds as well as patches
> (`snapshot/subsistence.rs` → `upkeep_kits.herd(...)`), so the pair mounts on a herd row from the
> same producer. **What differs is only the LEFT picker's word**: the rename below is the PLANT web's
> crew, so an animal row's take crew is still `Hunters` and the pair reads
> `Hunters [Stalking kit ▾]  Upkeep [Hurdling kit ▾]`. The right-hand picker is the same control on
> both webs, and `upkeep_kit`'s own grammar already carries both source shapes
> (`upkeep_kit <faction> <x> <y> [kit <id>]` / `upkeep_kit <faction> <herd_id> [kit <id>]`).
>
> ⛔ **SUPERSEDED BY ITEM 12d — THE PAIR IS A PICKER, NOT A PERMANENTLY DRAWN LINE.** The two
> paragraphs quoted below were written against the inspector as a block inside the work zone, and both
> are wrong about the shipped client. They are kept because a reader who finds only an absence will
> re-derive them:
>
> > *"THE PAIR IS ONE `flex-wrap` ROW, NOT TWO HAND-PLACED ONES. It rides one line where there is room
> > and drops the second onto its own line where there is not … the height to reserve is the stacked
> > one and the single line is the saving."*
> >
> > *"The metrics are the shipped ones … the strip is 356px, its base `WORK_INSPECTOR_EXTENT` 58 +
> > `WORK_INSPECTOR_SLACK` 6 = 64px, a text line 14 + 6 = 20px, a picker 32 + 6 = 38px. The rung costs
> > +0 … and the kit line +38."*
>
> **Three of those figures were wrong when measured, and the fourth held.** A picker in the WORK zone
> is **22px**, not 38 — the 32 is the *compose sheet's* control, and the block gap is charged once per
> block rather than per line. The pair **never rides one line at any shipped dock width**: one line
> needs 472px against a widest-ever work zone of 382, so the wrap is not a saving, it is the only
> case. And the strip had **4px of spare**, so a `+38` line — or a `+22` one, or any line at all —
> overflowed it; item 12d has the measurements. The rung's **+0** is the one figure that survived, and
> it shipped exactly as written.
>
> **WHAT SHIPPED:** the rung state on the head line (+0, as promised), and the pair as a **`Kits`
> picker** beside `Change policy` and `Priority`, opened on demand. Ray's, on being shown the
> overflow: `_work_picker_open` was already three-valued so the strip *"pays for AT MOST ONE picker by
> construction rather than by discipline"*, so the reservation takes the **max, not the sum**, and a
> fourth value shorter than the tallest existing picker costs the panel **nothing**. Item 12d then
> moved the whole inspector out of the zone and the sections became always-drawn; see there.
>
> ⛔ **THREE THINGS THE DESIGN REJECTED, so they are not re-proposed.** A **second picker on the
> Agriculture pool card** — redundant once upkeep is settable on the row, and two controls writing one
> value is a drift surface. A **read-only upkeep line** beside an editable harvest one — it was written
> when the kit was per-band, and a control with one visible option today is not a control with one
> option tomorrow. And a **scope warning** on the upkeep picker (hover, or a `(band)` word on the
> label) — that existed only to admit that setting it on one row changed every row, which §2.5's
> per-site kit deletes outright.
>
> ⛔ **AND THE MATERIAL SHORTFALL NOTE STOPS SAYING A PEN *EATS*, WHICH IS NOT A WORDING NIT.**
> `WORK_ROW_MATERIAL_SHORT_FORMAT` ships as *"Short of hurdles — 0.40 of the 0.58 a turn this pen
> eats"*, and a pen genuinely **does** eat: grass and hay. §2.7's whole argument is that **hay is FEED,
> not upkeep**, and #578 retired a defect that billed a pen's shortfall to the keepers' food larder —
> so this sentence puts feed and upkeep back under one verb, on the surface whose `Fed: 100% — all
> pasture` row sits two lines above it. The readout undoing the model is a defect, not a preference.
>
> **It reads `Short of hurdles — 0.40 of the 0.58 a turn it needs.`**, and it **names a remedy** the
> way both its siblings do (*"raise this band's Agriculture role"* / *"…Husbandry role"*): the remedy
> for a missing GOOD is the bench or a trade and **never more hands**, which is already the wording the
> blocked-build cause carries. The `a turn this %s eats` tail retires with the
> `MATERIAL_SHORT_NOUN_HERD` / `_PATCH` pair, which existed only to fill it.
>
> **AND IT BELONGS ON THE WORK ROW, NOT THE TILE CARD — §4.7 ALREADY SET THAT SHAPE.** That slice
> pulled the `At risk:` block off the tile card for being *"way too wordy"*, left one state word on the
> rung row (`🌾 Tended 90% ⚠ slipping`) and put the sentence on a hover, keeping the countdown on the
> work board **and nowhere else**, because *"the board is where staffing is decided this turn … on the
> tile card it is a number you cannot act on."* Item 12 shipped the full sentence to **both** entry
> points (`under_kept_note` and `under_kept_note_for_source`) and so regressed against that rule.
>
> **The staffing half of §4.7's reason does not carry over, and a better one replaces it**: no head
> count fixes a missing good. What this UI actually offers against a scarce one is the row's
> **`SourcePriority` rank** — `High`/`Normal`/`Low` is precisely what decides which pen the hurdles
> reach when there are not enough (`settle_scarce_store`) — and that control sits in the work row's own
> strip. The tile card has nothing to press. So: the sentence on the work row and its strip, and the
> card keeps at most the short state word its keeping shortfall already reads.
>
> **AND `extend_pen` IS DECLARED FROM THE WORK ROW, ON THE STANDING RUNG'S OWN MARK.** Reported from
> play: extending a pen is a button on the TILE card, and it produces a **build queue entry** — so it
> is the one queue entry in the game declared from somewhere other than the work tab.
>
> **The mechanical reason it ended up there:** `RungLadder.has_track` is false when nothing sits above
> the standing rung, and `animal:pen` is the top of the animal branch — so a corralled herd's row
> renders **no `⌃` at all** and there was no affordance to hang it on. Extending a pen is precisely
> what you do *after* the ladder is finished. The prose in `selection-card.md` blamed it on being *"a
> one-click standing action, not a compose flow"*, which is true and is the second reason.
>
> **The fix is a `⌃` on the STANDING-RUNG MARK, not a row in the track.** Ray's, and it is better than
> putting it in the track on three counts: the track stays **one position on a ladder** (a ring is a
> repeatable increment with no position, so it has no honest row shape there); the mark sits on **the
> thing the job acts on**, since a ring extends the pen the mark denotes; and the day the animal
> branch gains a rung above `pen`, a herd offers **both** — extend what stands, or climb — which is
> two actions on two things and reads as two marks far better than as one list.
>
> **A ROW ALREADY CARRIES TWO MARKS AND THAT IS THE PRECEDENT.** The standing-rung glyph
> (`BandPanelController`'s rung-mark resolver — `CORRAL_GLYPH` penned, the pastoral glyph tamed, the
> tended/field glyphs on the plant side) is a **separate slot** from the `⌃`/building face, so a
> mid-Sow patch already reads `🌾` *what it is* beside `▦ 28%` *what is being built on it*.
>
> **IT OPENS A PRICE, NOT A BARE COMMIT — and item 12 is what makes that necessary.** A ring draws
> `animal:pen`'s own hurdle pile now (§2.7; it drew none until the defect was fixed alongside item 12),
> so a one-click button states a cost nowhere. Opening the same shape the track's `⌃` opens — *what it
> eats to raise, what it costs to hold, and where it will stall* — keeps the caret meaning ONE thing on
> every mark that wears it, and puts the goods cost at the point of decision rather than in the queue
> afterwards.
>
> ⛔ **AND THAT ARGUMENT NAMED A NUMBER THAT WAS NOT ON THE WIRE.** The card shipped stating the ring's
> WORK price and its standing bill, and **not** the pile — the one fact the paragraph above argues it
> exists to state. `RungLadder.ring_row` asked `buildMaterialCost`, whose field prices the rung
> DIRECTLY ABOVE where the source stands; the card only ever opens on a herd on `animal:pen`,
> `RungKey::AnimalPen.above()` is `None`, and the sim published an empty pile there **on purpose** —
> *"Empty at the top of the branch, which is the honest reading rather than a repeat of the pen's
> own"*. Every `MaterialPayoff` field on the herd table was searched: nothing carried `animal:pen`'s
> own build pile to a herd standing on it, so this was a **sim-side gap**, and re-deriving the pile in
> the client would have been the second producer this arc keeps deleting.
>
> **Closed by `corralBuildMaterialCost`** on the herd wire — the `animal:pen` rung's own pile,
> UNSCALED (penning takes no per-species multiplier and `head_ring_leg` prices the ring's width at
> `RUNG_COST_UNSCALED`, so a scaled quote would show one price and charge another), resolved live off
> the ladder and published whether or not a ring is in flight. It is the **build** twin of
> `corralUpkeepMaterialDemand`, which exists for the mirror-image reason one row down: a *pastoral*
> herd is the only source the `⌃` offers the Pen rung from and its own rung declares no material, so
> the stamped bill is empty on exactly the row the player is deciding on. On a pastoral herd the two
> build fields agree **by construction** — one `rung_material_pile(ladder, AnimalPen)` call reached
> through two selectors, not a second reading. There is deliberately no `tameBuildMaterialCost`: the
> tame rung has no repeatable increment, so nothing would read it.
>
> **The reusable lesson, and it is the same one item 12d recorded:** a design argument can name a
> quantity no published field carries, and a client slice can ship *around* the hole while every
> assertion it makes passes — because those assertions said the card was PRESENT and never that it
> said what the argument promised. The harness claim written at the time was deliberately partial
> (liveness, the work price and the hold aside, and **nothing** about the pile) precisely so it would
> neither cement the defect nor fail on shipped behaviour; the pile's claim lands with the field.
>
> **A ring in flight shows a BADGE, not a caret**, exactly as a queued build does, so a second ring
> cannot be declared over the first — `Herd::pen_extending` is already the sim's gate for that and no
> new one is needed. **The tile card's `Extend pen` button and its `Fencing N%` badge retire with the
> move**, both being duplicates of what the row and the build queue then state.
>
> **UX prototype**, drawn against the shipped surfaces at their real metrics:
> `https://claude.ai/code/artifact/9ea539e6-9a65-4ca2-9fc1-32dda77d2d14`
>
> ⛔ **THAT PROTOTYPE DRAWS THE SUPERSEDED BLOCK FORM** — a permanently mounted kit line in the strip,
> at the 38px picker that measurement later disproved. It is the record of the design as decided, not
> as built. The measured one, drawn 1:1 against the real geometry and showing why the block form could
> not fit, is `https://claude.ai/code/artifact/cb21b3d1-e2c6-4579-b72f-fbb4eaaff890`.

**12d — THE WORK INSPECTOR BECOMES A DIALOG, AND THE ZONE STOPS PAYING FOR IT.** Ray's, decided
against the measurements item 12c forced. Not a readout slice: it is the structural answer
`band-city-panel.md` has been asking for, in a cheaper form than either shape that file names.

> **THE ZONE HAS FOUR PIXELS AND EVERY EXPANSION OVERFLOWS IT.** Measured on a 1920 bottom dock with
> a row selected: the box is **396** and the strip closed asks **392**. Open the *shipped* priority
> picker and it asks **444** — over by **48**. Open item 12c's kits picker and it asks 436, over by
> 40. There is no expansion small enough: `_work_board_capacity` is already floored at `maxi(1, …)`,
> so the board gives back 4px of `int()` truncation and **zero rows**.
>
> **THE PRIORITY PICKER HAS OVERRUN THAT DOCK SINCE §4.9 ITEM 9b and no frame ever rendered it**, for
> the reason `band-city-panel.md` records twice: *every picker-open frame is a tall dock, and every
> wide-dock frame has the expansion closed*. Two disjoint frame families with the defect living in
> the gap — the same shape as the bug that raised `PANEL_HEIGHT_WIDE` to 456 in the first place. Item
> 12c's own measurement is what finally rendered it.
>
> **AND TWO SMALL BOTTOM DOCKS ALREADY OVERFLOW WITH NOTHING EXPANDED** — `1152×720` by **25** and
> `1024×768` by **27**, on shipped code with no kit pair, both against boxes `MAX_WIDE_HEIGHT_FRACTION`
> has clamped. So the horizontal dock's height is not short by one control; it is short.
>
> **A WINDOW CANNOT CHANGE A ZONE'S HEIGHT, and that is the whole fix.** `_open_rung_track`'s own
> docstring already argues it for a smaller piece of content in this same zone — *"the detail
> breakdowns are popovers and the destructive confirms are `ConfirmationDialog`s. It costs the zone
> nothing at all."* The inspector is the piece the argument was never applied to. Hosting it as a
> window makes the overflow **impossible** rather than made to fit, and hands the board back the
> **84–190px** the strip reserves — two to four rows on a dock floored at one.
>
> **IT IS A DIALOG, NOT A POPOVER, AND THE PAIRING IS THE POINT.** Anchored-and-transient is one
> coherent thing (the rung track: open, pick, gone); persistent-and-placed is the other. The
> inspector is a WORKING surface — open a picker, change the policy, change the rank, then reach for
> a stepper — so it stays open until dismissed, and an anchored control that never goes away is the
> incoherent combination. Four properties, and each is load-bearing:
>
>   * **CENTRED IN THE ROOM THE DOCK LEAVES**, not centred on the panel and not anchored to the row.
>     On a bottom dock the panel is a strip with a screen of map above it; on a side dock a column with
>     the map beside it — so the dialog lands **over the MAP** on every edge and the board stays fully
>     visible. Centring on the panel would only move the occlusion onto the rows being compared.
>
>     ⛔ **IT SAID *VIEWPORT*-CENTRED AND THAT DID NOT SURVIVE THE SECTIONS.** Quoted, because the
>     reasoning is still right and only the frame of reference moved: *"VIEWPORT-CENTRED … so a
>     viewport-centred dialog lands over the MAP in both."* True while the card was 104–156px tall;
>     once POLICY / PRIORITY / KITS were all drawn it measured **340**, and on a 1920×1080 bottom dock
>     a viewport-centred card spanned y=370…710 through a panel starting at 624 — covering the board
>     this item exists to free. The dialog's own assertion caught it. Cutting the room back off the
>     panel's map-facing edge keeps **one centre and no dock-edge fork**, which is the property the
>     bullet was defending.
>   * ⛔ **NON-MODAL — NO INPUT-BLOCKING SCRIM.** It re-targets when another row is selected, which is
>     only possible if the board stays live underneath. This is the property most likely to be built
>     wrong from the word *dialog*, and a scrim would delete the workflow the persistence exists for.
>   * **PERSISTENT, with an explicit dismiss.** It does not close on an outside click; a stepper press
>     on a different row is ordinary use, not a dismissal gesture.
>   * **EVERY DOCK, not the horizontal one.** The vertical dock does not need it — its box is the full
>     window height less chrome (939 at 1080 against a strip reserving 134) — but a fork means two
>     layouts and two frame families, which is precisely the shape that hid both defects above. One
>     behaviour, one code path, one test matrix. The vertical dock gets the freed rows too.
>
> **AND ONE PLACEMENT RATHER THAN FOUR.** Floating it off the panel's map-facing edge aligned to the
> selected row reads better and travels the eye less, and it was rejected: four dock edges means four
> placements plus clamping, against the same one-behaviour argument that settled the fork above.
>
> **WHAT THIS SLICE DOES NOT DO:** it does not raise `PANEL_HEIGHT_WIDE` (ruled out — the horizontal
> dock may not get taller), and it retunes no config value. The two shipped overflows above are FIXED
> BY CONSTRUCTION once the strip stops competing for zone height, so neither needs its own patch.
>
> #### WHAT THE DIALOG BECAME ONCE IT WAS ONE — three things decided after the above was written
>
> **THE SECTIONS ARE ALWAYS DRAWN, and `_work_picker_open` is gone.** Ray, on seeing the dialog: now
> that it is its own surface it does not need the strip's layout. **POLICY, PRIORITY and KITS are
> sections with their content shown**; `Jump to source` and `Unassign` carry no content and are the
> only two buttons. That state — and `WORK_PICKER_NONE` / `_FLOOR` / `_PRIORITY` and
> `_toggle_work_picker` with it — existed for exactly one reason, that the strip paid for the tallest
> picker and could afford one at a time. It was rent control for rent the dialog no longer pays, and
> removing it also deletes a bug class: a picker left open when the row changes.
>
> ⛔ **SO THE RESERVATION IS A SUM, WHICH INVERTS ITEM 12c's OWN PROPERTY.** *"The strip pays for at
> most one picker"* was the whole argument that made the `Kits` picker free; one slice later it is the
> wrong shape, and the assertion that pinned it had to be inverted rather than deleted. Worth keeping
> in view: a claim can be exactly right and become exactly wrong without the code around it being
> wrong at any point.
>
> **THE UPKEEP ROW DRAWS ONLY WHERE THE SITE OWES UPKEEP.** Reported from play, on both webs: a WILD
> source was offered an Upkeep picker and silently defaulted it to a kit — a hurdling kit for a wild
> aurochs. Upkeep is a property of a standing rung and a wild source has no improvement at all, so the
> row offered a tool for a job that does not exist. **The gate is the site's PUBLISHED BILL**
> (`upkeepDemand` ∨ `upkeepMaterialDemand`), never a re-derived rung test: it is a disjunction, so a
> rung owing only work still draws the row; and it gets mid-climb right, which a rung word cannot,
> since a source raising its first rung stands on `wild` and is already billed. In its place the
> section states the bill itself — `Kept at 1 work · 0.05 hurdles a turn.` — **terms only, no rung
> word**, the head line already stating the rung through the same `rung_row_value`.
>
> **AND THE `none` HINT LINE DIED HERE.** It was added when the sections gained room, on the theory
> that a visible sentence beats two tooltips, and Ray's word for it was *nonsense*: it explained a
> control that, on the surface where it read worst, should not have been drawn at all. The `none`
> CHOICE is untouched and is still what item 12c describes — only the on-screen line went.
>
> **EVERY ONE OF THOSE CAME FROM PLAYING A TURN, not from the suite.** The harness was green through
> all of them, because its claims asserted the pair was PRESENT and never that it was RIGHT: no
> fixture opened the dialog on a wild source, none on a pending row, and none asserted a picker's face
> or its lit index. That is the shape to design tests against on the next slice — presence is not
> correctness, and a dead control passes every claim that only counts children.

13. **The route branch (#532 proper).** Routes as the ladder's third branch, `infrastructure_cost`
    wired for the first time, traversal-driven progress from supply links, shipments and movement.
    > **⛔ THREE CLAIMS IN THE SOURCE DOCUMENTS WERE FALSE WHEN THIS SLICE OPENED.** #532 and
    > `docs/plan_contact_and_logistics.md` §Q4 are corrected in place rather than left to be
    > re-derived — the lesson of #543, which was filed off a document describing deleted code and
    > cost a full investigation to unwind.
    >
    > **① *"Upkeep is a standing cost, and the ladder only has a build cost"* — CLOSED, by this arc.**
    > Both documents list it as one of *"two known gaps to be closed in the slice that builds this"*.
    > §2.4–§2.8 shipped `RungUpkeep` entire: `work_per_turn`, a `scaled_by` measure, a material rate,
    > `meter_decay`, `grace_turns`, the two band-level keeping pools, the fund-mode split and the
    > shortfall/shed paths — load-bearing on both webs. **Routes inherit a standing-cost model and
    > must not rebuild one.** (`retain_fraction` is not part of the inheritance either: it was a
    > fourth dial and §2.8 deleted it along with the cliff it patched.)
    >
    > **② *"#232 already derives an overlay from movement/logistics traversals — the two should read
    > one source rather than two"* — #232 IS OPEN AND NOTHING IS BUILT.** Verified: `RouteNetwork`
    > appears nowhere in the workspace, and the only `traversal` in `core_sim` is an unrelated
    > tree-walk comment in `telling/predicate.rs`. This slice therefore does not *find* a traversal
    > seam, it **builds** the traffic counting — and #232 becomes a **consumer** of it (phase 13c
    > below), not a supplier to it. Budget for that.
    >
    > **③ THE REMAINING GAP IS THE REAL WORK, AND IT IS UNTOUCHED** — *"a route is an edge, not a
    > source… the site and behavior primitives are source-shaped and need route-shaped siblings."*
    > That is what everything below answers.
    >
    > #### ⛔ THE PAYOFF IS ALREADY WRITTEN INTO SHIPPED CODE, AND IT IS A CAPABILITY, NOT A DISCOUNT
    >
    > The ladder's whole claim is **cheaper to travel and dearer to keep**, and this arc hands the
    > dearer half over for free. **The cheaper half had no consumer**: `infrastructure_cost` is
    > authored for all 37 terrains and verified to have *zero* readers outside `terrain.rs`'s own
    > declaration, and nothing anywhere reads a route rung to reduce a movement or a shipment cost.
    > **Shipping the branch without one builds a tax, not a ladder** — the trap §4.9 item 12 named
    > and dodged, *"a mechanism whose only consumer is the next slice is the failure mode."*
    >
    > **`SupplyNetworkConfig::reach_tiles` already says what a route is for**, in its own doc comment,
    > shipped: *"**The distance at which a logistics link holds itself for free**, in tiles… **beyond
    > it a link needs a route to hold it open**, and goods move by a shipment."* `supply.rs`'s module
    > doc says the same and adds *"that state belongs to the route ladder, not here."* So the consumer
    > was chosen before this slice opened; it had simply never been wired.
    >
    > **THE THREE PAYOFF TERMS**, all per-rung. Two are read by `balance_supply_networks`; the third is
    > a visibility source of its own.
    >
    > | Term | What it answers | Why it is needed |
    > |---|---|---|
    > | **`holds_link_to_tiles`** | *may these two bands pool at all?* A link forms at `distance <= reach_tiles` **OR** where a route of this rung spans it. | A **capability**. Without a road two bands 6 tiles apart cannot pool; with a dirt road they can. This is what makes the top rungs about **distance**, which is what roads are for. |
    > | **`friction_multiplier`** | *how much of what is sent arrives?* Multiplies `SupplyNetworkConfig::friction` on a routed link. | Reach alone pays **nothing** to a road between two neighbours already inside `reach_tiles` — the commonest road in the game. That road has to buy something. |
    > | **`Seen` along the road** | *what can I watch?* A route at a **built** rung **whose keeping is met** holds its own tiles `Seen`. **Not a per-rung number — a per-rung yes/no**, and `game trail` is the no. | The one payoff a player understands without a readout, and the one worth most in a fog-of-war game. **Ray's call**: a maintained road has traffic on it, and traffic is presence. See the callout below for why the keystone survives it. |
    >
    > **ALL THREE ARE PURELY ADDITIVE, so §Q4's "no early-game regression, by construction" guarantee
    > holds unchanged**: an unrouted pair inside `reach_tiles` pools exactly as it does today, at
    > exactly today's friction, and sees exactly what it sees today. A rung can only widen the set of
    > links, lower a loss, and light ground that was dark — never the reverse.
    >
    > **AND THIS IS WHAT DISSOLVES THE BOOTSTRAP.** A route is built by traffic; if reach needed a
    > route and traffic needed reach, no long road could ever form. It resolves because **each of
    > #532's three traffic sources has its own job**:
    > - **inside `reach_tiles`** the pooling link is already running, so *pooling traffic itself* wears
    >   the trail in, and what it buys is **friction**;
    > - **beyond `reach_tiles`** there is no pooling yet, so the wearing-in is done by **shipments and
    >   ordinary band movement** — you walk it by hand until the road makes itself, and then the road
    >   carries it for you.
    >
    > That is also #215's origin told twice over: the first roads are the ones the animals made, and
    > the second are the ones two neighbouring camps wore between them. **Neither is a decision anyone
    > typed.**
    >
    > #### ⛔ §4.13a — THE FREE FLOOR IS **TWO** RUNGS, AND TRAFFIC STOPS AT THE TOP OF IT
    >
    > **13a shipped the free/paid boundary one rung too low, and the symptom was a bill nobody asked
    > for.** Traffic climbs a road automatically, and `route:trail` carried an `upkeep` — so two camps
    > sharing a larder wore a trail into existence by themselves and the band acquired a **standing
    > labour bill it never opted into**. Reported from play: a 7-worker nomadic band with no roads it
    > had chosen, showing `Roadwork ⚠` on the Work board.
    >
    > **Nothing else in the game does that.** A wild patch and a wild herd sit at the free floor for
    > ever until the player issues `cultivate` / `tame` and staffs the builders. Ray: *"similar to how
    > the initial forage and hunt don't require worker pools, I don't think the initial trails should
    > either. Those are formed only from usage and are **not upgraded**."*
    >
    > **⛔ AND MOVING THE BOUNDARY ALONE ONLY RELOCATES THE FAULT.** Make the trail free and traffic
    > keeps climbing — into the dirt road, which is also billed, and the same ⚠ arrives one rung later
    > and dearer. **The auto-climb is the fault, not where the line sits.** So the branch becomes
    > exactly the other two webs' shape:
    >
    > | | free floor, formed by use | built, and paid for |
    > |---|---|---|
    > | `plant` | wild | tended · field |
    > | `animal` | wild | pastoral · pen |
    > | `route` | **game trail · trail** | **dirt road · paved road** |
    >
    > **THE FOUR RULES THAT FOLLOW:**
    >
    > **① Traffic banks work up to the top of `route:trail` AND NO FURTHER.** Uncapped it would wear a
    > dirt road in for free and hand the player its bill anyway.
    >
    > **② `grade` and `pave` are real verbs on the builders' pool**, exactly like the four that ship.
    > `grade` takes a road to `dirt_road` (you grade a roadbed), `pave` to `paved_road`. This retires
    > `RungBranch::is_crew_built`: *"does this rung declare a `verb`"* already answers it per rung,
    > which is the finer and correct grain — the branch is no longer uniformly crew-free.
    >
    > **③ TWO DECAY TRIGGERS, because the free rungs cannot be short.** A built road reverts on
    > **unpaid keeping**; a trail costs nothing, so it reverts on **disuse**. That is not an invention —
    > it is §Q4's own *"an unused road reverts"*, which 13a collapsed into the shortfall path.
    >
    > **④ THE QUEUE ENTRY IS A BAND'S; THE ROAD STILL BELONGS TO NOBODY.** One band's builders raise it,
    > and once it is up anyone standing on it uses it and anyone standing on it may pay its keeping.
    > **Building a road no more owns it than paying for it does** — which is what preserves the
    > shared-public-good property the ground-shaped model bought.
    >
    > #### ⛔ §4.13b — A ROAD IS A **TILE IMPROVEMENT**, AND THE PATH OBJECT WAS A CONFLATION
    >
    > **13a modelled a road as a stored path with its own id** — one object holding a `Vec<UVec2>`, a
    > ladder position, and a keeping bill for the whole run. Ray broke it with one sentence: *"a road
    > is a single tile improvement, not the entire path, so one band could maintain 1/2 the tile roads
    > for the distance of a connection between two bands and another the other 1/2."*
    >
    > **A path object cannot be half-maintained.** There is no way to express *these people look after
    > this end and those people look after that end*, and that is the ordinary case the moment two
    > camps sit at either end of a long road.
    >
    > **So a road is a per-TILE improvement, structurally identical to a forage patch**: each tile
    > carries its own rung, its own meter, its own keeping and its own decay, worked by whoever takes
    > that tile on.
    >
    > ##### ⛔ THREE OBJECTS WERE RUNNING TOGETHER, AND ONLY THE THIRD IS BEING REMOVED
    >
    > This is the part a future reader must not re-merge. Ray, on the risk: *"Connections between bands
    > (i.e. supply depots) will probably be important later in the game for logistics… just want to
    > make sure you won't remove something that we need."*
    >
    > | | what it is | status |
    > |---|---|---|
    > | **connection** | two bands know each other, this much (`connections.rs`) | **shipped, untouched** — #517 / #547 |
    > | **logistics link** | goods move between them; what a shipment walks and what pooling rides | **shipped, untouched** |
    > | **road** | a built, maintained improvement **on one tile** | the new shape |
    > | **link quality** | how good the going is between two camps | **DERIVED each turn** by reading the road tiles along the way — not stored |
    >
    > **What is removed is only the stored path inside `Route`**, which was my invention and which
    > nothing needs: a link already knows its two endpoints, so the tiles between them are computable.
    > `trace_path` survives as a **function**; it stops being **state**. Ray's own phrasing for this is
    > the right one — *"a route projected onto the tiles"*: the **link** is the object, the **roads**
    > are the ground it runs over.
    >
    > ##### WHAT THE PER-TILE MODEL DISSOLVES
    >
    > - **"Can two bands maintain one road?"** — the question this arc spent three rounds on. It has no
    >   content now: they maintain **different tiles**. No exclusivity rule, no split, nothing to
    >   arbitrate. §4.13a's *"several bands each pay a share and the contributions add together"* is
    >   **retired** — Ray: *"Don't agree with at all"*.
    > - **`Roadwork` as a band-wide POOL is the wrong shape and is retired.** A pool is what a role
    >   takes when it covers *everything the band holds* on a web — true of agriculture (every patch)
    >   and husbandry (every flock), and false of roads, which a band does not hold. Roads are worked as
    >   **per-tile rows**, like `Forage { tile }`. That is what makes the per-road choice exist at all:
    >   Ray, on automatic payment for every road under a camp, *"Don't agree with this one at all,
    >   per-road choice must exist"*.
    > - **`grade` and `pave` become TILE commands** — `grade <faction> <band> <x> <y>`, the same grammar
    >   as `cultivate <x> <y>` and `sow <x> <y>`, rather than a route-id form nothing else in the game
    >   uses.
    > - **`UpkeepScale::RouteSpan` probably retires with it.** It exists to express `length × terrain`;
    >   per tile, the length term is gone and what remains is the tile's own `infrastructure_cost` — the
    >   same *shape* as the plant web reading its tile's `K`. If it collapses into `SourceLoad` with a
    >   per-branch reading, that is §4.11's stated preference (*"one primitive with a per-branch reading
    >   beat a second variant"*) and this slice should take it.
    > - **The tile card becomes the structurally correct home for the readout**, rather than the
    >   judgement call it was.
    >
    > ##### ⛔ WHO KEEPS A ROAD: THE BAND THAT BUILT IT, AND NOBODY ELSE
    >
    > Confirmed with Ray, in three parts:
    >
    > 1. **A trail has no keeper, and that is fine because there is nothing to keep.** The free floor
    >    costs nothing; it is formed by use and lost by disuse, and no band is associated with it.
    > 2. **Improving a trail makes it that band's job.** `grade` is the declaration — the same act
    >    `cultivate` performs on a patch. **One builder, one keeper, no shares**, which is what finally
    >    disposes of the *"several bands each pay a part"* model Ray rejected: it is unrepresentable
    >    rather than merely discouraged.
    > 3. **Distance raises the cost; it never forbids the road.**
    >
    > **THERE IS NO WORK RANGE RULE, AND THE REASON IS NOT COMPLEXITY.** Ray: *"already forage and
    > hunting have different work ranges, expeditions are even farther. I don't think it makes sense to
    > restrict it."* A fourth arbitrary radius would say nothing. What bounds a distant road is that it
    > is **dearer to hold and slower to build** — which is the argument `TradeExpeditionConfig` already
    > makes for refusing a friction lever: *"what a long haul costs is already paid, and paid in the
    > right currency."*
    >
    > **A THRESHOLD, NOT A CURVE.** Within a base range a road costs what the rung says; beyond it the
    > build and the upkeep both rise. Simpler to tune than a sliding function and it is what Ray asked
    > for.
    >
    > > **⛔ THE THRESHOLD IS READ THROUGH A FUNCTION, NEVER AS A BARE CONFIG CONSTANT.** Ray: *"Be
    > > flexible on the threshold… make it a function that can expand over time, don't just create a
    > > hardcoded constant. You can have a configuration item for the 'base' range, but still make a
    > > function accessor for it so we can calculate it later."*
    > >
    > > So the config holds a **base**, and every caller asks a **seam** for the effective range. The
    > > day it grows with knowledge, faction size or a central authority, that is one function body
    > > changing and no call site moving. A `cfg.range` read scattered across the build cost, the
    > > upkeep, the refusal and the wire is four places to find and three to miss — the same reason
    > > `herd_ecology` and `patch_land_capacity` are seams rather than field reads.
    >
    > **WHEN THE KEEPING BAND IS GONE**, the road has no keeper and decays like any unkept improvement.
    > Re-issuing `grade` / `pave` on it is how another band picks it up — no new verb, and adoption is
    > the same act as building.
    >
    > ##### DEFERRED, AND IT IS ITS OWN ISSUE: INFRASTRUCTURE ABOVE THE BAND
    >
    > **Issue #598**, filed by Ray. A faction-level authority maintaining roads beyond any single
    > band's reach is more realistic than a per-band keeper and is *"a whole entire mechanism in the
    > game we don't have"* — a central government. **Explicitly out of this arc.** Recorded here only
    > so the per-band keeper above is understood as this arc's answer rather than the final one.
    >
    > ##### ⛔ THE WORD **OWNERSHIP** IS RETIRED FROM THIS ARC
    >
    > Ray: *"A road could have no owner, but that is an abstract term in this game so it really has no
    > meaning."* There is no owning. There is a **job** — *these people look after this tile's road* —
    > which is a work row like any other, and the question *"who owns a road"* should not be asked
    > again. What survives of §4.13a's ownership prose is only the concrete half: a band whose camp is
    > on a road tile is served by it, and a road nobody maintains decays and goes.
    >
    > ##### WHAT DOES NOT EXIST YET, NAMED SO IT IS NOT ASSUMED INTO "ROUTE"
    >
    > **A depot.** Neither a connection nor a road is *a place goods sit*. It is a third object and
    > nothing in this model provides one; Ray raised it as the thing later logistics will want. Do not
    > let it be quietly folded into either.
    >
    > #### THE KNOWLEDGE CHAIN LOSES A LESSON, AND THE ONE IT LOSES TAUGHT NOTHING
    >
    > 13a shipped **four** lessons: `game trail` earned `trailcraft`, which gated `trail`. **A lesson
    > for something you cannot fail to do.** You wear a path by walking it; there is no knowing-how
    > involved and no way to be refused. Ray: *"Creating the trail leads to 'road building' knowledge,
    > with dirt being the first build, then paved."*
    >
    > **`trailcraft` is DELETED** (its discovery id is retired, not reused — a gap is safer than a
    > renumber). The surviving chain has every lesson gating something a player actually decides:
    >
    > | rung | gate | teaches |
    > |---|---|---|
    > | `game_trail` | — | — |
    > | `trail` | **none — it forms from use** | `roadbuilding` |
    > | `dirt_road` | `roadbuilding` | `paving` |
    > | `paved_road` | `paving` | — |
    >
    > **`roadbuilding` is earned by a TRAIL CARRYING TRAFFIC**, which is the route branch's reading of
    > *"a rung teaches while it is practised"* — the practice is the walking. The faction that learns
    > is the faction of a band standing on the road, the same catchment rule 2 uses everywhere else.
    >
    > #### THE FOUR RUNGS
    >
    > `route`: **game trail → trail → dirt road → paved road**. Rung 1 is the **floor** — `build: null`,
    > `upkeep: null`, costs nothing to reach and buys nothing, exactly as `plant:wild` and
    > `animal:wild` are floors. A position of `RUNG_UNSTARTED` **holds** the game trail. **Rung 2 is
    > the floor's second storey** — worn in by traffic, still `upkeep: null`, still no verb.
    >
    > **`partial_credit: continuous` on all three.** A half-worn trail is genuinely half a trail —
    > unlike `animal:pen`, where half a fence is not a fence. Nothing here earns `on_completion`.
    >
    > **`site_requirement` is `null` on every route rung**, and that is the honest answer rather than a
    > gap: `site_requirement` asks *what must the LAND under this tile be*, and a route does not stand
    > on a tile. What the route asks of the land is **priced, not gated** — it is the `scaled_by` term
    > below. **Do not invent a `route_requirement` sibling to fill the hole**; a rung asking nothing of
    > its site is what every animal rung already ships.
    >
    > #### THE SCALE TERM — the one place this slice extends the ladder's vocabulary
    >
    > §2.6 and §6 both reserve the `scaled_by` key for the route's **`length × terrain`**, *"genuinely
    > a different shape: it reads the improvement's own geometry rather than the source it sits on."*
    > It lands as a second variant beside `source_load`:
    >
    > ```
    > UpkeepScale::RouteSpan  ⇒  Σ over the tiles the route crosses of infrastructure_cost(terrain)
    > ```
    >
    > **ONE SUM, NOT A PRODUCT.** Length falls out of the *number* of terms and terrain out of each
    > term's *value*, so *"length × terrain"* is a single measure rather than two multiplied — which is
    > what lets it be one `f32` through the same `RungUpkeep::scaled_by` seam both currencies read.
    >
    > - **SUMMED PER TILE CROSSED, never averaged.** Three tiles of marsh cost three tiles of marsh.
    >   Averaging would make a long road and a short one through the same country cost the same, which
    >   deletes the length half of the term.
    > - **§2.7's line holds: the land is a SCALE term, not an offset.** `infrastructure_cost`
    >   *multiplies* the demand; it never subtracts from it.
    > - **The rung-monotonicity check needs no change, and that is the point.** It *"compares only
    >   adjacent rungs sharing a `scaled_by`"* — a rule §4.11 deliberately kept stated rather than
    >   simplifying away *"because it is what makes adding the route branch's scale safe."* Route rungs
    >   share `RouteSpan` and are compared against each other; no pair is compared across the unit
    >   boundary. **`Flat` stays deleted** — this is the variant the key was held open for.
    >
    > #### ⛔ TRAFFIC PAYS FOR THE FLOOR; A CREW PAYS FOR THE ROADS — corrected by §4.13a above
    >
    > **This section read *"THE BUILD IS PAID BY TRAFFIC, SO A ROUTE TAKES NO BUILDER AND NO QUEUE
    > ENTRY"*, of the whole branch. That is true of the free floor only.** Traffic wears in the game
    > trail and the trail and stops there; `grade` and `pave` are ordinary verbs on the builders' pool.
    > See §4.13a for why the uniform reading produced a bill the player never opted into.
    >
    > *"`build.progress_per_turn` | a crew clears ground | **traffic wears the route in**."* On the two
    > **free** rungs, traffic is the crew: they take no `BuildJob`, no queue entry and no draw on the
    > builders' pool. On `dirt_road` and `paved_road` a band's builders raise the road exactly as they
    > raise a Field or a pen.
    >
    > **⛔ AND A QUEUED ROAD IS STILL OWNED BY NOBODY.** The queue is the most band-shaped thing in the
    > engine — §2.5's entry names a destination **on a source that a band holds**, and the head takes
    > every builder that band has — so it *looks* like queueing a road makes it that band's. It does
    > not, and the reason is the one the keeping already runs on: **several bands may pay one road's
    > bill, and the road belongs to none of them.** Building it is the same kind of act as paying for
    > it. What a band's queue holds is *this band's builders are working on that road*, never *that
    > road is ours* — and once it is up, anyone standing on it uses it.
    >
    > **Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in, so
    > *"what does it cost to raise this"* keeps one answer in one unit whichever branch is asked.
    > `progress_per_turn` was retired arc-wide and is not coming back for routes.
    >
    > | Traffic | Measured as | Converted by |
    > |---|---|---|
    > | a **live pooling link** (`balance_supply_networks`) | **link-tiles** — one turn of the link, × the road's length | `route_traffic.work_per_link_tile_per_turn` |
    > | a **shipment** walking a connection | *13b's* | *13b's* |
    > | ordinary **band / expedition movement** | *13b's* | *13b's* |
    >
    > > **⛔ THIS TABLE READ `mass-tiles` AND `worker-tiles`, TWO LEVERS, AND THE FIRST ONE WAS WRONG.**
    > > `balance_supply_networks` drops sub-`min_transfer` moves, so a **balanced** network ships
    > > nothing — a mass-driven rate would leave two camps who have shared a larder for thirty turns
    > > with **no path at all**, exactly the case #532 forbids. It is the *link*, not the tonnage: a
    > > trail forms because they are neighbours who walk to each other. One lever, and it shipped in
    > > 13a; the other two rows are 13b's to fill in.
    >
    > **Two levers, not three**: goods and people are the only two things that move, and a shipment is
    > people — its cargo rides in their packs (`TradeExpeditionConfig::per_worker_carry`, *"the party
    > **is** the route"*). Counting a shipment's mass *and* its workers would price one trip twice,
    > which is the error `TradeExpeditionConfig` already refuses to make about friction.
    >
    > **No accumulator is needed and none may be added.** The ladder position *is* the accumulator —
    > cumulative work in a single `f32`, with `accrue_to` returning every rung a call crossed. A
    > fractional turn's traffic banks as a fraction of a work unit and crosses a rung boundary when the
    > sum crosses it, which is the whole-unit discipline already in force.
    >
    > #### THE UPKEEP IS PAID BY HANDS, AND WANTS THE THIRD KEEPING POOL
    >
    > Traffic raises a road; it does not hold one. *"A paved road costing more to hold than a trail is
    > a crew that **stays**."* So the upkeep half draws on a band-level standing pool exactly as
    > agriculture and husbandry do — **`LaborTarget::Roadwork`, a third role.**
    >
    > **That is the existing rule, not a new axis.** §2.5: *"One role per **WEB** because the two webs
    > are already separate ladders — this is their existing split."* A third branch is a third ladder,
    > so it takes a third role by the same argument that gave the first two theirs. It needs the
    > route-shaped siblings of the two per-web seams — `route_claims_keeping` and
    > `route_keeping_basis` — and **the basis must be the STAMPED bill, never the live demand**, for
    > the reason §2.5 states in a ⛔: the wire asserts `demand − supplied == shortfall` verbatim, and a
    > demand that moves mid-turn is only safe to read once. **A route's demand moves more than a
    > source's**, because its geometry moves (below), so this is the branch most exposed to that
    > defect.
    >
    > **`meter_decay` is declared** on all three built rungs — an unkept road washes out, which is a
    > meter bleed and not a shed. (`null` is the animal branch's answer because a flock sheds *animals*
    > at its own escape fractions; a road has nothing to shed.) *"A trail reverts in a season, a paved
    > road stands for years"* is `grace_turns`, per rung, and the direction is the animal branch's:
    > **the highest rung is the most forgiving**, because the paving does the holding.
    >
    > **Materials are config, not mechanism** — §4.9 item 12 shipped both terms. A dirt road declares
    > none; a paved road declares stone on the pile *and* on the rate, and §2.7's *"decay refunds
    > nothing / neglect is self-limiting"* applies unchanged. **The quarrying and hauling ARE the
    > upkeep, not a second line beside it** — §2.7 disposes of that explicitly.
    >
    > #### ⛔ A ROAD IS IN THE GROUND. IT DOES NOT FOLLOW THE CAMP — and that is a DESIGN pillar, not a fidelity nicety
    >
    > **DECIDED. Ray: *"How would a road follow a camp? That makes no sense and is in fact a factor in
    > moving vs staying. Roads can't follow camps."*** The rejected alternative — a route defined as
    > *"the road between band A and band B"*, re-derived each turn from wherever those two bands
    > currently stand — was rejected for the reason that matters, and it is not the unphysicality.
    > **A road that follows its camp DELETES A DECISION.** A road you have paid to build and pay to
    > hold is one of the strongest reasons the game can give you to *stay* — it is the move/stay/fork
    > choice this project is built around (`docs/…concept_technical_plan_v_0.md`, and the same pillar
    > the pen argues from). A road that packs up and comes with you costs nothing to leave, so it can
    > never weigh on that choice. **The simpler implementation was buying its simplicity with the
    > decision the feature exists to create.**
    >
    > **⛔ SO *"A ROUTE IS AN EDGE"* IS ITSELF THE WRONG SHAPE, and #532 / §Q4's first gap is sharpened
    > rather than answered as written.** Both documents say *"a route… belongs to a connection"*. It
    > does not. **A route is a thing in the world with a fixed tile path and its own identity** — the
    > band pair is who *uses* it, never what it *is*. There is no `RouteKey { low, high }` and no
    > band-pair keying: a `RouteLedger` holds routes by `RouteId`, each with a stamped `Vec<UVec2>`
    > path, plus a tile→routes index (`BTreeMap`, for the iteration-order reason `ConnectionLedger`
    > gives) so *"which roads is this band standing on"* is a lookup and not a scan.
    >
    > **THE FOUR RULES THAT FALL OUT, and every one of them replaces a rule the rejected shape needed:**
    >
    > | | Rule | What it dissolves |
    > |---|---|---|
    > | **1** | **A road's tiles are stamped once, from the path the first traffic actually walked, and never re-derived.** | The re-stamp rule. |
    > | **2** | **A band is served by a road when it is STANDING ON ONE OF ITS TILES.** The road's own path is the catchment — there is no radius and no "close enough" constant to choose. Two bands on the same road may pool along it, at the along-road distance. | The tolerance constant. |
    > | **3** | **A road nobody stands on earns no traffic, is claimed by no band, and reverts** under the meter decay and grace that every rung already declares. | The orphan case, and the *"who pays for a road to nowhere"* question — nobody does, and nobody needs to. |
    > | **4** | **New traffic PREFERS AN EXISTING ROAD** whose path already joins where it is going. | The near-duplicate-road swarm, and it is why real networks consolidate: roads attract the traffic that widens them. |
    >
    > **AND IT MAKES A ROAD A SHARED PUBLIC GOOD, which the band-pair shape could not express.** Camp A
    > leaves and camp C moves onto the same ground: **C inherits the road**, because the road never
    > belonged to A. Every band standing on it claims its keeping from its own `Roadwork` pool, and the
    > existing *"several bands can pay one source in one turn"* accumulation (§2.5 — `upkeep_supplied`
    > accumulates, its clear lives in Logistics) is already the mechanism for that. **No new funding
    > rule.**
    >
    > **The known cost, named: a band that steps one tile off its own road loses it.** That is accepted
    > and is not softened with a radius — it is the legible half of the same pillar (*stay on your
    > road*), and a radius would be the "close enough" constant rule 2 exists to avoid.
    >
    > **THE TILE-OWNED SUCCESSOR IS NO LONGER A SUCCESSOR — this IS the ground-shaped model.** What
    > phase 13c adds is the *drawing* of it (#232), not a different ownership.
    >
    > #### ⛔ A MAINTAINED ROAD IS TRAFFIC, SO ITS TILES ARE `Seen` — and the keystone is UNTOUCHED
    >
    > **DECIDED. Ray: *"If a road exists and is maintained, the assumption is that there is traffic on
    > it and it is seen."*** `plan_contact_and_logistics.md` §Q5's rider row was right and this
    > design's first answer was wrong.
    >
    > **The objection that was raised and is now answered:** *"the commonest routed link is a pooling
    > link where nobody physically walks, so `held` would buy sight with an automatic transfer."* That
    > mistook the direction of the inference. **Maintenance is not free** — a road at a built rung is
    > costing a band `work_per_turn × RouteSpan` every turn out of the `Roadwork` pool, and what those
    > hands are *doing* is being on the road. **Paying the upkeep IS the presence.** A road nobody
    > walks is a road nobody pays for, and rule 3 above has it reverting.
    >
    > **⛔ AND THAT IS WHY THE KEYSTONE DOES NOT BEND.** §Q3 and `connections.rs` state it as
    > inviolable — *"**Only presence makes a tile `Seen`. A connection can only ever grant
    > `Discovered`**"* — and name **logistics** as the first rider that will be tempted. **This is not
    > that temptation.** The sight is granted by the *road*, which is maintained presence on specific
    > ground, and **not by the connection**, which still grants `Discovered` and nothing else. A band
    > with a live tie to a people it has never travelled to sees exactly what it sees today.
    > `core_sim/tests/connections.rs` keeps passing unchanged, and **the route's grant must be written
    > as its own visibility source beside a band's own presence — never routed through the connection
    > grant**, or the test is being satisfied by an accident of plumbing rather than by the rule.
    >
    > **THE CONDITION IS THE PAID BILL, NOT THE HELD RUNG.** A route lights its tiles while it stands
    > at a **built** rung (rung 2 and up) **and its keeping is met** — the `game trail` floor lights
    > nothing, because nobody maintains a game trail; that is what makes it free. A road in shortfall
    > goes dark before it decays, which is the honest early warning that the road is being lost.
    >
    > **THIS IS THE THIRD PAYOFF, AND IT IS THE MOST LEGIBLE OF THE THREE.** Sight along your own roads
    > in a fog-of-war game is worth more to a player than a friction percentage, and it needs no
    > readout to be understood. It strengthens the *"cheaper to travel"* half considerably — see the
    > payoff table above, which carries all three.
    >
    > #### THE ONE-TURN LAG IS THE LEDGER'S, AND IT IS ACCEPTED FOR THE LEDGER'S REASON
    >
    > `balance_supply_networks` runs in `TurnStage::Logistics`; the route accrual has to run **after**
    > it to count this turn's transfers. So the payoff is read at the standing as of the **previous**
    > turn — precisely the lag `balance_supply_networks` already accepts against `ConnectionLedger`
    > (*"`advance_connections` runs later the same turn… on the world's very first turn the ledger is
    > empty"*). **Do not reorder a stage for it**, and do not let supply seed the route standing
    > itself: that would make a second producer of a rung's position, which is the failure this arc has
    > already had three of (§"NO CALL SITE MAY RE-DERIVE A STANDING FROM A METER").
    >
    > #### THE CLIENT SURFACE IS DESIGNED FROM SCRATCH, AND IT LANDS BEFORE THE TUNING
    >
    > Verified: **there is none today.** Every `route` hit in the client is the English word, except
    > `map_preview.gd`'s `"routes"` annotation state, which draws **order paths** — the player's own
    > movement waypoints — and is unrelated. **That name is therefore already taken in the client's
    > vocabulary and this branch must not reuse it**; the ladder's object is a *route network*, which
    > is #232's phrase.
    >
    > The minimum readout, landing **with** the branch and not after it: a per-route row wherever a
    > band's connections are shown — the rung held, its meter, the upkeep demand and shortfall on the
    > `Roadwork` pool, and **what the rung is buying** (the span it holds open, the friction it is
    > saving). That last one is the whole point of the two-sided economy, and it is the one number that
    > tells a player whether to pave or to stop.
    >
    > #### ⛔ THE THREE STEPS — **AGREED WITH RAY, ONE PR EACH.** Each is playable alone
    >
    > **13a — ROADS EXIST, AND THE FOOD-SHARING BETWEEN NEIGHBOURING CAMPS WEARS THEM IN.**
    >
    > > **⛔ 13a IS LANDED IN FULL, AND THREE THINGS BELOW ARE CORRECTIONS TO THIS SECTION'S OWN
    > > SPEC.** The branch, `RouteLedger`, `UpkeepScale::RouteSpan`, the traffic accrual off pooling
    > > links, the **friction** payoff, the checkpoint, the `LaborTarget::Roadwork` keeping pool, the
    > > decay it funds, the `Seen`-along-a-kept-road grant, the whole `RouteState` wire and the client
    > > readout are all in. Rationale as built: `.claude/rules/core_sim/routes.md` and
    > > `.claude/rules/client/roads.md`.
    > >
    > > **⓪ ONE THING 13a WAS SPECIFIED TO SHIP AND DOES NOT: `holds_link_to_tiles` HAS NO SIM
    > > READER.** It is published, and the client renders it in the **future** tense (*"will hold a
    > > link 10 tiles out"*) precisely because nothing in `balance_supply_networks` consumes it yet.
    > > That is 13b's first job, and the tense is what keeps the readout honest until then — rendering
    > > it as an active effect would be a lie the player cannot check.
    > >
    > > **① THE TRAFFIC RATE IS THE LINK, NOT THE TONNAGE.** This section specified **mass-tiles**
    > > — quantity moved × distance. **That is wrong for the case the branch exists to serve.**
    > > `balance_supply_networks` drops sub-`min_transfer` moves, so a *balanced* network ships
    > > nothing: a mass-driven rate leaves two camps who have shared a larder for thirty turns with
    > > **no path at all**, which is exactly the case #532 forbids. A trail forms because they are
    > > neighbours who walk to each other, not because of what they were carrying. One lever —
    > > `route_traffic.work_per_link_tile_per_turn`, banked per tile of road per live turn — replaces
    > > the two this section named.
    > >
    > > **② NO ROUTE RUNG DECLARES A MATERIAL, because `stone` does not exist.** The paved road was to
    > > swallow stone on the pile and the rate. `materials.json` carries bone / fibre / grape / hide /
    > > hurdles / tea / tobacco / wood, and the ladder's load-time check rightly rejects a rung naming
    > > a material the table does not carry. **Adding a stone with no way to obtain one would ship a
    > > rung that can never be HELD** — a harder failure than one that is cheap to hold — so quarrying
    > > stays the crafting arc's. The engine half already ships; the rung takes its two lines when a
    > > stone material lands.
    > >
    > > **③ THE COMPONENT'S FRICTION IS THE BEST ROAD BINDING IT, and that is forced rather than
    > > generous.** `balance_commodity` pools a whole component against one friction scalar and has no
    > > path model, so the reading is an approximation either way — but under a **worst**-road reading,
    > > wearing a new poor trail into an existing network would **raise** its friction. A road would
    > > make things worse, which breaks *"a rung can only widen the set of links and lower a loss"*
    > > outright. A road counts only where **two** of a component's bands stand on it (rule 2).
    > - **Sim:** `RungBranch::Route` (and the `BOTH_BRANCHES` sweep it forces open — the const is
    >   deliberately exhaustive so no sweep can miss a third web), the four rungs in
    >   `intensification_ladder.json`, `UpkeepScale::RouteSpan`, the `RouteLedger` + `RouteId` + stamped
    >   tile path + tile→routes index, the `LaborTarget::Roadwork` pool with `route_claims_keeping` /
    >   `route_keeping_basis`, and traffic accrual from `balance_supply_networks` transfers only.
    > - **Payoffs shipped:** `friction_multiplier`, **and `Seen` along a kept road** — the visibility
    >   grant belongs here rather than in 13b because it is the payoff a player reads without being
    >   told, and 13a is the phase that has to prove the branch is worth having.
    > - **Client:** the readout — rung, meter, `Roadwork` demand/shortfall, and **what the rung is
    >   buying**. Lands with the branch, per this arc's readout-before-tuning rule.
    > - **Playable alone because:** two camps that already pool wear a road between them with no order
    >   typed, and the food they lose in transit drops. Needs no shipment and no movement traffic.
    >
    > **13b — TRADE PARTIES AND MARCHING BANDS WEAR ROADS IN TOO, AND A ROAD REACHES FURTHER THAN FREE
    > RANGE DOES.**
    > - **Sim:** traffic accrual from shipments (#517's trade expedition) and from ordinary band /
    >   expedition movement, both in worker-tiles; the `holds_link_to_tiles` payoff in
    >   `balance_supply_networks`; and **rule 4** — new traffic prefers an existing road.
    > - **Playable alone because:** it creates pooling links **that could not exist before**. This is
    >   the phase where the bootstrap pays off — you walk a long haul by hand, the road forms under
    >   your feet, and then the road carries the goods for you.
    >
    > > **⛔ IT IS THE DISTANCE OF THE JOURNEY THAT TEACHES, NOT THE KIND OF TRAFFIC.** Ray:
    > > *"Walking 100 yards to the next band won't make people think of dirt roads. Traveling a trail
    > > for 30 miles (trade route) would."*
    > >
    > > **⛔ AND TWO THINGS THAT LOOK LIKE ONE RULE ARE NOT.** This was written up first as *"weighted by
    > > the kind of traffic"*, which was wrong, and it also reached for the build meter, which was
    > > wronger:
    > >
    > > | | governed by | weighted? |
    > > |---|---|---|
    > > | **how fast a trail forms** (the build meter) | **plain usage** | **NO — unweighted, and 13a's behaviour is correct as it stands.** Ray: *"that forms from just using it."* Walking to the next camp wears a path perfectly well |
    > > | **how fast you learn `roadbuilding` / `paving`** | **how far the journey went** | **YES** |
    > >
    > > A caravan is not a better teacher because it is a caravan. It is a better teacher because it
    > > went **thirty miles**, and thirty miles of trail is what makes a people want a road. A band
    > > marching a long way learns the same lesson without carrying a single trade good — so this is
    > > **not** a rule about trade, and a per-traffic-kind lever table would encode the wrong thing.
    > >
    > > **THE PER-TILE MODEL GIVES MOST OF THIS FOR FREE, which is a point in its favour** (§4.13b). A
    > > road tile is a source, the ladder credits a lesson **once per source per turn**, and a thirty-tile
    > > journey crosses thirty of them against a neighbour link's two. Distance-scaling falls out of the
    > > model rather than being bolted on.
    > >
    > > **⛔ BUT FREQUENCY IS THE OTHER HALF, AND IT IS THE OPEN QUESTION 13b MUST ANSWER.** Neighbours
    > > pool **every turn**; a caravan goes **occasionally**. Under a flat per-tile-per-turn credit a
    > > 2-tile link running for thirty turns banks *sixty* against one thirty-tile haul's *thirty* — and
    > > the short hop out-teaches the long journey, which is the exact inversion this rule exists to
    > > prevent. **How far** and **how often** both have to be in the expression; only the first is
    > > settled. Do not ship a per-tile credit without deciding the second.
    >
    > **13c — DRAW THE ROAD NETWORK ON THE MAP (#232).**
    > - The overlay, fed by the roads 13a/13b now hold. #232 is a **consumer** of this work, not a
    >   supplier to it — see correction ② above. `map_preview.gd`'s existing `"routes"` annotation
    >   state is **order paths** and is a different thing; do not reuse that name.
    > - This is also where `map_preview.gd` earns back a frame it lost with the trade-link substrate.
    >
    > > **⛔ 13a ABSORBED THE BASIC DRAWING, so this step is smaller and sharper than it was written.**
    > > Under the path model a road was an object that needed its own network view; per-tile, a road is
    > > just a tile, so the per-tile stamp landed with 13a's client half. **What is left here is the
    > > OVERLAY and the presentation** — not "make roads appear", which they already do.
    >
    > > **⛔ A ROAD FADES IN AS IT IS WORN, RATHER THAN SNAPPING AT THE RUNG.** From play: seven
    > > game-trail tiles existed at 24–52% and every one of them drew identically, so a road half worn
    > > in looked exactly like one just begun. Ray: *"a partial trail… should still draw but at 52%
    > > opacity, so as it becomes more complete"* it reads more strongly — *"fade in the
    > > trails/dirt/paved."*
    > >
    > > **IT IS THE LADDER'S OWN INTERPOLATION, NOT A SECOND MECHANISM.** Opacity is a per-rung value
    > > like any other, so it goes through `intensification::interpolate` on the road's standing —
    > > exactly as the upkeep demand, the payout and the material rate already do. A road 52% of the way
    > > from game trail to trail draws 52% of the way from the game trail's opacity to the trail's.
    > >
    > > **The ramp is BETWEEN RUNGS and never from zero**, and that falls out of the same reading rather
    > > than needing a rule: a trail at 0% *is* a completed game trail, and it should look like one. A
    > > road never fades to nothing while it still holds a rung.
    > >
    > > **The tile card already states the number** (`HudRouteVocab.wearing_in_value`); this is the map
    > > saying the same thing without being asked. Two readings of one position, so they cannot
    > > disagree — which is the whole reason to interpolate rather than invent a second progress curve.
    >
    > > **RELATED, AND NOT THIS STEP'S: #215** — *"herd/game trails follow hex centers and become the
    > > basis of roads."* The game-trail rung's own origin, and its own issue.
    >
    > **13d — WHAT A STRANGER MAY DO WITH A ROAD.**
    > - Unreachable in play today: worldgen creates `FactionId(0)` and one start profile, so a normal
    >   game is single-faction. **Decide it before a second people lands, not after.**
    >
    > > **⛔ THE PER-TILE KEEPER MODEL CLOSED MOST OF THIS BY CONSTRUCTION, so this step is far smaller
    > > than when it was agreed.** It was scoped when a road was a path object nobody kept, and the open
    > > questions were use, sight, keeping and upgrading — four holes. Three shut on their own:
    > >
    > > | | scoped by | |
    > > |---|---|---|
    > > | the cheaper food-sharing | same people only, by the pooling policy | already closed |
    > > | seeing along a road | the **keeper's** faction | closed by §4.13b |
    > > | paying its upkeep | its keeper, and a road has exactly one | closed by §4.13b |
    > > | **upgrading it** | **nothing** | **the open question** |
    > >
    > > So what is actually left is narrow: **may a stranger's band `grade` or `pave` a tile, and may it
    > > adopt a keeperless road another people wore in?** Nothing tests faction on either path today.
    > >
    > > **⛔ AND IT IS A QUESTION ABOUT PERMISSION, NOT ABOUT OWNERSHIP.** That word is retired from this
    > > arc (§4.13b) — Ray: *"that is an abstract term in this game so it really has no meaning."* There
    > > is no owning, there is the job of keeping, and this step decides who may take that job on. A
    > > proposal that re-introduces a possessive road is answering a question nobody asked.
    >
    > **13e — THE ROUTE BRANCH'S OWN TUNING PASS. CONFIG ONLY, AND LAST.**
    > - **Why it is a step rather than deferred to §4.14**: §4.14 owns the *arc's* numbers and waits on
    >   the whole arc. The route branch can be tuned as soon as **13a–13d** are in, without waiting for
    >   the plant and animal spreads — and it needs to be, because the shipped pace is visibly wrong.
    > - **Why it must not happen sooner**, which is the real argument and is §4.14's own one level down:
    >   **13b adds two more traffic sources.** A pace tuned today is tuned against pooling links alone;
    >   the moment shipments and marching bands also wear roads in, the effective rate changes and every
    >   number set before it is a number set on a shape about to move. Ray: *"it would be better to have
    >   everything done before tuning."*
    >
    > **THE KNOWN OFFENDER, measured in a live game at turn 32:** `route_traffic.work_per_link_tile_per_turn`
    > is `0.35` and `route:trail` costs `40`, so a tile carrying one link needs **~114 turns** to become a
    > trail. Seven game-trail tiles existed between three bands at 24–52% worn and none could have
    > changed rung. The rate **halved in effect** when roads went per-tile (§4.13b) — it is now literally
    > per tile where it used to drive one path object — and was deliberately left uncompensated, because
    > a model change must not hide behind a retune. **This is where that debt is paid.**
    >
    > **Everything else this step owns**: `route_traffic.disuse_grace_turns` / `disuse_loss_per_turn`
    > (how fast an unwalked trail fades — currently ~44 turns against ~114 to form, which is very likely
    > backwards); `route_range.base_tiles` / `remote_cost_multiplier`; the four rungs' `work_cost`,
    > `work_per_turn` and `meter_decay`; and the payoff numbers — `friction_multiplier` and
    > `holds_link_to_tiles`, the second of which cannot be judged until 13b consumes it.
    >
    > **The knowledge weighting is 13b's to DESIGN and this step's to TUNE.** *How far* and *how often*
    > combine is a modelling question 13b must answer; the constants that fall out of it are numbers.
    >
    > **Tuning is not spread across the other steps** — §4.14 owns the arc's numbers and is explicitly
    > last, and this step is its route-branch counterpart. The rung rates, spans, friction multipliers
    > and traffic conversions land at shape-chosen values throughout 13a–13d and are moved **here**,
    > once there is nothing left to change the shape.
    >
    > **Nothing in this list renumbers anything.** Items 14 and 15 keep their places; 13a–13e are
    > the PRs *inside* item 13.
14. **The tuning spread.** Config-only, and **last** — §4.10 changes what the numbers do to the curve,
    so tuning before it would be tuning a shape that is about to move.
    > **§4.11 LANDS FIRST, for this item's own reason.** A flat per-rung demand and a size-scaled one
    > are different shapes, not different numbers, so tuning the plant demands before the scale
    > primitive exists would tune something about to move — the same argument that put this slice
    > after §4.10.
    >
    > #### WHAT §4.10's PLAYTEST LEFT ON THE TABLE — each measured, none tuned
    >
    > - **THE FIELD'S SPLIT BETWEEN CAPACITY AND REGROWTH IS ARBITRARY AND IS THE FIRST THING TO
    >   PLAY.** Both ship at **×2.53** because what was held was the *product* — the Field had to land
    >   within 5% of where it already paid, and it did (6.2400 → 6.2409). The split was never chosen.
    >   It matters because the two do different jobs: **capacity is the size of the store, regrowth is
    >   how fast it refills**, so a big-store slow-refill field is one you strip and then wait on,
    >   while a small-store fast-refill field must be harvested steadily or you waste it. Ray, on
    >   being told the product was what was held: *"that split is what decides whether a field is a
    >   granary or a treadmill."* **Trust the measurement over the algebra here** — `MSY = r·K/4`
    >   predicts a product of 8.25 and the real answer was 6.40; the clamp and the operating point eat
    >   the difference.
    >
    > - **SOWING UNTOUCHED GROUND COSTS 125 WORK UNITS, UP FROM 75, AND THE ANSWER IS THE SPANS.** The
    >   tended rung's work was previously skipped rather than paid, so this is a model change and not
    >   a tuning edit. If the combined climb is too steep the fix is **moving the rung spans** (tended
    >   40 + field 60 = 100), never exempting a rung from the climb. Ray: *"it isn't a tuning change,
    >   but tuning could help it."* **Do not shave it before it is played** — hiding a model change
    >   behind a config edit is how the jump stops being visible.
    >
    > - **`WearQuantum::UpkeepWork` HAS NO CONVERSION TO INVERT.** Recorded at §4.8 and repeated here
    >   because this is the slice that owns it: a keeping tool wears on the work it supplied, but the
    >   rate is an opening value rather than a re-minted one, because the quantum never existed
    >   before. It is the one number in the arc with no prior to be neutral against.
    >
    > #### AND TWO THAT ARE **NOT THIS ARC'S DIALS**, recorded because they were measured here
    >
    > - **`husbandry_regrowth_cap` SILENTLY DISCARDS PART OF `pen_gain` ON THE FAST BREEDERS.** The cap
    >   is `1.0` and `pen_gain` is `4.0`, so a species whose wild `r` exceeds `0.25` cannot receive the
    >   whole pen bonus. Of the seven **pennable** species, three lose some of it: **fowl** and **rabbit**
    >   forfeit **29%** (`0.35 × 4 = 1.4`, delivered `1.0`) and **snow hare** **17%** (`0.30 × 4 = 1.2`).
    >   **The cap never binds at the pastoral rung** — the fastest pastoral rate on the roster is `0.70`
    >   — so it is a pen-only effect, which is why it reads as the pen underperforming rather than as a
    >   cap. **It is also a tuning trap for this section**: raising `pen_gain` moves nothing at all for
    >   those three, so a spread tuned on the big-game rows would silently fail to reach the small ones.
    >   The mechanism is correct — a breeding rate of `1.0` per turn is already a doubling every turn,
    >   and an uncapped `1.4` is a discrete-logistic oscillation — but the roster and the cap were
    >   authored against different assumptions. It is the **fauna** arc's dial, not this one's, and it
    >   is the same shape as the `engage_rate` finding below: one global number that bites exactly one
    >   end of the roster.
    >
    > - **`fauna_config.json`'s `engage_rate` INVERTS THE ECONOMY OF SCALE ON BIG GAME.** Wild Boar
    >   sits at **0.33**, and a party that exists always reaches at least one animal, so **every crew
    >   from 1 to 6 hunters brings down exactly one boar** — a lone hunter is **four times more
    >   efficient per head** than a party of twelve, and twelve hunters split across twelve herds take
    >   twelve boar where twelve on one herd take three. The truncation is lumpy at the margin too:
    >   `floor(12 × 0.33) = 3`, so the twelfth hunter contributes nothing while the thirteenth is
    >   worth a third more food than the eleven before him. `fauna.md` already names the hazard —
    >   *"an `engage_rate` authored too low silently becomes a second floor"* — and
    >   `HuntTakeBound::Engagement` exists to make it visible. **The mechanism is correct and the
    >   number is not**; at a rate near 1.0 the minimum-of-one stops being a bonus and party size
    >   means what it looks like it means. It is the **fauna** arc's edit, not this one's.

15. **A SOW IS PRICED BY HOW MUCH OF THE TILE IT REPLACES.** A **scale primitive**, of §4.11's and
    §4.13's kind and not §4.14's — the numbers below are §4.14's to own, the shape is not. It is the
    ladder's existing per-source price hook (`RungStanding::at`'s `cost_at`, which the animal web
    already spends on a species' `taming_cost_multiplier`) claimed by the plant web, which passed
    `RUNG_COST_UNSCALED` with a comment saying a plant has no species. **`plant:field` only** —
    `plant:tended` is untouched, because clearing wild ground is clearing wild ground.

    Ray: *"I think maybe it should take more work to sow a field based on what % the crop is on the
    tile."* Sowing a crop that already holds most of the ground is **tidying**; sowing one that holds
    a tenth is **replacing the tile**. Until this the crop's share was invisible except as the
    auto-picker's hidden criterion — it now has a job, and the crop choice becomes a decision on the
    **cost** axis as well as the payoff one. It also reads correctly against the play report that
    prompted it: a tile offering 100% tobacco is *cheap* to sow and feeds nobody, while sowing grain
    there is dear and feeds you.
    > **THE SHAPE IS A RATIO AGAINST A REFERENCE, NEVER A PENALTY.**
    >
    > ```text
    > replacement = 1 - crop_share
    > share_load  = replacement / (1 - field_reference_crop_share)
    > field_cost  = work_cost x clamp(share_load, field_share_cost_floor, field_share_cost_ceiling)
    > ```
    >
    > `field_reference_crop_share` is **0.5625, the reference basket's own weeded share** —
    > `wild_emmer` holds `0.375` of `AlluvialPlain`'s realized basket and a Cultivate weeds it to
    > `0.375 × 1.5` — so the shipped `plant:field` price of 75 work units is **provably
    > pacing-neutral there**, exactly as `capacity_per_tender` is 195.0 for being that same tile's own
    > `K`. A bare penalty would make the ladder's declared cost the *cheapest* case and inflate the
    > whole plant branch; a bare discount would deflate it. With an anchor, ordinary sowable ground
    > costs what the ladder already said, and §4.14 moves the anchor rather than re-tuning the rung.
    >
    > **Both clamps are load-bearing.** Floor `0.25` (18.75 units, ~6 turns at the rung's reference
    > crew of three): ground already wholly the crop replaces *nothing*, and a free Sow there would
    > still collect `field_capacity_gain` and `field_regrowth_gain` for having laid the rows and put
    > the seed in. Ceiling `2.0` (150 units, ~50 turns), binding below a crop share of about an
    > eighth: without it a marginal crop's price is bounded only by the anchor, which is a dial and
    > not a promise.
    >
    > **⛔ ① THE SHARE IS MEASURED ONCE, WHEN THE LEG STARTS, AND HELD FOR THAT LEG.** It is stamped
    > on the **patch** (`ForagePatch::field_cost_multiplier`, the exact twin of
    > `Herd::taming_cost_multiplier`) rather than on the queue entry, because the patch's own standing
    > is derived from its rung spans: a price the source could not see would leave the position's
    > meaning and the job's price in two places. `None` is *the leg has not started*, and while it is
    > `None` the Field rung's width provably changes nothing the patch derives — the position is at or
    > below the rung's base.
    >
    > **It is NOT live.** §4.10 made the mix interpolate across the rung being raised, so a Sow raises
    > its own crop's share continuously as it proceeds: a live price would shrink the remaining work
    > as the work was done — a job that accelerates itself — and it would turn §4.6b's chained finish
    > date from an exact construction into an estimate that drifts under the player.
    >
    > **And it is NOT stamped once at declaration either.** A `Sow` on untended ground is two legs
    > (§2.8), and the Cultivate leg genuinely weeds toward the crop before the Field leg begins. Ray's
    > ruling is that the mechanism *"doesn't care if it was cultivate"* — it reads the current share —
    > so the price is re-quoted **at the leg boundary**. A discrete re-quote, not a drift: fixed for
    > the whole of the leg it prices. It lapses again if the position ever bleeds back to the rung's
    > base, which is the same rule applied to a re-attempt.
    >
    > **⛔ ② IT READS THE BASKET OF THE RUNG *BELOW*, NOT THE PATCH'S LIVE MIX.** A turn's accrual
    > routinely overshoots the rung boundary, so a live reading taken when the leg starts is taken
    > *after* the build has already moved it — the build pricing itself, which is `capacity_per_tender`'s
    > ① one account over (*"the measure reads the TILE's `K` and never the patch's
    > `carrying_capacity`, which has already been multiplied"*). The rung below's basket is free of it
    > **and is exact**: a Field leg can only begin from a full tended rung, and a full tended rung's mix
    > is `weeded` by construction. So the number quoted before a two-leg Sow starts and the number
    > stamped when its Field leg finally begins are the same number, which is what keeps the chained
    > date a construction.
    >
    > **⛔ ③ IT DOES NOT TOUCH THE UPKEEP, AND THAT IS THE §4.11 RULE.** `plant:field`'s hold cost is
    > `scaled_by: source_load`, which reads the **tile's** `K` — holding a field is about how big the
    > place is, never about what used to grow there. Two scale terms on one bill is exactly the
    > compounding §4.11 ① measured (a Field billing 10.898 against the 4.308 it owed), so a Field that
    > was dear to sow and one that was cheap owe the identical rate once they stand.
    >
    > **④ AND THE QUOTE MOVED WITH THE CHARGE**, on §4.3's rule. Every surface that states what a Sow
    > will cost resolves through one seam (`forage::patch_field_cost_multiplier`): the arm that charges
    > it, `patch_build_legs`' work figures and their chained dates, the pre-commit projection the `⌃`
    > mark and the compose sheet read, and the published `fieldWorkCost`. **The patch's own price
    > added no wire field** — `fieldWorkCost` already carried it and now carries the scaled one, so
    > the client quotes what the sim charges without re-deriving anything. The **per-crop** half below
    > is a different question, and it is the one thing here that did add a field.
    >
    > **⑤ AND THE PICKER PRICES EVERY CROP, NOT ONLY THE ONE THE PATCH ALREADY NAMES.** A patch
    > prices exactly *one* crop — its commitment, or the rung's auto-pick — so the crop picker on the
    > `⌃`'s destination-rung popover, which lists each legal crop beside what it would pay
    > (`sowPayoff` / `sowFodderPayoff` / `sowMaterialPayoff`), had the *same* work figure against
    > every row while only the payoffs moved, and the true figure appeared only once the leg started
    > and re-quoted. **That defeats the picker**, whose whole job is to let work be weighed against
    > payoff *before* committing.
    >
    > So the work half answers per crop exactly as the payoffs do: `FloraShareInfo.sowWorkCost`, one
    > figure per composition entry, in the same work units as `fieldWorkCost`. It is
    > `field_cost_multiplier_at_share` over **that crop's own** `field_replaced_share`, priced by the
    > ladder's own `build_cost` — *one expression* with the patch's own price
    > (`forage::crop_field_cost_multiplier`, which `patch_field_cost_multiplier` goes through), so the
    > figure quoted for the crop a patch is committed to **is** that patch's `fieldWorkCost` rather
    > than a second derivation that happens to agree. Asserted on the encoded envelope, at
    > declaration, at the commitment and again with the leg stamped.
    >
    > **Empty for a crop that cannot climb to a Field here** (`species_climbs` at `plant:field`),
    > which renders as *no row*: a `0` would read as a free Sow, and a real price never is one — the
    > floor clamp exists precisely because laying the rows and putting the seed in costs work on any
    > ground.

> **Every number in this arc is provisional until §4.14.** The plant demands of `2.0` / `4.0` are
> whole-number placeholders chosen to be legible, not balanced; the graces of `2` and `1` are
> inherited from the rung they replaced rather than chosen; and the Field's `2.53` / `2.53` were
> never a split at all — only their product was held. Do not tune any of them in an earlier slice —
> the mechanism is what the earlier slices are for. **`retain_fraction` is no longer on this list:
> §4.10 deleted it outright rather than dissolving it, because interpolation removed the cliff it
> was patching.**

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
- **ANSWERED in §4.11 — a RUNG RAISING THE CEILING RAISES THE FLOOR WITH IT, and that was fatal.**
  The escapement floor is `floor_fraction × K` and a rung multiplies `K`, so the floor climbs while the
  herd stays the size it was. It is not a slow squeeze: because the build's own eligibility gate read
  the room above the floor, room reaching zero **closed the gate**, and a tame begun on its floor
  **never completed at any crew size** — turn 6 at one herder, turn 3 at four, turn 2 at eight, so
  *building faster starved you sooner*. Five of the eleven tameable species are on the losing side of
  that race; only the fast breeders clear it comfortably.
  > **The take is now `max(room above the floor, growth × (1 − floor))`, and the build gate reads the
  > same expression**, which makes *a legal build target that yields nothing* unrepresentable rather
  > than merely avoided. **No new dial**: the player's own floor scales it — *you keep the share of the
  > growth you were willing to take* — so `floor = 1.0` still pays nothing at both seams with no special
  > case. A flat share would have made *leave the whole herd standing* cull every turn.
  >
  > **The escapement predicate fed TWO seams and only one moved.** The lesson keeps the pure room:
  > `learn_multiplier`'s self-limit is deliberate — a floor just under `1.0` learns at nearly ×2 while
  > taking almost nothing, and its doc forbids clamping it — so widening that seam would have made a
  > full floor free ×2 learning for ever. Global across both webs, because a build-scoped rule would be
  > a rung changing the draw.
- **ANSWERED in §4.11 — the scale primitives' bounded set is ONE primitive with a per-branch
  reading.** `SourceLoad` is the only variant: the animal branch reads keeper-loads
  (`head count / animals_per_herder`), the plant branch tender-loads
  (`tile forage capacity / capacity_per_tender`). A second variant would have restated the `branch`
  the rung already declares. **`Flat` is deleted** — nothing declared it once both plant rungs moved,
  and the rung-monotonicity check skips adjacent rungs that do *not* share a `scaled_by`, so an
  unused variant was a silent opt-out from that check rather than a harmless spare. **The `scaled_by`
  key stays** for §4.13's `length × terrain` (`infrastructure_cost`), which is the one remaining
  candidate and is genuinely a different shape: it reads the improvement's own geometry rather than
  the source it sits on.
- **Whether the two keeping pools should split further.** Agriculture and husbandry split because the
  webs do. A finer split — a herd keeper's kit versus a field tender's — is only meaningful once a kit
  declares a maintenance contribution, which none does today, so splitting now would invent a
  distinction nothing can express. It becomes a config-shaped change the moment §4's gear-as-
  productivity lands.
- **AN UNKEPT ANIMAL BUILD STALLS PERMANENTLY, and §4.6b's queue has to answer for it.** Measured:
  a half-tamed herd with an empty `husbandry` role advances for three turns and then freezes — the
  hunters draw the flock to their floor, the unmet keeping suppresses its regrowth, the escapement
  room never returns and the `Tame`'s own gate closes. It is not self-correcting and the only remedy
  is staffing the keeping, which nothing on the build line points at. **The plant web does not have
  it** (an ungathered patch regrows, so its gate stays open and it publishes an honest `-3`), and the
  mechanism is `husbandry.md`'s. What makes it §4.6b's problem rather than a curiosity: **a stalled
  entry at the head of a queue funded all-hands-on-the-head holds every builder the band has**, on a
  build that can never advance, while everything behind it waits.
  > **DECIDED in §4.6b — the head STAYS PUT and says loudly that it is stuck.** Ray: *"stay put and
  > say loudly it is stuck."* A head whose own gate refuses it while builders stand on it publishes
  > `BUILD_QUEUE_BLOCKED` (`-4`), and **every entry behind it carries that same answer** — if the head
  > never finishes, nothing below it does either, so the chained date is telling the truth rather than
  > taking a special case. **Passing over an ineligible head was the rejected alternative**: it would
  > silently re-order the one list whose order is the player's own input, and it would hide the stall
  > that the remedy depends on being visible. The remedy — staff the keeping — is named on the row,
  > and gated on the keeping actually being short so it cannot fire on a rung stalled for some other
  > reason.
- **Whether ALL-HANDS-ON-THE-HEAD is the right funding rule** (§4.6b). Ray: *"that is logical, we
  will have to play test and see how it goes."* The thing to watch is a band with several worthwhile
  builds queued behind one long one, and whether re-ordering feels like a decision or like a chore.
  The chained finish date is what makes it judgeable — if the answer turns out to be *"let me run two
  at once"*, that is a spread with a stated split rather than a return to per-source crews.

> **THREE THINGS §4.6b's PLAYTEST FOUND THAT THIS ARC DOES NOT OWN.** They are recorded here because
> that playtest is where they surfaced and because each of them made the arc's own behaviour hard to
> read — not because a later slice of it should absorb them. None is a build or an upkeep question.

- **The map's `⚠` cannot be interrogated.** `BandOverlayRenderer._draw_source_badge` paints it with
  `_view.draw_string` onto the map canvas, so it is not a `Control` and takes no `tooltip_text`;
  §4.6b put the sentence on the tile card's `At risk:` row instead, which is reachable but is not
  where the player is looking. Making the mark itself interrogable is a MapView hover mechanism —
  there is no such thing today — and it would serve every map badge rather than this one.
- **The event dock PINS a transport alert indefinitely.** A failed send at turn 12 was still the
  pinned Alert at turn 13 with the turn stamp the only thing distinguishing it from a live failure,
  so a single dropped command read as a permanent disconnection and made the arc's own readouts look
  like they were lying. Whether a pinned alert should expire, or be stamped as stale, is
  `event-dock.md`'s question.
- **THE CLIENT CANNOT WARN BEFORE A SEND FAILS.** `Inspector._ensure_command_connection` checks only
  that the native command bridge OBJECT exists, never that a socket is live, so a dead command socket
  is discovered by a failed write and not before. §4.6b closed the half that was actively misleading
  — a failed send now rolls back its own optimistic overlay entry, so the panel stops showing hands
  the sim never received (`hud-modules.md` → "AN OPTIMISTIC WRITE NEEDS A ROLLBACK") — and it
  deliberately added **no retry, no reconnect and no queue of unsent commands**, a resend the player
  did not ask for being worse than a dropped one. Making the connection's health *knowable* is the
  part left, and it belongs with the transport rather than with any gameplay arc.

> **Three items were retired rather than answered, and all three are worth recording as such.**
> *"What the maintain toggle does to a build in flight"* went with the toggle itself — keeping is a
> band-level pool, so *"stop maintaining this one thing"* is no longer expressible, and that is
> deliberate; **`abandon` is not that lever coming back**, it puts the whole source down rather than
> tuning its share. *"A building crew takes nothing"* was true only of the one-budget model it was
> written under: with the crews separate, the gatherers beside a build carry exactly what gatherers
> carry. And *"a crew at or below the maintenance rate never finishes"* went with the fullness test
> in §4.6a — a build crew supplies no rate now, so the threshold it names does not exist. **The `∞`
> pair survives with a different denominator** (the rot, §2.2), which is why the readout was not
> deleted with the rule that first justified it.

- **TWO ISSUES ARE OWED AND UNFILED, and each needs an arc parent chosen before it can be.** Neither
  is a design question — both are settled work with no home on the board yet.
  - **Wood from forest foraging.** §4.9 item 12 adds `wood` as a material and supplies it through
    worldgen's `StartKit`, which means it is **produced by nothing**. That is deliberate — a
    production path is flora work, not upkeep work — but it is a real gap the moment the start stock
    runs out, and it is the only thing between this arc and a material economy that closes.
  - **§4.9 item 12b.** Written up in full at §4.9 so it is not re-derived; it just has no issue.

- **AND THE MATERIALS THEMSELVES NEED AXES, which item 12 does not choose.** A material in this
  repo is generic with characteristic axes (`docs/plan_crafting_and_materials.md` §1), and
  `RecipeOutput::material` **requires** them on the output — so `wood` and `hurdles` cannot load
  without them. **Nothing reads them in item 12**, and the trap is why: a quality axis on a hurdle
  invites *a better fence contains better*, which is precisely the containment-scaling item 12
  defers. Give them axes for consistency with the model, wire no effect to either, and let the
  question arrive with whatever slice actually wants it.

---

## See Also

- `docs/plan_unit_costed_work.md` — the arc that priced **building** in work units; this prices
  **holding** in the same currency.
- `docs/plan_contact_and_logistics.md` §Q4 — the route ladder, which needs this term to exist.
- `.claude/rules/core_sim/intensification.md` — the ladder engine as built.
- `.claude/rules/core_sim/husbandry.md` — the pen's feed and the shed, the two shipped mechanisms
  §2.7 and §3 generalize.
