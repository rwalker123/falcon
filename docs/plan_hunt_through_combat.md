# The hunt is a fight — resolving the take through the combat system

**Status:** §9 slices **1–4 have landed**, plus the minimal TOE of §4.8; slices 5–7 are design. §4.8's kit split (slice 5) is a **correction to shipped behaviour** — this doc
pointed baskets at the hunt's carry, and the implementation faithfully built that. Issue #456, repurposed: the
denial raid that issue asks for turns out to need this first, and becomes small once it exists
(`docs/plan_denial_raid.md`).

Landed: the §4.3 body-mass correction, `SpeciesDef::engage_rate` with the engagement bound in
`fauna::quantise_animal_take`, and `CombatStats::wariness` with the seeded retreat stage
(`fauna::animals_that_stay` / `retreat_seed`), inert at `0` until slice 4 authors values. **Not**
landed: the take resolving through `resolve_fight`, durability, the attack/defense gate, TOE, and
the range forecast — so §4.2's `turns_to_kill` and §4.8's `attack 20` describe a model the sim does
not yet run.

**This is not a new subsystem.** `core_sim/src/combat/mod.rs` opens by saying it: *"A predator
encounter, a dangerous hunt, and (one day) a TOE-vs-TOE battle are all just a fight."* The hunt
already calls the resolver. This finishes the integration it started.

---

## 0. The hunt has two resolutions today, and they can disagree

### 0.1 One event, two models

A hunting party that engages a mammoth is resolved twice:

- **What happens to the hunters** goes through the combat system. The hunt path builds a payload and
  calls the resolver; `effective_attack = attack × ferocity` is that call.
- **What happens to the animals** goes through `fauna::quantise_animal_take`, which knows nothing
  about combat and computes the take from the party's *carrying capacity*.

Nothing reconciles them. A party can succeed at the take on one path while the other says the
mammoth routed them. `plan_predators.md` §7 already forbids exactly this — *"Casualties resolve
through a first-class combat subsystem, **never a bespoke hunt formula**"* — and the take is a
bespoke hunt formula that happens to be about the other side of the same fight.

### 0.2 Headcount cannot substitute for a weapon

The decisive case: **eight hundred bare-handed people cannot kill a mammoth.** Not slowly — not at
all.

Any model where the party's power is a per-worker rate multiplied by headcount gets this wrong,
because enough workers always clear any threshold eventually. Killing power does not add across
people the way carrying does: a hundred bare hands do not produce more penetrating force than one
bare hand, because force is a property of the individual strike. Whether you can hurt the animal at
all is settled *before* headcount enters.

That is `attack` vs `defense` — a `CombatProfile` comparison, in a module built to make it. Any
attempt to express it in the take path reinvents combat vocabulary beside the combat system.

### 0.3 So the take belongs to the resolver

Routing the whole hunt through the fight deletes the parallel model rather than reconciling it, and
**TOE then lands once**: equipment plugs into `CombatProfile`, and hunting, camp defense, predator
raids and future battles all read it from the same place. Plumbing equipment into a hunt-specific
kill formula *and* the resolver would guarantee the two drift.

---

## 1. The model — four stages, one owner each

> **Engagement sizes the fight. Wariness decides who stays for it. Combat resolves it. Carry decides
> how much comes home.**

```text
engaged   = floor( hunters × species.engage_rate )        // §2 — how many animals are in the fight
stayed    = engaged − retreat(engaged, wariness, seed)    // §3 — some break off at contact
outcome   = resolve_fight(party_force, herd_force(stayed))// §4 — combat owns the kill
killed    = outcome.enemy_losses                          // whole animals, as today

carried   = min( killed × body_mass, carry_capacity, carry_room )   // §5 — unchanged
wasted    = killed × body_mass − carried
herd.biomass -= killed × body_mass
```

Each stage answers one question, and no stage answers another's:

| stage | question | owner |
|---|---|---|
| engagement | how many of them can this party take on at once | `SpeciesDef::engage_rate` |
| retreat | how many of those actually stay to be fought | `CombatProfile::wariness` |
| the fight | who dies — theirs and ours | `combat::resolve_fight` |
| carry | how much of the kill gets home | `hunt.per_worker_biomass_capacity` |

The escapement floor (`docs/plan_harvest_floor.md`) sits above all four unchanged, and it bounds
**`engaged`** — not `killed`.

**It is not the same thing as wariness, and the two blur easily.** Both reduce how many animals end
up in the fight, from opposite directions:

| | who decides | shape |
|---|---|---|
| `wariness` | **the animal** — it bolts at contact | stochastic, from the herd |
| escapement floor | **the player** — orders say stop at half the herd | deterministic, from the mission |

Bounding `killed` instead would have a party at the floor engage normally, take casualties, wear its
kit, and then decline to kill what it had already fought — and killing without taking is denial, not
restraint. So **restraint is free**: a party at the floor does not go, and the only cost is the food
it chose to leave standing. The ordering follows — the floor decides how many you go after, then
wariness takes some of those away.

---

## 2. Engagement — how many animals are in contact

`SpeciesDef::engage_rate`: **how many animals of this species one hunter can have in contact at
once.** Twenty hunters can surround one mammoth; one hunter can work a line of snares. It is a
**spatial** constraint and nothing else.

**It says nothing about how fast they die.** That is §4's job, and keeping the two apart is the whole
correction: an engagement rate that also encoded lethality would be a kill model outside the
resolver, which is what §0 exists to delete. `engage_rate` bounds the size of the fight; the fight
decides its outcome and its duration.

**Turns-to-kill is therefore an OUTPUT, never authored.** It is
`durability / (hunters × effective_attack)` — see §4.2. Any per-species "this takes N turns" table is
the bespoke hunt formula wearing a different hat: it hard-codes an answer that must respond to
equipment, party size and the quarry's defense, and it will silently stop tracking all three the
moment any of them changes.

**A fractional engagement rounds up to one, not down to zero.** Three hunters against a mammoth come
to `3 × 0.05 = 0.15`; flooring says a small band can never *reach* a mammoth, which is not what the
rate means — they can walk up to it. What stops them is that they cannot hurt it fast enough to
matter, which the fight already reports. Contact is not the gate.

**It scales linearly with party size.** One hunter engages `X`, two engage `2X`; there is nothing
magic about crowding, and the fractional values below are **throughput, not a threshold**. Forty
hunters engage two mammoths a turn; five still engage one and grind it down over many turns.

### 2.1 Values — SETTLED

`ceiling = engage_rate × body_mass` is the most biomass one hunter can ever take from a species per
turn, at any weapon tier (§4.6), so it is the number these are authored against.

| species | body | engage_rate | reads as | **ceiling** |
|---|---|---|---|---|
| mammoth | 800 | 0.05 | 20 hunters per animal | **40** |
| steppe runner | 53 | 0.5 | 2 hunters | **26.5** |
| seal | 12 | 2 | 1 hunter, 2 animals | **24** |
| marsh grazer | 47 | 0.5 | 2 hunters | **23.5** |
| wild elk | 47 | 0.5 | 2 hunters | **23.5** |
| aurochs | 120 | 0.17 | 6 hunters | **20** |
| wild horse | 40 | 0.5 | 2 hunters | **20** |
| reindeer | 20 | 1 | 1 hunter | **20** |
| deer | 15 | 1 | 1 hunter | **15** |
| alpine ibex | 10 | 1 | 1 hunter | **10** |
| river fish | 0.67 | 15 | nets | **10** |
| crag goat | 6 | 1.5 | 1 hunter | **9** |
| wild sheep | 5.6 | 1.5 | 1 hunter | **8.4** |
| gazelle | 3.3 | 2 | 1 hunter | **6.6** |
| snow hare | 0.6 | 10 | snares | **6** |
| boar | 12 | 0.33 | 3 hunters | **4** |
| forest grouse | 0.47 | 8 | snares | **3.8** |
| rabbit | 0.27 | 10 | snares | **2.7** |
| wolf | 5.3 | 0.33 | 3 hunters | **1.75** |
| fowl | 0.13 | 10 | snares | **1.3** |

Four things the ordering is *meant* to say, and which a re-tune must not quietly undo:

- **Mammoth is an outlier, not the top of a curve** — 40 against 26.5 for the next. That margin is
  what makes organising twenty hunters worth doing.
- **The tameable species are also the good hunting.** Steppe runner, marsh grazer, reindeer and wild
  horse (20–26.5) are all `pastoral`; aurochs (20) is the `pen` branch's prize. You hunt them until
  you can tame them, which is the ladder's own story.
- **Pen small game is at the bottom** — rabbit 2.7, fowl 1.3, grouse 3.8. Hunting them is pointless,
  which is the pressure toward penning.
- **Dangerous-for-their-size are the worst deals in the game** — boar 4, wolf 1.75. Three hunters for
  a 12-unit animal is a bad trade however it is sliced, and a boar hunt should read as a mistake.

**Seal at 24 is the row most likely to cause trouble.** Historically right — seals are helpless on a
haul-out — but it makes a coastal start materially stronger than an inland one. Known, not
discovered.

**The gate is §4.2's attack-vs-defense.** Twenty bare-handed hunters *do* engage a mammoth — the
fight then resolves as casualties with no kills, because their effective attack is zero. That is
deliberately better than a disabled button: the sim teaches the lesson instead of a tooltip.

---

## 3. Wariness is a combat field, not a fauna field

`CombatProfile::wariness`: **the probability a combatant breaks off when contact is made.** Engaged,
then gone before the fight resolves.

**It is not a hunting concept.** Shocked troops break on contact by the same mechanism; this is the
retreat dial for the combat system generally. The only difference between the two cases is how the
value is *maintained*, not what it means:

| | animals | troops |
|---|---|---|
| where it comes from | authored per species, **static** | morale, **dynamic** — past fights, losses, supply |
| what it does | identical | identical |

This is why it belongs on `CombatProfile` rather than `SpeciesDef`. Modelled as *"how many you can
get near"* it would have been a hunt-only invention needing its own justification; modelled as
*"who stays when it starts"* it is one field serving both, and a combat profile carrying a morale
term needs no defending.

**Escaped animals are not dead, so the herd loses nothing for them.** A wary herd therefore costs the
party **hunter-turns**, not herd biomass — you spent the turn and got fewer animals. That is the right
pressure and it falls out with no extra rule.

**Animal wariness is static — SETTLED.** A herd that has been hunted hard does not learn to flee.
Only humans grow warier, through the morale half of this field; the animal half is authored once and
never moves. That keeps species values reproducible and the field's two halves cleanly split.

**Wariness `0` is an exact identity**, not a roll with probability zero: no draw is made, nothing is
consumed, and the outcome is the deterministic one. That is what keeps every existing yield test
pinning the numbers it pins today (§6).

---

## 4. The fight

### 4.1 A herd is a Force

The take already quantises to whole animals, so a herd already knows its animal count — `biomass /
body_mass`, derived as it is today. A herd of `stayed` animals maps to a `Force` of contingents with
the species' `CombatProfile`, and the resolver's enemy losses map back to biomass on the way out.
Nothing about `AnimalTake { killed, carried, wasted }` changes.

### 4.2 Durability, damage, and the gate

A fight resolves by attrition, and the shape is the same for a mammoth, a wolf pack or a rival
warband:

```text
effective_attack = max(0, attack − defense)          // per hunter — THE GATE
damage_per_turn  = hunters × effective_attack
turns_to_kill    = durability / damage_per_turn      // an OUTPUT
```

**`durability` is authored per species, NOT derived from `body_mass`.** Deriving it was tempting and
is wrong: durability is a defensive *strategy*, and plenty of animals do not use it. A gazelle is not
one-third of a deer's toughness — it is not tough at all, and survives by not being there. Armour and
bulk are not the same axis, and neither tracks mass reliably.

That makes four independent ways a species survives being hunted, and a species is characterised by
**which** it leans on:

| strategy | field | exemplar |
|---|---|---|
| do not be there | `wariness` | gazelle — gone before contact, nothing else |
| cannot be penetrated | `defense` | mammoth — hide stops what you are throwing |
| soak it | `durability` | aurochs — hits land, it keeps coming |
| hurt you back | `ferocity` | boar — small and frail, still costs you people |

`defense` and `durability` blur easily and must not: **defense is whether a hit counts at all,
durability is how many counting hits it takes.**

#### Values — SETTLED

Effort shown at the settled spear `attack 20` (§4.8), so `hunter-turns = durability / (20 − defense)`.
`defense` is the existing field, unchanged.

| species | defense | **durability** | hunter-turns | survives by |
|---|---|---|---|---|
| mammoth | 12 | **500** | 62 | fortress + fights back |
| aurochs | 6 | **150** | 11 | soak + aggression |
| steppe runner | 3 | **60** | 3.5 | bulk |
| marsh grazer | 3 | **60** | 3.5 | bulk |
| wild elk | 3 | **60** | 3.5 | bulk |
| wild horse | 2 | **35** | 1.9 | speed |
| reindeer | 1 | **25** | 1.3 | nothing much |
| deer | 1 | **25** | 1.3 | wariness |
| boar | 2 | **20** | 1.1 | **ferocity alone** — frail, still costs you people |
| wolf | 3 | **20** | 1.2 | **evasion + ferocity, no toughness** |
| alpine ibex | 1 | **15** | 0.8 | terrain |
| seal | 2 | **12** | 0.7 | **nothing** — easy prey |
| crag goat | 1 | **12** | 0.6 | terrain |
| wild sheep | 1 | **12** | 0.6 | wariness |
| gazelle | 1 | **8** | 0.4 | **wariness alone** — frail and fast |
| forest grouse | 0 | **3** | 0.15 | wariness |
| snow hare | 0 | **3** | 0.15 | wariness |
| river fish | 0 | **2** | 0.1 | numbers |
| rabbit | 0 | **2** | 0.1 | wariness + breeding |
| fowl | 0 | **2** | 0.1 | nothing |

**The decoupling from mass is real where it matters**, and it is what deriving would have destroyed:
boar and seal are the same body mass and boar is nearly twice as durable; wolf is lighter than a wild
sheep and tougher than one; ibex outlasts a seal at less than half the weight.

**Excess damage spills to the next animal in the engagement.** This is what makes "many small animals
per turn" fall out instead of being authored: a hunter doing 20 damage against 5-durability rabbits
kills four, because that is what 20 damage does — and the number rises on its own when the party gets
better weapons. No species carries a "how many of these per turn" figure.

Whether a hunter can hurt the animal *at all* is settled per hunter, before headcount:

- **bare hands vs mammoth** — attack below defense. No kills at any headcount, and `ferocity` means
  the mammoth is landing real blows. Engaging it is a way to lose people.
- **spears vs mammoth** — attack above defense, but barely, so it takes a crowd.
- **anything vs rabbit** — defense at floor, so the fight is decided the moment it starts.

The gate is not a new rule to write. It is what a combat resolver comparing attack to defense already
does; §0.2's failure only arises when the comparison is *outside* the resolver.

#### Damage carries between turns — a wounded animal stays wounded

**A fight that does not kill this turn is not forgotten.** Damage dealt to the engaged animals
accumulates while the party stays in contact, so twenty hunters with weak spears wear a mammoth down
over several turns rather than bouncing off it forever.

**Without this the gate is absolute rather than steep**, and that is the wrong model: a stateless
resolver means `ceil(durability / (attack − defense))` hunters is a hard threshold — 63 for a
mammoth at the shipped spear — and a party of 62 takes casualties every turn and never kills
anything, on any horizon. *"Twenty weak spears and then follow it around for days"* is the intended
experience, and it requires the days to count for something.

**This is a combat-system feature, not a hunt one.** Any fight that spans turns needs it; a
TOE-vs-TOE battle has the same requirement, and putting it in the resolver keeps the hunt from
growing a private copy.

**Banking damage is legitimate where banking the ceiling was not**, and the distinction is the same
one §7 records: `hunt_credit` was deleted because the escapement ceiling is a **stock**, and
accumulating a stock compounds it. Damage is a **flow** — a rate of harm per turn — so an
accumulator is its correct integral. A stock must not bank; a rate must.

Open: whether wounds **heal** when the party disengages, and how fast. A herd that forgets instantly
makes a broken-off hunt worthless; one that never forgets lets a party chip at a mammoth across fifty
turns of unrelated play.

### 4.3 The body masses were wrong — SETTLED

`body_mass` no longer sets durability (§4.2), but it still sets the food a carcass yields, and
several values were off by multiples. The clearest: **a reindeer is heavier than a typical deer** (reindeer 120–300 kg,
white-tailed deer 45–135 kg, red deer 82–240 kg), while the config had reindeer `18` against deer
`60` — inverted, by more than 3×. Boar and wolf were inflated the same way.

Anchored on the mammoth (~6,000 kg = `800`, so ~7.5 kg per unit), leaving the top of the scale
unchanged:

| species | real adult (kg) | was | **is** | | species | real adult (kg) | was | **is** |
|---|---|---|---|---|---|---|---|---|
| mammoth | ~6,000 | 800 | **800** | | wild sheep | 35–50 | 12 | **5.6** |
| aurochs | 900–1,500 | 80 | **120** | | wolf | 18–80 | 30 | **5.3** |
| steppe runner | *fictional* | 120 | **53** | | gazelle | 15–35 | 4 | **3.3** |
| marsh grazer | *fictional* | 100 | **47** | | river fish | ~5 | 2 | **0.67** |
| wild elk | 300–450 | 40 | **47** | | snow hare | 4–5 | 0.4 | **0.6** |
| wild horse | 250–360 | 45 | **40** | | forest grouse | 3–5 | 0.3 | **0.47** |
| reindeer | 120–300 | 18 | **20** | | rabbit | 1.5–2.5 | 0.3 | **0.27** |
| deer | 45–240 | 60 | **15** | | fowl | ~1 | 0.25 | **0.13** |
| boar | 50–200 | 50 | **12** | | | | | |
| seal | 82–129 | 30 | **12** | | | | | |
| alpine ibex | 50–100 | 15 | **10** | | | | | |
| crag goat | ~45 | 10 | **6** | | | | | |

**This is a balance change, not a free correction** — the real mammoth-to-deer mass ratio is ~40:1
against the old config's 13:1, so mid-game hunting yield falls to roughly a third while megafauna is
untouched. Deliberately **not** compensated for elsewhere: the food economy is expected to settle
once the underlying data is right, and pre-emptively tuning around numbers that are still moving
would bake in a correction for a problem that may not survive them.

### 4.4 Ferocity is already the right hinge

`SpeciesDef::ferocity` means *"does it fight back or flee"* — in resolver terms, **whether the animal
side contributes attack at all.** A ferocity-`0` gazelle is a one-sided engagement; a ferocity-`0.9`
mammoth is a real fight with real casualties. No separate "is this dangerous" flag is needed, and the
casualty path that exists today keeps working through the same field.

### 4.5 Most hunts must not feel like battles

Snaring rabbits is not a war. The answer is **one model with a degenerate fast path**, never two
models: when the animal side contributes no attack and its defense is at floor, the fight resolves
to *"everything that stayed dies, nobody is hurt"* without ceremony, cost or a battle report. A
second code path for small game would recreate exactly the parallel-model problem §0 exists to
delete.

### 4.6 Better weapons pay off on big game and nowhere else

Every species has a **hard ceiling** on what a hunter can take from it per turn:

```text
ceiling = engage_rate × body_mass       // biomass per hunter per turn
```

No weapon exceeds it — there are only so many rabbits you can lay hands on. Weapons decide how close
to the ceiling you get, and that is where megafauna wins:

| | ceiling | at spear `attack 20` | at 2× | at 4× |
|---|---|---|---|---|
| mammoth | **40** | 12.8 — 32% of it | capped **40** | **40** |
| deer | 15 | 11.4 — 76% | **15** | **15** |
| rabbit | 2.7 | 2.7 — **100%** | 2.7 | 2.7 |

**Small game is already maxed out at the first spear.** Extra damage falls on the floor, because
engagement binds and nothing else does. Megafauna sits at a third of its ceiling with all the
headroom in the roster.

**Two effects compound here, and only one is obvious.** The engagement cap is the visible half. The
other is that `max(0, attack − defense)` makes high-defense quarry gain **super-linearly**: doubling
attack from `20` to `40` raises a mammoth's effective attack from `8` to `28` — 3.5× — while a
rabbit's merely doubles. The animal furthest from its ceiling is also the one that closes the gap
fastest.

So *"twenty weak spears and then follow it for days"* is the correct low-tech experience, and better
points turn the same herd into the richest food on the map without a single number being re-authored.
**This is a property to pin, not a happy accident** (§10): it falls out of defense subtraction and
the engagement cap together, and a change to either could silently flatten it.

#### The hunt itself injures people, whatever the quarry does

Casualties currently come only from the animal fighting back, so at the shipped roster **only mammoth,
aurochs and wolf can hurt anyone** — a boar costs nothing, which contradicts §4.2's own "survives by
ferocity alone, still costs you people".

**Hunting is dangerous before anything bites you.** Hunters fall, break bones, are trampled in a
drive, cut themselves butchering. So a hunt carries a **small baseline injury risk independent of the
quarry's `attack`**, on top of whatever the fight itself does. A harmless animal is not a risk-free
day out.

It scales with the **engagement**, not with the quarry — more animals worked means more chances to
get hurt — and it is a config lever, not a per-species field: the danger is in the activity, not in
the rabbit.

### 4.7 Randomness lives in the attack, and never in the gate

A pure attrition formula is a spreadsheet. Variance belongs **in the resolver**, so hunts, raids and
battles share one source of it rather than growing three.

**It attaches to the individual attack landing**, not to a percentage fudge on the damage total. That
buys a property a flat ±X% cannot:

> **Variance shrinks as the force grows.** Three hunters are a gamble; thirty are reliable. It is
> binomial, and it makes party size a real decision rather than a threshold to clear.

**The gate stays hard, and this is the trap.** Replacing `max(0, attack − defense)` with a smooth hit
probability is the tempting next step and it silently breaks §0.2. Bare hands (`attack 1`) against a
mammoth (`defense 12`) under a naive `p = a/(a+d) = 0.077` gives eight hundred hunters ~61 damage a
turn — a dead mammoth in **sixteen turns**. Enough dice always roll through a soft gate.

So: **below the gate the probability is exactly zero, not merely small**; above it, every attack is a
roll. Three requirements on the resolver, which owns the choice of model:

1. Variance is **binomial in force size**, never a flat percentage.
2. Below the gate, probability is **exactly zero** — asymptotic is not good enough.
3. Seeded per event (§6.2), the same seeding the retreat roll needs.

### 4.8 A minimal TOE is in scope

Without equipment a hunter's `attack` is `1`, which is below every megafauna's `defense` — so **no
band can hunt a mammoth until spears exist**, and the progression stops being a note and starts being
load-bearing. That makes a minimal TOE part of this arc rather than a dependency beside it:

- **spears** — `attack 20` against an unequipped `1`. **SETTLED**, and the number that opens the gate;
- **a sled** (travois, drag harness) — the **hunt's** carry, §5. **A flat per-hunter multiplier, and
  it needs no pullers** — SETTLED. Modelling a crew cost would make the hunt's carry non-linear in
  party size, and `hunt_haul_workers`, the fill target's arithmetic and §5.1's trip length all assume
  that linearity; the ripple is not worth what the realism buys;
- **baskets** — the **forage** web's carry and yield, `plan_early_game_labor`'s other role.

Consumable with the durability cliff, start-stocked, **not craftable**.

#### One kit, one job — and an earlier draft got the carry half wrong

This section used to read *"the carry kit — **baskets**, raising the carry side (§5)"*, and §5 is the
**hunt's** carry. That is a physical nonsense and it shipped: baskets currently raise a hunter's haul
rate and do nothing at all for foraging.

**A carcass is one lumpy object you drag out whole.** A container does not help you move a deer —
what helps is a *sled*. Berries are the opposite case: loose, divisible, and bounded entirely by what
you can hold, which is exactly what a basket fixes. So the two webs want different kits, and their
unequipped tiers want different **shapes**, not merely different numbers:

| | forage | hunt |
|---|---|---|
| the constraint | **containment** — a handful against a basketful | **transport** — dragging a carcass |
| bare-handed tier | a small fraction of equipped; the ratio is large | moderately reduced; you can always drag *something* |
| where the shortfall shows | less gathered | **`wasted`** — meat left on the range, already computed and already on screen |

The hunt's sledless case therefore needs **no new mechanic**: a party that cannot haul its kill
leaves more of it, which `AnimalTake.wasted` has always expressed.

**Megafauna then needs BOTH kits**, which is the property worth having: spears get you through a
mammoth's `defense 12`, the sled gets 800 biomass home, and neither alone is enough. "Megafauna is
the prize" gets two gates rather than one.

**A sled does not shorten a raid — it lengthens it.** Trip length is
`carry / (engage_rate × body_mass)` (§5.1), so a bigger pack takes *longer* to fill. The sled buys
more meat per trip; the **fill target** (§5.2) is the lever for trip length. They are complementary
and it is easy to assume otherwise.

The crafting economy that replenishes kit stays deferred. What ships is enough to make `attack` a
real number and the equipped/unequipped distinction visible.

**At `attack 1` a band can hunt only what has no `defense` at all** — rabbit, fowl, grouse, hare,
fish — at one damage a turn, so two turns per rabbit. Everything from a gazelle upward is untouchable.
Without kit you are a trapper, not a hunter, and running your spears dry does not reduce hunting so
much as end it.

**The `1 → 20` jump is the largest single multiplier in the design**, and it is deliberate rather
than overlooked: the first spear should feel like a different game. It is also pure configuration, so
it is the cheapest number here to revisit once the loop is playable.

---

## 5. Carry is separate — and it is what ends a trip

The two halves of a hunt have opposite shapes, which is why one number could never do both:

| | bringing it down | carrying it home |
|---|---|---|
| hard gate? | **yes** — attack vs defense | no; you can always take *some* |
| headcount helps? | only *above* the gate | always, linearly |
| driven by | weapon quality first, then numbers | mass ÷ people |
| failure mode | you cannot hunt it at all | you waste the remainder |

**`max(1, carryable)` survives untouched.** A party too small to haul a whole animal still takes one
and wastes the rest — that is where hunting's waste comes from, and deleting it would silently remove
waste from the game everywhere except a denial raid.

### 5.1 A raid's length is a species constant, and the party stepper cannot change it

**Found in play, not in review.** Eight hunters sent after a Wild Fowl herd reported *"away ≈43 turns
— 31 hunting, 12 travel"*, with no control that moved the number.

A raid ends when the **pack fills** (or the herd reaches the floor, or the herd is lost — whichever
comes first). The pack is measured in **carry**; since §2 the take is measured in **reach**. That
mismatch is the whole of it:

```text
pack        = workers × per_worker_carry            = 8 × 40   = 320 biomass
rate        = workers × engage_rate × body_mass     = 8 × 10 × 0.13 = 10.4 biomass/turn
turns       = 320 / 10.4                            = 31
```

**Party size cancels.** Both terms scale linearly with `workers`, so:

```text
turns_to_fill = per_worker_carry / (engage_rate × body_mass) = per_worker_carry / ceiling
```

Four hunters take 31 turns; sixteen take 31 turns. The stepper is not a weak lever here, it is
**structurally not a lever at all** — which is exactly what the player reported.

**So §4.6's ceiling table is silently also the trip-length table**, and nobody noticed it was setting
two things:

| | ceiling | turns to fill |
|---|---|---|
| mammoth | 40.0 | **1.0** |
| steppe runner / seal / marsh grazer / wild elk | 23.5–26.5 | 1.5–1.7 |
| aurochs / reindeer / wild horse | 20 | 2.0 |
| deer | 15 | 2.7 |
| ibex / river fish / crag goat / wild sheep | 8.4–10.1 | 4.0–4.8 |
| gazelle / snow hare | 6.0–6.6 | 6.1–6.7 |
| boar / forest grouse | 3.8–4.0 | 10.1–10.6 |
| rabbit | 2.7 | 14.8 |
| wolf | 1.7 | 22.9 |
| fowl | 1.3 | **30.8** |

The *ordering* is the design working — a mammoth raid is one hunting turn plus travel, which is what
"megafauna is the prize" should feel like. The small end is not merely unattractive, it is
**unusable**, and that is the defect.

### 5.2 The fill target is the party-side twin of the floor

The termination condition already has a party-side term — the pack — but it is **a physical constant
nobody chose**. Make it a number the player sets, below capacity:

> **The floor says how deep to draw the herd. The fill target says how long you will wait.**

That needs **no new termination logic**: it replaces a constant in a condition the raid already
evaluates. *"Take ≈50 and come home"* is a fill target under capacity, and a target at or above
capacity is exactly today's behaviour — which is what makes the change safe to land.

**The escapement graph returns on the expedition sheet** for the herd-side half. The two levers then
read as a pair rather than as one dial and one mystery.

**The forecast's job becomes naming which bound ends the trip**, and that is the readout that makes
the choice legible — more than either number alone:

- *"You come home on your fill target in 4 turns; the herd never reaches the floor."*
- *"You reach the floor in 2 turns with the pack a third full."*

**The party stepper stops pretending to be a trip-length dial** — it never was one — and becomes
purely *how much you bring back per trip*, which is honest and still worth deciding.

**This blocks the remaining slices** rather than sitting beside them: slice 4 changes what a take is,
and re-tuning trip lengths against a mechanic that has no player lever would be tuning the wrong
thing twice.

---

## 6. Determinism, and what the player is told

### 6.1 Existing tests are unaffected

`forecast == actual` is a hard invariant here, and wariness does not challenge it: at wariness `0` the
retreat stage is an identity, so every existing yield test resolves the same numbers it resolves
today. Range behaviour arrives with **new** tests against species that carry a non-zero value.

### 6.2 Seed per event, never from a shared stream

The retreat draw must be seeded from `(herd, tick, party)` — **not** taken from a global RNG.
A shared stream makes every draw order-dependent, so adding or reordering one hunt shifts every
downstream result and rollback stops reproducing. Per-event seeding is order-independent, which is
what checkpoint replay requires (`.claude/rules/core_sim/checkpoints.md`).

The player cannot predict the roll; the sim reproduces it exactly. Both properties are wanted and
they are not in tension.

### 6.3 Two test shapes, and a range assertion is the weaker one

Because the draw is seeded, a test can pin the **exact** outcome for a chosen seed. Distribution
claims are then asserted **across many seeds** rather than as a tolerance on one run. A bare "the
answer is between 6 and 11" assertion is flaky by construction and passes when the feature is dead;
pair every distribution assertion with a liveness one.

### 6.4 The forecast reports a range

The pre-commit readout changes from a promise to a distribution: *"6–11, likely 9."* This is a change
in what the forecast *means*, and it touches every yield readout in the client, so it is in scope
here rather than a surprise later. It is also an improvement — communicating risk is what makes the
mammoth decision a decision rather than arithmetic.

### 6.5 A fight the party cannot win must say so before it is launched

The gate (§4.2) produces a real outcome that reads as a bug if unexplained: hunters die, nothing is
killed. The hunt panel therefore checks it **at launch** and says so in words, and the forecast
independently estimates **zero food** — two signals from different paths, so a failure in either
still leaves the player warned.

### 6.6 The hunt emits events, even before anything consumes them

**Which bound actually stopped the party is an output of the resolution, not a diagnostic field
computed beside it.** It belongs in a hunt report, which is an event — the consumer is the event
notification system (issue #272), and this arc's job is to make sure the facts exist for it to pick
up.

A hunt report carries: animals **engaged**, how many **fled** before contact, animals **killed**,
hunters **lost or wounded**, what **ran out first** (engagement, the floor, carry, or the fight
itself), and what came home — **carried** and **wasted**.

**Facts, never a composed string.** #272 owns importance and phrasing; the hunt owns what happened.
Emitting presentation-ready text here would bake this arc's guesses about an importance ladder into
the sim, and the sim already treats the client this way everywhere else.

---

## 7. What `plan_denial_raid.md` loses to this doc

That doc's §1–§2 specified a kill model **outside** the combat system: `per_worker_kill_capacity`, a
biomass-denominated kill budget, `stalk_overhead`, `toughness(defense)`, and a partial-kill bank. All
of it existed to answer questions the resolver answers, and all of it is deleted:

| deleted | why | replaced by |
|---|---|---|
| `per_worker_kill_capacity` | a per-worker rate can never express §0.2's gate | combat |
| `toughness(defense)` | reinvents `CombatProfile` beside it | `defense`, in the resolver |
| biomass-denominated kill budget | a workaround for having no resolver | `engage_rate`, in animals |
| `stalk_overhead` | patched that workaround's small-game end | `engage_rate` is already per animal |
| the partial-kill bank | assumed a partial engagement carries forward; **the animal does not wait** | nothing — sub-threshold parties simply cannot hunt it |
| `wariness` on `SpeciesDef` | it is not a hunting concept | `CombatProfile::wariness` |

`plan_denial_raid.md` keeps its §0 (why `floor = 0` is not denial), the collapse-threshold win
condition, its scope decisions and its UI, and becomes small: **denial is the mission that engages
hard and does not clamp to carry.**

---

## 8. Not in scope

- **The resolver's own model.** `plan_predators.md` shipped the seam with a placeholder — *"the seam
  is the deliverable"* — and this arc rides that same bet. Improving the resolver is its own work,
  and it improves hunting for free when it lands.
- **The crafting economy.** A *minimal* TOE is in scope (§4.8); replenishing or upgrading kit is not
  (`docs/plan_early_game_labor.md`, deferred M2+).
- **Dynamic morale for troops.** §3 says wariness is static for animals and dynamic for troops; only
  the static half ships here. The field is shaped for both.
- **Technique as a substitute for weaponry.** Driving a herd off a cliff is a real third path to
  megafauna — terrain plus numbers instead of gear, and the honest answer to how anyone took a
  mammoth before good spears. It is a place-gated method, not a gear-gated one, and it wants its own
  design.

---

## 9. Slices

Each lands on its own PR.

1. **`engage_rate`** on `SpeciesDef`, authored across the roster; the take path bounds the engagement
   and nothing else changes yet. Kill still resolves as today.

   **This is NOT a no-op, and an earlier draft of this line claimed it was.** Per-hunter carry is
   `40` biomass, and §2.1's ceiling column (`engage_rate × body_mass`) is below `40` for **19 of the
   20 species** — so engagement is the binding term across essentially the whole roster from the
   moment it ships. Ten hunters on a rabbit warren at floor `0.5` took `370` animals (100 biomass)
   and now take `27` biomass, a 73% cut. Slice 1 is the arc's first real balance change; only slice
   2 is an identity.
2. **`CombatProfile::wariness`** + the retreat stage, seeded per event; wariness `0` everywhere, so
   this slice is a provable identity.
3. **The fill target** (§5.2) — a player-set stop below the pack's capacity, the escapement graph
   restored on the expedition sheet, and a forecast that names which bound ends the trip. **Next,
   and blocking**: without it a raid's length is a species constant with no lever (§5.1), and every
   later slice would be tuned against that.
4. **The take resolves through `resolve_fight`.** `quantise_animal_take`'s kill arm is replaced by
   the resolver's enemy losses; the herd-as-`Force` mapping and the one-sided fast path land here.
   Carry, waste and `max(1, carryable)` are untouched.
5. **The three kits split correctly** (§4.8) — a sled takes over the hunt's carry, baskets move to
   the forage web with a much lower bare-handed tier, spears keep `attack`. **A correction to shipped
   behaviour, not new scope**: minimal TOE landed with baskets raising the *hunt's* haul rate and
   doing nothing for foraging, because this doc told it to. Worth landing before any balance
   evaluation, since it moves both webs' carry.
6. **Forecast reports a range** (§6.4) + the client readout, and the hunters-per-animal figure on the
   pre-launch panel.
7. **Wariness values authored** across the roster — the first slice with visible retreat behaviour.

   **7 must follow 6, and the order is not a preference.** Authoring wariness makes the take
   stochastic; until the forecast reports a distribution, `forecast == actual` breaks on the animal
   web the moment a non-zero value ships. There is a second, harder half to settle with it: a
   forecast has no event seed (a projection cannot know a future tick), so the preview cannot draw
   the retreat the live take will draw. Either the forecast reports the **expectation**, or the draw
   is made forecast-reproducible — a design call, not an implementation detail.

Slice 2 is deliberately an identity (wariness `0` makes the retreat stage a provable no-op), so its
review can be about the seam rather than about balance. Slices 1, 3 and 4 all move numbers.

---

## 10. Validation

- **`forecast == actual` still holds at wariness `0`**, per component, on the exported snapshot.
  The existing suite is the assertion, but **not unmodified** — slice 1's engagement bound is a real
  balance change (§9), and three crew constants in `core_sim/tests/hunt_yield_vector.rs` had to grow
  because they were sized against the carry bound alone and began measuring a crew limit instead of
  the property they name: `HAUL_THE_WHOLE_HERD_CREW` 100→300, `DIP_VISIBLE_CREW` 5→12,
  `LABOR_BOUND_CREW` 1→2. A test that asserts a difference between two takes must staff a crew where
  engagement is **not** the binding term, or it silently asserts nothing.
- **Wariness `0` consumes no randomness.** Pinned directly: a turn with hunts at wariness `0` leaves
  the RNG state identical to a turn with none.
- **Replay determinism across hunt ordering.** Two runs that resolve the same hunts in different
  orders produce identical outcomes — the assertion that per-event seeding is real and no shared
  stream crept in.
- **The gate.** A party whose attack is below the quarry's defense kills **zero** at any headcount,
  and takes casualties proportional to `ferocity`. This is §0.2, pinned.
- **Better weapons pay off on big game and not on small.** Raising `attack` must raise biomass per
  hunter-turn for a high-defense quarry and leave an engagement-bound one flat (§4.6). The assertion
  that neither the defense subtraction nor the engagement cap has been quietly linearised.
- **No species exceeds `engage_rate × body_mass` per hunter**, at any weapon tier — the ceiling is
  real, so arbitrarily good kit cannot turn small game into a food engine.
- **Turns-to-kill responds to all three of its inputs.** Doubling the party, upgrading the weapon,
  or facing a tougher quarry each move it in the right direction — the assertion that no per-species
  turn count was baked in anywhere.
- **Damage spillover is exact.** One hunter against a line of rabbits kills `floor(damage /
  durability)` of them, not an authored count, and the number rises when the weapon improves.
- **No quantity of attackers rolls through the gate.** A large bare-handed party against megafauna
  kills zero over any horizon — pinned, because it is the one property a probabilistic gate would
  silently break (see §4.7).
- **A fractional engagement reaches one animal**, not zero — contact is not the gate, and a
  three-hunter mammoth party fails at the *fight*, with casualties, rather than failing to find it.
- **The fast path is free.** A one-sided engagement produces no casualties, no battle report, and
  costs no more than today's take path.
- **Distribution over seeds, with liveness.** Non-zero wariness produces a spread whose mean tracks
  the retreat probability, asserted across many seeds; paired with an assertion that takes are
  non-zero, because a dead retreat stage and a dead engagement stage both "pass" a range check.
- **A fill target below capacity shortens the trip; a target at or above it is an exact identity**
  (§5.2). Both halves are the assertion — the second is what makes the slice safe to land, and a
  target that silently did nothing would pass the first alone.
- **Trip length responds to the target.** Pinned directly, because §5.1's defect is precisely a
  number that looked like a lever and was not: assert that two different fill targets give two
  different trip lengths, and pair it with the invariance that caused the bug — with **no** target
  set, `turns_to_fill` is unchanged by party size.
- **The forecast names which bound ends the trip**, and its answer matches which one actually did.
  A raid that comes home on its fill target and a raid that comes home on the floor must be
  distinguishable in the readout, not merely in the turn count.
- **The pen is untouched** — a corral-tend take is byte-identical before and after slice 4. Penned
  animals are not engaged, not fought, and not wary.

---

## 11. Open questions

Every value is settled (§2.1, §4.2, §4.3, §4.8). What remains is what only play can answer.

| # | Question | Notes |
|---|---|---|
| 1 | **Do the roster values hold up in play?** | The risk they carry is specific: for most species the escapement floor binds long before engagement does, so an `engage_rate` set too low silently becomes a *second* floor. §6.6's hunt report is what makes that visible rather than mysterious. |
| 2 | **Is a coastal start too strong?** | Seal's ceiling of 24 is second only to megafauna, because seals are helpless on a haul-out. Historically right; possibly a start-position imbalance. Known, not discovered. |
| 3 | **What the fill target measures.** | **SETTLED as ANIMALS** — it matches the escapement graph's own units, so both levers speak one language. But it measures what comes **home**, not what dies: it caps a carry quantity, so a party too small to haul a whole animal kills more than the number typed. *"Take ≈50"* reads as *"bring home 50 worth"*. A kill target needs trip-scoped kill state and is a different change. |
| 4 | **Does the food economy settle after the body-mass correction?** | §4.3 cuts mid-game hunting yield to roughly a third and is deliberately uncompensated, on the reasoning that tuning around still-moving numbers bakes in a fix for a problem that may not survive them. |

---

## See Also

- `docs/plan_predators.md` — the combat subsystem this arc completes: `Force` / `Contingent` /
  `CombatProfile` / `FightOutcome`, the composition-not-a-scalar rule, and the seam-is-the-deliverable
  precedent
- `docs/plan_denial_raid.md` — the mission this unblocks, and the §1–§2 this doc replaces
- `docs/plan_harvest_floor.md` — the escapement floor, which sits above all four stages unchanged
- `docs/plan_early_game_labor.md` — TOE, which reaches the hunt through `CombatProfile`
- `.claude/rules/core_sim/combat.md` — combat & casualties, predation
- `.claude/rules/core_sim/fauna.md` — the herd model and `quantise_animal_take`
- `.claude/rules/core_sim/checkpoints.md` — why per-event seeding, not a shared stream
- `.claude/rules/core_sim/yield-forecast.md` — `forecast == actual`, and what a range does to it
