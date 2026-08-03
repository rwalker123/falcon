# The hunt is a fight — resolving the take through the combat system

**Status:** DESIGN. Nothing here is implemented. Issue #456, repurposed: the denial raid that issue
asks for turns out to need this first, and becomes small once it exists
(`docs/plan_denial_raid.md`).

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
turn, at any weapon tier (§4.4), so it is the number these are authored against.

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

### 4.3 Ferocity is already the right hinge

`SpeciesDef::ferocity` means *"does it fight back or flee"* — in resolver terms, **whether the animal
side contributes attack at all.** A ferocity-`0` gazelle is a one-sided engagement; a ferocity-`0.9`
mammoth is a real fight with real casualties. No separate "is this dangerous" flag is needed, and the
casualty path that exists today keeps working through the same field.

### 4.4 Most hunts must not feel like battles

Snaring rabbits is not a war. The answer is **one model with a degenerate fast path**, never two
models: when the animal side contributes no attack and its defense is at floor, the fight resolves
to *"everything that stayed dies, nobody is hurt"* without ceremony, cost or a battle report. A
second code path for small game would recreate exactly the parallel-model problem §0 exists to
delete.

### 4.4 Better weapons pay off on big game and nowhere else

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

### 4.5 Randomness lives in the attack, and never in the gate

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

### 4.6 A minimal TOE is in scope

Without equipment a hunter's `attack` is `1`, which is below every megafauna's `defense` — so **no
band can hunt a mammoth until spears exist**, and the progression stops being a note and starts being
load-bearing. That makes a minimal TOE part of this arc rather than a dependency beside it:

- **the hunting kit** — spears, raising `attack` above `1`, which is what opens the gate;
- **the carry kit** — baskets, raising the carry side (§5), the other half of `plan_early_game_labor`'s
  role table;
- consumable with the durability cliff, start-stocked, **not craftable**.

The crafting economy that replenishes kit stays deferred. What ships is enough to make `attack` a
real number and the equipped/unequipped distinction visible.

---

## 5. Carry is separate, and stays exactly as it is

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
- **The crafting economy.** A *minimal* TOE is in scope (§4.6); replenishing or upgrading kit is not
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
   and nothing else changes yet. Kill still resolves as today, so this slice is a no-op on outcomes
   for every species whose engagement exceeds today's take.
2. **`CombatProfile::wariness`** + the retreat stage, seeded per event; wariness `0` everywhere, so
   this slice is a provable identity.
3. **The take resolves through `resolve_fight`.** `quantise_animal_take`'s kill arm is replaced by
   the resolver's enemy losses; the herd-as-`Force` mapping and the one-sided fast path land here.
   Carry, waste and `max(1, carryable)` are untouched.
4. **Wariness values authored** across the roster — the first slice with visible behaviour change.
5. **Forecast + client**: the range readout, and the hunters-per-animal figure on the pre-launch
   panel.

Slices 1–2 are deliberately identities so slice 3 is the only one that can move a number.

---

## 10. Validation

- **`forecast == actual` still holds at wariness `0`**, per component, on the exported snapshot —
  the existing suite, unmodified, is the assertion.
- **Wariness `0` consumes no randomness.** Pinned directly: a turn with hunts at wariness `0` leaves
  the RNG state identical to a turn with none.
- **Replay determinism across hunt ordering.** Two runs that resolve the same hunts in different
  orders produce identical outcomes — the assertion that per-event seeding is real and no shared
  stream crept in.
- **The gate.** A party whose attack is below the quarry's defense kills **zero** at any headcount,
  and takes casualties proportional to `ferocity`. This is §0.2, pinned.
- **Better weapons pay off on big game and not on small.** Raising `attack` must raise biomass per
  hunter-turn for a high-defense quarry and leave an engagement-bound one flat (§4.4). The assertion
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
  silently break (see §4.5).
- **A fractional engagement reaches one animal**, not zero — contact is not the gate, and a
  three-hunter mammoth party fails at the *fight*, with casualties, rather than failing to find it.
- **The fast path is free.** A one-sided engagement produces no casualties, no battle report, and
  costs no more than today's take path.
- **Distribution over seeds, with liveness.** Non-zero wariness produces a spread whose mean tracks
  the retreat probability, asserted across many seeds; paired with an assertion that takes are
  non-zero, because a dead retreat stage and a dead engagement stage both "pass" a range check.
- **The pen is untouched** — a corral-tend take is byte-identical before and after slice 3. Penned
  animals are not engaged, not fought, and not wary.

---

## 11. Open questions

| # | Question | Notes |
|---|---|---|
| 1 | **`engage_rate` values.** | The roster needs a pass. The readable form (`1 / engage_rate` = hunters per animal) is what to author against — "twenty hunters to take a mammoth" is a judgement anyone can make; "0.05" is not. |
| 2 | **Do the `engage_rate` values hold up in play?** | §2.1 settles them against the ceiling ordering. The risk they carry is that for most species the escapement floor binds long before engagement does, so a rate set too low silently becomes a *second* floor. |
| 3 | **How much does a spear do?** | The only unset number in the model. Every effort figure in this doc is quoted against a provisional `attack 20`, and the unequipped-to-equipped jump (`1 → 20`) is the largest single multiplier in the design. Settled with the TOE kit values (§4.6). |

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
