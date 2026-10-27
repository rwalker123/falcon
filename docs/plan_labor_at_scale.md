# Plan: Labor at Scale — the player sets the floor, the sim staffs the crew

Status: **design.** The spec for [#842](https://github.com/rwalker123/falcon/issues/842). Parent arc:
[#179](https://github.com/rwalker123/falcon/issues/179) (Early-Game Labor).

It amends two earlier specs, and where they disagree this doc wins: `docs/plan_harvest_floor.md`
(the crew stepper is retired — a source takes a floor and an optional max, and the sim staffs it) and
`docs/plan_site_crews.md` §2.4 (the build queue and its Build mark are retired — builds join the
site's crew and rank by the site's one Priority).

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

**Scouts come off the top.** The scouting range (see "Scouts: a range, not a count") is an explicit
player order, so it is staffed before pass 1 — scouts with wayfinding gear before bare-handed ones —
and every pass staffs from what is left. When people are short, the sources go short, not the range.

**Within a priority, sources take turns, nearest first.** Each pass deals one worker at a time to
every source in the priority group that still wants one, in order of distance from camp (ties keep
the order they were added), until the group is full or the people run out; only then does the next
priority get that pass. Two equal sources split the band evenly, and an odd worker goes to the nearer
one. Distance never drops a source: the player asked for it, so a far source is dealt every round,
just after the nearer ones. Each source's Max and each pass's crew still bound what it gets.

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

## Decided — builds, the bench, and what each kind of work takes

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
- **One builder on every build, ranked by its site's priority group.** Keepers come first on every
  row at every priority (keeping an improvement from sliding down its ladder is the claim that
  never rots to pay for new ground). Then, per priority group High to Low, each of the group's
  builds that can make progress gets one builder, then the group's hold crews. A hold crew is a
  food take, not rot protection, so a Normal row's hold crew does not beat a High build, and a
  Low build still gets its builder before the Low hold crews.
- **A crew rounds once.** The keeper keeps first and gathers with what is left, so a row's heads
  are `ceil(keep_hands + gather hands)`, not `ceil(keep) + ceil(gather)`; the fraction of a hand
  of keeping rides inside a gatherer's head. A row with nothing to take (a tile or band
  improvement, a source at its floor) gets `ceil(keep_hands)` heads. `keepersStaffed` counts the
  heads whose turn is entirely keeping (`floor(keep_hands)`, plus one when the row has no other
  head).
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
  on one spear. The value is each recipe's `crafters_per_item` in `core_sim/src/data/recipes.json`
  (#852): 1 for a thing one person holds and shapes (spears, clubs, hoes, snares, the awl), 2 for a
  thing a second pair of hands holds while the first lashes or weaves (the sled, baskets, hurdles, the
  tanning frame, the loom, the road tool sets).
- **Orders are staffed the way builds are.** One crafter on every workable order first, at any
  priority; then round-robin within High up to each order's cap, then Normal, then Low.
- **Each order carries a priority**, and it ranks against sources on the one priority list. So a High
  craft order can take kitted workers ahead of a Normal source's reach crew — some kitted workers are
  spared for the crafting the player asked for.
- **Materials are set aside per item, all or nothing.** An item's inputs are set aside the turn a
  crafter starts it, and only when every one of them is on hand — never part of an item. A batch
  starts the items it can supply and sets aside the next item's inputs when that item starts, so a
  large order never waits on materials for items nobody is working yet. Because nothing is ever held
  half-supplied, two orders can never each hold what the other needs and deadlock; an item that cannot
  be supplied holds nothing and waits, showing what it is short of. Cancelling an order returns what
  it holds. The pile shows how much of it is set aside for the bench.
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

**The range is counted from the edge of the band's local range, not from the camp.** The local range
is the band's own neighbourhood — `reach_tiles` in `supply_network_config.json`, 3 today, the reach
within which nearby bands already pool — so a range counted from the camp spends its first tiles on
ground the band already holds. The setting reads **+N tiles past the local range**: +0 is no scouts,
+2 is two tiles beyond it. The local range is a fixed edge read from that one setting, so the number
means the same distance every turn — unlike the band's own sight, which moves with elevation, line of
sight and size. A tribe counts from the outer edge of all its bands' local ranges together (below).

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
| **1. Band** | One band, as today | The band does enough work that needs an organiser (below) | a **chief** (Hierarchical) or an **elders' council** (Egalitarian) |
| **2. Tribe** | Bands linked by the supply network — those within reach that already pool goods | The linked bands share a **gathering place** they walk to and work on | a **paramount chief** or a **council of chiefs** |
| **3. City** | A settled cluster large enough to carry the *city* label (#176), still independent | Its first administered store — a granary and the rations drawn from it | a **ruler** or an **assembly** |
| **4. Nation** | Cities, tribes and bands united as one faction | Union by conquest or confederation (civilization steps 8–11, #695) | a **faction leader** — a king or a high council |

- **Every people reaches every level.** Hierarchical or Egalitarian changes the kind of leader, not
  whether there is one. A Hierarchical leader carries orders out fully and tolerates long stretches
  of directed work; an Egalitarian council carries out the same orders and grows restless sooner
  under directed work — the consent rule (`docs/plan_civilization_steps.md` → "Directed labor and
  consent"). So there is no dead end, and
  the culture still costs something.
- **A leader is a named person.** The Telling narrates them (*a chief, then a line of chiefs*).
  Leaders carry no stats yet.
- **Before a nation, the player commands each city and tribe directly.** There are no orders above
  them until level 4 exists.

### How a chief emerges: work that needs an organiser

A band comes to need a chief by doing things no one can keep in their head alone. Each such thing
adds points to the band's own count; when the count reaches the threshold the band has a chief or a
council, for good. Success is not what counts: a band that spears a horse it happened to spawn
beside has organised nothing.

| What the band does | Why it needs an organiser | Points |
|---|---|---|
| Finishes a Cultivate, a Farm, a Tame or a Corral | someone sets who works the patch or the herd, and when | **3** each |
| Splits — the player orders it | someone decides who goes and what they take; the leavers need someone to lead them away | **3**, to **both** bands |
| Walks with a followed herd | moving the whole band and its gear after a herd — every hex counts, the walk to reach the herd included | **1** per hex |
| Reaches a settlement stage (Seasonal camp at 50, Village at 100) | more people, more to manage | **5**, the first time each stage is reached |

The threshold is **10**. Every number is a lever in `leadership_config.json`, tunable in the
Workbench; these are opening values for playtest.

- **No single act makes a chief.** The largest single award is a stage, at 5.
- **The count is the band's own, not the people's.** Leadership as a learned skill belongs to the
  nation level, where it is a people's knowledge; a band's chief comes from what that band does.
- **Every finished intensification build counts, Farm and Corral included.** A pen's extension ring is
  fencing, not a new rung, and pays nothing; roads, felling, coppicing and quarrying are not directed
  work and pay nothing.
- **A stage pays once.** A band that shrinks below a stage and grows back is not paid again.
- **What it takes in play** (opening values): a band that splits twice on its first turn holds 6 and
  needs two builds; each splinter holds 3 and needs three builds or seven hexes of herd. A band that
  never splits needs four builds, or three and a stage. A migratory band that never splits needs
  about two legs of its herd's round.
- **A band that does none of it never gets a leader.** It is doing nothing a leader is for.

A migratory people needs no settling for either level: its chief comes from following its herds, and
its tribe forms at a gathering place on its round — which is what the gathering place was for
foragers in the first place (`docs/plan_civilization_steps.md` → "The contact chain").

### How a tribe forms: one network, one gathering place (#861)

The gathering place is the band's **belief anchor** — the urn on the map, the strongest belief tile
the band has stood within reach of (`.claude/rules/core_sim/belief.md`). Deaths and gatherings both
raise it.

**The conditions.** Bands can only be a tribe when they are of one faction, linked in one supply
network, and hold the same anchor — and at least one of them has a chief. Being linked is a
condition, not a score: it is what "local" means.

**The closeness score.** How close two bands are is a score, the way a chief's practice is, but read
fresh each turn rather than banked. Each input reads 0 to 1 and carries a weight; the pair's score is
the sum. Every weight is a lever in `leadership_config.json`, tunable in the Workbench; these are
opening values.

| Input | What it is | Weight |
|---|---|---|
| Shared origin | the two descend from the same starting band — split from one another or from a common ancestor | 3 |
| Culture likeness | how alike their cultures are now; bands in touch drift together (#702), bands apart drift apart | 3 |
| Knowing each other | the existing tie between them, which fills with contact and is forgotten over about 50 quiet turns | 2 |
| Shared history | how long they have lived local to each other **with a chief among them**: builds over about 25 turns while the pair is local and at least one of the two has a leader, holds while local with none, fades over about 100 apart — history is not erased, it is forgotten | 5 |
| Belief on the shared urn | a stronger sacred place pulls harder | 1 |
| Acting as a tribe | work done at the tribe level — the shared pool (#862), the tribe bench (#864), shared works; only a tribe has it, so a working tribe holds together | 3 |
| A kept monument at the urn | the plainest case of a place they walk to and work on (#692); 1 while it stands kept, 0 half-raised or decayed | 2 |

- **A tribe forms at 9 and dissolves below it.** A band joins when its score with a member reaches the
  line, and leaves when it falls under.
- **What it takes in play** (opening values): a fresh split scores about 8.5 — one people, identical,
  well known to each other — so it is a tribe after a few turns of shared history, and shared
  history only starts building once one of the two has a chief: until then nothing counts toward a
  tribe. Two unrelated bands, even alike, need most of the history. Bands that part keep their
  history a while, then forget each other and drift apart, and the score falls on its own.
- **A band needs no chief of its own to join.** A member without one carries the tribe's orders out
  worse — the lossy shape of "A leader can only run so much well" below.
- **The tribe's leader is that chief, promoted** — a band's chief becomes the paramount chief, an
  elders' council becomes the council of chiefs; with several, the largest band's. If the leader's
  band leaves, the next chief among the members takes over; with no chief left the tribe dissolves.
- **Identity survives a split.** When a network splits, the side holding most of the old members
  keeps the tribe; any other side that meets the conditions and the line is a new tribe. When two
  tribes come together, the one with more of its members there keeps it.
- **The tribe is named for its largest band** at formation ("Reedwater tribe").

These are opening rules for playtest.

### What each level's orders are

The player gives orders at the highest level they have. A change made by hand at a lower level stays
pinned until the player releases it.

**1. Band orders — the chief.** The player sets goals; the chief sets each source's floor, priority
and builds.

- **Food:** *keep N turns of food.* Below it the chief draws food sources harder; above it, eases them
  toward Best or Learning.
- **Materials:** keep-on-hand targets — stone works this way (see "Two interfaces" above); wood and
  hide join it.
- **Land:** *improve the land.* The chief starts rung builds where they pay best, within a share of the
  workers the player sets.
- **Scouting range** (see "Scouts" above) and the self-filling bench (`auto_craft_fill`, which ships
  today).
- **Each order is a number or None.** A number starts at the chief's suggestion — about a season of
  food, keep-on-hand targets sized to what the band's builds and upkeep will draw, a modest share of
  workers on improvements — and the player moves it from there. **None** gives no order at all: the
  chief pursues nothing for it. Food None aims for no particular store, so the food rows keep their own
  floor; a keep-on-hand None sends nobody for that material; Improve None starts no builds.
- **Each number is set with the game's ordinary − / + stepper**, never by clicking a chip to cycle it.
  The stone row's keep-on-hand chip on the Work tab is a label; the target is set on the Band orders
  card (or, before a chief, on the Stone sheet).
- **A row the player sets by hand is pinned**: its Floor chip reads *yours*, the chief leaves it alone,
  and *Back to the chief* releases it.

Before a chief, the band runs on the per-source settings in "Decided" above.

**2. Tribe orders — the paramount chief.**

- **One pool of people.** Once bands form a tribe, all their working-age people are one pool and the
  paramount chief distributes them — the same three passes, over one priority list holding every
  member band's sources, builds, benches and the shared works. No band staffs "its own work first";
  a worker sent to another band's site walks there and back, priced the way a far source is today, so
  distance still keeps most people near home.
- **No band refuses.** Under a tribal leader a band may push back (grievance, the governance axis), but
  it never withholds its people; refusing outright is a band leaving the tribe. Whether a band can
  refuse particular work is left to playtest.
- **A tribe bench.** Orders queued there are worked by any member band's people, drawn from the
  network's pooled materials, and the gear goes to whichever band is short of it ("→ Greyfen"). A
  bench tool serves the tribe within reach the way goods do — one loom is still one weaver at a time.
- **Shared works** — a granary or a road serving several bands — are rows on the tribe's one list,
  kept and built from the one pool.
- **Who comes first when the network runs short**, in place of today's automatic pooling.
- **One set of band orders for every member band.**

**The tribe on screen.** The tribe *replaces* its bands in the Band panel's band switcher and in
Materials & Crafting's picker — one entry, "Reedwater tribe · 3 bands", so a grown people does not
list every band. It is the same panel, rows and work strip, rolled up:

- **Work tab:** one collapsed line per member band — its name, crew bar and idle count — then the
  shared works. Opening a band's line shows its rows exactly as on the Band panel.
- **Drilling into a band is one click.** A band's name on its line (or the band on the map) opens that
  band's own panel, with a breadcrumb back (*Reedwater tribe › Greyfen*); while drilled in, the
  switcher's ◀ ▶ step through the tribe's member bands.
- **Crafting:** On hand is the network's pooled materials (each pile's tooltip names who holds it);
  Make next looks across the bands (*Spears ×5 — Greyfen 3, Pinewold 2*).
- **People bar:** the one bar, totalled across the tribe, idle included.
- **Starting a shared work uses the card that already exists.** The tile card's branch actions
  (`Road ▸`, and each storage branch the same way) open the same rung card with the same `Band:`
  picker; once there is a tribe, the tribe stands in that picker in place of its member bands, as it
  does in the band switcher, and is the default. A rung the tribe starts lands in its Shared works.
- **A member band can still be picked**, folded under the tribe (`Reedwater tribe ▸`) so the list stays
  one line per tribe until the player opens it. The default is the tribe, unless the card was opened
  from one of its bands' own panel, which defaults to that band. A work that belongs to one band's
  camp is that band's: it sits on that band's line in the tribe view, and its own people are sent to
  it first, before anyone walks in from a neighbour. Assigning harvesters uses the same picker.
- **One scouting range for the whole tribe**, counted out from the outer edge of all its bands' local
  ranges together — the scouts ride the tribe's whole border, not a ring round each camp. It is set on the tribe's
  panel; a member band's own panel shows it read-only ("set by the tribe"). Scouts are a standing duty
  from the one pool, drawn from the band nearest each stretch of the edge, so a longer border needs
  more scouts for the same range.

**3. City orders — the ruler.**

- **A city grows out of a tribe and replaces it**, the way a tribe replaced its bands: when the
  tribe's bands settle into one cluster that carries the *city* label and it runs its first
  administered store, the city takes the tribe's place in the band switcher, its bands one click down.
  A lone band can grow into a city the same way. **Inside a city a band is called a quarter**
  (*Greyfen quarter*, *3 quarters*) — the old word for a part of a city, each grown from the band that
  settled there.
- **Trades:** the player sets a share of working-age people (crafters, builders, traders) taken out of
  the pool for good. A full-time trade is better at its work than whoever is spare — a full-time
  crafter works faster and to a better result — and that is what pays for the hands the pool loses,
  since they never forage. Full-time builders go to great works first, then to the city's other builds.
  The people bar gains a Trades part.
- **Each trade has an Auto, on by default.** Auto sizes the trade to what the city needs — crafters
  to the bench's workable orders up to their crafter caps, builders to the great works' caps, traders to the trade routes' cargo — but only from people the food does
  not need: the hold crews and the scouts are taken first, so a great work never pulls the harvesters
  off the fields. **It moves each trade by at most one person a turn** toward that need, so a
  full-time trade stays a career rather than a turn's posting. Switching Auto off keeps the share the
  player sets.
- **Great works** — walls, palisades — are ordinary builds with a far larger builder cap, started from
  the same rung card. (The monument is not one: it is a band's tile rung, `belief:monument`, built by
  a crew of three — `docs/plan_civilization_steps.md`.)
- **Rations:** a ration level on the city's food — Full, Short, Starvation. Short stretches the
  granary at a cost in morale; Starvation stretches it further and costs health too. It is the choice
  before a hard winter or during a great work.

**4. Nation orders — the faction leader.** The work #598 describes:

- **Your people become a nation by confederation:** once your members — at least a city and one other
  member — share writing under a faction leader (`docs/plan_civilization_steps.md` → "Step 11 for
  your own people"). Conquest of other peoples is a later route (#695).
- **The Faction page becomes the nation page.** Its rows are the nation's members — cities, tribes,
  bands — each opening its own panel. The band switchers below it are unchanged.
- **The nation does not pool people.** Each member keeps its own pool; the nation moves goods, not
  people. **A national work** — a road between cities — is handed to the nearest member and shows in
  that member's works marked *for the nation*: the forced labour early states ran on.
- **Tax is in kind, and Auto.** There is no money; the first states taxed grain, goods and labour, and
  so does this one. Each turn the nation collects only what that turn's aid and national works need,
  plus a step toward a reserve for a bad year — nothing beyond, so its stores never pile up unused. It
  takes from each member's **spare** goods (food, hide, wood, stone — what the member has beyond its
  own needs), never from what it needs to eat, so a member that only just feeds itself pays nothing
  and the stores never hand a member back what they just took. The player's one control is a
  **ceiling** (None / Low / Normal / High): the most it may take from any one member. A low ceiling is
  easy on morale and may leave aid and works short, which the page says. A member can be exempted.
  Tax costs morale in the members it is taken from, in proportion to how much of their spare it takes.
  Labour is taxed too: that is the national works handed to members. Issue #853 builds it.
- **What will raise what tax collects** are each their own design, filed: trade with other peoples
  (#854), arming the defenders a battle calls up (#855), officials who stretch the leader's reach
  (#856), and whether money ever replaces tax in kind (#857).
- **Aid between members:** Auto by default — the nation sends from its stores to members whose own
  food falls short. The one setting is a **help-first order** for when there is not enough for all.
- **The leader's reach is on the page** (*Queen Asha runs 4 of 5 members well*). Past it, orders are
  carried out worse, never refused: less of what tax takes reaches the stores, and aid loses some on
  the way.
  Writing raises the reach.
- **Not in the first nation page:** trade terms, tariffs, diplomacy and calling defenders. They wait on
  other peoples existing (#513) and on the battles design (#850).

### A leader can only run so much well

- A chief runs one band, up to the cohesion ceiling (~150 people).
- A paramount chief runs so many bands — a config lever.
- A tribe member band with no chief of its own carries the paramount chief's orders out worse, the
  same lossy shape as over-reach (#870).
- A ruler's and a faction leader's reach is the **management capacity** writing lifts
  (`docs/plan_civilization_steps.md` → "Writing: the third ceiling caps management").

Over its capacity a leader carries orders out worse — lossy, never a refusal — the same shape as
writing's miscount and the other ceilings.

## Open questions

None open in this doc. The battle flow that decides who defends is `docs/plan_battles.md`'s; whether
a tile's cap also counts the people living on it is the settlement design's.

## Related

- [#465](https://github.com/rwalker123/falcon/issues/465) — labor standing orders (absorbed, above)
- [#709](https://github.com/rwalker123/falcon/issues/709) — the granary as a network node
- [#858](https://github.com/rwalker123/falcon/issues/858) — the arc that builds "Leaders: four levels of orders" (#859–#870)
- [#598](https://github.com/rwalker123/falcon/issues/598) — the faction as central government (the nation level; its tax is #853)
- [#176](https://github.com/rwalker123/falcon/issues/176) — settlements as derived clusters (the city label)
- [#695](https://github.com/rwalker123/falcon/issues/695) — borders, absorption and the two routes to one polity (how a nation forms)
- `docs/plan_civilization_steps.md` — the ladder the four levels follow: consent to direction, the
  gathering place, writing's management capacity
- [#798](https://github.com/rwalker123/falcon/issues/798) — hunting by need: the one row type where the
  sim already sizes the crew itself
- `docs/plan_harvest_floor.md` — the floor and the two crew targets
- `docs/plan_pool_toe.md` — the band's pooled gear and how scarce items are settled by Priority
