# Plan: The steps to a civilization — resistance to settling, the pulls that beat it, and one hunt verb

Status: **design dialogue captured, not a spec.** Nothing here is filed or built. Decisions marked
*leaning* are where the discussion landed; *open* is where it did not.

Source: *How Did Humans Invent Countries?* (Neon Rush, 29 min, published 2026-07-31,
https://youtu.be/vh2TNc7ASiw). Timestamps below refer to its captions. Discussion 2026-09-17/18.

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

**Standing still depletes the patch. Deaths and a shared site tether you anyway, and neither feeds
anyone. The cap stops growth until belief lifts it. The ladder turns the patch into fields, the
surplus becomes a place, and controlling it is the first act of politics.** Every step is a reason to
stay fighting a reason to go.

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

- **Leaving costs grievance.** A band that walks away from its dead takes a grievance hit scaled by
  how much is in the ground. The numbers say go; the people say stay. Grievance is already the
  channel that drives fission drift, so one lever does both jobs.
- **Standing on it raises the cap.** Above the cohesion cap (150 in the video; a config lever for
  us) a band loses people to the surroundings. Belief on the tile you stand on lifts that cap. That
  is how a cluster of local bands around a site becomes a settlement larger than any one band,
  instead of a band with satellites. Writing (step 7) is the second cap-raiser, much later.

Food stays the keeper of whether you *can* stay. Belief becomes the keeper of whether you *want*
to, and of whether the place can hold you once you do.

### Step 5 is a seam, not a design yet

Once the pulls hold a band through enough depletion that it climbs to fields, stored surplus is fixed
to a tile for the first time. The political event is not the granary but who controls it, and the
band's egalitarian pressure resists until the surplus overrides it. We carry per-cohort grievance
and stance vectors in The Telling; that is enough to say *a stored surplus above some level creates a
leader role, and grievance pushes back*. Not designed further.

## One hunt verb (leaning)

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

Two commands, two mechanisms, for one activity.

### The proposal

**Every hunt sends hunters out, and they follow the herd.** The only line between "local" and "far"
is where the kill lands relative to another of your bands:

- Kill within reach of a band → the meat is that band's, automatically.
- Kill beyond reach → the meat sits with the hunters, and what they do not eat can be **hauled back**
  by a party (the shipment verb: a party that walks cargo between two nodes,
  `.claude/rules/core_sim/expeditions.md` → "A shipment is a party that WALKS IT").

The leash goes away: a party that follows the herd never goes out of range. The expedition's
drop-off rule *is* the local-hunt rule, so nothing about "local" needs its own mechanism.

### If the hunting party is a real band, what comes free

The discussion converged on treating the hunting party as a **split-off resident band** (the
fission verb) rather than an expedition. Checked against the code, this is what a `ResidentBand`
gets that an `Expedition` is excluded from by construction:

- **Automatic exchange within reach.** `balance_supply_networks` auto-pools stores between
  same-faction resident bands within `reach_tiles` (3). A hunting band within 3 hexes of another band
  hands over its larder with no command — bounded by the network's `throughput_per_turn` and
  `friction`, which are the natural "carry" limit and spoilage-in-transit for a local hunt. Note
  `reach_tiles` and `drop_off_within_tiles` are both 3 today; under one verb the drop-off radius is
  redundant with the supply reach.
- **Trade routes to whoever you choose.** The shipment verb already launches from a band to any
  connected band; a far hunting band can send excess to a chosen destination, not only "home".
- **Connection is cohesion.** Hauling keeps the hunting band on the supply network, which is exactly
  the signal the fission rule reads. A hunting band that follows a herd for a season and keeps
  sending meat home stays yours; one that stops sending is drifting. The hunt becomes the first
  natural way a band falls out of touch — steps 1→3 told through food.
- Everything else a band is: births, ageing, its own runway, culture layer, live fog reveal,
  sedentarization tick, ordinary `MoveBand`.

### What is *not* free

- **Herd-following.** Retargeting to the herd's live tile each turn is expedition-only
  (`advance_expeditions`). A resident band goes where the player sends it. Either the player moves
  the hunting band every turn (tedious, and the exact problem the 5-leash was patched over) or a
  resident band gains a *follow herd* order. The latter is the missing piece.
- **Hunters stop being labor.** Today's local hunt is same-turn income into the larder. Under one
  verb, hunters leave the pool, and meat arrives as pooled transfers or hauls. The food-arrivals arc
  already projects lumpy arrivals forward, so the Food line survives; but the first turns gain a
  launch → walk → kill → transfer lag. Judged the right feel (a hunt is an event, not a rate), but it
  changes turn 1.
- **Roster and map.** Every hunt is a moving marker. Within reach it should read as "your hunters
  are over there", and the band roster and the map counter must agree it is part of the band. The
  band-naming rule already records a bug from exactly that split.

### Who eats what (open)

Two consistent answers, pick one:

- **The local hunting band keeps no larder.** It sends 100% back (the pooling does this) and eats
  from the home band's larder. Simplest; the home band's runway readout is unchanged.
- **Every hunting band has a larder and eats from it.** Consistent with a far band, which must. The
  home band's consumption drops by the hunters' share and the pooled meat is net of what they ate.
  Realistic; changes the runway readout and the first turns' food math.

If the local band is simply "a resident band within reach", the second answer is what the systems
already do, and the first would have to be special-cased.

## Open questions

- Belief as a tile value: what accrues it (deaths, gatherings, time), what decays it, and whether it
  is per-faction or per-place.
- The cohesion cap: config value, what "losing people to the surroundings" does mechanically (they
  leave as a wild/independent cohort? they die? they become a local band?), and how belief lifts it.
- Grievance on leaving the dead: scale, and whether it is a one-time hit or a standing term while
  away.
- The gathering place: is it a site tag on the map (the wondrous-sites seam), a built improvement,
  or an event? Who can contribute? With one faction until #513, "several bands" means your local
  bands.
- Hunting: *follow herd* as a resident-band order; whether `drop_off_within_tiles` survives or
  collapses into `reach_tiles`; which of the two "who eats" answers.
- Why moving never paid: measure it before touching depletion tuning.

## Related

- `docs/plan_settlement_population.md` — tether, decay, emergent settlement label
- `docs/plan_band_fission.md` — the split verb; independence = disconnection + grievance
- `docs/plan_early_game_labor.md` — carry capacity as the nomad cap; spoilage deferred
- `docs/plan_intensification.md` — actual vs sustainable income (the push)
- `docs/plan_exploration_and_sites.md` — scout/hunt expeditions, wondrous sites
- `docs/plan_the_telling.md` — stance vectors, the story that binds
