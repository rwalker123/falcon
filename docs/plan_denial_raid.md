# Denial is a raid, not a harvest rate — the mission that does not clamp to carry

> **The forecast's trade terms are RETIRED** (arc #527, `docs/plan_contact_and_logistics.md`).
> `DenialForecast::delivered_trade` / `wasted_trade` are gone with the axis, so the launch sheet no
> longer states what a carcass left on the range takes with it. **Deliberately not replaced**: the
> waste is already legible as a percentage, and a flat "wasted materials" scalar would be the retired
> axis under a new name. Everything else here — the engagement stop, the collapse completion,
> `turns_to_collapse`, `party_needed` — is unchanged.

**Status:** **§4 slice 1 (the sim) has LANDED**; slice 2 (the client) is design. Issue #456.

Landed: `ExpeditionMission::Deny`, the `send_denial_raid` command, `fauna::EngagementStop` and the one
behavioural difference it carries, the collapse completion, `DenialForecast::turns_to_collapse` as a
range, and the `denialEstimates` wire table. Engineering as-built:
`.claude/rules/core_sim/expeditions.md` → "Denial is a MISSION, not a floor".

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

`fauna::quantise_animal_take` bounds the kill by the party's **carry** — and by the fight, and by
nothing else: it holds **no room ceiling at all** now, the herd's spare being spent one stage earlier
at the engagement (`.claude/rules/core_sim/expeditions.md` → the take-bound table).

```text
killed = min(brought_down, animals_the_pack_seats(collection))
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

### 3.1 A viable party must be REACHABLE, and the sheet opens on it

Reported from play: **Red Deer, herd 51 of 119 head, band with 16 idle workers.** The party stepper
capped at **8** and the verdict correctly read *"breeds back faster than this party kills"* at every
size the player could reach.

```text
one hunter kills    engage_rate 1 × (1 − wariness 0.65) = 0.35 deer/turn
the herd replaces   0.10 × 51 × (1 − 51/119)            = 2.91 deer/turn
break-even          2.91 / 0.35                          = 8.3 hunters   ⇒  9 to decline
the config lever                                         = 8
```

**Two unrelated eights.** The lever is a flat config number applying to any expedition; the 8.3 is
this herd's requirement. The lever landed one below it, so denial on this quarry was unreachable —
not because the band was too small, but because a lever said so.

#### The lever was doing two jobs under one name, and only one is legitimate

1. **A sampling bound, which is real.** `huntTripEstimates` and `denialEstimates` hang off
   `HerdTelemetryState` — per **herd**, not per band — so the sim cannot know which band is asking or
   how many workers it has. Those tables need a fixed axis, and the hunt table is already
   `floors × party sizes`, so the row count is a genuine budget.
2. **A rules cap on what the player may send, which has no justification.** No design note ever
   backed it, and the honest bound is the one the panel already displays: **the band's own workers.**
   You cannot send hunters you do not have; you can send all the ones you do.

**So they are split.** `expedition_config.max_party_size` is renamed **`estimate_party_sizes`** and
does job 1 only. Every launch verb — `send_expedition`, `send_hunt_expedition`, `send_denial_raid` —
bounds a party by `available_workers` and nothing else. A party of 9, 12 or 16 is legal when the band
has the people. Wary herds are therefore **expensive, not undeniable**, which is what wariness is for.

> **This deliberately changes a HUNT's party sizing too.** A hunting party is no longer capped at 8
> either. That is the ruling followed to its conclusion rather than an accident, and it is pinned as
> such: `server::tests::a_raiding_party_is_bounded_by_the_band_and_not_by_the_sampling_lever` asserts
> both verbs launch past the sampling bound **and** that both still refuse a party past the band.

#### What the sheet shows for a party size that was not sampled

A legal party can now run past the last quoted row. **The client must not compose the missing
estimate**: the take passes through `fauna::quantise_animal_take`'s `floor()`, so it is non-linear,
and `.claude/rules/core_sim/yield-forecast.md`'s terms-vs-answers rule says non-linear ships as an
**answer**.

**It quotes the largest sampled row, naming the party size it was sampled for** — *"at 8 hunters:
≈14 food over ~6 turns"* beside a stepper reading 16. That is safe in the honest direction, because
both tables are monotone in party size: the quoted row **under-states** a larger party's take and
**over-states** its turn count, so the player is never promised more than they will get.

`PopulationCohortState.maxExpeditionPartySize` is the field that says where the rows stop. Its **name
is now wrong** and is kept only because renaming a wire slot costs a client decode change for no
behaviour; both the schema comment and the Rust doc state emphatically that it is a sampling bound
and that the stepper must clamp to `idleWorkers` instead.

#### The sheet opens on `denialPartyNeeded`

`HerdTelemetryState.denialPartyNeeded` (appended) is the smallest party in `denialEstimates` whose
own row **succeeded** — `past_recovery` or `herd_lost`, the `DenialOutcome::succeeded` test. It is
read off the rows rather than recomputed, so the sheet cannot open on a value whose verdict one line
below refuses to say the herd goes down.

**It is not *"the smallest row that is not `repelled`"*, and the difference is `horizon`.** That row
is a raid the projection ran its whole length with the herd still standing — it demonstrates nothing
the sim will vouch for. Seeding there quoted a Wild Aurochs party of 5 under its own verdict line
*"still standing when the forecast runs out"*, and in play it was one short; across the shipped
roster the distance between the first non-repelled row and the first row that actually crosses the
line reaches **21 hunters**.

**It rounds UP, always.** 8.3 hunters is 9. `fauna::denial_party_needed` is `floor(x) + 1`, not
`ceil(x)`: a party that exactly *ties* with the regrowth declines nothing, and `ceil` is wrong by one
at precisely the round number a tuner is most likely to author. The requirement is measured against
`fauna::herd_replacement_animals` — the **peak** regrowth on the path down, not the rate where the
herd stands, because the logistic curve peaks at `K/2` and a party sized on a full herd's
(instantaneous zero) regrowth would stall at the food peak forever.

**Where there is no such number**, one sentinel (`denialPartyNeeded == 0`, *"no quoted party drives
this herd down"* — never *"send nobody"*), and `repelled` keeps working on every row:

| case | what the sim says |
|---|---|
| a quarry nothing brings into contact (`wariness >= 1`, `engage_rate 0`) | `0`; every row `repelled` |
| a requirement past `expedition_config` `deny.max_party_quoted` (the readout's cost bound) | `0`; every row `repelled` |
| a herd that declines but does not cross the line inside the horizon **at any quoted size** | `0`; the rows read `horizon` — a raid nobody quoted finishes, not a refused one |
| a requirement larger than the band's idle workers | the requirement, honestly — *"you need more people than you have"*, and the panel already shows both numbers |

**The denial table's axis is wider than the hunt table's**, deliberately: *what the herd needs +
`estimate_party_sizes` of headroom*, capped by `deny.max_party_quoted` (**64**). The closed-form
requirement's own row therefore always exists. The headroom matters because the decision above the
requirement is *how fast*: on the reported herd 9 hunters grind past the 60-turn horizon and 16 cross
the line in 11 turns.

**The axis is sized by the closed form, so it does not guarantee a SUCCESS row.** The closed form is
blind to the quantiser and to the fight, so the simulated
requirement can sit above it: swept over the shipped roster (~670 herd × stock-fraction samples per
map), 0–5 rows per map have their first success 1–4 parties above the axis, and a Thunder Mammoth at
full `K` sat **9** above (closed form 4, simulated 21). Those herds report the `0` sentinel. Widening
`estimate_party_sizes` — or sizing the headroom off the simulation instead — is the open lever.

**What the wider axis costs.** Measured on a fully-revealed 80×52 map (130 huntable herds, debug
build): capture ran **~59 ms** against a **~49 ms** flat-`estimate_party_sizes` baseline — about
+20%, and only on the denial half. A **flat** `deny.max_party_quoted` axis (64 rows for every herd)
cost **~104 ms**, which is why the axis is herd-sized rather than flat: snapshot capture is the hot
half of a turn. The added rows are the cheap ones — a party past the requirement collapses the herd in
a handful of turns, so its projection returns long before the horizon, while the sub-requirement rows
are the ones that run it out.

#### What this change did NOT do, and where the seam is — RESOLVED

> **All three blockers below are resolved.** The forecast query channel
> (`.claude/rules/core_sim/expeditions.md` → "The forecast is ASKED FOR") replaced both estimate
> tables with an on-demand request/response over the command socket. The original analysis is kept
> because it is what set the shape of the answer, and because the third blocker's numbers are the
> before-half of the measurement that justified it.

**The sampled table survives, and it is priced for a band that may not exist.**
`snapshot::subsistence` prices every field on `HerdTelemetryState` — `denialEstimates`,
`huntTripEstimates`, `denialPartyNeeded` and the per-worker rates — with
`equipment_config.hunter_profile(.., equipped = true)`, a hardcoded equipped tier
(`snapshot/capture.rs`). A herd row is a fact about the *herd*, and the table has no band to ask.

Since TOE landed that is no longer sufficient: the take resolves through the fight, so it depends on
the band's own `hunterAttack` and its resolved carry tier, **both per band**. A band whose spears
have run dry hunts at attack `1` against a Red Deer's defense `1.0` — effective attack `0`, so **no
party of any size works** — and it is being quoted `9`.

So the honest end state is a **per-band** denial answer: the minimum viable party priced for the
asking band's actual kit, published instead of a sampled table across party sizes, with a dry band
seeing a larger number or `repelled`. Three things stopped that being part of this change:

1. **It is a wire-shape decision, not a repricing.** A per-(band, herd) answer has nowhere to live
   today: `HerdTelemetryState` is per herd and `PopulationCohortState` is per band, so it needs a new
   repeated field on the cohort keyed by `faunaId` — and the retired `denialEstimates` becomes a
   `(deprecated)` slot the sim stops writing, since `snapshot.fbs` is append-only.

   > **Resolved differently, and better: it does not live on the snapshot at all.** The premise was
   > that an answer has to be *published*. It does not — the client asks
   > (`DenialRaidForecastQuery { faction, band, herd, kit, party_workers, max_party_workers }`) and
   > is answered on the same socket. There is no per-(band, herd) cross product on the wire because
   > there is no wire field: the sim answers the one question in front of it. `denialEstimates`,
   > `denialPartyNeeded`, `huntTripEstimates` and the two `*_kit_id` disclaimers are `(deprecated)`
   > slots, exactly as predicted.

2. **It is also a UI decision.** The sheet has a party *stepper*, and the rows are what give a
   stepped-off size any verdict at all. Publishing only the requirement's row means the stepper
   either goes away (the sheet states one number) or loses its readout. That is a design call, not an
   implementation detail.

   > **Resolved: the stepper stays, and it gained precision.** Every stepped-to size gets a verdict,
   > because each is a question the sim will answer — and the answer is for *that* size rather than
   > the nearest sampled rung. `party_needed` still seeds the control; `useful_cap` now walks
   > `1..=idleWorkers` contiguously instead of a ladder, so the max-useful party is the real plateau
   > rather than the rung after which a sampled payload stopped rising.

3. **It cannot be a straight repricing, because of what the tables cost.** Measured on a
   fully-revealed 80×52 map, 132 huntable herds, debug build, over 5 captures:

   | capture | per capture |
   |---|---|
   | both estimate tables | **57.5 ms** |
   | hunt table only (denial stripped) | 22.5 ms |
   | denial table only (hunt stripped) | 39.0 ms |
   | neither table | **2.9 ms** |

   The two tables are **~95% of snapshot capture** — the hunt table ≈ 18.5 ms, the denial table
   ≈ 35 ms, against 2.9 ms for everything else. A per-(band, herd) answer multiplies that by the band
   count, so three bands would put capture at **~165 ms per turn** on a path
   `.claude/rules/core_sim/turn-profiling.md` already measures at 94% of turn time. Repricing
   therefore *forces* one of two structural answers — collapse the axes so the cross product is
   affordable, or move the estimates off the per-turn capture entirely (which the one-way command
   channel does not support today) — and neither is a decision to take by reflex.

   > **Resolved by the second structural answer, and the multiplication is never paid.** The command
   > socket learned to answer — it was always an ordinary bidirectional stream; "one-way" was a
   > protocol choice, not a transport limit. Re-measured on the same harness
   > (`core_sim/tests/capture_cost.rs`, fully-revealed 80×52, debug, 5 captures):
   >
   > | phase | with the tables | without |
   > |---|---|---|
   > | `snapshot.build` | **49.51 ms** | **3.15 ms** |
   > | `snapshot.build.herds` | **46.22 ms** (93.4%) | **0.06 ms** (1.8%) |
   >
   > Capture is **15.7× cheaper**; the herd pass ~770×. The band count never enters it: a query is
   > answered when a player asks, for one herd, not 131 times a turn for nobody.

4. **`huntTripEstimates` has the identical defect for the identical reason**, and the same cost
   argument applies to it, but the two tables answer different questions: denial asks *"what party do
   I need"* (one number) while a hunt asks *"what will I get"* at a floor **and** a party size, so
   whether the hunt's axes survive a repricing is its own case to make.
   `estimate_party_sizes` cannot be deleted until that table is dealt with, which is why the lever
   survives — renamed to say what it does — rather than being removed here.

   > **Resolved: both tables went together, and the axes went with them.** A query has no axes to
   > survive — it takes the floor and the party as arguments and echoes them back on the row.
   > `estimate_party_sizes` and `deny.requirement_rows` are **deleted**, with their validators and
   > drift tests, and so is `PopulationCohortState.maxExpeditionPartySize` (which echoed the ladder's
   > last rung and capped nothing).

**The clean line this change stops at:** the launch cap is gone (the reported defect), the minimum
viable party is computed, rounded up, and published as an *answer*, and the sheet can open on it. The
number is quoted at the equipped tier, stated as such on the field, on the schema and in the table
below.

**`estimate_party_sizes` was deliberately NOT raised from 8.** Raising it multiplies the *hunt* table
by 5 (its axis is `floors × party sizes`), so 8 → 16 would double an already-large per-herd cost to
buy rows the "quote the largest sampled row" rule already covers honestly. If play shows the quoted
row is too far below typical party sizes to be useful, that is the lever to turn, and the cost is
linear in it.

---

## 4. Slices

Both land after `plan_hunt_through_combat.md` slice 3, which is what gives denial something to
unclamp.

1. **The mission** — **LANDED**. `ExpeditionMission::Deny`, wire key `"deny"`, the
   `send_denial_raid` command, `hunt_floor() = 0`, and the one behavioural difference:
   `fauna::EngagementStop::Never`, which drops the quantiser's carry arm and leaves `carried`
   untouched. `DenialForecast::turns_to_collapse` reports as a range and rides the wire as
   `HerdTelemetry.denialEstimates`.
2. **Client.** A third verb at launch; the collapse verdict line as a range; the waste readout. Plus
   the §3.1 half, which now applies to **all three** verbs: every outfit stepper caps at the band's
   **idle workers** and never at `maxExpeditionPartySize`; the denial stepper *seeds* at
   `denialPartyNeeded`, rendering the `0` sentinel as *"no party can"* rather than as a party size;
   and a selected size past the last quoted row shows that row **with the size it was quoted for**,
   never a client-composed estimate.

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
| 2 | ~~**Does a raid recur?**~~ | **SETTLED in slice 1: it does not.** A denial raid completes when the herd goes past recovery and never relaunches — there is nothing to come back for, and `raid_is_recurring` is a question about a *floor*, which the mission does not carry. A raid that cannot get there keeps working the herd until it is recalled; the launch verdict warns first (§3). |
| 3 | **Is denial legible as distinct from a deep hunt?** | The mechanical difference is one clamp. If a player cannot feel why the raid is different from `floor = 0`, the mission has failed even if the sim is right. |
| 4 | **Where does a PER-BAND raid estimate live on the wire?** | **ANSWERED: nowhere.** The premise was that an answer must be published. The client asks and the sim answers on the command socket, so there is no per-(band, herd) field and no cross product — see §3.1's resolved blockers. |

---

## See Also

- `docs/plan_hunt_through_combat.md` — the arc this rides on, and the §1–§2 this doc used to carry
- `docs/plan_harvest_floor.md` — the escapement dial, and why `floor = 0` is a harvest and not a denial
- `docs/plan_predators.md` — the combat subsystem, and the Warriors-do-not-escort rule
- `docs/plan_early_game_labor.md` — TOE, the kit a raid burns
- `docs/plan_exploration_and_sites.md` — the expedition machinery the mission reuses wholesale
- `.claude/rules/core_sim/fauna.md` — the ecology bands a raid aims to cross
