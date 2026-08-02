# Denial is a raid, not a harvest rate — the kill rate, and the mission that unclamps it

**Status:** DESIGN. Nothing here is implemented. Issue #456.

**The dependency is resolved.** #456 was filed as blocked on #451, which has since landed on all four
slices: `FollowPolicy` and its `Eradicate` variant no longer exist, and `ExpeditionMission::Hunt`
carries a `floor: f32`. The field's own doc comment already hands denial off to this doc.

**This doc specifies two things that land under one issue, in order.** §1–§3 are a change to the
**general hunt model** that affects every hunt in the game; §4–§6 are the denial mission, which rides
on it. They are separate slices (§7) because the balance change wants its own PR to be reviewable,
not because they are separate pieces of work.

---

## 0. Why `floor = 0` is not denial

#451 collapsed the four-stance axis onto one escapement dial, and `floor = 0` — *"leave nothing
standing"* — inherited the job Eradicate used to do. It cannot do that job, for a reason that has
nothing to do with the floor.

### 0.1 The kill is bounded by carry, and for subsistence that is correct

`fauna::quantise_animal_take` is the one place a take becomes whole animals:

```text
affordable = floor(ceiling    / body_mass)   // what the herd can spare — a STOCK
carryable  = floor(collection / body_mass)   // what the party can haul — a CARRY term
killed     = min(affordable, max(1, carryable))
```

where `collection = workers × per_worker_biomass_capacity × build_dip`, clamped by the caller's
carry room. So the **kill is bounded by the carry**. `systems::hunt_take` says why, and is right:
folding carry room into the collection "keeps a nearly-full party from slaughtering an animal it has
no room for." Hunters do not kill what they cannot use.

That is a good model of subsistence hunting. It is exactly the wrong model of denial, whose entire
premise is killing what you have no intention of using.

### 0.2 Removing the carry bound alone erases a herd in a single turn

The obvious reading of #456 §2 — *invert the line so the kill is bounded by what the party can kill*
— does not work as stated, because there is nothing to invert **into**. Drop `carryable` and the kill
falls back to `affordable` alone, which at `floor = 0` is the herd's entire standing biomass. Two
hunters would erase a 12,000-biomass mammoth herd in one turn.

That contradicts what the issue asks for in the same breath: *"a small party genuinely able to erase
a large placid herd **over a few turns**"*, and *"how many hunters it takes should come from the
animal."*

### 0.3 There is no killing-rate term in the model at all

This is the actual hole. `per_worker_biomass_capacity` is documented on `SpeciesDef::body_mass` as
**"Party size = how much of the kill you keep"** — one hunter keeps 80% of a boar, 5% of a mammoth.
It is a *carry* term that the model also leans on as the kill bound because nothing else existed.

The consequence reaches well past denial: **crew throughput has no species term whatsoever.** Hunting
a gazelle and hunting a mammoth have identical per-worker biomass throughput; only `body_mass` (the
quantiser) and `HuntYield` (what the carcass converts to) differ. A herd that scatters at the sight
of you yields the same rate as one that stands and watches.

So denial is not the feature that needs a new number. Denial is the case that **exposes** a missing
one.

---

## 1. The model

> **Kill rate is the ceiling. A hunt additionally clamps to carry. Denial does not.**

```text
kill_capacity  = workers × hunt.per_worker_kill_capacity × equipment
                         × engagement(wariness) / toughness(defense)

carry_capacity = workers × hunt.per_worker_biomass_capacity × build_dip      // unchanged

kill_bank += kill_capacity                                                   // NEW — see §1.2

affordable = floor( ceiling   / body_mass )                                  // the herd's spare stock
killable   = floor( kill_bank / (body_mass + hunt.stalk_overhead) )          // NEW
carryable  = floor( min(carry_capacity, carry_room) / body_mass )            // today's bound

killed  = if denial { min(affordable, killable) }
          else      { min(affordable, killable, max(1, carryable)) }

kill_bank -= killed × (body_mass + hunt.stalk_overhead)

carried = min( killed × body_mass, carry_capacity, carry_room )
wasted  = killed × body_mass − carried
```

`ceiling` is unchanged: `hunt_escapement_ceiling(floor, B, K)` = `max(0, B − floor·K)`. The escapement
model from #451 is untouched — this adds a bound beside it, it does not replace it.

**`carried` keeps its existing formula in both missions.** The mission flag changes exactly one
thing: whether `carryable` participates in the *kill*. Denial still banks whatever the party can haul
on the way home — a rounding error against what it killed, which is the point.

### 1.1 What the kill capacity is, and why it is denominated in biomass

`hunt.per_worker_kill_capacity` is **the biomass of animal one hunter can bring down per turn** — the
killing twin of `hunt.per_worker_biomass_capacity`, which is the biomass one hunter can *carry* per
turn (`40` today). Same unit on purpose: the two are compared, so they have to be commensurable.

It is a **global rate**, and it is not wariness. Wariness is the per-species modifier applied to it.
The full chain reads:

| term | scope | says |
|---|---|---|
| `workers` | the party | how many hunters |
| `hunt.per_worker_kill_capacity` | global config | what one hunter brings down per turn, baseline |
| `equipment` | the band (TOE) | spears or bare hands |
| `engagement(wariness)` | **per species** | whether it lets you close |
| `toughness(defense)` | **per species** | how hard it is once you have |

Denominating the budget in biomass is the short way of writing *"killing effort scales with the
animal's size"* — a mammoth is 20 deer of work because it is 13 deer of animal and fights harder.
Written out as an animal count it would be `kills_per_worker / difficulty(body_mass, defense)` with
`difficulty` linear in mass, which is the same expression with more moving parts.

**`hunt.stalk_overhead` is what stops that from breaking at the small end.** Effort proportional to
mass alone says a party that takes one 800-biomass mammoth can take **2,600 rabbits** in the same
turn, which is absurd: you have to find, stalk and kill each animal individually no matter how small
it is. The overhead is that fixed per-animal cost, expressed in the same biomass currency, so every
animal costs `body_mass + stalk_overhead` against the budget. At `5` it is invisible on a mammoth
(`805` vs `800`) and dominant on a rabbit (`5.3` vs `0.3`), which is exactly the right shape.

### 1.2 The one-animal floor stays on carry; the kill side banks instead

**`max(1, carryable)` survives untouched.** It exists so a party too small to *haul* a whole animal
still takes one and wastes the rest, and that is still the right rule — it is the source of hunting's
waste, and removing it silently deletes waste from the game everywhere except denial. Worse, without
it big game becomes all-or-nothing: `carryable` is `0` for any party under twenty hunters on an
800-biomass mammoth, so `min(…, carryable)` would zero the kill outright and a mammoth herd would
feed nobody at all until the party crossed exactly twenty.

**The kill side takes the opposite treatment: it accumulates.** A party whose kill capacity is below
one animal does not fail — it banks toward one, in `Herd::hunt_credit`, and lands a kill when the
bank clears `body_mass + stalk_overhead`. Five hunters take a mammoth every third turn; three hunters
every fifth. That is the wait-then-feast rhythm `SpeciesDef::body_mass` already describes, and it is
what keeps megafauna *accessible* to a small band rather than gated behind a headcount cliff.

**Banking here is legitimate, and banking the ceiling was not.** #451 stopped `hunt_take` from
advancing `hunt_credit` because the escapement ceiling is a **stock**: adding it to an accumulator
offered the herd's whole surplus plus everything it had already handed over, compounding a quantity
that was never a flow. Kill capacity *is* a flow — biomass-of-animal per turn — so an accumulator is
its correct integral, and the field the bank left behind is the one it moves into. **The distinction
is structural, not a convention to remember:** a stock must not bank, a rate must.

### 1.3 The kill bound is opt-in per caller

`quantise_animal_take` is shared by the resident band's Hunt, the scout's opportunistic replenish,
the expedition, the forecast, **and the pen's corral-tend branch**. A penned animal is not being
hunted — nothing is stalked, nothing flees — so **the pen passes an infinite kill capacity** and its
behaviour is unchanged. Wariness and defense are properties of taking an animal *in the wild*.

---

## 2. What sets the kill rate

Three factors, and only one of them is new authored data.

### 2.1 `wariness` — authored, because nothing in the repo covers it

`engagement(wariness) = 1 − wariness`, on `[0, 1)`. High wariness = the herd scatters before you are
in range; low wariness = it stands and lets you work.

**Derivation was tried against both candidates and fails on the motivating case.**

- **`animals_per_herder`** reads like placidity — "how many of these one person can control" — but it
  is **absent on precisely the untameable species**. Every `husbandry_ceiling: "wild"` entry —
  mammoth, deer, seal, wild elk, alpine ibex, gazelle, wolf — carries no `animals_per_herder` and no
  `taming_rate`. A derived placidity is undefined on the whole set the mechanic exists for, and makes
  *"placid but never domesticated"* unrepresentable. That is the bison, which is the entire reason
  for the mechanic.
- **`ferocity`** is the tempting fallback and reads **backwards**. Gazelle is `0.05` and deer `0.15`,
  while mammoth is `0.9`. Ferocity asks *"does it fight back"*; skittish-and-fast is indistinguishable
  from placid-and-standing-still on that axis, so inverting it would make gazelle the easiest herd in
  the game to slaughter en masse. It is also already **spent**: hunt danger is `attack × ferocity`,
  consumed by the casualty adapters, so it was never available for rate.

Wariness is orthogonal to both, and to defense: mammoth is high-defense / low-wariness (stands and
fights), gazelle is low-defense / high-wariness (gone before you are in range), boar is
moderate-both. Neither predicts the other, which is the authoring case.

This is against the repo's usual derive-don't-author instinct, and `husbandry_ceiling` exists
precisely to avoid parallel flags. The distinction: `husbandry_ceiling` was introduced because
*three* derived signals disagreed about one question. Here **no signal answers the question at all.**

It completes the behaviour vocabulary rather than adding a fourth loose flag, with no overlap:

| field | question | consumed by |
|---|---|---|
| `aggression` | does it come to your camp | predator raid trigger (`attack × aggression`) |
| `ferocity` | what it costs you to fight it | hunt casualties (`attack × ferocity`) |
| `defense` | how hard it is to bring down | **kill rate** (new) |
| `wariness` | whether you get near it at all | **kill rate** (new) |

`#[serde(default)] = 0.0` and validated finite in `[0, 1)`, matching `aggression` / `ferocity`. A
species at `1.0` would be unhuntable, so the range is half-open. All twenty species are authored
explicitly; nothing relies on the default.

Proposed values — playtest dials, and the ordering matters more than the numbers:

| low (stands) | | moderate | | high (scatters) | |
|---|---|---|---|---|---|
| mammoth | 0.10 | reindeer | 0.45 | crag_goat | 0.60 |
| aurochs | 0.20 | marsh_grazer | 0.50 | fowl | 0.65 |
| boar | 0.25 | wild_elk | 0.50 | deer | 0.65 |
| seal | 0.35 | wild_sheep | 0.50 | alpine_ibex | 0.70 |
| river_fish | 0.40 | wild_horse | 0.55 | wolf | 0.70 |
| | | steppe_runner | 0.60 | rabbit / snow_hare | 0.75 |
| | | forest_grouse | 0.60 | gazelle | 0.85 |

### 2.2 `defense` — derived, and already authored

`toughness(defense) = 1 + defense × hunt.defense_weight`. `combat.defense` exists on every species
with a combat block and means exactly *"how hard is it to bring down"*; it is currently read only by
the predator path.

**`body_mass` does the size work; `defense` only adds a bounded "fights back hard" band.** At
`defense_weight = 0.08` the spread across the roster is `1.08` (deer, gazelle, goat) to `1.96`
(mammoth) — roughly 2×. That is what keeps this from double-penalising big game: a mammoth is hard to
kill *and* hard to haul, both true, but the size term lives in `body_mass` and is not paid twice.

Small game — rabbit, fowl, forest_grouse, snow_hare, river_fish — carries **no combat block at all**.
`defense` defaults to `0`, `toughness` to `1.0`, and those species end up governed by wariness alone.
That is correct: killing rabbits is limited by finding them, not by overpowering them.

### 2.3 `equipment` — the TOE seam

**Weapons are not a new system.** `docs/plan_early_game_labor.md` specifies TOE (Table of Equipment)
in full — per-role kit, *unequipped* / *equipped* throughput tiers, consumable with a durability
cliff, start-stocked and not craftable in M1, with depletion as "the pacing dial of the first act."
It is **slice 5 of that arc and entirely unbuilt**: there is no equipment struct, no durability, and
no role enum anywhere in `core_sim`. The only two code references are comments marking it deferred.

`plan_predators.md` already asserts what this doc makes mechanical:

> *"the hunt's only levers are the hunting party: its numbers, and (via TOE) its gear."*

**What this arc contributes to TOE is a disambiguation.** Its role table says *"spears/traps → higher
take"*. Before the kill/carry split that was ambiguous, because take was one number. It now resolves
in the direction the same table already anticipates for the forage role — note that baskets raise
*"forage yield **and** carry capacity"*, two effects, stated separately:

> **The Hunting TOE multiplies the KILL rate, and nothing else.** A spear helps you bring the animal
> down; it does nothing to help you haul it home. What raises hunting *carry* is containers — a
> different kit, with the basket-side effect.

**Shipped as a seam.** `equipment` is a constant at the unequipped tier until TOE slice 5 fills it,
following the precedent `plan_predators.md` set for the combat resolver: *"Placeholder resolver now;
the seam is the deliverable, so the real model drops in without touching callers."*

### 2.4 Calibration

The constants are playtest dials; what must hold is the **shape**. Illustrative, at
`per_worker_kill_capacity = 120`, `stalk_overhead = 5`, `defense_weight = 0.08`, equipped `= 1.0`,
unequipped `= 0.4`, against today's `per_worker_biomass_capacity = 40`:

| case | killable | carryable | killed | binds | today |
|---|---|---|---|---|---|
| 20 hunters, mammoth (w 0.10, def 12, 800) | 1 | 1 | **1** | either | 1 |
| 5 hunters, mammoth — banks 276/turn | 1 per **2.9 turns** | 0 → `max(1,·)` | **1 per 2.9** | **kill** | 1 per turn |
| 5 hunters, wild elk (w 0.50, def 3, 40) | 5 | 5 | **5** | carry | 5 |
| 5 hunters, deer (w 0.65, def 1, 60) | 2 | 3 | **2** | **kill** | 3 |
| 5 hunters, gazelle (w 0.85, def 1, 4) | 9 | 50 | **9** | **kill** | 50 |
| 5 hunters, rabbit (w 0.75, no combat, 0.3) | 28 | 666 | **28** | **kill** | 116 (herd-bound) |
| 20 hunters, mammoth, **unequipped** | 1 per 2.5 turns | 1 | **1 per 2.5** | **kill** | 1 per turn |

Read the middle rows: a skittish herd now yields materially less than the same crew could haul,
which is the general-hunting effect this change exists for; small game is bounded by how many animals
you can stalk rather than by the absurd carry number; and equipment shows up as *cadence* — an
unequipped party still works a mammoth, at two and a half turns per kill instead of one.

**The dial has a real trade-off in it, and it is the design.** `per_worker_kill_capacity` sets how
often kill binds instead of carry. Push it high and normal hunting is always carry-bound (wariness
stops mattering); push it low and denial is barely faster than harvesting. The interesting band is
where **which bound binds depends on the species** — and the resulting property is a good one:

> **You can only erase the animals that let you near you.** A placid herd is carry-bound in ordinary
> hunting and collapses fast under denial; a wary herd is kill-bound in ordinary hunting and is
> denial-resistant, because you could never get near enough to do it.

---

## 3. What this changes for ordinary hunting

Everything in §1–§2 is live for every hunt, not just raids. Landing it is a balance change:

- Wary species yield less to the same crew than they did. Placid species are unchanged (carry still
  binds).
- A party can now be **kill-bound**, a state that did not exist. `HuntTripForecast` and the per-herd
  yield preview must be able to say which bound is binding, or the player sees a number drop with no
  explanation.
- Unequipped parties get materially worse at big game — which is the pull into TOE, and inert until
  slice 5 lands.
- **The client already exports the answer rather than re-deriving it.** #451 established that a take
  is not client-reproducible (`floor()` is not linear), so `fauna::hunt_source_yield_preview` →
  `SourceYield` is the seam, and it picks the kill bound up for free.

One thing gets *more* wrong and is already flagged in code:
`expedition_per_worker_provisions` is species-blind by necessity (a cohort has no herd, so no
`HuntYield` to resolve). It already carries a warning not to use it for a per-herd preview; the kill
rate widens the gap it warns about, and the warning should be updated to say so.

---

## 4. The denial mission

`ExpeditionMission::Deny { fauna_id }`, wire key `"deny"`.

**It carries no floor and no rate.** That is the whole reason it is a separate mission rather than a
number on the assign dialog — no floor, no crew stepper, nothing to tune. You choose a herd and a
party size. `hunt_floor()` reports `0.0` for it (the escapement ceiling is the herd's whole standing
stock), and `floor` never appears in its command text or its UI.

Everything else is reused unchanged: `ExpeditionPhase`, party outfitting, travel, the `Hunting` /
`Delivering` / `Returning` cycle, and `AnimalTake { killed, carried, wasted }`, which already models
kill ≠ carry and needs no new field.

### 4.1 Success is the point of no return, not zero

The ecology is already in place. Below `ecology.collapse_fraction` (`0.15`) `net_biomass_delta` zeroes
the growth flow and the herd declines irreversibly at `collapse_rate`; below `extinction_floor`
(`0.02`) it despawns.

So a raid's goal is **push the herd under the collapse threshold and walk away** — not kill every
animal. That is what lets a small party erase a large placid herd in a few turns, and it is why
ordinary subsistence hunting never does it by accident: any floor above `collapse_fraction` stops
long before the kill bound is reached.

**The forecast twin.** `DenialForecast::turns_to_collapse` — *"this party takes them below the point
of no return in 3 turns"* — is the denial analogue of `HuntTripForecast::turns_to_fill`, produced by
the same bounded forward simulation, with the same `forecast == actual` obligation.

### 4.2 What it costs

Travel, party exposure, and near-zero return are the costs #456 lists. TOE supplies a fourth, and it
is the best one, with no new mechanism required:

> **A denial raid is the most equipment-intensive act in the game.** You kill at maximum rate with
> the carry bound removed, for no food return. If durability wears with *use*, denial burns the
> hunting kit in proportion to the slaughter — and in M1 that kit is **irreplaceable**. You spend
> your own ability to feed yourself to remove someone else's.

**This does not gate anything.** Denial ships against the equipment seam at the unequipped tier; its
costs until TOE lands are travel, exposure and the forgone food, which is a playable mission. The kit
cost is a *consequence to compose* when slice 5 arrives, not a precondition to wait for.

The one thing to carry into that conversation: the payoff only appears if durability wears with
**kills** rather than with turns elapsed, since a turn-based clock charges a raid the same as an idle
march. The TOE doc's own wording — "wears down with use" — already reads that way.

---

## 5. Not in scope

- **No target faction.** Denial is aimed at a herd, not at a player. Whether a raid names a victim and
  surfaces to them is a diplomacy question this arc does not answer, and adding a nullable target now
  would be a field nothing reads. **Settled deliberately, not deferred by omission.**
- **No plant twin.** `Eradicate` on a forage patch does not ship. The asymmetry is worth stating
  rather than papering over, and it falls out of config that already exists: `reseed_floor_fraction`
  (`0.02`) guarantees a patch comes back, and plants have no Allee term, so **a herd can be erased
  permanently and a stand can only be set back.** Burning a stand is a delay, not a denial. If the
  plant twin is ever wanted it is a different mechanic, not this one with a different noun.
- **Equipment affecting hunt survivability.** `plan_predators.md` already designates TOE as the answer
  to dangerous game ("answered by equipping them (TOE), never by a guard"). That is the TOE arc's to
  specify; this doc claims only the kill-rate effect.
- **Warriors escorting a raid.** Unchanged from `plan_predators.md` §7d — a hunt's danger is the
  hunting party's own.

---

## 6. UI

- **Mission choice at launch**, beside Scout and Hunt. Not a floor preset, not a checkbox on the hunt
  dialog — a third verb.
- **The verdict line is `turns_to_collapse`**, the same way `turns_to_fill` is the hunt's. *"Your 8
  hunters take this herd past recovery in 4 turns."* When the party cannot get there at all
  (kill-bound below the herd's regrowth), it must say **that**, not show a blank.
- **Waste is stated, never hidden.** `AnimalTake::wasted` is nearly the whole take; the readout says
  so. `SourceYield.wasted` already carries it on the hunt path.
- **The bound that binds is named** on the hunt readout (§3) — "your crew can carry more than it can
  kill" is the sentence that explains an otherwise inexplicable number.

---

## 7. Slices

One issue (#456), five slices, each its own PR. Slices 1–3 are the general hunt model and are
independently reviewable as a balance change; 4–5 are the mission.

1. **The species terms.** `SpeciesDef::wariness` + all twenty values;
   `hunt.per_worker_kill_capacity`, `hunt.stalk_overhead`, `hunt.defense_weight`, and the `equipment`
   seam constant at the unequipped tier.
2. **The bound.** `quantise_animal_take` takes a kill capacity; `max(1, carryable)` comes out; the pen
   branch passes infinity. `hunt_take` and `expedition_take_biomass` compute it; the forecast follows
   through the same helper so `forecast == actual` holds per component.
3. **The wire + client.** `SourceYield` / `HuntTripForecast` report which bound binds; the readout
   says so.
4. **The mission.** `ExpeditionMission::Deny`, wire key, command, checkpoint, `hunt_floor() = 0`,
   `carryable` dropped from the kill. `DenialForecast::turns_to_collapse`.
5. **Client.** Third verb at launch; the collapse verdict line; waste readout.

**TOE slice 5 is elsewhere and blocks nothing** — it fills the equipment seam and lights up denial's
kit cost (§4.2) whenever it lands.

---

## 8. Validation

- **`forecast == actual` per component, on the exported snapshot** — the standing obligation, over
  both bounds (kill-bound and carry-bound staffing), a defaulting species and an inedible one
  (`wolf`), and both missions. `hunt_yield_vector.rs` is the model.
- **A liveness assertion beside every ordering one.** A diff-based metric improves when the feature
  breaks; assert takes are non-zero where they should be, not merely ordered.
- **Monotonicity:** the kill bound is non-increasing in `wariness` and in `defense`, and
  non-decreasing in `workers` and `equipment`.
- **The pen is untouched** — a corral-tend take is byte-identical before and after slice 2. This is
  the regression the shared quantiser makes easy to cause.
- **A sub-threshold party banks rather than stalling.** Kill capacity below one animal yields zero
  kills for N turns and then exactly one — never zero forever, and never a fractional animal. Pinned
  with a liveness assertion, because "always zero" and "correctly waiting" look identical for the
  first few turns.
- **`max(1, carryable)` still fires**, and its waste with it: a five-hunter party takes a mammoth and
  leaves 600 biomass on the range. Pinned, because deleting it is the easy mistake — it reads like
  the kill bound's job now, and removing it silently zeroes big game below twenty hunters.
- **The §2.4 table is a test**, not just prose — all six rows pinned against the shipped config.
- **Small game is stalk-bound, not carry-bound.** A rabbit take is bounded by `stalk_overhead` and
  lands in the tens, never the hundreds; a party's rabbit and mammoth takes must not differ by their
  mass ratio. This is the assertion that would have caught the model before `stalk_overhead` existed.
- **Denial reaches collapse; hunting does not.** A placid herd under a denial raid crosses
  `collapse_fraction` in bounded turns and then declines without further pressure; the same herd under
  any floor above `collapse_fraction` never does, however long it is hunted.
- **A wary herd resists denial** — the same party size fails to reach collapse, and the forecast says
  so rather than returning `None` silently.

---

## 9. Open questions

| # | Question | Notes |
|---|---|---|
| 1 | **`per_worker_kill_capacity`'s value.** | §2.4 sets the trade-off it controls. `120` is a starting point calibrated to leave mid-roster species carry-bound; it wants a live pass. |
| 1a | **Is killing effort really linear in `body_mass`?** | §1.1's budget says a 13×-heavier animal is 13× the work, corrected only by `stalk_overhead` at the small end. If the big end also needs bending (a mammoth is not 2,600 rabbits of work in *either* direction) that is an exponent on `body_mass`, one more config lever and no structural change. |
| 1b | **Rename `per_worker_biomass_capacity` → `per_worker_carry_capacity`?** | It has meant "carry" since it was written (`SpeciesDef::body_mass` documents it as such) but was the only per-worker rate, so the bare name was unambiguous. With a kill twin beside it, it no longer is. Cheap rename, entirely internal. |
| 2 | **Does `engagement` interact with party size?** | Realistically a bigger party is easier to detect, making throughput sub-linear in workers for wary species. Specified as linear; the nonlinearity is a config lever if wanted, and the client already cannot re-derive the take. |
| 3 | **Does a collapsing herd tell anyone?** | §5 settles that denial names no target. But a herd crossing `collapse_fraction` is visible ecology — whether the *other* users of that herd are told, and how, is unresolved. |
| 4 | **The wariness values themselves.** | §2.1's table is a proposal. Ordering is the load-bearing part; the numbers are dials. |
| 5 | **Does denial have a recurrence?** | `raid_is_recurring(floor)` governs whether a hunt relaunches. A raid that reaches collapse has nothing to return to; one that fails probably should not auto-relaunch into a loss. |

---

## See Also

- `docs/plan_harvest_floor.md` — the escapement dial this adds a second bound beside; §1 is unchanged
  by this arc
- `docs/plan_early_game_labor.md` — TOE, the equipment model §2.3 seams for and disambiguates
- `docs/plan_predators.md` — the combat vocabulary `defense` and `ferocity` belong to, the
  seam-is-the-deliverable precedent, and the Warriors-do-not-escort rule
- `docs/plan_hunt_yield_model.md` — the yield vector `carried` is converted through, unchanged here
- `docs/plan_exploration_and_sites.md` — the expedition machinery the mission reuses wholesale
- `.claude/rules/core_sim/fauna.md` — the herd model, `quantise_animal_take`, and the ecology bands
- `.claude/rules/core_sim/yield-forecast.md` — `forecast == actual`, the invariant every slice holds
