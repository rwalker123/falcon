---
paths:
  - "core_sim/src/{equipment_config,creatures_config}.rs"
  - "core_sim/src/data/{equipment,creatures}.json"
  - "integration_tests/tests/equipment_toe.rs"
---

# TOE — the band's consumable equipment

**A TOE is the equipment set that lifts a ROLE from its *unequipped* to its *equipped* tier.**
Authoritative design: `docs/plan_early_game_labor.md` → "Equipment / TOE" (slice 5 of that arc);
`docs/plan_hunt_through_combat.md` §4.8 says why a *minimal* subset had to land before the hunt
resolves through combat — a bare-handed hunter's `attack` is `1`, below every megafauna's `defense`,
so without a spear the fight's gate `max(0, attack − defense)` is the entire game.

## One kit, one job

Three kits ship, and the pairing is **physical**, which is the whole of §4.8's correction:

| kit | raises | its use quantum |
|---|---|---|
| **spears** (`hunting_kit`) | a hunter's `attack` | per **animal killed** |
| **sled** (`sled_kit`) — travois, drag harness | the **hunt's** carry | per **biomass hauled home from a hunt** |
| **baskets** (`basket_kit`) | the **forage** web's carry | per **biomass gathered** |

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

**The sledless hunt needs no new mechanic.** A party that cannot haul its kill leaves more of it,
which `AnimalTake::wasted` has always computed and the client already displays — see "Waste came
back" below.

## Config files

| File | Purpose |
|---|---|
| `src/data/equipment.json` | **The TOE kit table** (loader `equipment_config.rs`, env override `EQUIPMENT_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Three blocks: **`hunting_kit`** — `equipped_attack` (**20.0**), `starting_durability` (**100.0**), `wear_per_kill` (**0.4** → 250 kills); **`sled_kit`** — `unequipped_per_worker_biomass_capacity` (**12.0**), `starting_durability` (**100.0**), `wear_per_biomass_hauled` (**0.02** → 5000 biomass); **`basket_kit`** — `unequipped_per_worker_biomass_capacity` (**1.6**), `starting_durability` (**100.0**), `wear_per_biomass_gathered` (**0.04** → 2500 biomass). `validate` rejects any of the nine as non-finite or `<= 0`: a kit with **no wear rate is not consumable** and one with no durability is **born dry**. A missing *block* is a parse error, so a file that forgot `basket_kit` cannot silently leave the forage web unkitted again. |
| `src/data/creatures.json` | The creatures roster — intrinsic `CombatStats` for non-fauna units. `person.combat.attack` (**1.0**) is the hunting kit's **unequipped** tier. See `combat.md` for the roster's role in the fight. |

**Only the unequipped side lives in `equipment.json`, and that is one-home-per-fact, not an
oversight.** Every *equipped* tier already had a home and stays there: the bare hand's `attack 1` is
the `creatures.json` `person` row, the kitted haul rate `40` is `labor_config.json`'s
`hunt.per_worker_biomass_capacity`, and the kitted gather rate `8` is its
`forage.per_worker_biomass_capacity` — the rates the shipped game has always run on, because a band
has always started kitted. Copying any of them here would give a shipped number a second home to
drift from. `equipment.json` carries only what the *kits themselves* own: what they do, and how long
they last.

**All three wear rates are on `plan_early_game_labor`'s ~15–20-turn kit-duration clock**, against the
shipped ~16-worker band: hunting Red Deer it reaches 16 animals / 240 biomass a turn (250/16 ≈ 15.6
turns of spears, 5000/240 ≈ 20.8 turns of sled); gathering it reaches 16 × 8 = 128 biomass a turn
(2500/128 ≈ 19.5 turns of baskets).

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

`components::BandEquipment { hunting_wear, sled_wear, basket_wear }` — and storing *wear* rather than
*stock* is what makes "the band starts kitted" free: `Default` is zero wear, so a spawn site inserts a
full kit without reading config, and an **absent** component reads as *no wear recorded* — a full kit
— via the same `copied().unwrap_or_default()` reading `SimState` gives `DemographicFlowAccumulator`.
There is deliberately **no third "carries no kit at all" state**; dry is expressed as wear reaching
`starting_durability` (strictly-below is equipped, so a kit worn exactly to its limit is spent). Both
carry kits floor a wear charge at zero through one shared `usable_biomass` helper — a negative take
must never *restore* a kit, and neither kit may grow its own rule for that.

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

## On the wire

`PopulationCohortState` carries six append-only kit fields (`sim_schema/schemas/snapshot.fbs`,
captured in `snapshot/population.rs` through the `BandKitLevers` bundle so the resolution happens in
one place):

| Field | Meaning |
|---|---|
| `huntingKitDurability:float` | Remaining condition, 0–100 scale; `0` = dry |
| `sledKitDurability:float` | ditto, the sled |
| `basketKitDurability:float` | ditto, the baskets |
| `hunterAttack:float` | The band's resolved per-hunter `attack` (1 bare / 20 kitted) — the left side of the fight's gate against a herd's `HerdTelemetryState.defense` |
| `huntCarryPerWorkerBiomass:float` | The band's resolved per-worker **hunt** haul rate (40 sledded / 12 sledless) |
| `forageCarryPerWorkerBiomass:float` | The band's resolved per-**gatherer** throughput, *before* the tile's seasonal weight (8 with baskets / 1.6 bare-handed) |

**The last two are separate fields on purpose.** A band can be out of baskets with its sled untouched;
a client that rendered one on the other web's row would be repeating the defect the split corrected.
`HerdTelemetryState.perWorkerBiomass` and `ForagePatchState.perWorkerBiomass` both stay the *equipped
reference* rate: neither a herd nor a patch has a band to resolve a tier against.

## Balance

The shipped opening is **unchanged** — a start-kitted band hunts and gathers at exactly the numbers it
always did. What is new is the state below each cliff. Measured on the shipped ~16-worker starting
band, one turn:

| hunt (Red Deer, `engage_rate 1`, `body_mass 15`, fat herd) | sledded | sledless |
|---|---|---|
| per-worker haul rate | 40.0 | 12.0 |
| biomass hauled home | 240 | 180 |
| food income | 4.80 | 3.60 |

| gather (the starting band's own patch, floor `0.0`) | with baskets | bare-handed |
|---|---|---|
| per-worker gather rate | 8.0 | 1.6 |
| biomass gathered | 128 | 25.6 |
| food income | 8.49 | 1.70 |

**A known config skew, not a model problem:** a per-kill charge is species-blind, so a party on
`Wild Fowl` (10 engaged per hunter, 160 kills a turn) burns the same hunting kit in under two turns
where the deer party gets ~15. The lever if that proves to wreck pacing is a per-species use cost,
never a turn clock.

See Also: `combat.md` (the resolver and the `person` roster row the attack tier composes onto),
`yield-forecast.md` (the forecast-equals-actual invariant both seed arms preserve),
`docs/plan_early_game_labor.md`, `docs/plan_hunt_through_combat.md` §4.8.
