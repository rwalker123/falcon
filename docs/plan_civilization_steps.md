# Plan: The steps to a civilization — resistance to settling, the pulls that beat it, and one work party

Status: **design dialogue captured, not a spec.** Filed under arc #682 (early steps) and placeholder arcs #693, #695, #696; nothing is built. Decisions marked
*leaning* are where the discussion landed; *open* is where it did not.

Source: *How Did Humans Invent Countries?* (Neon Rush, 29 min, published 2026-07-31,
https://youtu.be/vh2TNc7ASiw). Timestamps below refer to its captions. Discussion 2026-09-17 to 19.

## Why this doc exists

The video lays out how countries formed as a ladder of layers, each one solving a scale problem the
previous one created. Steps 1–5 of that ladder are all pre-city and mostly pre-farming, which is the
stretch our game currently plays. The game starts at roughly step 4 (the verge of farming) and has
no reason to walk around: moving was a design goal early on, but in play it never paid. The point of
this doc is to record how steps 2–5 supply what the early game is missing — a **push** that makes
staying costly and **pulls** that are not economic — and the hunting change that fell out of the
same conversation.

## The video's ladder

| # | Step | The video's mechanism | Game translation |
|---|---|---|---|
| 1 | The band | 20–50 related people, no territory; you belong to people, not ground. Cohesion runs on everyone knowing everyone. Ceiling ≈150 stable relationships (Dunbar, 1992). Above it gossip and reputation stop working. [01:05–02:21] | Our start state: one ~30-person band. The cap is the reason the first hard decision exists. |
| 2 | Shared belief before settlement | Göbekli Tepe, ~9500 BC: scattered hunter-gatherer bands gathered to build a monument, with no farming and no village. "First the temple, then the city." Belief is the first technology that lets strangers cooperate past the cap. [04:51–06:33] | A gathering place several bands contribute to before anyone settles. The first thing that belongs to more than one band. |
| 3 | Settling, and burying the dead | Natufians, ~12,500 BC: year-round villages with storage pits and cemeteries, still hunter-gatherers. Settling turns land from a thing you pass through into a thing you invest in. Graves are the first territorial markers. [02:59–04:38] | Settlement is already an emergent label of place-bound improvements. The video adds a tether that is not an improvement: the ancestors in the ground. |
| 4 | Grain, because it can be counted | Scott, *Against the Grain*: first states formed around wheat and barley because grain ripens at once, stands visible, and can be measured, stored, moved — taxed. Tubers cannot be assessed. [06:41–08:20] | Legibility is a property of the food source. We are judged to have this right: food is the currency and the early economy is built on it. |
| 5 | The granary, and who holds it | Dhra, ~9300 BC: a raised-floor granary. Control of stored surplus is the first real power; it is what lets an ambitious individual override the leveling mechanisms (ridicule, gossip, ostracism) that kept bands egalitarian. Temporary prestige → hereditary rank → king. [09:39–11:01] | The larder exists as a stock. The political step is *who controls it*, and the band's egalitarian pressure resists until the surplus is large enough to win. |
| 6 | Walls | Jericho, ~8000 BC: ~11,000 person-days of directed, fed labor. Administration is "the skeleton of a country." [08:22–09:33] | A work-costed improvement so large it needs a coordinated workforce. Unit-costed work already prices this. |
| 7 | Writing as accounting | Uruk, ~3400 BC, 40–80k people: >85% of the earliest tablets are ration lists and inventories. Standard ration bowls are a payroll. [11:08–13:31] | The second cap-raiser after belief: record-keeping as a knowledge unlock that lifts how many one polity can administer. |
| 8 | Formal borders | Lagash vs Umma, ~2500 BC: a surveyed, inscribed, cursed boundary, and a war when it was crossed. [13:32–14:34] | A claimed boundary another polity can recognise or contest; the first step that is a relationship between polities. |
| 9 | Circumscription | Carneiro, 1970: where fertile land is hemmed in, war's losers cannot leave and are absorbed. Village absorbs village; a state results. Where land is open, losers walk. [14:51–15:58] | Our scarcity pillar as a state-formation theory. Geography decides whether escape exists. |
| 10 | Violence before borders | Jebel Sahaba, Nataruk: repeated killing over rich shores millennia before any border. [16:00–17:20] | Contact and denial-raid plans already exist. Fighting over a food-rich tile precedes any claim. |
| 11 | Two routes | Egypt unified by conquest. The Indus Valley possibly by standard bricks, weights and guilds, with no palaces or armies. [17:20–19:30] | A confederation path (shared standards) beside a conquest path. |
| 12 | Status → contract → nation | Everything after: sovereignty, print, conscription, lines drawn on maps. A nation is a story enough people believe for long enough (Renan, Anderson, Gellner). [19:31–29:09] | Beyond our horizon, but its framing is The Telling. |

All twelve are in scope eventually. The rest of this doc is about steps 1–5.

## Where the game stands, in the ladder's terms

- **The tether is real but one-sided.** `docs/plan_settlement_population.md` already defines
  sedentarization as accumulated *tether*: the cost of walking away from built improvements and
  stored surplus, with decay making the sunk cost bite. Every tether we have is economic, and nothing
  pushes back. There is no resistance to settling, only a pull — which is why moving never paid.
- **"Local bands" are not separate bands.** A split that stays within the supply-network reach
  (`supply_network_config.json` → `reach_tiles`, currently 3 hexes) is part of your band hunting or
  foraging "over there"; it did not leave. `docs/plan_band_fission.md` already says the rest:
  independence is earned by **disconnection + grievance over time, never distance alone**, with the
  supply network's connected components as the disconnection signal. A band that drops off the
  network and stews is the one that drifts. (Worth stating in the fission doc in these plain terms.)
- **Food is the keeper today.** Carry capacity caps a nomad; storage is the way past it
  (`docs/plan_early_game_labor.md`). The video does not contradict this; it adds a second keeper.

## The loop for steps 1–5 (leaning)

**A band alone cannot grow, so it walks to meet others. Where bands keep meeting, belief accrues
and the dead go into the ground. Standing still depletes the patch, but the partners and the
ancestors tether you anyway, and neither feeds anyone. Each ceiling stops growth until the next
lifter — contact, belief, writing — raises it. The ladder turns the patch into fields, the surplus
becomes a place, and controlling it is the first act of politics.** Every step is a reason to stay
fighting a reason to go.

### The push is depletion, and we already own it

A band that stands still eats its patch down. The intensification arc models this as actual vs
sustainable income on every source row. The resistance to settling is therefore not a new system: it
is the existing overdraw made to bite on a stationary band *before* the tether forms. Why moving did
not pay in play has **not been measured** — the guess is that depletion is slow enough and land
uniform enough that a band never feels the patch thin under it. Measure before tuning.

### The first pulls are not productive

The Natufians and the Göbekli Tepe builders stayed for reasons that produced no food. Our tether
list (improvements + stored surplus) is all labor. Two tethers accrue from **time and events**
instead, which is exactly why they can come before farming:

- **Ancestors in the ground.** Every death while the band stands on a place adds to that tile's
  claim. Costs nothing, cannot be moved, invisible to the economy.
- **A gathering place.** A site several bands walk to and contribute labor to. It is a cultural pull
  where the "local bands" cluster is a food pull.

### Gamifying belief: a property of a place, on two existing channels

Belief is not a resource you spend. It is a value a **tile** accrues (from deaths buried there, from
gatherings held there) that changes what the band will tolerate. It needs no new subsystem because
two channels already move people:

- **Leaving costs morale.** A band that walks away from its dead carries a negative morale term
  scaled by how much is in the ground. The numbers say go; the people say stay. Morale already
  drives discontent, productivity, migration and grievance, so one lever does every job.
- **Standing on it raises the cap.** Above the cohesion cap (150 in the video; a config lever for
  us) a band loses people to the surroundings. Belief on the tile you stand on lifts that cap. That
  is how a cluster of local bands around a site becomes a settlement larger than any one band,
  instead of a band with satellites. Writing (step 7) is the second cap-raiser, much later.

The mechanism for both is in "Belief and cohesion" below.

Food stays the keeper of whether you *can* stay. Belief becomes the keeper of whether you *want*
to, and of whether the place can hold you once you do.

### The contact chain: what comes before belief (leaning)

The video's two early examples are three thousand years apart in the wrong order for its own
thesis: the Natufian cemeteries (~12,500 BC) come *before* the Göbekli Tepe gathering (~9500 BC).
Its claim is causal, not chronological — the desire to gather pulled people into staying — and the
claim holds once the burial half is corrected.

**Burial did not start with settling.** People buried their dead long before any village. A nomadic
band buries someone where they die and walks on. What is new with the Natufians is the *cemetery*:
the same dead in the same ground, under or beside the houses, again and again. A cemetery is
evidence of **returning**, not of grief. So the chain runs:

1. **Foragers return to the same rich spots on a circuit.** Neither nomadic nor settled.
2. **Bands aggregate at those spots**, for things a band of ~30 cannot supply itself: mates above
   all (a band that size is not viable alone over generations), alliances for bad years,
   information, goods from far away. Seasonal aggregation is documented among foragers everywhere.
3. **Ritual is what makes the aggregation work.** Strangers who share no kin need something else to
   trust each other — a feast, a rite, a monument built together (Göbekli Tepe shows heavy
   feasting). Belief is a *consequence* of gathering, not its cause.
4. **The dead go into the ground where everyone is together.** The aggregation site becomes where
   the ancestors are. That is the tether.
5. **The circuit shrinks around it, then stops.** The Natufians got there first because their site
   also had the wild cereal stands.

Lack of movement is the *last* link. Belief comes before it, and before belief comes a **need a
band cannot meet alone**. That is the thing our sim lacks: a single band is completely
self-sufficient, so it has no reason to walk, gather, or split.

> The "not viable alone" claim reaches past the video into general anthropology and is **not
> verified**; the mechanism below holds even if the number is soft, but the claim should be checked
> before the spec leans on it.

#### The mechanism: a breeding population cannot grow past its lines

The need is **inbreeding**, so the unit that is capped is not the band but the **breeding
population**: everyone in contact. That is what the supply network already computes as a connected
component. "Faction limit" is right in practice, with one refinement — a far band that has dropped
off the network is its own breeding population even though it is still your faction.

The sim has three age brackets, no individuals, no sexes and no kinship, so relatedness cannot be
read from state. It is a proxy, and the proxy is **founding lines**: how many unrelated families a
group descends from.

- **Each band carries a count of lines.** The starting band has `L` (config), standing for its
  unrelated families.
- **A split takes a proportional share, minimum one.** Five out of thirty takes one or two.
- **A breeding population's ceiling is its lines × a per-line cap `K`.** Births stop there — the
  pattern carry capacity already uses (`docs/plan_early_game_labor.md`: population ≤ carry cap,
  births stop at the cap).
- **Contact merges line sets.** Each side gains the lines it lacks. That is what a gathering does.
  Contact with your *own* split band counts — that is what a tribe was, exogamy between bands of
  the same people — but a *recent* split shares every line and adds nothing.
- **Lines do not decay** in the plain version. The one refinement worth holding in reserve is that
  lines regrow slowly in a long-separated group, so contact after a long separation is worth more
  than after a short one. Not to be built first.

**The numbers, hedged.** From general population genetics and forager anthropology, not the video;
check before a spec leans on them. Roughly 50 effective breeders avoids short-term inbreeding
damage and ~500 keeps a population healthy long-term; effective breeders are about a third of
headcount. Forager mating networks ran around 500 people across many bands, and simulations put the
minimum self-sustaining network at ~175–475. Founder groups of 15–30 (Pitcairn, Tristan da Cunha,
Polynesian islands) grew to a few hundred over generations with visible inbreeding costs. A group
of 5 is two or three couples; every second-generation marriage is between first cousins.

**Our own clock.** `maturation_rate` 0.05 makes a generation ~20 turns; a well-fed band doubles in
~35 turns at the reserve and trend bonuses (`demographics_config.json`). A splinter of 5 reaches
30–40 in about three generations — exactly where real isolated founders start to hurt. The timing
matches the history without tuning.

With `K` chosen so a lone band of 30 ceilings near 150:

| Isolated breeding population | Founding lines | Ceiling |
|---|---|---|
| Splinter of 5 | 1–2 | ~25–30 |
| Starting band of 30 | `L` | ~150–200 |
| Three or four bands in contact | union | ~500 |

At ~500 inbreeding stops being the binding constraint and the next ceiling takes over.

**Two ceilings on two different units, and they coincide at 150.** A lone band's inbreeding
ceiling lands at ~150, the same number the video gives for where a band stops being able to run on
personal relationships and starts losing people. That is a coincidence in the sources, but in the
model it separates cleanly:

| Ceiling | Unit | Lifted by | Video step |
|---|---|---|---|
| Lines × `K` (inbreeding) | the **breeding population** — a connected component | contact | 1 → 2 |
| Cohesion (~150) | the **co-located group** — a band, or a cluster on one site | belief on the tile | 2 → 3 |
| Administration | a settlement | writing / record-keeping | 7 |

Contact lifts the *network's* ceiling but not any one band's: many bands of under 150 each, in
touch. Belief is what lets a single *place* hold more than 150. Writing is what lets a polity
administer more than a place can hold. For a lone band the two first ceilings are the same number,
which is why it reads as one cap until the band has partners.

**Shedding goes along routes, not into the void.** One rule, two edges: **births stop at the
ceiling; a group pushed above it sheds people.** A group goes above its ceiling only when a lifter
lapses — it walked away from its partners, or off the tile that held its belief — and then it leaks
back down. The people who leave **walk the network**: they go to a connected band, along a trade
route or the local reach, and join it. Nobody vanishes into the surroundings while a route exists.
Where no route exists (a truly isolated group above its ceiling) the fallback is open. This is the
resistance to leaving expressed in people rather than mood, and it may make the grievance-on-leaving
term above redundant; which to keep is open.

What falls out without further rules:

- **Splitting has a purpose, and staying connected is the point.** A lone band caps low. Split, and
  the cluster can grow past what one band could — limited by land, since each band needs its own
  work range. But a split that walks off the network takes its lines with it and *lowers both
  ceilings*: the splinter of 5 caps at ~25–30 and the parent loses what it gave away. The
  beneficial move is to keep the new band connected, locally or by a trade route, so the breeding
  population stays whole.
- **The gathering matters for far bands.** A band beyond reach has no standing contact; the
  gathering is the episode that renews it. The sim already has the contact primitive: the
  connection ledger (`.claude/rules/core_sim/connections.md`) gains a tie from presence in sight
  range and bleeds it over ~50 quiet turns; the gathering should reuse it, not add a second notion
  of contact. A far band that never gathers stops growing *and* drifts
  toward independence under the fission rule, from the same missing signal.
- **The gathering place is wherever bands keep meeting**, and belief accrues there. The dead go into
  that ground. The settling decision is then real: stay where the partners and the ancestors are, or
  keep walking the circuit that feeds you better.
- **Contact, not exchange.** Trade (shipments, network pooling) is a thing you can do at a gathering,
  not the gate. Gating growth on trade would make a food good stand in for a social need.
- **Scouting gets teeth.** The scout expedition is rarely used today because nothing depends on what
  it finds. Under a contact ceiling, the thing a scout finds is a **partner**: another faction's band
  once #513 lands, and until then where your own far bands are and where the circuit's rich spots
  (the future gathering places) lie. A scout that reports a band within reach of a route is the
  difference between a ceiling and growth.

Levers: `L` (starting lines), `K` (people per line), contact range, and what belief adds to the
cohesion ceiling. `L × K` must sit above the start size, or the game opens capped.

### Step 5 is a seam, not a design yet

Once the pulls hold a band through enough depletion that it climbs to fields, stored surplus is fixed
to a tile for the first time. The political event is not the granary but who controls it, and the
band's egalitarian pressure resists until the surplus overrides it. We carry per-cohort grievance
and stance vectors in The Telling; that is enough to say *a stored surplus above some level creates a
leader role, and grievance pushes back*. Not designed further.

## Belief and cohesion (leaning)

How belief becomes something the player feels. Three facts about the existing code and manual
shape it:

- **Religion is already a trait, not a system.** The manual (§7c) puts it on the culture axes —
  Secular↔Devout, Rationalist↔Mystical, Syncretic↔Purist — with sect mechanics as event packs.
  That stands. Belief is not a faith bar.
- **Morale has empty slots waiting.** `docs/plan_civ_wellbeing.md` reserved `culture`, `crowding`
  and `leadership` as morale contributors; nothing fills them. Morale already flows into
  productivity, migration and grievance, and the contributor list *is* the itemized breakdown the
  player sees.
- **Shedding along routes already exists.** `advance_population_migration` sends discontented
  people to the best same-faction band within reach (`.claude/rules/core_sim/campaign.md`,
  Layer 3b). The lineage and cohesion work feeds it; it does not build it.

### Belief is a property of a place

A tile accrues belief from three sources, in the order the contact chain gives them:

1. **Deaths** while a band stands there (the cemetery — evidence of returning).
2. **Gatherings** held there — the contact event from the lineage model.
3. **A monument** — a work-costed improvement whose output is belief, not food. The Göbekli Tepe
   move: it comes *before* walls (step 6), and it is the first improvement in the catalog with no
   yield, which is a new kind of thing for the catalog.

Belief does not decay. An abandoned place keeps its dead.

### Cohesion is a property of a group

A co-located group — a band, or the cluster on one site — has a cohesion ceiling (the video's
~150; config). Belief on the tile it stands on raises it. This is the second row of the ceiling
ladder, and it is soft: nothing stops at 150, the leak starts there.

### What belief does, through seams that exist

| Effect | Seam | What the player sees |
|---|---|---|
| Standing on or within reach of your belief is a positive morale term; being away is a negative one, scaled by how much is there | the reserved `culture` morale contributor | "far from the ancestors" on the morale breakdown |
| Above the cohesion ceiling, morale falls; belief on the tile raises the ceiling | the reserved `crowding` morale contributor | "crowded" on the breakdown; people leaving for a connected band |
| Honouring the dead and gathering push Devout and Traditionalist (the manual already names memorialization and ritual authority on those axes) | culture trait vector | The Telling's `sacred_secular` stance reads that axis today |
| Belief is an input to the tether beside stored food and improvements | `SedentarizationScore` | the `roam_settle` stance, the settle prompts |

Nothing new reaches the player directly. Belief changes morale, morale does what it already does,
and the breakdown names the cause.

### The gathering is detected, not commanded

**The sim detects bands meeting.** When two bands of a breeding population come within contact
range, that is a gathering: lines merge, the tile gains belief, and the event goes to The Telling
and the event log, with a turn-orb message the way a discovery is announced. There is no gather
command. The player causes it by moving bands, and learns it happened the way they learn anything
else the world did.

### Gathering is where culture converges

The culture module already has layer-drift meters, tension and schism (`culture.rs`;
manual §7c "Divergence & Conflict"). Two bands sharing a sacred place **converge** their local
layers. A far band that stops gathering **diverges**, and hard divergence is already defined as
splitting into a new faction. That is the cultural drift a far band undergoes, and it is the same
machinery as the fission rule's independence, fed by the same missing contact.

### The decisions it creates

- **Where to stand** — the belief tile or the richer patch; morale against food.
- **Whether to walk to the gathering** — turns and depletion for contact, belief and convergence.
- **Whether to spend labor on a monument** that feeds nobody.
- **Whether a splinter keeps returning.**

### Left out on purpose

A separate religion subsystem; prophets or priests as units; any hard cap on cohesion.

## One work party: hunt and forage are the same thing (decided)

### Why the current shape exists

- A **local hunt** is a labor assignment on a resident band. Its leash is `band_work_range` +
  `hunt_leash_tiles` (2 + 3 = 5, `LaborConfig::hunt_reach()`); out of leash, the assignment lapses.
  The 5 was set so the herd did not roam out of range once a hunt was set up — in hindsight, a patch
  over the wrong model.
- A **hunt expedition** is a detached party (a `StartingUnit` cohort with the `Expedition` component
  and deliberately no `ResidentBand`) that retargets to the herd's live tile each turn, lives off its
  kills, and **drops off** to the home band whenever the herd is within `drop_off_within_tiles`
  (3, `expedition_config.json`). Otherwise it hunts until the pack fills or the surplus is spent,
  then walks home and folds back. See `.claude/rules/core_sim/expeditions.md`.
- **Forage** is a labor assignment on a resident band, range `band_work_range` (2), into that
  band's own larder. It has no far mode and no carry-back of any kind. A band split off with
  `split_band` and walked to a far patch forages it into its own larder with no way to send food
  home, because a shipment launch is gated on a connection tie and two bands beyond sight range
  hold none (`.claude/rules/core_sim/connections.md`).

Two hunt commands and one forage command, three mechanisms, for one activity: workers go where the
food is and the food comes back.

### The model

**Assigning workers to hunt or forage is the only command. The workers are still the band's; they
are just somewhere else.**

- **The sim places the party at the source.** No split, no move order, no follow order. A hunt
  party follows the herd on its own, because that is where the source is; a forage party stands on
  the patch, because the patch does not move.
- **The party is a supply-network node with its own larder.** Within `reach_tiles` (3) it pools
  with the band automatically through `balance_supply_networks`, bounded by the network's
  `throughput_per_turn` and `friction` — the natural carry limit and loss-in-transit for a near
  party. Beyond reach, **hauling** is the carry-back: a shipment from the party to any band the
  player chooses. The shipment launch gate has to be rethought here, because a far party's only tie
  is the haul itself, and that traffic is what keeps it from being cut off.
- **Unassigning brings them home.** The existing fold-back (`fold_party_into_band`) settles workers,
  pack and materials into the band. There is no merge, because they never stopped being the band.
- **Hunters and foragers stay labor.** The party earns per-turn income into its larder the way the
  assignment does today. The lumpy raid model, its forecast and its completion rules go away with
  the expedition.
- **Trails come free.** The route branch of the intensification ladder already says a path is what
  traffic wears in before anyone builds a road. Hauling is traffic; a far patch hauled from
  regularly wears its own path home.

What the earlier "treat the party as a split-off band" idea was buying — network pooling and the
ability to send food along a trade route — comes from the party being a **network node**, not from
it being a separate band. A separate band would have needed a merge verb and a move order per turn.

### What goes

- The local-hunt leash: a party that follows the herd never goes out of range.
- `drop_off_within_tiles`: redundant with `reach_tiles` once the party is a network node.
- The hunt expedition path, its forecast, and the second hunt command.

### Who eats what (open)

Two consistent answers, pick one:

- **The party keeps no larder when near.** It pools 100% and eats from the band's larder. The
  band's runway readout is unchanged.
- **Every party has a larder and eats from it.** Consistent with a far party, which must. The
  band's consumption drops by the party's share and the pooled food is net of what they ate.

If the party is simply a network node, the second answer is what the systems already do, and the
first would have to be special-cased.

### In the anthropology

Binford's forager/collector distinction: foragers move the whole camp to the food; collectors send
task groups out from a base camp and bring the food back. The shift from the first to the second is
the recognised step toward sedentism. The base camp is the gathering point; the trails are the
routes.

### Tasks

Three, and the first is built **UX prototype first** — the hunt and forage panel, with the far
case, the haul and the party reading as part of the band, before any sim code.

1. **The work party** — hunt and forage share one model, in one PR: placement at the source, herd
   following for hunt, the network node, pooling within reach, hauling beyond it with the gate
   rethought, fold-back on unassign.
2. **Retire the expedition hunt path and the leash** once the work party covers everything they
   did.
3. **Client: the hunt and forage panel**, prototype first.

## Open questions

- Belief as a tile value: whether it is per-faction or per-place (another faction's dead on your
  tile), and how much each source adds.
- The lineage levers: `L`, `K`, contact range; whether lines ever regrow in a long-separated group.
- Shedding with no route: what happens to people leaving a truly isolated group above its ceiling
  (a wild/independent cohort? deaths? nothing until a route exists?).
- The away-from-belief morale term: its scale, and whether it reads distance or only presence.
- The population-genetics numbers (50/500, ~500 mating networks, founder-group histories): verify
  before the spec leans on them.
- The monument in the improvement catalog: the first improvement with no yield — what its
  `labor_draw` and decay mean for a thing that produces belief.
- The work party: the shipment launch gate for a far party whose only tie is the haul; which of
  the two "who eats" answers; whether a far forage party's larder counts toward the tether.
- Why moving never paid: measure it before touching depletion tuning.

## Related

- `docs/plan_settlement_population.md` — tether, decay, emergent settlement label
- `docs/plan_band_fission.md` — the split verb; independence = disconnection + grievance
- `docs/plan_early_game_labor.md` — carry capacity as the nomad cap; spoilage deferred
- `docs/plan_intensification.md` — actual vs sustainable income (the push)
- `docs/plan_exploration_and_sites.md` — scout/hunt expeditions, wondrous sites
- `docs/plan_the_telling.md` — stance vectors, the story that binds
