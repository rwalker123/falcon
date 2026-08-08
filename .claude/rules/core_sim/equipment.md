---
paths:
  - "core_sim/src/{equipment_config,creatures_config}.rs"
  - "clients/godot_thin_client/src/scripts/ui/hud/KitRoster.gd"
  - "core_sim/src/data/{equipment,creatures}.json"
  - "integration_tests/tests/equipment_toe.rs"
  - "core_sim/tests/kit_selection.rs"
---

# TOE — the band's consumable equipment

**A TOE is the equipment set that lifts a ROLE from its *unequipped* to its *equipped* tier.**
Authoritative design: `docs/plan_early_game_labor.md` → "Equipment / TOE" (slice 5 of that arc);
`docs/plan_hunt_through_combat.md` §4.8 says why a *minimal* subset had to land before the hunt
resolves through combat — a bare-handed hunter's `attack` is `1`, below every megafauna's `defense`,
so without a spear the fight's gate `max(0, attack − defense)` is the entire game.

## Two nouns, and they are not the same

An **ITEM** is a piece of equipment. It owns what it does (its `effects`), how long it lasts
(`starting_durability`) and what wears it (its `wear.per` quantum). A **KIT** is a roster entry that
*lists* the items a party is sent out with.

The items used to be named `hunting_kit` / `sled_kit` / `basket_kit` while the kits were
`big_game` / `gathering` / `none` — one word for two concepts, which made a kit look like a leaf and
cost a whole design conversation to unpick. **"Kit" now means only the roster entry.**

| item | declares | its use quantum |
|---|---|---|
| **`spears`** | `attack` **20** (equipped) | per **animal killed** |
| **`sled`** — travois, drag harness | `hunt_carry` **12** (unequipped) | per **biomass hauled home from a hunt** |
| **`baskets`** | `forage_carry` **1.6** (unequipped) | per **biomass gathered** |
| **`traps`** — the passive device (snares, nets, weirs) | `attack` **20** bounded to `max_body_mass` **1.0**, `dispersion` **0**, `exposure` **0** | per **animal killed** |

Shipped kits: **`big_game`** (`spears` + `sled`), **`trapping`** (`traps` + `sled`),
**`gathering`** (`baskets`), **`none`** (nothing).

**An EFFECT names the value a stat TAKES — never a delta, never a multiplier stacking on something
else.** `EquipmentEffect` has no representation for a taper, which is what makes "flat until expiry,
then a step down" structural rather than a rule someone has to remember.

**Which TIER an effect declares is one-home-per-fact showing through**, not free choice: the other
tier already had a home and keeps it. `spears` declares the **equipped** `attack` because the bare
hand's `1.0` is `creatures.json`'s `person` row; the two carry items declare the **unequipped** side
because the equipped rates are `labor_config.json`'s and the shipped game has always run kitted.

**Quality tiers (flint against bronze) are deliberately ABSENT.** Nothing can craft one, so the
structure would ship with no way to exercise it. They ride the crafting slice (**#494**), which is
also what forces the equipped rates out of `labor_config.json` and into the tier.

## One item, one job

The pairing is **physical**, which is the whole of §4.8's correction:

**A carcass is one lumpy object you drag out whole**, so a container does not help you move a deer —
a sled does. **Berries are the opposite**: loose, divisible, bounded entirely by what you can hold,
which is exactly what a basket fixes. The minimal TOE shipped with *one* carry kit, called baskets,
raising the **hunt's** haul — backwards on the physics, and it left `forage.per_worker_biomass_capacity`
untouched by any kit at all. The three-kit split is that correction, not new scope.

**The two unequipped tiers want different SHAPES.** Forage is **containment**-bound — a handful
against a basketful — so its ratio is large (`8.0 → 1.6`, exactly a fifth). The hunt is
**transport**-bound — a sledless party can always drag *something* — so its ratio is smaller
(`40.0 → 12.0`, under a third). Pinned as an ordering *between the ratios* by
`losing_your_baskets_costs_proportionally_more_than_losing_your_sled`.

## Dispersion, exposure and reach — the three multipliers

All three are **neutral at `1.0`**, so an item declaring none of them is priced exactly as it was
before the effects model existed.

- **`dispersion` multiplies the QUARRY'S OWN `wariness`** at the retreat:
  `effective_wariness = clamp(wariness × dispersion, 0, 1)`, and `stayers = engaged × (1 −
  effective_wariness)`. A trap ships `0` — nothing breaks off at contact.

  **A multiplier rather than a subtraction, so the SPECIES decides how much a noisy approach costs.**
  At `wariness 0.85` a gazelle loses almost its whole engagement to one; at `0.10` a mammoth barely
  notices. That is what lets a single spear line scatter a warren and *contain* a mammoth with no
  per-target authoring — **and it is why equipment needs no size-class or "targets" axis at all**.
  `wariness 0` is already an exact identity in the retreat (no draw is made), so `dispersion 0` lands
  in a regime the sim has always had.
- **`engage_multiplier` multiplies the species' `engage_rate`** — how many animals one hunter reaches.
  **This is the term that binds on light game**, where `attack` buys nothing because there is no
  `defense` to clear, and it is why a trap raises reach rather than damage.
- **`exposure` multiplies the hunt's baseline injury hazard** (`fauna::hunt_injuries`). `0` is a
  stand-off instrument: it wears out **instead of** its user getting hurt, which is the trade rather
  than a free lunch.

### Snares, nets and weirs are ONE item, and it wins by not being seen

**At this game's abstraction they are one thing**: something you set down, walk away from, and come
back to. Whether a device holds one animal or a whole run, and whether it is discriminate, is detail
the game never surfaces — and two items separated by numbers that mean nothing would be worse than
one honest one. So `traps` is *the passive device*, and a "net" is not a second item.

**Its whole advantage is `dispersion 0`.** It is not there to be seen, so nothing bolts and the party
keeps everything it reaches. Against a Rabbit Warren at `wariness 0.75` a spear party loses three
animals in four to the retreat and this loses none — that *is* the 50 → 200 gap, measured.

**Its `attack` is the spear's own number, and that is the point.** At 0.13–0.67 kg the weapon is not
what is scarce, so the device must not win by hitting harder. `max_body_mass` is what keeps it off
everything else.

**There is no reach multiplier, and its removal was measured rather than assumed.** A `×4` was
authored on this item and turned out inert: the fight binds before reach does on every small-game row
(reach per worker `engage_rate × mult` against fight per worker `attack / durability` is 40 vs 10 on a
rabbit, 60 vs 10 on a catfish), so it changed no outcome anywhere. **`EquipmentStat::EngageMultiplier`
was deleted outright** rather than shipped neutral — a lever with no reader is what `fauna.md` already
flags for removal on the `follow.*` keys.

**It was invented for a premise that turned out false** — *"small game has no `defense`, so `attack`
buys nothing and reach must be what binds"* — and both halves are wrong: at `defense 0` the gate *is*
the attack, and reach never binds there anyway. **It is not a combined-arms mechanism.** The thing it
superficially resembles — an archer loosing at quarry it cannot close with — is `RangeBand` on the
combat resolver, already reserved for the ranged pre-phase (#501). Re-add a reach stat the day
something genuinely raises reach (dogs, a drive), with its own justification and its own test.

Measured per turn at 20 hunters, and pinned across **every** small-game row by
`hunt_fight::the_passive_device_beats_spears_on_every_small_game_row_and_takes_no_large_game`:

| quarry | body mass | `big_game` | `trapping` |
|---|---|---|---|
| Wild Fowl | 0.13 | 70 | **200** |
| Rabbit Warren | 0.27 | 50 | **200** |
| Forest Grouse | 0.47 | 64 | **133** |
| Snow Hare Warren | 0.60 | 50 | **133** |
| Silt Catfish | 0.67 | 180 | **200** |
| Desert Gazelle and everything above | ≥ 3.3 | wins | **0** |

**Silt Catfish is the row that found the tuning bug.** `engage_rate 15` and `wariness 0.40` make it
the easiest small game to spear, so it is where a device tuned only against rabbits quietly comes
second — which is why the test sweeps all five rather than one.

### An effect can be bounded by the quarry's BODY MASS

**`dispersion` alone does not make traps small-game-only, and believing it did shipped a bug.**
Dispersion answers *does the animal bolt before you reach it*; it says nothing about *what a snare can
physically hold*. Traps shipped with a flat `attack 8`, which cleared a Red Deer's `defense 1` exactly
as it cleared a rabbit's `0`, and a trap line quietly became a universal upgrade.

An effect may therefore carry **`min_body_mass` / `max_body_mass`**, and `traps` ships `max 1.0`.
Above the bound the item grants **nothing**, the party falls back to the bare hand's `attack 1`, and
`max(0, 1 − defense)` is the **existing gate** refusing the hunt — there is no "you cannot trap that"
branch anywhere.

**It reads `body_mass`, which the roster already authors — not a size CATEGORY.** A `size_class` here
would be a second authority to drift from the masses, exactly as `dispersion` reads `wariness` rather
than a "jumpy" flag. The roster separates cleanly, which is why one number does it: every `defense 0`
row is `0.13..=0.67` and the next species up is a Desert Gazelle at `3.3`, so **any** ceiling in that
gap behaves identically.

`min_body_mass` is the same field's other end — a bow is poor against something small and fast — and
is reserved for #501. Nothing ships one.

**Only `attack` is resolved against a quarry, and a bound on any other stat is REJECTED at validate**
rather than silently ignored (`config-loading.md`'s "looks live but isn't").

**Two named resolvers, so a take path cannot get the display answer by leaving an argument off:**
`hunter_profile_against(…, body_mass)` is the only form a take or forecast may use;
`hunter_profile_unbounded` is *"the best this kit can do against something"* and is for surfaces with
no target — the published kit roster and a band's own `hunterAttack` row.

> **The hunt job's DEFAULT kit must carry no mass-bounded attack, and `validate` enforces it.**
> `snapshot/capture.rs` builds **one** party to price **every** herd's estimate tables, so it cannot
> carry a per-quarry attack and resolves unbounded. A bounded default weapon would make every table
> quote a kitted take against animals it cannot touch — wrong in the *reassuring* direction, on the
> surface a player commits from. The fix when that day comes is to resolve per herd inside
> `herd_snapshot_entries`; the check is what makes that a decision rather than a bug found in play.

### A kit resolves a multiplier as the MAX of what its LIVE items DECLARE

Two clauses, both load-bearing, both in `KitChoice::multiplier`:

1. **Only declared values participate.** An item that says nothing about a stat contributes nothing —
   *not* the neutral `1.0`. Without that clause a **sled**, carry gear nobody approaches an animal
   with, would drag a trapping party's dispersion back to `1.0` simply by being in the kit, and traps
   would never work.
2. **The MAXIMUM, not the minimum**, for the stats that describe *how the party hunts*. If you are
   also running up and throwing spears you are scaring the herd and you are in reach of it, however
   many traps you also set.

**This is why `spears` declares `dispersion 1.0` and `exposure 1.0` explicitly although both are the
neutral value.** The declaration is what makes a hypothetical spears-and-traps kit resolve to *loud
and exposed* instead of inheriting the trap's stand-off for free. Nothing shipped pairs them today;
the declaration keeps the answer right the day something does.

**Combined arms — two weapons genuinely in play at once — is NOT this.** It is issue **#501** under
the **#500** Combat engine arc, and it needs the resolver's reserved ranged pre-phase, wound-driven
degradation and per-item hit chance. None of it is required for anything above.

**The sledless hunt needs no new mechanic.** A party that cannot haul its kill leaves more of it,
which `AnimalTake::wasted` has always computed and the client already displays — see "Waste came
back" below.

## Config files

| File | Purpose |
|---|---|
| `src/data/equipment.json` | **The TOE** (loader `equipment_config.rs`, env override `EQUIPMENT_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks. **`items`** — a map of id → `{ starting_durability, wear: { per, amount }, effects: [{ stat, equipped\|unequipped }] }`. `stat` is one of `attack` / `hunt_carry` / `forage_carry` / `dispersion` / `engage_multiplier` / `exposure`; `per` is `kill` / `biomass_hauled` / `biomass_gathered` — **there is no `turn` variant, and that is `docs/plan_denial_raid.md` §1.2 enforced by the type**. Shipped: `spears` (100 durability, 0.4/kill → 250 kills), `sled` (100, 0.02/biomass → 5000), `baskets` (100, 0.04/biomass → 2500), `traps` (100, 0.2/kill → 500 — twice the spear's life per kill because a trap is *worked* rather than thrown, and on the **same quantum** so a trapping party cannot hunt for free). **`kits`** — `{ id, display_name, jobs, uses }`, where `uses` names items and `jobs` is `hunt` / `forage`; plus `default_kits`. **`validate` rejects**: an empty item table; any non-finite or `<= 0` durability or wear amount (an item with no wear rate is not consumable, one with no durability is born dry); an item with no effects; a negative or non-finite effect value; **an item declaring the same stat twice** (`effect()` takes the first match, so the second would be silently dead); **two items declaring the same carry stat** (the unequipped fallback searches the whole table and takes the first match, so it would resolve by `BTreeMap` order — i.e. alphabetically); a duplicate kit id; a kit listing no jobs; a default naming no roster entry or not covering its own job; and **a `uses` entry naming an item the table does not carry**. That last one is a DEBT, not a nicety — see below. |
| `src/data/creatures.json` | The creatures roster — intrinsic `CombatStats` for non-fauna units. `person.combat.attack` (**1.0**) is the hunting kit's **unequipped** tier. See `combat.md` for the roster's role in the fight. |

### `UnknownItem` pays back a guarantee the model used to get for free

The retired `KitComponent` enum's three variants **were** the JSON block keys, so a roster naming a
component with no block **could not deserialize** — the invariant was carried by the type, at no
cost, on every load path. An item id is a `String`, so nothing stops a config naming `spearz`, and
the only thing between that file and a running sim is now `validate`'s `UnknownItem` check. A kit
that silently granted nothing and wore nothing is exactly the failure §4.8 corrects, so
`a_kit_using_an_item_that_does_not_exist_is_rejected` must not be deleted.

**Only the unequipped side of the two CARRIES lives in `equipment.json`, and that is
one-home-per-fact, not an oversight.** Every *equipped* tier already had a home and stays there: the bare hand's `attack 1` is
the `creatures.json` `person` row, the kitted haul rate `40` is `labor_config.json`'s
`hunt.per_worker_biomass_capacity`, and the kitted gather rate `8` is its
`forage.per_worker_biomass_capacity` — the rates the shipped game has always run on, because a band
has always started kitted. Copying any of them here would give a shipped number a second home to
drift from. `equipment.json` carries only what the *kits themselves* own: what they do, and how long
they last.

**The BASKET is on `plan_early_game_labor`'s ~15–20-turn kit-duration clock; the two HUNT kits
overshoot it by ~3×.** Against the shipped ~16-worker band, gathering reaches 16 × 8 = 128 biomass a
turn, so 2500/128 ≈ **19.5 turns of baskets** — on target. Hunting Red Deer the band *engages* 16
animals a turn, but engaged is not killed: Red Deer ship `combat.wariness 0.65`, so the retreat leaves
**~5.6** of them (`~84` biomass) and the kits last 250/5.6 ≈ **45 turns of spears**, 5000/84 ≈ **60
turns of sled**. The ≈15.6 / ≈20.8 this section used to quote were computed at `wariness 0`, before
slice 7 authored the roster's values.

**The wear rates are NOT retuned for it.** Closing a 3× gap is a balance change against numbers the
hunt arc is still moving; it rides with the hunt-effectiveness tuning on **issue #491**, and
`equipment.json`'s `_comment_durability` says so at the dials.

## The three rules

1. **Two tiers, never a taper.** Performance is **flat until expiry**, then the role **steps down**.
   Durability and performance are deliberately **orthogonal axes** so a future crafting economy can
   tune them independently, and nothing may scale a readout by remaining condition. Pinned by
   `the_durability_cliff_is_a_step_not_a_taper`, which sweeps wear across all three kits' lives at
   once and asserts each exported tier is *the same number* at every point below expiry.
2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2 depends on
   it: a turn clock charges an idle march the same as a slaughter, which makes denial free). **Each
   kit has its own quantum** (table above), so the three cannot cross-charge. Pinned by
   `wear_is_charged_for_kills_not_for_turns_elapsed` — same world, same turn count, a scouting band
   loses exactly zero — and by `the_sled_and_the_baskets_wear_on_different_quanta`: a hunting band
   finishes with whole baskets, a gathering band with a whole sled and whole spears.
3. **Start-stocked and NOT craftable.** Running dry is the intended pressure and the pull into the
   Milestone-2 crafting economy. Nothing in the sim reduces wear, so each unequipped tier is
   **absorbing** (`a_kit_run_dry_stays_dry`, `baskets_run_dry_on_their_own_quantum_and_stay_dry`).

## The band carries WEAR, not stock

`components::BandEquipment` is a **`BTreeMap<String, f32>` of wear per item id** — three named floats
until the effects model, which could not have carried `traps`. Storing *wear* rather than *stock* is
what makes "the band starts kitted" free: `Default` is an empty ledger, so a spawn site inserts a full
kit without reading config, and an **absent entry reads as no wear recorded — a full item**, which is
also what keeps a band spawned before the config gained an item from being born with it already
spent. There is deliberately **no "carries no kit at all" state**; dry is expressed as wear reaching
`starting_durability` (strictly-below is equipped, so an item worn exactly to its limit is spent).
`BTreeMap` rather than `HashMap` so the checkpoint and the wire serialize in a stable order — a
rollback that reordered this would diff as a change every frame.

**Every charge goes through `wear_item`**, which floors a non-finite or negative `uses` at zero, so no
item can grow a private flooring rule: a degenerate take must never *restore* a kit.

**`wear_kit` names the QUANTUM, not the items.** A wear site says *"this party just made N kills"* and
every item in its kit that wears per kill is charged — so **an item added to a kit is charged with no
call-site edit**, and an item the kit does not carry is never charged at all. That last clause is what
makes the bare-handed comparison free to run: otherwise running the comparison would consume the very
kit it is being compared against.

> **`wear_kit` also gates on CONDITION, and that is not redundant with the mask.** A spent item is
> already paying its cost — the role has stepped down — so charging it again would let a ledger run
> arbitrarily far past its own durability, and any future crafting would have to buy back that
> invisible overdraft before the item came back at all. Caught by
> `kit_selection::a_kitted_partys_own_wear_still_steps_it_down` when the quantum refactor first
> dropped the gate.

Inserted by `spawn_profile_population` (`systems/worldgen.rs`), by both expedition-outfitting paths
in `bin/server.rs`, and restored by `sim_state.rs` (`BandRecord::equipment`, carried unconditionally
— a checkpoint that forgot how worn your spears were would silently re-stock them on rollback).

## Where the tiers are consumed

- **`advance_labor_allocation`** resolves **both** carry tiers **once per band per turn**, *before*
  the assignment loop, so a kit that expires part-way through cannot pay two different rates to two
  sources in one turn.
  - `hunt_per_worker_biomass` (the **sled's**) feeds every hunt-arm site: `hunt_take`, the pen's
    `collection`, `project_realized_hunt` / `project_arrivals_hunt`, `hunt_take_workers` and
    `hunt_haul_workers`.
  - `forage_per_worker_capacity` (the **basket's**) feeds every gather site: `forage_take`,
    `forage_forecast`, `project_realized_forage` / `project_arrivals_forage`, and the
    `workers_needed` inversion.

  Wear is charged **after** the take on both webs (the same accrue-after-take ordering every rung's
  build meter uses), so the turn is paid at the tier it was priced with and the cliff lands on the
  next turn.
- **The assign-time seed** (`seed_source_yield` in `bin/server.rs`) resolves the band's own tier on
  *both* arms, through the same two `EquipmentConfig` seams. It has to: the forecast-equals-actual
  invariant (`yield-forecast.md`) would otherwise promise a dry band a kitted haul or a bare-handed
  crew a basketful.
- **A pen harvest wears the SLED only.** A penned beast is slaughtered, not stalked — no fight, no
  spear to blunt — which is the same reason that branch passes no engagement bound to the quantiser.
- **An expedition never touches baskets.** A raid is a hunt (`ExpeditionMission` has no gather verb),
  so `advance_expeditions` resolves the sled and the hunting kit and nothing else.

**`forage_per_worker_biomass(capacity, seasonal)` takes a RESOLVED tier, not a config handle.** That
is the seam the basket tier rides; sites with no band to resolve against (the patch telemetry in
`snapshot/subsistence.rs`, a Field's managed collection cap) pass the shipped *equipped reference*
rate, exactly as `HerdTelemetryState::per_worker_biomass` does on the animal web.

## Waste came back, and it is pinned

Slice 4 made the wild hunt's `max(1, carryable)` waste branch **unreachable** at the shipped tier:
any crew that could make the kill could also carry it. Waste needs
`workers × per_worker_carry < body_mass`, and the sledless rate puts that regime back within reach.
`a_sledless_party_wastes_the_kill_it_cannot_carry` pins it on **both** sides of the same fixture — 2
hunters on a 50-biomass body collect `2 × 40 = 80` sledded (whole body seated, `wasted == 0`) and
`2 × 12 = 24` sledless (one body down, less than half of it home, `wasted > 0`) — with a liveness
assertion that each party actually killed, or "wasted nothing" would be the trivial truth about a
party that never engaged.

## What is NOT wired yet, deliberately

- **The Crafter role, replenishment/upgrade, and the Scouting and Warrior kits** from that arc's role
  table are out of scope for this slice.
- **A Field's (rung-3) managed collection cap stays on the equipped reference rate.** Rung 3's
  harvest is quoted per *account* (`managed_per_worker_yield` / `_fodder` / `_trade`) and draws no
  biomass down, so it has no single biomass quantum to charge a basket against; wiring it needs the
  managed harvest restated in biomass, which is a restructure rather than a tier swap. Rungs 1–2 —
  every wild and tended gather — are basket-resolved.

## The attack tier went live with the fight

`docs/plan_hunt_through_combat.md` slice 4 moved the kill into `combat::resolve_fight`, so
`EquipmentConfig::hunter_profile` is now read on **every** take and forecast path (through
`fauna::HuntingParty`) and the hunting kit is the difference between eating and not:

- **`max(0, attack − defense)` is the gate**, so a dry-speared band drops to `attack 1` and can hurt
  only quarry with **no `defense` at all** — rabbit, fowl, grouse, snow hare, catfish. Everything from
  a gazelle upward becomes untouchable, at any headcount. `the_attack_tier_decides_the_take`
  (`integration_tests/tests/equipment_toe.rs`) pins both halves: the kitted band takes Red Deer and
  wears its kit; the bare-handed one takes **exactly zero** and wears nothing.
- **A detached party resolves and wears its own kit.** `advance_expeditions` queries
  `&mut BandEquipment`, resolves the attack tier via `hunter_profile` and the haul tier via
  `hunt_per_worker_biomass_capacity`, and charges `wear_hunting` per animal killed + `wear_sled` per
  biomass hauled — the same use quanta a resident band pays. Before slice 4 a raid ran on free,
  immortal equipment, which is the cost model `docs/plan_denial_raid.md` §1.2 depends on.
- **The sled only decides a take where the fight leaves it room.** §4.6's per-hunter-turn ceiling is
  `min(engage_rate, (attack − defense) / durability) × body_mass`; the sled is a lever only where that
  sits between the two haul rates (12 and 40). A Red Deer's is `11.4` — under both, so neither tier
  binds on deer — while a Wild Horse's is `20.0`, which is why
  `both_hunt_carry_tiers_are_live_and_a_sledless_party_hauls_less` measures horses.

## A kit is a MASK over the item table — nothing else

A party is **sent out with a named kit** from the roster rather than implicitly using whatever the
band owns. The whole mechanism is one line:

```text
item_live(item) = kit_uses(item) AND band_has_condition(item)
```

- **`big_game`** uses `spears` + `sled` — the pair every hunt path used to consult unconditionally,
  so it is **bit-identical** to the pre-roster game.
- **`gathering`** uses `baskets` — likewise for every gather path.
- **`trapping`** uses `traps` + `sled`. It is the first kit that is **not** a subset of the old
  behaviour: traps clear no `defense`, so a trapper falls back to the bare hand's `attack 1` and can
  take **only quarry with no defence at all** — rabbit, fowl, grouse, snow hare, catfish. On exactly
  that quarry it is the better instrument, because reach and not damage is what binds there. It
  carries the sled too: a trapper still has to get the catch home.
- **`none`** uses nothing, so every predicate reads false and the party runs at every *unequipped*
  tier throughout.

`KitChoice` (`equipment_config.rs`) is the id plus that mask, and **`item_live` is the only way
anything asks the question**: `BandEquipment::has_condition` is `pub(crate)`, so the *condition* half
cannot be read alone by a caller that has forgotten to consult the mask — which is exactly the
reading that silently re-arms a party sent out bare.

**`EquipmentConfig::no_kit()` is SYNTHETIC, not a lookup of the roster's `none`.** The roster is
config and a file is free to drop that entry, but "this crew carries no kit" is a state the sim
reaches on its own — a band-wide role like Scout or Warrior has no kit axis at all. Resolving it
through the roster would let a config edit panic the labor loop. Its id is empty, which is what
`LaborAssignment.kitId` already publishes for a row with no kit axis.

**Nothing resolves a stat by naming an item.** `hunter_profile` asks the kit for the best *equipped*
`attack` among its live items; the carries ask whether any live item *supplies* their stat and fall
back to what the table declares. So a future bow is a config row, not a code change — and the only
place an item id is spelled in the sim is a test fixture.

**`none` is an ORDINARY roster member, not a sentinel.** Nothing branches on its id anywhere; it is a
kit whose `uses` list is empty, and every behaviour attributed to it falls out of that. A future
`fishing` kit with an empty `uses` would behave identically, which is the test of whether it has been
special-cased.

### Wear rides the SAME predicate that chose the tier

Every wear site is gated on the effective predicate its own tier came from — the three in
`systems/labor.rs` (baskets on the gather, the sled on a pen harvest, both on a wild hunt) and the
two in `systems/expeditions.rs` (the raid's take and the scout's roadside kill). So a party using no
component spends no durability on any of them.

**This pairing is the whole reason the bare-handed option is usable.** If it were not gated, running
the comparison would consume the very kit it is being compared against — the player would pay for the
experiment they ran in order to decide *not* to. A kitted party's charges are still independent of
each other: a kit with spears but no sled blunts spears only.

### Resolved ONCE for a party, per turn for a crew

An `Expedition` stores its `KitChoice` at launch and prices its whole life from it, **never
re-resolving against the home band's current stock** — a party sent with `none` would otherwise
silently re-arm the moment the band's spears were counted again. Its own `BandEquipment` wear still
moves it, so a `big_game` party still steps down when its spears run out; what is fixed is *which
components it reaches for*.

A `LaborAssignment` carries `kit: Option<KitChoice>` and re-resolves from **there** each turn, not
from the band. `None` reads as the job's default and is the only reading for the band-wide roles
(Scout / Warrior), which consume no component and have no kit axis at all. `assign_labor` stores the
*resolved* choice, so a replayed command lands on the kit it named rather than on whatever the
default is today.

The **wear** snapshot is still taken once per band per turn (`advance_labor_allocation`'s
`band_kit`), so a kit that expires part-way through the assignment loop cannot pay two different
rates to two herds in the same turn; only the *mask* varies per assignment.

### The commands, and how they fail

| verb | grammar |
|---|---|
| `assign_labor` | `… forage <x> <y> [floor] [species] <workers> [kit <id>]` / `… hunt <herd> [floor] <workers> [kit <id>]` |
| `send_hunt_expedition` | `… <party_workers> <fauna_id> [floor] [kit <id>]` |
| `send_denial_raid` | `… <party_workers> <fauna_id> [kit <id>]` |

**`kit <id>` is a NAMED token, order-independent within the tail.** Named rather than positional
because `send_hunt_expedition` already carries an optional positional tail and a second would make
`floor` un-omittable; the space-separated `name value` shape is the repo's existing one
(`queue_espionage_mission … owner 1 target 2 tier 2`, `counterintel_budget … reserve 40`) rather than
an invented `kit=<id>`. It is also **the one token the denial raid's otherwise closed grammar
admits** — a kit is a property of the *party*, not of the mission, so it is the only order a raid
carrying no floor still has to give.

**An unknown id, or one whose `jobs` does not cover the verb, is a command failure with a reason.**
Never a silent fall back to the default: naming a kit is how the player *compares* tiers, so a quiet
substitution answers a different question than the one asked and looks exactly like an answer. Absent
is the job's default, which is the pre-roster behaviour.

### The two estimate tables are NOT repriced per kit — and they say so

`huntTripEstimates` and `denialEstimates` stay quoted at the **hunt job's default kit**, and publish
which (`huntTripEstimatesKitId` / `denialEstimatesKitId`). They are ~95% of snapshot capture and a
kit axis multiplies them — the same structural cost question per-band repricing already faces (see
`expeditions.md` → "THE WHOLE HERD TABLE IS PRICED AT THE EQUIPPED TIER"). The field exists so a
client whose player has selected another kit can **refuse to present the table as an answer** for
that selection rather than quoting a kitted raid's numbers to a bare-handed party. Two fields rather
than one because they are two tables: if one is later repriced and the other is not, a single field
would lie about whichever was left behind.

Everything the player *does* commit to is priced at the chosen kit: the launch feed line
(`launch_forecast_party` + `launch_forecast_haul`), the in-flight delivery ETA, and both assign-time
compose seeds (`hunt_source_yield_preview` / `forage_source_yield_preview`).

## On the wire

`PopulationCohortState` carries four append-only kit fields (`sim_schema/schemas/snapshot.fbs`,
captured in `snapshot/population.rs` through the `BandKitLevers` bundle so the resolution happens in
one place):

| Field | Meaning |
|---|---|
| `kitItemConditions:[KitItemCondition]` | **One row per item the config carries** — `itemId` + `remaining` on the 0–100 scale, `0` = dry. It replaced three fixed floats (`huntingKitDurability` / `sledKitDurability` / `basketKitDurability`), which are **`(deprecated)` in the schema rather than deleted**: FlatBuffers field ids are positional, so removing one renumbers every field after it. **Driven by the CONFIG's item table, not the band's sparse ledger** — an item the band has never used reads as *full* rather than going missing |
| `hunterAttack:float` | The band's resolved per-hunter `attack` (1 bare / 20 kitted) — the left side of the fight's gate against a herd's `HerdTelemetryState.defense` |
| `huntCarryPerWorkerBiomass:float` | The band's resolved per-worker **hunt** haul rate (40 sledded / 12 sledless) |
| `forageCarryPerWorkerBiomass:float` | The band's resolved per-**gatherer** throughput, *before* the tile's seasonal weight (8 with baskets / 1.6 bare-handed) |

**The last two are separate fields on purpose.** A band can be out of baskets with its sled untouched;
a client that rendered one on the other web's row would be repeating the defect the split corrected.
`HerdTelemetryState.perWorkerBiomass` and `ForagePatchState.perWorkerBiomass` both stay the *equipped
reference* rate: neither a herd nor a patch has a band to resolve a tier against.

The kit selection adds five more slots, all append-only:

| Field | Meaning |
|---|---|
| `SubsistenceSection.kits:[KitOption]` | **The roster, once per world** — `id`, `displayName`, `jobs`, and the three tiers each kit grants a party whose components are **fresh** (`attack`, `huntCarryPerWorkerBiomass`, `forageCarryPerWorkerBiomass`), so the picker renders real numbers without a second copy of the TOE table |
| `SubsistenceSection.defaultHuntKitId` / `defaultForageKitId:string` | What each verb runs on when the player names none |
| `PopulationCohortState.kitId:string` | Which kit the two **hunt** tiers above are quoted at — an in-flight party's **own** kit (one kit, so it covers that party's forage tier too), a resident band's **hunt job default** (a band has one kit per assignment and this row is per cohort). **It does not name a resident band's forage kit**: `forageCarryPerWorkerBiomass` resolves through the *forage* default, so pairing the two reads a gathering rate off `big_game`, which has no basket component. The forage default rides the wire once, as `defaultForageKitId`; pinned by `kit_selection::a_resident_bands_published_kit_answers_for_the_hunt_tiers_only` |
| `LaborAssignment.kitId:string` | The kit that row's yields are priced at, **resolved** — never "unspecified". `""` on a band-wide role, which has no kit axis |
| `HerdTelemetryState.huntTripEstimatesKitId` / `denialEstimatesKitId:string` | Which kit each estimate table was computed at — see above |

### `SubsistenceSection.equipmentConfigJson` — the designer catalogue, and the one blob on this wire

The whole effective `EquipmentConfig`, `serde_json`-serialized into a single string. It exists so the
Workbench's designer pages can print the TOE configuration **as it is**, key by key: a dial added to
`equipment.json` appears with no client edit and no schema edit, which is exactly what a typed field
per key cannot do.

**Only the Workbench may consume it.** It is a read-only catalogue with **no gameplay consumer** —
nothing in the HUD or the sim reads it back, and nothing branches on it. A gameplay readout that
wants one of these numbers gets a **typed field of its own**; reaching into this string to dodge a
schema change is how a blob becomes a second, untyped wire contract.

**The STRUCT is serialized, never the file** (`snapshot/capture.rs::serialize_equipment_config`), and
that decides two things at once: it publishes what the sim is actually *running*, so an
`EQUIPMENT_CONFIG_PATH` override is reflected; and `equipment.json`'s `_comment*` keys are not struct
fields, so the prose never reaches the wire. A serialization failure publishes `""` and warns — a
designer page must not be able to fail a frame.

A per-world constant, so it is a `Whole<String>` baseline like the roster and is re-sent only on a
world rebuild. Pinned by
`kit_selection::the_published_equipment_config_json_round_trips_to_the_config_the_sim_runs`, which
reads the string **off the encoded envelope** and feeds it back through
`EquipmentConfig::from_json_str` — the failure it exists to catch is a field that serializes under a
different name than it deserializes, which no compiler and no check against the live struct sees.

## Balance

The shipped opening is **unchanged** — a start-kitted band hunts and gathers at exactly the numbers it
always did. What is new is the state below each cliff. Measured on the shipped ~16-worker starting
band, one turn:

| hunt (Red Deer, `engage_rate 1`, `body_mass 15`, `wariness 0.65`, fat herd) | sledded | sledless |
|---|---|---|
| per-worker haul rate | 40.0 | 12.0 |
| animals engaged → animals that stay | 16 → ~5.6 | 16 → ~5.6 |
| whole animals the pack can seat | 42 | 12 |
| biomass hauled home | ~84 | ~84 |
| food income | ~1.68 | ~1.68 |

**On THIS species at THIS party size the sled cliff does not bite, and the retreat is why.** The
sledless pack seats 12 whole deer and the retreat leaves ~5.6, so both tiers haul the same take —
where at `wariness 0` all 16 stayed and the sledless pack bound at 12 (the 240 / 180 split this table
used to show). The cliff still bites wherever the stayers outnumber the sledless seat: a bigger party,
a lighter body, or a species the roster made less wary. **Not compensated here** — same reason, same
issue (**#491**).

| gather (the starting band's own patch, floor `0.0`) | with baskets | bare-handed |
|---|---|---|
| per-worker gather rate | 8.0 | 1.6 |
| biomass gathered | 128 | 25.6 |
| food income | 8.49 | 1.70 |

**A known config skew, not a model problem:** a per-kill charge is species-blind, so a party on
`Wild Fowl` (10 engaged per hunter → 160 engaged, `wariness 0.65` → ~56 kills a turn) burns the same
hunting kit in **~4.5 turns** where the deer party gets ~45 — an order of magnitude, whatever the
absolute figures settle at. The lever if that proves to wreck pacing is a per-species use cost, never
a turn clock.

**The roster is a per-world constant**, so it diffs out on every frame after the first
(`Whole<Vec<KitOptionState>>` in `snapshot/capture.rs`) and is re-sent only when the world is rebuilt
on new tuning.

See Also: `combat.md` (the resolver and the `person` roster row the attack tier composes onto),
`yield-forecast.md` (the forecast-equals-actual invariant both seed arms preserve),
`docs/plan_early_game_labor.md`, `docs/plan_hunt_through_combat.md` §4.8.
