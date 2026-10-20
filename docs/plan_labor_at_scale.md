# Plan: Labor at Scale — the player sets the floor, the sim staffs the crew

Status: **design.** The spec for [#842](https://github.com/rwalker123/falcon/issues/842). Parent arc:
[#179](https://github.com/rwalker123/falcon/issues/179) (Early-Game Labor).

## Why this doc exists

Every worker is placed by hand. Early that is the game: ~50 workers, a new one every few turns, and
each placement is a real choice. Once population takes off, new workers arrive every turn and placing
each one is a chore. At a million workers it is impossible.

**The rule this design is held to: the player's work must not grow with population.** If doubling the
people doubles the clicks, the design fails eventually, however good the automation.

## Decided

### The player sets the floor; the sim sets the crew

The compose sheet already has two independent inputs: the **floor** (how much to leave standing — the
`Everything` / `Best` / `Learning` presets over the draggable chart) and the **crew** (the stepper).
The floor is the real decision. The crew's two useful sizes already follow from it, and the sheet
already names them:

- **hold it after** — the crew that takes what regrows at the floor each turn (the *keep* rate);
- **reach the floor** — the crew that draws the source down to the floor (the verdict's
  *"N gatherers would reach the floor"*; *clear it now* is its one-turn form).

So the player stops choosing a headcount. They choose the floor, and the sim staffs the crew from it,
every turn, out of the band's working-age people.

### The crew stepper is retired; a MAX replaces it

There is no hand-placed crew and no per-row override. A source carries an optional **max**: the most
workers the sim may put on it. It exists for the case where the player wants fewer hands on a source
than would help — *"only 2 here"* — to spare the herd or save hands for something else. No max means
the sim staffs to whatever the passes below allow.

### Staffing runs in three passes over the band's priority list

Each source's existing **Priority** (`High` / `Normal` / `Low`) orders the band's sources. Each turn,
working-age people are assigned in three passes:

1. **Hold, kitted.** In priority order, staff every source up to its *hold it after* crew, using only
   workers who can be kitted for that source.
2. **Reach, kitted.** If kitted workers remain, staff sources further, up to the crew that reaches the
   floor, again in priority order.
3. **Bare hands, where they help.** Workers who cannot be kitted are assigned only to sources where a
   bare-handed worker actually adds to the take.

**Kitted first, across priorities.** A lower-priority source worked with gear is preferred to a
higher-priority source worked bare-handed. Gear is what makes a worker productive, so a kit should
never sit unused in the band's pile while someone works bare-handed elsewhere.

**"Kitted" means kitted for that source's job.** A worker is kitted for a source when the band's pooled
items can arm them under the source's kit — the same coverage the sim already resolves per row, with
scarce items settled by Priority as they are today (`settle_scarce_store`).

**"Can contribute" is the sim's own answer.** A bare hand contributes where the crew-take curve still
rises when one more unarmed worker is added: a forager without a basket still gathers, a bare-handed
hunter against an animal whose `defense` they cannot clear adds nothing. No new rule — the take model
already answers it.

**A source's MAX caps every pass.** No pass puts more workers on a source than its max.

**A source below its floor gets no workers.** It takes nothing until it regrows past the floor, so it
is skipped until there is something above the floor to take. Staffing is re-evaluated every turn, so
it is picked up the turn it crosses.

**Crews are re-evaluated every turn, and workers move freely.** A worker may be on a different source
each turn; no smoothing holds a crew in place.

**A pen is hold work** — staffed in pass 1 at its hold count. **Roadwork is hold work** — staffed in
pass 1 at the crew its keeping bill needs.

### Hand placement is the bottom layer, and the layers above it come with growth

| Population | What the player manages |
|---|---|
| A starting band | floors and priorities on a handful of sources |
| A settled band | the same, over more sources — the sim staffs every one of them |
| Several bands | each band's priorities (a band's list can be copied to others) |
| A faction | policy above the bands — #598 |

Nothing is taken away as the game grows. A lower layer stays reachable, a click or two deeper.

## Proposed — for review

### Un-kitted workers make the missing kits

Pass 3 sends un-kitted workers to the **bench** first, to craft the gear the band is short of, and only
then to sources where a bare hand still contributes (foraging is the main one). The bench already
picks what to make: when no order can progress, `auto_craft_fill` queues the snapshot's own craft
suggestions — the gear the band's rows are short of. What changes is that the sim staffs the bench
rather than the player (`bench_crew` today).

### Stone: a stock target, not a crew

A deposit's useful crew is the crew that cuts the whole room above the floor in **one turn** — 958
cutters on the Firbrook deadfall. That is meaningless as a target. Stone has no floor at all, so it
needs a different "how much".

Proposal: the player sets **how much to keep on hand** (e.g. *keep 60 stone*). That is the floor's
counterpart for a store: the floor says how much to leave in the ground, the target says how much to
keep in the pile.

- **Hold** (pass 1): the crew whose take matches what the band draws per turn — its builds' and its
  upkeep's draw on that material.
- **Reach** (pass 2): the extra crew that refills the pile toward the target.
- **At or above the target**: no workers.

A **renewing** working (wood) keeps its floor, which protects the stand, and works like a forage
patch: hold at the floor's regrowth, reach toward the floor — the 958 is *clear it now*, not the reach
crew. Whether wood also takes a keep-on-hand target is open (question 4 below).

### Builders: what is left over

A build has no natural crew size: every extra builder finishes it sooner, up to the gear that arms
them. Proposal: **builders take the workers left after passes 1 and 2**, while the build queue holds
an entry, with no setting of their own. A band whose food and holding work take every hand does not
build until it frees some — by lowering a floor, or a priority. A bare-handed builder still builds,
so builders are also a pass-3 destination (question 5 below).

### Scouts: a range, not a count

The player sets **how far the band sees**; the sim staffs the scouts that reach it. That is what adding
scouts does today, and a range stays right as gear improves — a better wayfinding kit extends each
scout's vantage, so fewer scouts are needed for the same range.

### Warriors: a response, not a standing role

Warriors defend the camp against predator raids: the raid's defenders are the band's warriors, armed
by the warrior kit, and the rest of its exposed workers take losses unarmed. A standing count is hard
to reason about because nothing tells the player what one more warrior buys.

Proposal: **warriors muster by need**, the way #798 musters hunters. The band keeps no standing guard.
When a raid lands, workers who can be armed (clubs first) defend, pulled from their work in reverse
priority, and go back afterwards. The player's setting, if any, is a stance — *defend* or *flee* —
not a headcount. A standing guard that gives up work every turn comes back when military gear
creates real trade-offs (#519).

### What #465 becomes

[#465](https://github.com/rwalker123/falcon/issues/465) proposed a min/max worker range per source,
staffed by the sim. This design absorbs it: its *min* and *max* are the floor's two crews, computed by
the sim rather than set by the player, and its open question — *which source gets a newly-freed worker
first* — is the priority list. The player-facing **max** above is the surviving half of its range.

## Open questions

1. **Pass 3's order after the bench.** Once the bench is staffed, do the remaining bare-handed workers
   follow the kitted passes' two steps — every source's *hold* first, then *reach*, each in priority
   order?
2. **The bench's size.** Does the bench take every un-kitted worker, or only as many as its queue can
   use (a craft has its own worker-turns, so extra crafters can add nothing)?
3. **Stone's keep-on-hand target.** Is a stock target the right "how much" for a finite seam?
4. **Wood.** Does a renewing working keep only its floor, or also take a keep-on-hand target?
5. **Builders in pass 3.** Do bare-handed workers go to the build queue, and in what order against the
   bench and bare-handed foraging?
6. **Warriors.** Is mustering by need the right abstraction, and is *defend / flee* the right setting?

## Related

- [#465](https://github.com/rwalker123/falcon/issues/465) — labor standing orders (absorbed, above)
- [#709](https://github.com/rwalker123/falcon/issues/709) — the granary as a network node
- [#598](https://github.com/rwalker123/falcon/issues/598) — the faction as central government (the top layer)
- [#798](https://github.com/rwalker123/falcon/issues/798) — hunting by need: the one row type where the
  sim already sizes the crew itself
- `docs/plan_harvest_floor.md` — the floor and the two crew targets
- `docs/plan_pool_toe.md` — the band's pooled gear and how scarce items are settled by Priority
