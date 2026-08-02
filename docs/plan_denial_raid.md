# Denial is a raid, not a harvest rate — the mission that does not clamp to carry

**Status:** DESIGN. Nothing here is implemented. Issue #456.

**It rides on `docs/plan_hunt_through_combat.md`, which came out of this one.** Specifying denial
surfaced the fact that the sim has no model of *killing* at all — the take is bounded by what the
party can carry — so denial had nothing to unclamp. That turned out to be a hole in the general hunt
model, and its answer is to resolve the hunt through the combat system. **This doc's earlier §1–§2
specified a kill model outside the resolver and has been deleted**; see that doc's §7 for what was
removed and why.

What remains here is denial itself, which is small once combat owns the kill: **the mission that
engages hard and does not clamp to carry.**

---

## 0. Why `floor = 0` is not denial

#451 collapsed the four-stance axis onto one escapement dial, and `floor = 0` — *"leave nothing
standing"* — inherited the job Eradicate used to do. It cannot do that job, for a reason that has
nothing to do with the floor.

`fauna::quantise_animal_take` bounds the kill by the party's **carry**:

```text
killed = min(affordable, max(1, carryable))
```

`systems::hunt_take` says why, and is right: folding carry room into the collection *"keeps a
nearly-full party from slaughtering an animal it has no room for."* Hunters do not kill what they
cannot use. That is a good model of subsistence hunting and exactly the wrong model of denial, whose
entire premise is killing what you have no intention of using.

So at `floor = 0` a party still only kills what it can haul, which makes erasing a herd as slow and
as crew-hungry as eating it. **Denial has to be a mission, because the thing it changes is a bound,
not a number.**

---

## 1. The mission

`ExpeditionMission::Deny { fauna_id }`, wire key `"deny"`.

**It carries no floor and no rate.** That is the whole reason it is a separate mission rather than a
number on the assign dialog — no floor, no crew stepper, nothing to tune. You choose a herd and a
party size. `hunt_floor()` reports `0.0` for it (the escapement ceiling is the herd's whole standing
stock), and `floor` never appears in its command text or its UI.

**One line differs from a hunt.** With the hunt resolving through combat
(`plan_hunt_through_combat.md` §1), the four stages are engagement → retreat → fight → carry, and
denial changes only the last:

```text
hunt:    carried = min(killed × body_mass, carry_capacity, carry_room)
         …and the party stops engaging once its pack is full

denial:  carried = min(killed × body_mass, carry_capacity, carry_room)   // identical
         …and the party never stops engaging
```

Denial still banks whatever it can haul on the way home — a rounding error against what it killed,
which is the point. `AnimalTake { killed, carried, wasted }` already models kill ≠ carry and needs no
new field, and `ExpeditionPhase`, party outfitting, travel and the `Hunting` / `Delivering` /
`Returning` cycle are reused unchanged.

**Denial is where `wasted` finally matters.** On a hunt it is the occasional overflow of an animal
too big to haul; on a raid it is essentially the whole take.

### 1.1 Success is the point of no return, not zero

The ecology is already in place. Below `ecology.collapse_fraction` (`0.15`) `net_biomass_delta` zeroes
the growth flow and the herd declines irreversibly at `collapse_rate`; below `extinction_floor`
(`0.02`) it despawns.

So a raid's goal is **push the herd under the collapse threshold and walk away** — not kill every
animal. That is what lets a small party erase a large placid herd in a few turns, and it is why
ordinary subsistence hunting never does it by accident: any floor above `collapse_fraction` stops
long before the party's engagement capacity is reached.

**The forecast twin.** `DenialForecast::turns_to_collapse` — *"this party takes them below the point
of no return in 3 turns"* — is the denial analogue of `HuntTripForecast::turns_to_fill`, produced by
the same bounded forward simulation — reported as a **range**, since retreat is stochastic
(`plan_hunt_through_combat.md` §6.4).

### 1.2 What it costs

Travel, party exposure, and near-zero return are the costs #456 lists. TOE supplies a fourth, and it
is the best one, with no new mechanism required:

> **A denial raid is the most equipment-intensive act in the game.** You engage continuously, for no
> food return. If durability wears with *use*, denial burns the hunting kit in proportion to the
> slaughter — and in M1 that kit is **irreplaceable**. You spend your own ability to feed yourself
> to remove someone else's.

**This does not gate anything.** Denial ships against the equipment seam at the unequipped tier; its
costs until TOE lands are travel, exposure and the forgone food, which is a playable mission. The kit
cost is a *consequence to compose* when slice 5 arrives, not a precondition to wait for.

The one thing to carry into that conversation: the payoff only appears if durability wears with
**kills** rather than with turns elapsed, since a turn-based clock charges a raid the same as an idle
march. The TOE doc's own wording — "wears down with use" — already reads that way.

---

## 2. Not in scope

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
  specify; equipment reaches the hunt through `CombatProfile` either way.
- **Warriors escorting a raid.** Unchanged from `plan_predators.md` §7d — a hunt's danger is the
  hunting party's own.

---

## 3. UI

- **Mission choice at launch**, beside Scout and Hunt. Not a floor preset, not a checkbox on the hunt
  dialog — a third verb.
- **The verdict line is `turns_to_collapse`**, the same way `turns_to_fill` is the hunt's. *"Your 8
  hunters take this herd past recovery in 4 turns."* When the party cannot get there at all
  (its kills per turn below the herd's regrowth), it must say **that**, not show a blank.
- **Waste is stated, never hidden.** `AnimalTake::wasted` is nearly the whole take; the readout says
  so. `SourceYield.wasted` already carries it on the hunt path.
- **The forecast is a range**, not a promise, because retreat is stochastic
  (`plan_hunt_through_combat.md` §6.4). A raid's verdict line inherits that: *"past recovery in 3–5
  turns."*

---

## 4. Slices

Both land after `plan_hunt_through_combat.md` slice 3, which is what gives denial something to
unclamp.

1. **The mission.** `ExpeditionMission::Deny`, wire key, command text, checkpoint,
   `hunt_floor() = 0`, and the one behavioural difference: the party does not stop engaging when its
   pack is full. `DenialForecast::turns_to_collapse`.
2. **Client.** A third verb at launch; the collapse verdict line as a range; the waste readout.

---

## 5. Validation

- **A denial raid reaches collapse where a hunt does not.** A placid herd raided by a given party
  crosses `collapse_fraction` in bounded turns and then declines with no further pressure; the same
  herd, same party, hunted at any floor above `collapse_fraction`, never does — however long it is
  hunted. This is the whole mechanic, and it is one test.
- **A wary herd resists denial**, and the forecast says so rather than returning a silent `None`.
- **`wasted` is the bulk of a raid's take** and is reported, not hidden.
- **A raid delivers food.** Small, but non-zero — paired with the above as a liveness assertion,
  since a raid that delivers nothing at all also "passes" a waste-is-large check.
- **`forecast == actual` in distribution**, over seeds, per component — the standing obligation in
  the form §6 of the combat doc leaves it in.

---

## 6. Open questions

| # | Question | Notes |
|---|---|---|
| 1 | **Does a collapsing herd tell anyone?** | §2 settles that denial names no target faction. But a herd crossing `collapse_fraction` is visible ecology, and whether its *other* users are told — and how — is unresolved. |
| 2 | **Does a raid recur?** | `raid_is_recurring(floor)` governs whether a hunt relaunches. A raid that reached collapse has nothing to return to; one that failed probably should not auto-relaunch into a loss. |
| 3 | **Is denial legible as distinct from a deep hunt?** | The mechanical difference is one clamp. If a player cannot feel why the raid is different from `floor = 0`, the mission has failed even if the sim is right. |

---

## See Also

- `docs/plan_hunt_through_combat.md` — the arc this rides on, and the §1–§2 this doc used to carry
- `docs/plan_harvest_floor.md` — the escapement dial, and why `floor = 0` is a harvest and not a denial
- `docs/plan_predators.md` — the combat subsystem, and the Warriors-do-not-escort rule
- `docs/plan_early_game_labor.md` — TOE, the kit a raid burns
- `docs/plan_exploration_and_sites.md` — the expedition machinery the mission reuses wholesale
- `.claude/rules/core_sim/fauna.md` — the ecology bands a raid aims to cross
