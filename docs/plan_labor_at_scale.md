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

### Hand placement is the bottom layer, and the layers above it come with growth

| Population | What the player manages |
|---|---|
| A starting band | floors and priorities on a handful of sources |
| A settled band | the same, over more sources — the sim staffs every one of them |
| Several bands | each band's priorities (a band's list can be copied to others) |
| A faction | policy above the bands — #598 |

Nothing is taken away as the game grows. A lower layer stays reachable, a click or two deeper.

## Proposed — for review

### Jobs with no floor

Some work has no floor to set. Proposed answer for each:

| Job | Why no floor | Proposal |
|---|---|---|
| **A finite seam** (stone) | rock does not grow back, so the sheet offers no floor dial today | Its whole useful crew is *reach* work: it is staffed in pass 2, at its priority, up to its useful crew and its max. A quarry never takes hands from a source that is merely holding. |
| **A pen** | a managed source pays managed production; it has no floor axis | Its whole useful crew is *hold* work: staffed in pass 1. A pen is steady-state by nature, and an under-kept pen sheds its animals. |
| **Builders** | a build is a job, not a stock | The builders role is *reach* work: staffed in pass 2 at the build queue's priority, up to the role's max. A build is investment — it waits until the band is fed. |
| **Roadwork** | a road's keeping is a standing bill | Staffed in pass 1, at the crew its keeping bill needs. |
| **Scouts and warriors** | they take nothing; their size is a choice | The player sets a count (the role's max is its target). Staffed in pass 1, in priority order — a standing duty, like holding a source. |

### What #465 becomes

[#465](https://github.com/rwalker123/falcon/issues/465) proposed a min/max worker range per source,
staffed by the sim. This design absorbs it: its *min* and *max* are the floor's two crews, computed by
the sim rather than set by the player, and its open question — *which source gets a newly-freed worker
first* — is the priority list. The player-facing **max** above is the surviving half of its range.

## Open questions

1. **Pass 3's order.** Read as written, pass 3 assigns bare-handed workers wherever they contribute.
   Does it follow the same two steps as the kitted passes — every source's *hold* first, then *reach*,
   each in priority order?
2. **A source below its floor.** A source standing below its floor takes nothing until it regrows past
   it, while its *hold it after* crew is still positive. Does pass 1 staff it anyway (the hands are
   there the turn it crosses), or skip it until there is something above the floor?
3. **Reshuffling.** Crews now change size without a command. A worker freed by one source moving to
   another every turn could read as jitter. Is per-turn reassignment acceptable, or should a crew only
   be reshuffled when a source changes state (reaches its floor, runs out of gear, is added or
   removed)?

## Related

- [#465](https://github.com/rwalker123/falcon/issues/465) — labor standing orders (absorbed, above)
- [#709](https://github.com/rwalker123/falcon/issues/709) — the granary as a network node
- [#598](https://github.com/rwalker123/falcon/issues/598) — the faction as central government (the top layer)
- [#798](https://github.com/rwalker123/falcon/issues/798) — hunting by need: the one row type where the
  sim already sizes the crew itself
- `docs/plan_harvest_floor.md` — the floor and the two crew targets
- `docs/plan_pool_toe.md` — the band's pooled gear and how scarce items are settled by Priority
