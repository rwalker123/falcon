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
- **A band that SHRANK sheds**, tail-first, and a role is a row like any other — so where a band's
  builders fall in the shedding order is where the player put that row in the list.

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
| `pen.upkeep_per_biomass` + the pasture/hay/larder split | the **resource half** of the pen rung's upkeep (hay and larder), with pasture as its **offset** — unchanged in behavior |
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
   Each is reachable in play today and none is a defect in the model. The first three are one shape —
   a band-level act with no band-level surface — and the fourth is an asymmetry the pending rows
   introduced.

   - **Nothing on the Work tab declares a build** (①), so the tile sheet is still the only door.
   - **The kit override has no home** (②) — the card's picker is deleted, so the derivation currently
     stands alone and cannot be overridden at all.
   - **The queue cannot be reordered from the UI.** `build_order` is command-line only, as is
     `abandon`; the block caps at three rows plus a `+N more` overflow, chosen so the board keeps
     legible rows in a height-capped horizontal dock. Drag-to-reorder is this slice's, and the cap
     should be re-measured against whatever the pool header costs the zone.
   - **UNQUEUE IS NOT OPTIMISTIC.** A declaration made this turn shows immediately (a pending row at
     the tail, `○`, no date), but unticking a **confirmed** entry does not leave the block until the
     turn resolves — the queue's positions are wire state and the optimistic overlay carries
     additions only. The asymmetry is visible and should be closed with the row's own controls.

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
   > **LANDED — in full, and it diverged from this plan in three ways worth recording.**
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
11. **Plant upkeep SCALES WITH THE SOURCE.** Both plant rungs ship `scaled_by: flat`, so a rich
    alluvial patch and a thin one cost the same to hold. Ray: *"the flora track should scale by size,
    just like animals."* The whole-number demands were an explicit short-term step, not the model —
    §2.6 already says a flat per-rung number cannot be right, because what makes a thing expensive to
    hold differs by what it is. The animal web has had this since slice 4 (`SourceLoad`, the herd's
    own keeper load); the plant web needs its own measure, most likely the patch's capacity.
    **Mechanism, not tuning** — it is a scale primitive, and the numbers move in §4.14 regardless.
12. **The RESOURCE HALF of upkeep** (§2.7). Designed and **not built**: upkeep currently costs work
    and nothing else, while the pen's feed runs as its own separate mechanism, deliberately untouched
    so that moving it would not risk the pen-food ledger identity for no behaviour change.
    > **Routes are what force it.** A road wants hands *and* quarried stone, and it is the first
    > improvement whose resource draw does not already exist somewhere else — the pen's does. So this
    > lands before §4.13 rather than after, and generalising the pen's shipped
    > pasture-offsets-hay-offsets-larder split is the reference implementation.
13. **The route branch (#532 proper).** Routes as the ladder's third branch, `infrastructure_cost`
    wired for the first time, traversal-driven progress from supply links, shipments and movement.
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
    > #### AND ONE THAT IS **NOT THIS ARC'S DIAL**, recorded because it was measured here
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
- **The scale primitives' bounded set.** `Flat` and `SourceLoad` ship. A **plant** measure is §4.11's
  to add and a **route** wants length × terrain (`infrastructure_cost`), which is §4.13's. Whether
  those are two more primitives or one parameterisation is a question for the code, not for this doc —
  but note that after §4.11 **no shipped rung uses `Flat`**, which is the point at which to ask
  whether the variant still earns its place.
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

---

## See Also

- `docs/plan_unit_costed_work.md` — the arc that priced **building** in work units; this prices
  **holding** in the same currency.
- `docs/plan_contact_and_logistics.md` §Q4 — the route ladder, which needs this term to exist.
- `.claude/rules/core_sim/intensification.md` — the ladder engine as built.
- `.claude/rules/core_sim/husbandry.md` — the pen's feed and the shed, the two shipped mechanisms
  §2.7 and §3 generalize.
