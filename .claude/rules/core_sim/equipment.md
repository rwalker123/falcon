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

## Config files

| File | Purpose |
|---|---|
| `src/data/equipment.json` | **The TOE kit table** (loader `equipment_config.rs`, env override `EQUIPMENT_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks: **`hunting_kit`** — `equipped_attack` (**20.0**), `starting_durability` (**100.0**), `wear_per_kill` (**0.4** → 250 kills); **`carry_kit`** — `unequipped_per_worker_biomass_capacity` (**12.0**), `starting_durability` (**100.0**), `wear_per_biomass_carried` (**0.02** → 5000 biomass). `validate` rejects any of the six as non-finite or `<= 0`: a kit with **no wear rate is not consumable** and one with no durability is **born dry**. |
| `src/data/creatures.json` | The creatures roster — intrinsic `CombatStats` for non-fauna units. `person.combat.attack` (**1.0**) is the hunting kit's **unequipped** tier. See `combat.md` for the roster's role in the fight. |

**Only two of the four tiers live in `equipment.json`, and that is one-home-per-fact, not an
oversight.** The two *unequipped-side* facts already had homes and stay there: the bare hand's
`attack 1` is the `creatures.json` `person` row, and the kitted haul rate `40` is
`labor_config.json`'s `hunt.per_worker_biomass_capacity` — the rate the shipped game has always run
on, because a band has always started kitted. Copying either into `equipment.json` would give a
shipped number a second home to drift from. `equipment.json` carries only what the *kit itself*
owns: what it does, and how long it lasts.

## The three rules

1. **Two tiers, never a taper.** Performance is **flat until expiry**, then the role **steps down**.
   Durability and performance are deliberately **orthogonal axes** so a future crafting economy can
   tune them independently, and nothing may scale a readout by remaining condition. Pinned by
   `the_durability_cliff_is_a_step_not_a_taper`, which sweeps wear across the kit's life and asserts
   the exported tier is *the same number* at every point below expiry.
2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2 depends on
   it: a turn clock charges an idle march the same as a slaughter, which makes denial free). The
   hunting kit wears per **animal killed**; the carry kit per **biomass carried home**. Pinned by
   `wear_is_charged_for_kills_not_for_turns_elapsed` — same world, same turn count, a scouting band
   loses exactly zero.
3. **Start-stocked and NOT craftable.** Running dry is the intended pressure and the pull into the
   Milestone-2 crafting economy. Nothing in the sim reduces wear, so the unequipped tier is
   **absorbing** (`a_kit_run_dry_stays_dry`).

## The band carries WEAR, not stock

`components::BandEquipment { hunting_wear, carry_wear }` — and storing *wear* rather than *stock* is
what makes "the band starts kitted" free: `Default` is zero wear, so a spawn site inserts a full kit
without reading config, and an **absent** component reads as *no wear recorded* — a full kit — via
the same `copied().unwrap_or_default()` reading `SimState` gives `DemographicFlowAccumulator`. There
is deliberately **no third "carries no kit at all" state**; dry is expressed as wear reaching
`starting_durability` (strictly-below is equipped, so a kit worn exactly to its limit is spent).

Inserted by `spawn_profile_population` (`systems/worldgen.rs`), by both expedition-outfitting paths
in `bin/server.rs`, and restored by `sim_state.rs` (`BandRecord::equipment`, carried unconditionally
— a checkpoint that forgot how worn your spears were would silently re-stock them on rollback).

## Where the tiers are consumed

- **`advance_labor_allocation`** resolves the carry tier **once per band per turn**, *before* the
  assignment loop, so a kit that expires part-way through cannot pay two different rates to two herds
  in one turn. That one `hunt_per_worker_biomass` feeds every hunt-arm site: `hunt_take`, the pen's
  `collection`, `project_realized_hunt` / `project_arrivals_hunt`, `hunt_take_workers` and
  `hunt_haul_workers`. Wear is charged **after** the take (the same accrue-after-take ordering every
  rung's build meter uses), so the turn is paid at the tier it was priced with and the cliff lands on
  the next turn.
- **The assign-time seed** (`seed_source_yield` in `bin/server.rs`) resolves the same tier through the
  same `EquipmentConfig::per_worker_biomass_capacity` seam. It has to: the forecast-equals-actual
  invariant (`yield-forecast.md`) would otherwise promise a dry band a kitted haul.
- **A pen harvest wears the CARRY kit only.** A penned beast is slaughtered, not stalked — no fight,
  no spear to blunt — which is the same reason that branch passes no engagement bound to the
  quantiser.

## What is NOT wired yet, deliberately

- **The attack tier is published and inert in the fight.** The Phase-0 hunt-danger adapter still
  fields the intrinsic `person` profile; `EquipmentConfig::hunter_profile` is the composition seam
  the slice that moves the kill into `combat::resolve_fight` will call. Pinned as an identity by
  `the_attack_tier_is_published_but_inert_in_the_take`: a spent hunting kit and a full one resolve a
  hunt with the same take, the same food income and the same casualties, while the exported
  `hunterAttack` differs 20 against 1.
- **Expeditions are implicitly always equipped.** `advance_expeditions` still reads
  `labor.hunt.per_worker_biomass_capacity` directly and never wears a party's kit, so a detached
  party hauls at the kitted rate for as long as it is out. It is spawned with a full
  `BandEquipment` so its wire row does not contradict its haul rate.
- **The Crafter role, replenishment/upgrade, and the Scouting and Warrior kits** from that arc's role
  table are out of scope for this slice.

## On the wire

`PopulationCohortState` gains four append-only fields (`sim_schema/schemas/snapshot.fbs`, captured in
`snapshot/population.rs` through the `BandKitLevers` bundle so the resolution happens in one place):

| Field | Meaning |
|---|---|
| `huntingKitDurability:float` | Remaining condition, 0–100 scale; `0` = dry |
| `carryKitDurability:float` | ditto, the carry kit |
| `hunterAttack:float` | The band's resolved per-hunter `attack` (1 bare / 20 kitted) — the left side of the fight's gate against a herd's `HerdTelemetryState.defense` |
| `carryPerWorkerBiomass:float` | The band's resolved per-worker haul rate (40 kitted / 12 dry) |

**`HerdTelemetryState.perWorkerBiomass` stays the *equipped reference* rate** and is unchanged: a
herd has no band to resolve a tier against, and the band's own rate is the field above.

## Balance

The shipped opening is **unchanged** — a start-kitted band hunts at exactly the numbers it always
did. What is new is the state below the cliff. Measured on the shipped ~16-worker starting band
hunting Red Deer (`engage_rate 1`, `body_mass 15`) with a fat herd, one turn:

| | equipped | dry |
|---|---|---|
| per-worker haul rate | 40.0 | 12.0 |
| biomass hauled home | 240 | 180 |
| food income | 4.80 | 3.60 |

**A known config skew, not a model problem:** a per-kill charge is species-blind, so a party on
`Wild Fowl` (10 engaged per hunter, 160 kills a turn) burns the same hunting kit in under two turns
where the deer party gets ~15. The lever if that proves to wreck pacing is a per-species use cost,
never a turn clock.

See Also: `combat.md` (the resolver and the `person` roster row the attack tier composes onto),
`yield-forecast.md` (the forecast-equals-actual invariant the seed site preserves),
`docs/plan_early_game_labor.md`, `docs/plan_hunt_through_combat.md` §4.8.
