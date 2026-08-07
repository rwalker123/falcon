---
paths:
  - "core_sim/src/{equipment_config,creatures_config}.rs"
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
| `src/data/equipment.json` | **The TOE kit table** (loader `equipment_config.rs`, env override `EQUIPMENT_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Three blocks: **`hunting_kit`** — `equipped_attack` (**20.0**), `starting_durability` (**100.0**), `wear_per_kill` (**0.4** → 250 kills); **`sled_kit`** — `unequipped_per_worker_biomass_capacity` (**12.0**), `starting_durability` (**100.0**), `wear_per_biomass_hauled` (**0.02** → 5000 biomass); **`basket_kit`** — `unequipped_per_worker_biomass_capacity` (**1.6**), `starting_durability` (**100.0**), `wear_per_biomass_gathered` (**0.04** → 2500 biomass). `validate` rejects any of the nine as non-finite or `<= 0`: a kit with **no wear rate is not consumable** and one with no durability is **born dry**. A missing *block* is a parse error, so a file that forgot `basket_kit` cannot silently leave the forage web unkitted again. **Plus the KIT ROSTER**: `kits` — a list of `{ id, display_name, jobs, uses }`, where `uses` names the component blocks above and `jobs` is `hunt` / `forage`; and `default_kits` — `{ hunt, forage }`, what each verb runs on when the player names none. Shipped: **`big_game`** (`hunt`; `hunting_kit` + `sled_kit`), **`gathering`** (`forage`; `basket_kit`), **`none`** (both jobs; nothing). `validate` rejects a duplicate id, a kit listing no jobs, a default naming no roster entry, and a default whose `jobs` omit its own job; a `uses` entry naming a component block that does not exist fails to **deserialize**, because `KitComponent`'s variants *are* the block keys. See "A kit is a MASK". |
| `src/data/creatures.json` | The creatures roster — intrinsic `CombatStats` for non-fauna units. `person.combat.attack` (**1.0**) is the hunting kit's **unequipped** tier. See `combat.md` for the roster's role in the fight. |

**Only the unequipped side lives in `equipment.json`, and that is one-home-per-fact, not an
oversight.** Every *equipped* tier already had a home and stays there: the bare hand's `attack 1` is
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

## A kit is a MASK over the three predicates — nothing else

A party is **sent out with a named kit** from the roster rather than implicitly using whatever the
band owns. The whole mechanism is one line:

```text
effective_equipped(component) = kit_uses(component) AND band_has_condition(component)
```

- **`big_game`** uses `hunting_kit` + `sled_kit` — the pair every hunt path used to consult
  unconditionally, so it is **bit-identical** to the pre-roster game.
- **`gathering`** uses `basket_kit` — likewise for every gather path.
- **`none`** uses nothing, so every predicate reads false and the party runs at the three
  *unequipped* tiers throughout.

`KitChoice` (`equipment_config.rs`) is the id plus that mask, and its three predicates
`hunting_equipped` / `sled_equipped` / `basket_equipped` take `(&BandEquipment, &EquipmentConfig)`.
**They are the only way anything asks the question**: `BandEquipment`'s own condition tests are
`pub(crate)` and renamed `has_*_condition`, so the *condition* half cannot be read alone by a caller
that has forgotten to consult the mask — which is exactly the reading that silently re-arms a party
sent out bare.

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

The kit selection adds five more slots, all append-only:

| Field | Meaning |
|---|---|
| `SubsistenceSection.kits:[KitOption]` | **The roster, once per world** — `id`, `displayName`, `jobs`, and the three tiers each kit grants a party whose components are **fresh** (`attack`, `huntCarryPerWorkerBiomass`, `forageCarryPerWorkerBiomass`), so the picker renders real numbers without a second copy of the TOE table |
| `SubsistenceSection.defaultHuntKitId` / `defaultForageKitId:string` | What each verb runs on when the player names none |
| `PopulationCohortState.kitId:string` | Which kit the two **hunt** tiers above are quoted at — an in-flight party's **own** kit (one kit, so it covers that party's forage tier too), a resident band's **hunt job default** (a band has one kit per assignment and this row is per cohort). **It does not name a resident band's forage kit**: `forageCarryPerWorkerBiomass` resolves through the *forage* default, so pairing the two reads a gathering rate off `big_game`, which has no basket component. The forage default rides the wire once, as `defaultForageKitId`; pinned by `kit_selection::a_resident_bands_published_kit_answers_for_the_hunt_tiers_only` |
| `LaborAssignment.kitId:string` | The kit that row's yields are priced at, **resolved** — never "unspecified". `""` on a band-wide role, which has no kit axis |
| `HerdTelemetryState.huntTripEstimatesKitId` / `denialEstimatesKitId:string` | Which kit each estimate table was computed at — see above |

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
