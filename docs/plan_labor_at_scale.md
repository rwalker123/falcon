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
3. **Bare hands, where they help.** Workers who cannot be kitted are assigned, in priority order, only
   to jobs where a bare-handed worker actually adds value — see "Pass 3, and what is left over".

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

### Queue jobs: the bench and the build queue

The bench and the build queue are both queues of orders, and neither has a natural crew size — every
extra hand finishes the head order sooner. They are staffed the same way.

- **The player adds orders to both**, as today. The bench also fills itself: when no order can
  progress, `auto_craft_fill` queues the snapshot's own craft suggestions — the gear the band's rows
  are short of. What changes is that the sim staffs both queues; the player no longer sets
  `bench_crew` or the Builders role.
- **Each order carries a priority** (build queue entries already do), and it ranks against sources on
  the one priority list. So a High craft order can take kitted workers ahead of a Normal source's
  reach crew — some kitted workers are spared for the crafting the player asked for.
- **A queue job has no hold crew.** It is staffed in pass 2 (and pass 3 where bare hands help), never
  pass 1.
- **A queue job takes every worker it can use, and no more.** Its useful crew is the crew that
  finishes its head workable order in one turn — the order's remaining worker-turns (bench) or
  remaining work (build). Where nothing else wants the hands, the queue fills up to that crew. An
  order short of an input is not workable and takes nobody, exactly as the bench skips it today.
- **A bare-handed builder builds**: build gear adds to what each builder delivers, it is not required
  to deliver anything. A bare-handed crafter crafts too, at the material's hand-working rate. So both
  queues are pass-3 destinations.
- **"Kitted" for the bench means holding its bench tool** (the tanning frame, the awl, the billet);
  for the build queue, the build kit the entry's rung uses.

### Pass 3, and what is left over

Pass 3 is one pass, in priority order, over **every job where a bare-handed worker adds value** — the
bench, the build queue and bare-handed sources (foraging is the main one), each up to its useful crew.
"Adds value" is the sim's own answer for each job, the same test passes 1 and 2 use.

**Workers left after pass 3 are idle, and the game must say so clearly.** Idle people are not waste:
they are the band's spare capacity, and later they become traders, diplomats and other roles that
need no gear. So the band's idle count is a first-class readout — on the band, on the faction page,
and in the existing `N idle workers` alert — not a remainder the player has to work out.

### Two interfaces: what grows back, and what is mined

**Anything that grows back is worked like a food source**: a floor per site, and the hold and reach
passes. That covers foraging, hunting, pens — and **wood**, whose stands regrow. One interface for
every living resource, whatever it pays.

**Anything mined — it runs out and does not come back — takes a keep-on-hand target** at band level
(e.g. *keep 60 stone*): stone today, ore and gold later. The player never assigns workers to a mine;
the game picks the sites.

- **Hold** (pass 1): the crew whose take matches what the band draws on that material per turn — its
  builds' and its upkeep's draw.
- **Reach** (pass 2): the extra crew that refills the pile toward the target.
- **At or above the target**: no workers.
- **Which sites**: the ones that give the most per worker — the best-geared rung, the shortest walk.

A deposit's current "useful crew" (the crew that cuts everything above the floor in one turn — 958
cutters on the Firbrook deadfall) is not a target anywhere in this design.

**No separate food-first rule is needed.** The passes already order the work: every food source's hold
crew is in pass 1, and a mine's pass-1 share is only what the band draws each turn. Refilling a
stockpile is pass 2, so it never takes hands from food that is merely holding. Beyond that, priority
decides.

### Scouts: a range, not a count

The player sets **how far the band sees**; the sim staffs the scouts that reach it. That is what adding
scouts does today, and a range stays right as gear improves — a better wayfinding kit extends each
scout's vantage, so fewer scouts are needed for the same range.

### Warriors: no standing count — defenders are chosen in the battle

There is no standing warrior count for the sim to staff. Who defends is chosen when a battle happens,
in the battle's own flow, and the defenders leave their work for as long as the battle runs and go back
to their jobs afterwards. A work party past the camp's range is not in camp and cannot defend.

How a battle runs — who chooses, in whose turn, how it ends — is the combat design's, not this doc's.

### What #465 becomes

[#465](https://github.com/rwalker123/falcon/issues/465) proposed a min/max worker range per source,
staffed by the sim. This design absorbs it: its *min* and *max* are the floor's two crews, computed by
the sim rather than set by the player, and its open question — *which source gets a newly-freed worker
first* — is the priority list. The player-facing **max** above is the surviving half of its range.

## Open questions

None open in this doc. The battle flow that decides who defends is the combat design's.

## Related

- [#465](https://github.com/rwalker123/falcon/issues/465) — labor standing orders (absorbed, above)
- [#709](https://github.com/rwalker123/falcon/issues/709) — the granary as a network node
- [#598](https://github.com/rwalker123/falcon/issues/598) — the faction as central government (the top layer)
- [#798](https://github.com/rwalker123/falcon/issues/798) — hunting by need: the one row type where the
  sim already sizes the crew itself
- `docs/plan_harvest_floor.md` — the floor and the two crew targets
- `docs/plan_pool_toe.md` — the band's pooled gear and how scarce items are settled by Priority
