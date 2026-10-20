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

**A source below its floor gets no takers.** It takes nothing until it regrows past the floor, so its
take is skipped until there is something above the floor to take. Staffing is re-evaluated every turn,
so it is picked up the turn it crosses.

**Its keepers are still sent.** An improved source (a tended patch, a farm, a tamed herd, a pen)
owes its keeping every turn, whether or not anything is harvested. Without keepers it rots back down
the ladder and never grows back, so pass 1 staffs a below-floor source's keeping crew and nobody else.

**Crews are re-evaluated every turn, and workers move freely.** A worker may be on a different source
each turn; no smoothing holds a crew in place.

**A pen is hold work** — staffed in pass 1 at its hold count. **Roadwork is hold work** — staffed in
pass 1 at the crew its keeping bill needs.

### Per-source settings are the bottom layer, and leaders add the layers above

The floors, priorities, maxes and rung builds above are what the player sets on each source. As the
people grow they gain leaders — a band's chief, a tribe's, a city's ruler, a nation's — and each
leader takes goals from the player and sets the layer below it. See "Leaders: four levels of
orders". Nothing is taken away as the game grows: a lower layer stays reachable, a click or two
deeper.

## Proposed — for review

### Builds join the site's crew — there is no build queue

A site's crew already keeps it and harvests it (`docs/plan_site_crews.md`). It builds it too: when
the player starts an improvement on a site, its builders and their build kit join that site's crew.
There is no separate build queue and no Builders role, so **builds on different sites run in
parallel**.

- **The player starts a build from the site's rung control** — the row's existing `⌃` button and the
  "Take it to…" rung card it opens. The site's sheet shows the **same control**, not a second one: one
  widget — the **work strip**: the rung mark, the `⌃` button and its card, the floor glyph, `⚠`, then
  the crew number and its crew bar — so the row and the sheet can never disagree. While a build runs the button becomes its
  in-flight chip, filling with progress, with the builders and turns left in its tooltip.
- **A rung caps its builders.** Each rung states how many hands can work on it at once (a per-rung
  config value beside its work cost — e.g. a farm takes up to 4 sowers). A build never takes more,
  however much work is left.
- **A tile caps everyone working it.** Keepers, builders and harvesters together never exceed what the
  tile can hold.
- **A build ranks by its site's Priority.** There is no separate Build mark; the site's one Priority
  covers its keeping, its build and its harvest (this retires `docs/plan_site_crews.md` §2.4's Build
  mark).
- **One builder on every build first.** After pass 1, every build that can make progress gets one
  builder, at any priority: the player started it, so the game tries to staff it. Keepers and hold
  crews still come first, so the ground the band already has never rots to pay for new ground.
- **Then builders round-robin within a priority.** In passes 2 and 3, High builds each take one more
  builder in turn until every High build is at its cap; only then Normal, then Low. Within a priority
  group, builds are staffed before sources reach toward their floor.
- **A bare-handed builder builds**: build gear adds to what each builder delivers, it is not required
  to deliver anything. So builds are pass-3 destinations too, and "kitted" for a build means holding
  the build kit its rung uses.
- **A half-built rung nobody works rots**, as it does today.
- **Roads are built the same way**: a road build's builders join the road's own row, beside its
  keeping.

### The bench: several orders at once, each with a crafter limit

The bench is not a site, so it keeps its orders. The player adds them, as today. What changes is that
**several orders run at once** and the sim staffs them; the player no longer sets `bench_crew`, and
the Crafters stepper is retired like the Builders one.

- **A recipe caps its crafters per item.** Each recipe states how many hands can work one item at once
  (a spear 1, a sled 2 — a per-recipe config value). An order may take that many per item it still
  has to make, so a batch (*Baskets ×2*) works its items side by side, and twenty crafters never land
  on one spear. Issue #852 carries the recipe values.
- **Orders are staffed the way builds are.** One crafter on every workable order first, at any
  priority; then round-robin within High up to each order's cap, then Normal, then Low.
- **Each order carries a priority**, and it ranks against sources on the one priority list. So a High
  craft order can take kitted workers ahead of a Normal source's reach crew — some kitted workers are
  spared for the crafting the player asked for.
- **An order's materials are set aside when it starts** — the turn it gets its first crafter. Two
  orders drawing on the same fibre can then never both stall halfway; the ones still waiting show
  what they are short of. The pile shows how much of it is set aside for the bench.
- **An order short of an input is not workable and takes nobody**, exactly as the bench skips it
  today.
- **A bench tool is held by one crafter.** One loom means one weaver works at the loom's quality; a
  second weaving order works by hand, at the material's hand-working rate and its lower result. The
  loom goes to the higher priority.
- **The bench fills itself only when crafters are free**, and only with the gear the band is shortest
  of — `auto_craft_fill` must not flood a bench that can run several orders.
- **The bench has no hold crew.** It is staffed in pass 2 (and pass 3 where bare hands help), never
  pass 1. A bare-handed crafter crafts, so the bench is a pass-3 destination.
- **"Kitted" for the bench means holding its bench tool** (the tanning frame, the awl, the billet).
- **On screen, each running order shows the work strip's crew bar**: crafters at the bench tool, by
  hand, and the room left under the order's cap. The work figures move into tooltips.

### Pass 3, and what is left over

Pass 3 is one pass, in priority order, over **every job where a bare-handed worker adds value** — the
bench, the builds and bare-handed sources (foraging is the main one), each up to its useful crew.
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
the game picks the sites. The target is set on the material's row on the Work tab, since it is work.

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

The player sets the band's **scouting range** — how far it sees — on the band side of the Band panel;
the sim staffs the scouts that reach it. That is what adding
scouts does today, and a range stays right as gear improves — a better wayfinding kit extends each
scout's vantage, so fewer scouts are needed for the same range.

### Warriors: no standing count — defenders are chosen in the battle

There is no standing warrior count for the sim to staff. Who defends is chosen when a battle happens,
in the battle's own flow, and the defenders leave their work for as long as the battle runs and go back
to their jobs afterwards. A work party past the camp's range is not in camp and cannot defend.

How a battle runs — who chooses, in whose turn, how it ends — is `docs/plan_battles.md`.

### What #465 becomes

[#465](https://github.com/rwalker123/falcon/issues/465) proposed a min/max worker range per source,
staffed by the sim. This design absorbs it: its *min* and *max* are the floor's two crews, computed by
the sim rather than set by the player, and its open question — *which source gets a newly-freed worker
first* — is the priority list. The player-facing **max** above is the surviving half of its range.

## Leaders: four levels of orders (decided)

Per-source settings stop the player's work growing with each new worker, but not with each new band
or settlement. Leaders do that. Each level of the people's organisation gets a leader, the player
gives that leader goals, and the leader turns them into the settings one level down. The levels are
the ones the civilization ladder (`docs/plan_civilization_steps.md`) already climbs, so each milestone
that today only changes a label or an icon now changes what the player manages.

### The four levels

| Level | What it is | How it starts | Who leads it |
|---|---|---|---|
| **1. Band** | One band, as today | The band's people accept direction once directed work succeeds (below) | a **chief** (Hierarchical) or an **elders' council** (Egalitarian) |
| **2. Tribe** | Bands linked by the supply network — those within reach that already pool goods | The linked bands share a **gathering place** they walk to and work on | a **paramount chief** or a **council of chiefs** |
| **3. City** | A settled cluster large enough to carry the *city* label (#176), still independent | Its first administered store — a granary and the rations drawn from it | a **ruler** or an **assembly** |
| **4. Nation** | Cities, tribes and bands united as one faction | Union by conquest or confederation (civilization steps 8–11, #695) | a **faction leader** — a king or a high council |

- **Every people reaches every level.** Hierarchical or Egalitarian changes the kind of leader, not
  whether there is one. A Hierarchical leader carries orders out fully and tolerates long stretches
  of directed work that feeds no one; an Egalitarian council carries out the same orders and grows
  restless sooner under unfed directed work — the existing step-5 rule. So there is no dead end, and
  the culture still costs something.
- **A leader is a named person.** The Telling narrates them (*a chief, then a line of chiefs*).
  Leaders carry no stats yet.
- **Before a nation, the player commands each city and tribe directly.** There are no orders above
  them until level 4 exists.

### How a chief emerges: directed work that succeeds

A band comes to accept a chief by watching directed work succeed. Every way of playing has some:

| Way of playing | Directed work that counts |
|---|---|
| Settled foragers and farmers | a finished build — the first Cultivate, Tame or Corral |
| Migratory hunters | a big-game hunt that lands its kill — a party against an aurochs or a mammoth, through the hunt's fight |
| Anyone | a battle won (`docs/plan_battles.md`), or work on a gathering place's monument |

Each success adds to one practice count, the way the knowledge ledger teaches a lesson by doing.
When it fills, the band has a chief or a council. A migratory people needs no settling for either
level: its chief comes from its hunts, and its tribe forms at a gathering place on its round —
which is what the gathering place was for foragers in the first place
(`docs/plan_civilization_steps.md` → "The contact chain").

### What each level's orders are

The player gives orders at the highest level they have. A change made by hand at a lower level stays
pinned until the player releases it.

**1. Band orders — the chief.** The player sets goals; the chief sets each source's floor, priority
and builds.

- **Food:** *keep N turns of food.* Below it the chief draws food sources harder; above it, eases them
  toward Best or Learning.
- **Materials:** keep-on-hand targets — stone already works this way; wood and hide join it.
- **Land:** *improve the land.* The chief starts rung builds where they pay best, within a share of the
  workers the player sets.
- **Scouting range** and the self-filling bench, which already exist.

Before a chief, the band runs on the per-source settings in "Decided" above.

**2. Tribe orders — the paramount chief.**

- **Shared workers.** Spare hands in one band work another member band's sites, bench and builds
  within reach. A band staffs its own work first; only what is left over goes to the tribe, so shared
  work never takes from a band's keeping or its food. Working another band's site is a walk there and
  back, priced the way a far source is today.
- **A tribe bench.** Materials & Crafting's band selector gets the tribe as an entry. Orders queued
  there are worked by spare hands from any member band, drawn from the network's pooled materials, and
  the gear goes to whichever band is short of it.
- **Shared works** — a granary or a road serving several bands, built by hands from all of them.
- **Who comes first when the network runs short**, in place of today's automatic pooling.
- **One set of band orders for every member band.**

**3. City orders — the ruler.**

- **Trades:** a share of people working full time as crafters, traders and builders. This is where
  idle people become traders and diplomats.
- **Great works** — walls and monuments — fed from the granary.
- **Rations:** handing out what the stores hold.

**4. Nation orders — the faction leader.** The work #598 describes:

- **Tax:** a share of each city's and tribe's product flows up to the faction.
- **Redistribution** from a full city to a hungry one.
- **Works between cities** — the roads that link them.
- **Trade terms, tariffs and diplomacy.**
- **Calling defenders** to war.

### A leader can only run so much well

- A chief runs one band, up to the cohesion ceiling (~150 people).
- A paramount chief runs so many bands — a config lever.
- A ruler's and a faction leader's reach is the **management capacity** writing lifts
  (`docs/plan_civilization_steps.md` → "Writing: the third ceiling caps management").

Over its capacity a leader carries orders out worse — lossy, never a refusal — the same shape as
writing's miscount and the other ceilings.

## Open questions

None open in this doc. The battle flow that decides who defends is `docs/plan_battles.md`'s; each
recipe's crafter cap per item is #852's (crafting arc #840); whether a tile's cap also counts the
people living on it is the settlement design's.

## Related

- [#465](https://github.com/rwalker123/falcon/issues/465) — labor standing orders (absorbed, above)
- [#709](https://github.com/rwalker123/falcon/issues/709) — the granary as a network node
- [#598](https://github.com/rwalker123/falcon/issues/598) — the faction as central government (the nation level)
- [#176](https://github.com/rwalker123/falcon/issues/176) — settlements as derived clusters (the city label)
- [#695](https://github.com/rwalker123/falcon/issues/695) — borders, absorption and the two routes to one polity (how a nation forms)
- `docs/plan_civilization_steps.md` — the ladder the four levels follow: consent to direction, the
  gathering place, writing's management capacity
- `docs/plan_battles.md` — a battle won is directed work that succeeds
- [#798](https://github.com/rwalker123/falcon/issues/798) — hunting by need: the one row type where the
  sim already sizes the crew itself
- `docs/plan_harvest_floor.md` — the floor and the two crew targets
- `docs/plan_pool_toe.md` — the band's pooled gear and how scarce items are settled by Priority
