---
paths:
  - "core_sim/src/{equipment_config,creatures_config}.rs"
  - "core_sim/src/visibility_systems.rs"
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
| **`sled`** — travois, drag harness | `hunt_carry` **40** (equipped, on its **tier**) | per **biomass hauled home from a hunt** |
| **`baskets`** | `forage_carry` **8** (equipped, on its **tier**) | per **biomass gathered** |
| **`traps`** — the passive device (snares, nets, weirs) | `attack` **20** bounded to `max_body_mass` **1.0**, `dispersion` **0**, `exposure` **0** | per **animal killed** |
| **`husbandry_gear`** — hurdles, halters, a butchering stone, vessels | `pen_carry` **12** (unequipped) | per **biomass BUTCHERED off a pen** (what was killed, not what was hauled home) |
| **`wayfinding`** — tallies, marked staves, a fire-drill | `scout_vantage_range` **1** (unequipped) | per **tile revealed for the FIRST time** |
| **`clubs`** | `attack` **6** (equipped) | per **fight resolved** |
| **`tanning_frame`** — a BENCH TOOL, bounds `hide` | `craft_speed` **2.0**, `craft_quality_ceiling` **0.90**, `craft_material_efficiency` **0.80**, all equipped | per **item completed at the bench** |
| **`loom`** — bounds `fibre` | `craft_speed` **2.0**, ceiling **0.95**, efficiency **0.85** | likewise |
| **`bone_awl`** — bounds `bone` | `craft_speed` **1.6**, ceiling **0.85**, efficiency **0.70** | likewise |

Shipped kits: **`big_game`** (`spears` + `sled`), **`trapping`** (`traps` + `sled`),
**`gathering`** (`baskets`), **`husbandry`** (`husbandry_gear` + `sled`), **`wayfinding`**
(`wayfinding`), **`warrior`** (`clubs`), **`none`** (nothing).

> **The three tools are in NO kit, and `validate` enforces that.** A tool serves the *bench*, not a
> party: no take path reads a craft stat and no take site charges `item_crafted`, so a kit naming one
> would carry it onto the range to grant nothing and never wear. Its live predicate is **ownership +
> condition** rather than a kit mask — `EquipmentConfig::live_bench_tool`, which is also the only
> reader of `BandEquipment::owns`. See `crafting.md` → "A BENCH TOOL's ownership is a real question".

## The two band-wide roles have a kit axis now, and that was the shape change

`KitJob` was `hunt` / `forage`; `LaborTarget::kit_job` answered **`None`** for Scout and Warrior, so
`LaborAssignment.kitId` published `""` on those rows and neither role had a tier to step down from.
The stated reason — *they consume no component* — was a fact about **this file**, not about the sim:
scouts have posted forward-observer vantages in `calculate_visibility` and warriors have been the
band's defending contingent in `advance_predator_raids` for some time. It stopped being true the
moment the roster carried gear for them.

So `KitJob` has four members, `kit_job()` is infallible, `LaborAssignment::kit_choice` is infallible,
`default_kits` names four kits, and `assign_labor … scout 3 kit none` is a real selection rather than
a token ignored the way `species` and `floor` are on those rows. **`no_kit()` survives** — it is still
what a crew resolves to when nothing is named at a site with no assignment to read.

### Each new kit's USE QUANTUM, which is the one genuinely new decision per kit

`WearQuantum` gained `biomass_collected`, `tile_revealed` and `fight`. Still no `turn` variant — that
is `docs/plan_denial_raid.md` §1.2 enforced by the type — and none of the three collides with the
three that shipped before, pinned by `each_new_kit_wears_on_its_own_use_quantum`.

- **`tile_revealed` means FIRST-EVER revealed, and only from a SCOUT VANTAGE.** A parked band re-sees
  the same ring every turn and its own centre reveals new ground whenever it walks, so charging per
  tile *seen* — or for any reveal rather than a vantage's — would be a turn clock in a per-use
  costume. `FactionVisibilityMap::mark_active` returns whether it lifted an `Unexplored` tile, because
  that is a **transition**: by the time a caller could look, the tile reads `Active` either way. The
  `sources` vec carries the band a vantage belongs to as a fifth field, `None` on every other kind.
  Pinned from both ends — `wear_is_charged_for_kills_not_for_turns_elapsed` asserts the gear really
  runs down, `only_a_staffed_scout_wears_the_wayfinding_kit` asserts a band with nobody on Scout
  spends none of it over the same span while revealing fog throughout.
- **`fight` is one ENGAGEMENT, not one casualty inflicted.** A defence that killed nothing was still
  fought, and pricing gear on its results charges the band that is losing the least. A band nobody
  raided pays zero; a band three packs turned on pays three. Gated on `warrior_count > 0` — nothing
  was swung by a band with nobody on the row.
- **`biomass_collected` is the pen's, and a pen charges it and `biomass_hauled` over DIFFERENT
  numbers.** The sled is charged for what it **hauled** (`take.carried`); the handling gear is
  charged for what it **butchered** (`take.killed_biomass()`). Hurdles, halters, a butchering stone
  and vessels are worked on the whole beast brought out of the pen and killed, not on the fraction
  that made it home — and the gap is reachable rather than theoretical: waste needs
  `workers × pen_carry < body_mass`, and a Wild Aurochs (`body_mass 120`, pennable, one required
  keeper) at the equipped `pen_carry` of 40 kills 120 and carries 40, so a single basis would
  under-charge the gear threefold on the animal it did the most work on.

  **Two quanta rather than one is a SECOND, independent reason** and is worth keeping distinct from
  the basis question: it is what lets a band that only keeps pens leave a sled it never took onto the
  range untouched, and what lets either life be retuned without moving the other. The first cut
  charged both over `take.carried` on the strength of that reason alone — which is exactly how a
  physical claim goes unexamined behind a correct-but-different one.

### A pen is collected on `pen_carry`, and bringing a sled to it costs you

**`PenCarry` is a separate stat from `HuntCarry`, and that is the physical claim "one item, one job"
already makes twice.** A sled drags a carcass in off the range; a pen stands at the camp, and what
bounds a slaughter there is handling gear. The *equipped* side stays
`labor_config.hunt.per_worker_biomass_capacity` — the number a pen harvest has always been capped by,
keeping its one home — so a keeper carrying husbandry gear collects **exactly what a pen always
collected**. What is new is the state below the cliff.

> **THE SHIPPED OPENING MOVES HERE, deliberately.** A band that corrals a herd and leaves the
> assignment on the hunt job's default (`big_game`) is working its pen with a drag harness and no
> handling gear, and collects at `12` rather than `40`. That is the same mistake as bringing baskets
> to a deer, and the roster exists so the player can stop making it — the `trapping` kit set the
> precedent for a kit that is *not* a subset of the old behaviour. The pen's collection cap rarely
> binds (a pen breeds at up to 3× the wild rate but its MSY clears about one body a turn), so what
> this bites is a large-bodied pen worked by a small crew.

**One rate serves the whole Hunt arm**, resolved once as `herd_carry_per_worker` from
`herd.is_corralled()`: `hunt_forecast` splits on exactly that predicate and early-returns the managed
path, so the branch that runs is decided by the herd and the other is never reached. The **assign-time
seed** (`seed_source_yield`) makes the identical split — a pen priced at the sled's tier while it
collects at the husbandry gear's is `yield-forecast.md`'s forecast-equals-actual invariant broken on
the one surface a player commits from, and it is what
`a_field_and_a_pen_collapse_the_policy_axis_but_still_need_carrying_home` caught the moment the two
stats parted.

### The warrior kit reuses `attack`, and a band-wide kit may carry no mass bound

**One stat for both roles rather than a `warrior_attack` of its own**, because `attack` is already
*"what this person hits with"* and a second stat would be a second authority over the number the
resolver reads. What keeps a club out of a hunt and a spear out of a raid is the kit's `jobs` list.
`EquipmentConfig::warrior_profile` is the seam, composing onto the same `person` roster row
`hunter_profile_*` does — and only the **warrior contingent** is armed: the exposed populace stays at
`attack 0` whatever the band carries, which is the whole reason it is a separate contingent.

Equipped **6** against the bare hand's **1**, well under the spear's 20: a raid is people fighting
animals at the camp with whatever is by the fire, not a hunting party that chose its ground. **This is
the one place the shipped opening improves** — a start-kitted band defends a predator raid at 6 where
it used to defend at 1 — and Predators Phase 1b is explicitly a placeholder resolver, so the raid
tuning is the right place to absorb it.

> **A mass-bounded weapon in a Scout or Warrior kit is REJECTED at validate.** `warrior_profile`
> resolves at `Quarry::Any`, so a bounded weapon would count *everywhere* — a snare rated to hold a
> hare would arm the camp against a wolf pack. That is `config-loading.md`'s "looks live but isn't" in
> its worst direction: the bound parses, validates, and is then ignored by the one resolver that reads
> it. The twin of the hunt default's own quarry-blindness check, and pinned by
> `a_band_wide_kit_may_not_carry_a_mass_bounded_weapon`.

**An EFFECT names the value a stat TAKES — never a delta, never a multiplier stacking on something
else.** `EquipmentEffect` has no representation for a taper, which is what makes "flat until expiry,
then a step down" structural rather than a rule someone has to remember.

**Which SIDE an effect declares is one-home-per-fact showing through**, not free choice: the other
side already had a home and keeps it. `spears` declares the **equipped** `attack` because the bare
hand's `1.0` is `creatures.json`'s `person` row; `husbandry_gear` and `wayfinding` declare the
**unequipped** side because their equipped values live elsewhere (the sled's tier and
`labor_config.scout.vantage_range`). **The two CARRIES flipped when quality tiers landed** — see
"Quality tiers" below.

## Quality tiers — what the MATERIAL bought

An item's `effects` and `starting_durability` moved under a **`tiers`** list. **What is SHARED stays
on the item** (`wear`, `dispersion`, `exposure`, the unequipped side of a rate, `bounds_material`);
**what the MATERIAL buys sits on the tier** (`starting_durability`, `attack`, the carry rates, a
tool's craft stats).

- **A tier is an AGE, and the vocabulary is shared across items.** Every shipped item's one tier is
  `flint`, so the day metal lands each gains a `bronze` beside it and the ladder is gated once
  (*"bronze needs Smithing"*) rather than per item.
- **`tiers[0]` is the default** — what a spawn stocks, what every reference rate resolves through,
  and the one tier `validate` forbids a `requires_knowledge` on. **The ORDER is the model**: a bench
  makes the best tier the faction knows (`ItemDefinition::craftable_tier`), which a map could not
  express, so the list is a `Vec`.
- **FLINT IS TODAY'S SPEAR, VERBATIM** — `starting_durability 100`, `attack 20` — so the whole move
  is a re-homing in which **not one number changes value**.
- **No bronze row ships.** An unreachable tier is dead content
  `SubsistenceSection.equipmentConfigJson` publishes to the Workbench, so tier switching is covered
  by a **fixture** instead (`a_tier_switches_an_items_attack_without_touching_its_shared_effects`),
  the same treatment `materials.json`'s `varieties` get.
- **A mass bound rides with the effect it bounds**, so `traps`' `max_body_mass` sits on its tier's
  `attack` rather than on the item. The design lists the bounds as shared, and this is the deliberate
  departure: an effect with a bound but **no value** is not representable — an effect names the value
  a stat takes — so a second tier of the passive device restates its bound, and `validate_mass_bounds`
  still checks it. A **grade** that replaces such an effect must restate the bounds verbatim
  (`recipes_config::validate_grades_against_item`), or an excellent snare would quietly become a mammoth
  trap.

### THE EQUIPPED CARRY RATES CAME OUT OF `labor_config.json` — the number moved, the key did not

`labor_config.json`'s `hunt.per_worker_biomass_capacity` and `forage.per_worker_biomass_capacity` are
the **no-equipment baselines** now (`12.0` and `1.6`, the values the two items used to declare as
their `unequipped` side); the sled's and the baskets' own `flint` tiers declare the equipped `40.0`
and `8.0`. The keys' **role** changed and their names did not, deliberately: every caller hands them
to `EquipmentConfig::{hunt,forage}_per_worker_biomass_capacity`, whose argument is the fallback
either way, so a rename would churn ~40 call sites to say the same thing.

> **The trap is a READOUT quoting `labor_config`'s number as "what a worker collects".** It is `12`
> where the game collects `40`. Every surface with **no band to resolve a tier against** must go
> through **`EquipmentConfig::equipped_reference(stat, baseline)`**, which reads the item table's
> default tier. The re-pointed readers are:
>
> | reader | now resolves |
> |---|---|
> | `snapshot/subsistence.rs` — a herd row's `hunt_forecast` and `HerdTelemetryState::per_worker_biomass` | `equipped_reference(HuntCarry)`, threaded in as `HerdSnapshotInputs::equipped_haul_rate` (the struct's `labor` field is **gone**, so nothing there can reach the baseline by accident) |
> | `snapshot/subsistence.rs` — a `ForagePatchState`'s forecast and `per_worker_biomass` | `equipped_reference(ForageCarry)`, threaded into `snapshot_forage_patches` |
> | `systems/labor.rs` — a rung-3 **Field**'s managed collection cap (`managed_per_worker_yield` / `_fodder` / `_trade`, `field_harvest_biomass`) | `equipped_reference(ForageCarry)`, threaded through `forage_forecast` / `project_realized_forage` / `project_arrivals_forage` / `forage_source_yield_preview` |
> | `systems/expeditions.rs` — `hunt_per_worker_provisions`, the expedition **outfit** lever on `PopulationCohortState` | `equipped_reference(HuntCarry)`, resolved at the capture site |
> | `systems/expeditions.rs` — `expedition_take_provisions` | takes a **resolved** haul tier rather than a `LaborConfig`, so it cannot reach for the key at all |
>
> Everything else was already passing the argument into an `EquipmentConfig` resolver, so it needed
> **no change** — it was handed the equipped rate and is now handed the baseline, and the resolver's
> answer is the same either way.

**`pen_carry` still shares the hunt haul's rate, and now shares it through the item table.**
`EquipmentStat::shares_equipped_rate_with` links `PenCarry → HuntCarry`, and
`pen_per_worker_biomass_capacity` resolves the equipped side through `equipped_reference` internally
— so the number keeps its **one home** (the sled's tier) and no pen call site had to change.

## One item, one job

The pairing is **physical**, which is the whole of §4.8's correction:

**A carcass is one lumpy object you drag out whole**, so a container does not help you move a deer —
a sled does. **Berries are the opposite**: loose, divisible, bounded entirely by what you can hold,
which is exactly what a basket fixes. The minimal TOE shipped with *one* carry kit, called baskets,
raising the **hunt's** haul — backwards on the physics, and it left `forage.per_worker_biomass_capacity`
untouched by any kit at all. The three-kit split is that correction, not new scope.

**The two carries want different SHAPES.** Forage is **containment**-bound — a handful against a
basketful — so its ratio is large (the baskets' tier `8.0` against the `1.6` baseline, exactly a
fifth). The hunt is **transport**-bound — a sledless party can always drag *something* — so its ratio
is smaller (the sled's tier `40.0` against `12.0`, under a third). Pinned as an ordering *between the ratios* by
`losing_your_baskets_costs_proportionally_more_than_losing_your_sled`.

## Dispersion, exposure and reach — the three multipliers

All three are **neutral at `1.0`**, so an item declaring none of them is priced exactly as it was
before the effects model existed.

> **`1` has to be spelled out THREE TIMES on the wire, once per defaulting mechanism, and they have
> no compiler relationship to each other.** `KitOptionState`'s `dispersion` / `exposure` and
> `HerdTelemetryState`'s `stay_fraction` are each reachable through the FlatBuffers schema's `= 1`,
> `serde`'s missing-field default, and the Rust `Default` impl — and a bare `#[serde(default)]` or a
> `#[derive(Default)]` answers **`0`**, which on two of them is wrong *in the reassuring direction*:
> `dispersion 0` says the party scares nothing and `exposure 0` says nobody can be hurt, i.e. a field
> that failed to arrive would hand every kit the passive device's entire advantage. Hence the named
> `multiplier_neutral()` helper and the hand-written `Default` impls in
> `sim_schema/src/state/subsistence.rs`, and
> `the_retreat_and_hazard_multipliers_are_neutral_at_one_on_every_defaulting_path`, which pins all
> three doors and is sabotage-verified to fail on each alone.
>
> **`attack_min_body_mass` / `attack_max_body_mass` are the deliberate exception**: `0` is their
> *sentinel* for "unbounded", it is their schema default too, and it is what every weapon but the
> passive device ships. A sweep that "fixed" them to `1.0` would silently bound every weapon at 1 kg,
> so the same test pins them at `0`.

- **`dispersion` multiplies the QUARRY'S OWN `wariness`** at the retreat:
  `effective_wariness = clamp(wariness × dispersion, 0, 1)`, and `stayers = engaged × (1 −
  effective_wariness)`. A trap ships `0` — nothing breaks off at contact.

  **A multiplier rather than a subtraction, so the SPECIES decides how much a noisy approach costs.**
  At `wariness 0.85` a gazelle loses almost its whole engagement to one; at `0.10` a mammoth barely
  notices. That is what lets a single spear line scatter a warren and *contain* a mammoth with no
  per-target authoring — **and it is why equipment needs no size-class or "targets" axis at all**.
  `wariness 0` is already an exact identity in the retreat (no draw is made), so `dispersion 0` lands
  in a regime the sim has always had.

  **Every retreat resolves through `fauna::HuntingParty::stayers` (drawn) or
  `HuntingParty::stay_fraction` (closed), never the bare `fauna::animals_that_stay` /
  `fauna::stay_fraction` with a species' `wariness`** — that pair is the primitive the party methods
  are built on, and calling it directly is how the kit's `dispersion` gets dropped. It was dropped, on
  both take paths at once (`systems::hunt_take` and `systems::expeditions::expedition_take_biomass`),
  which charged a trapping party the full retreat while the forecast beside it
  (`fauna::forecast_production_and_take_at`) and the kit picker
  (`fauna::per_hunter_take_biomass`, which is how a Rabbit Warren publishes `trapping` as its default)
  both kept the trap's stand-off. The whole ~4× advantage this section describes was quoted and never
  paid. Since the retreat also sizes the crew (`fauna.md` → "THE RETREAT PRICES THE CREW"), a
  bare-wariness take now puts three readouts at odds instead of two.
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
> `default_kits.hunt` is what this file answers wherever there is **no quarry to test a bound
> against**: a band's own `hunterAttack` row, `HuntingParty::builtin_equipped`, and a herd whose
> species the roster cannot resolve, all of which go through `hunter_profile_unbounded`. A bounded
> weapon there would be counted *everywhere* — the twin of the band-wide-kit check above, and
> `config-loading.md`'s "looks live but isn't" in its most reassuring direction.
>
> **A bounded kit may still be a QUARRY's default**, because that resolution passes the animal's own
> `body_mass` — which is exactly what makes `trapping` legal there and illegal here. The check's
> *original* reason was narrower and is retired: `snapshot/capture.rs` used to build one unbounded
> party for every herd, and it now resolves per species (see "Which kit a QUARRY wants is DERIVED").
>
> **The forecast query does not need this guarantee and does not rely on it either**: it has a quarry,
> so it resolves `hunter_profile_against(.., herd.body_mass)` and a trapping party sent after a
> mammoth is quoted the bare hand's attack — the gate refusing the raid, which is the same answer the
> take will give.

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
| `src/data/equipment.json` | **The TOE** (loader `equipment_config.rs`, env override `EQUIPMENT_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks plus one scalar. **`items`** — a map of id → `{ wear: { per, amount }, effects: [...], bounds_material?, tiers: [...] }`. **What is SHARED sits on the item and what the MATERIAL bought sits on a TIER** — `effects` here carries the multipliers and the *unequipped* side of a rate; each **tier** is `{ id, starting_durability, requires_knowledge?, effects }` and carries `attack` (with its mass bounds), the carry rates and a tool's craft stats. `stat` is one of `attack` / `hunt_carry` / `forage_carry` / `pen_carry` / `scout_vantage_range` / `dispersion` / `exposure` / `craft_speed` / `craft_quality_ceiling` / `craft_material_efficiency`; `per` is `kill` / `biomass_hauled` / `biomass_gathered` / `biomass_collected` / `tile_revealed` / `fight` / `item_crafted` — **there is no `turn` variant, and that is `docs/plan_denial_raid.md` §1.2 enforced by the type**. An item may also carry **`bounds_material`**, which makes it a **bench tool** (`crafting.md`). **Every shipped item ships ONE tier, `flint`** (see "Quality tiers"), at the durabilities the game has always had: `spears` (100, 0.4/kill → 250 kills, `attack 20`), `sled` (100, 0.02/biomass → 5000, `hunt_carry 40`), `baskets` (100, 0.04/biomass → 2500, `forage_carry 8`), `traps` (100, 0.2/kill → 500 — twice the spear's life per kill because a trap is *worked* rather than thrown, and on the **same quantum** so a trapping party cannot hunt for free), `husbandry_gear` (100, 0.04/biomass **butchered** → 2500 — halved from the 0.08 the collected-equals-carried basis shipped with, because `killed_biomass ≥ carried` always), `wayfinding` (100, 0.05/tile first-seen → 2000), `clubs` (100, 2.0/fight → 50 raids), and the three **bench tools** `tanning_frame` / `loom` / `bone_awl` (100, 4.0/item crafted → **25 items** each). **`kits`** — `{ id, display_name, jobs, uses }`, where `uses` names items and `jobs` is `hunt` / `forage` / `scout` / `warrior`; plus `default_kits`, which names one per job, **`quarry_default_kit_margin`** (**0.25**) — how decisively a kit must beat `default_kits.hunt` on a species before it replaces it as that *quarry's* published default. Required, like every other key here. And **`life_readout`** `{ warn_fraction 0.34, danger_fraction 0.10 }` — the two colour seams of the published `lifeSeverity`, as **fractions of one fresh unit's** quanta rather than absolute counts, because a spear's 250 kills and a sled's 5000 biomass are not comparable and one absolute would colour one of them permanently red. **Presentation only**: nothing in the sim branches on it (`crafting.md` → "the life meter is a fuel gauge"). **`validate` rejects**: an empty item table; a non-finite or `<= 0` wear amount or tier durability; an item with **no effects on itself or any tier** (it would wear out doing nothing); **an item with no tiers** (no durability — born dry); a **duplicate tier id**; a **knowledge gate on the first tier** (that one is what a spawn stocks and every reference rate resolves through, so it must ship known); a stat declared **twice within one layer** (`effect_entry` takes the first match, so the second is silently dead); a stat declared on **both the item and one of its tiers** (the tier wins, so the shared line would be dead config); an **`unequipped` side on a tier** (an unequipped value is what you get when the item is *not* there, which is true of every tier at once); a negative or non-finite effect value; a mass bound on any stat but `attack`, or an inverted one; **a mass-bounded `attack` on an item a Scout or Warrior kit uses**; **two items declaring the same two-sided rate** anywhere across their item and tier effects (`declared_tier` and `equipped_reference` both take the first match, so it would resolve alphabetically); a duplicate kit id; a kit listing no jobs; a default naming no roster entry or not covering its own job; a non-finite or negative `quarry_default_kit_margin`; a **`life_readout`** seam outside `0..=1` or a `danger_fraction` not strictly below `warn_fraction` (the warn band would be unreachable, so one colour would simply never appear); and **a `uses` entry naming an item the table does not carry**. That last one is a DEBT, not a nicety — see below. **The bench tool's own rejections** (`validate_bench_tools`, every one of which is otherwise silent at runtime): a craft stat on an item with no `bounds_material`; a craft stat declaring an `unequipped` side; two items bounding one material; a tool that does not wear per `item_crafted`, **or a non-tool that does**; a tool declaring no craft stat at all; and **a kit naming a bench tool**. Plus, at the composition seam, `validate_against_materials` rejects a `bounds_material` the materials table does not carry, **and a tier `requires_knowledge` naming a craft no material declares** — an authored tier that could never be earned is the `UnknownItem` debt in its most expensive direction. |
| `src/data/creatures.json` | The creatures roster — intrinsic `CombatStats` for non-fauna units. `person.combat.attack` (**1.0**) is the hunting kit's **unequipped** tier. See `combat.md` for the roster's role in the fight. |

### `UnknownItem` pays back a guarantee the model used to get for free

The retired `KitComponent` enum's three variants **were** the JSON block keys, so a roster naming a
component with no block **could not deserialize** — the invariant was carried by the type, at no
cost, on every load path. An item id is a `String`, so nothing stops a config naming `spearz`, and
the only thing between that file and a running sim is now `validate`'s `UnknownItem` check. A kit
that silently granted nothing and wore nothing is exactly the failure §4.8 corrects, so
`a_kit_using_an_item_that_does_not_exist_is_rejected` must not be deleted.

**Every number lives exactly once, and which file holds it follows from what it describes.** The
bare hand's `attack 1` is `creatures.json`'s `person` row; the **no-equipment** carry baselines
(`12` / `1.6`) are `labor_config.json`'s two `per_worker_biomass_capacity` keys; the *equipped* rates
(`40` / `8`) and every `starting_durability` are on the item's own **tier**, because those are what
the material bought. A pen's equipped rate is not a fourth number — it is the sled tier's, borrowed
through `EquipmentStat::shares_equipped_rate_with`.

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
   Durability and performance are deliberately **orthogonal axes** so the crafting economy can tune
   them independently, and nothing may scale a readout by remaining condition. Pinned by
   `the_durability_cliff_is_a_step_not_a_taper`, which sweeps wear across all three kits' lives at
   once and asserts each exported tier is *the same number* at every point below expiry. **Quality
   tier and craft grade are discrete and fixed at craft time**, so neither reintroduces one: a batch's
   tier decides what it grants and its wear decides only *whether* it still does.
2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2 depends on
   it: a turn clock charges an idle march the same as a slaughter, which makes denial free). **Each
   kit has its own quantum** (table above), so the three cannot cross-charge. Pinned by
   `wear_is_charged_for_kills_not_for_turns_elapsed` — same world, same turn count, a scouting band
   loses exactly zero — and by `the_sled_and_the_baskets_wear_on_different_quanta`: a hunting band
   finishes with whole baskets, a gathering band with a whole sled and whole spears.
3. **Start-stocked, and craftable since the bench landed.** Running dry was a one-way door for
   exactly as long as nothing could make a second spear; it is now the pull into a replenishment loop
   (`crafting.md`). **`BandEquipment::stock` is the one seam in the sim that ADDS condition**, and it
   is called from two kinds of place — a spawn, and `systems::advance_crafting` on an item the bench
   finished. Every unequipped tier is still **absorbing on its own** (`a_kit_run_dry_stays_dry`,
   `baskets_run_dry_on_their_own_quantum_and_stay_dry`, both of which run worlds with an empty
   bench): nothing *decays* wear, nothing repairs a batch, and a band that makes nothing stays dry.

## The band carries BATCHES, and an ABSENT ENTRY IS NOT OWNED

`components::BandEquipment` is a **`BTreeMap<String, Vec<EquipmentBatch>>`**, where an
`EquipmentBatch` is `{ count, tier, grade, wear }`. `BTreeMap` rather than `HashMap` so the checkpoint
and the wire serialize in a stable order — a rollback that reordered this would diff as a change
every frame — and a `Vec` inside it because batch order is insertion order, which is what makes
*"the most worn first, earliest batch on a tie"* a deterministic rule.

> ### THE ABSENT-ENTRY INVARIANT IS INVERTED. It used to read as a FULL item; it reads as NOT OWNED.
>
> That was correct for exactly as long as nothing could make a second spear. **Crafting can introduce
> an item a band has never had**, which the old reading made unrepresentable — so ownership is now
> the ordinary question, and *"does the band have one"* and *"has it any condition left"* are the
> single reading `has_condition` gives. `EquipmentConfig::live_bench_tool` used to be the one place
> that asked ownership honestly (nothing stocks a tool at spawn); this **generalises** it, and
> `BandEquipment::owns` is gone with the special case.
>
> **Every insert path therefore states the stock**, and this is the flip's load-bearing surface:
> `spawn_profile_population` (`systems/worldgen.rs`), **both** expedition-outfitting paths in
> `bin/server.rs` (through one `outfitted_party_equipment` helper, because *"a party leaves
> outfitted"* is one fact), and `sim_state.rs`'s restore, which carries `BandRecord::equipment`
> verbatim. All of them go through **`BandEquipment::start_stocked(config)`** — one unworn unit of
> every item some kit `uses` (`EquipmentConfig::start_stocked_items`; a bench tool can never appear
> there, because `validate` rejects a kit that names one). Pinned by
> `a_band_with_no_entry_for_an_item_resolves_the_unequipped_tier`, which asserts the empty ledger's
> bare tiers **and** the start-stocked ledger's equipped ones, so a sim that stopped resolving
> equipment at all could not pass it.
>
> **An absent COMPONENT is a different question and still reads as start-stocked.** `Default` is
> *owns nothing*; a band with no `BandEquipment` component at all has no ledger rather than an empty
> one, which is a hand-rolled fixture, and `advance_labor_allocation` / `advance_expeditions` /
> `snapshot/capture.rs` fall back to `start_stocked` there.

**A SPAWN STOCKS `count: 1`, AND THAT IS WHAT PRESERVES THE SHIPPED OPENING.** One unit is one item's
`starting_durability` — the life the game has always had — so **a count above 1 is something crafting
bought**, and counts do *not* multiply the shipped kit's life. Pinned against the literal use counts
this file records by `a_spawned_bands_kit_life_is_one_items_worth_and_no_more`.

**Ten spears made together wear together, and the stock runs out ONE BATCH AT A TIME.** A batch
carries one `wear` number, spending the unit currently in hand; crossing the tier's durability retires
that unit (`count -= 1`) and carries the remainder onto the next, so a batch of `count` holds
`count × starting_durability` of life. `wear_item` charges the **serving batch** — the most worn one
still live, earliest on a tie — and that is the same batch `EquipmentConfig::live_item` prices the
party at, so *what the party is priced at is what the party is spending*. A batch that runs out of
units is **removed**, which is what makes crafting a replenishment loop and *"turns left"* a real
readout rather than an average.

**A RETIRED UNIT LEAVES A TALLY BEHIND, and it is a readout, not gameplay.** `BandEquipment::retired`
counts the units of each item this band has worn out, incremented by `wear_item` — the one seam that
destroys a unit. It exists because an emptied batch is *removed*, so *"the sled broke"* and *"we have
never had a sled"* are the same empty ledger and are not the same sentence to a player
(`crafting.md` → "`Worn out` and `Never made` both read `count 0`"). **Nothing in the sim branches on
it and nothing may** — it must not become a repair discount. The checkpoint carries it for free,
because `BandRecord::equipment` clones the whole component.

**IDLE STOCK DOES NOT ROT.** Nothing charges a batch that did not go out and nothing decays one over
turns, so stockpiling ahead of a hard season is a real strategy. Both halves pinned by
`wear_runs_the_stock_out_one_batch_at_a_time_and_idle_stock_does_not_rot`.

**A party still resolves UNIFORMLY**, on the serving batch — the partly-equipped party is **#520** and
is deliberately not this. `has_condition` keeps its meaning for every existing caller (*the band owns
at least one unit with condition*), which is what kept #520 out of this slice.

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

  - `dispersion` (the **trap's**, and the spear's explicit neutral) rides the `HuntingParty` the arm
    builds, so it reaches all three of the retreat's readers at once: the drawn take (`hunt_take`),
    the closed-form take (`per_hunter_take_biomass`) and the **crew** (`hunt_engage_workers`, via
    `HuntingParty::stay_fraction` — `fauna.md` → "THE RETREAT PRICES THE CREW").

  Wear is charged **after** the take on both webs (the same accrue-after-take ordering every rung's
  build meter uses), so the turn is paid at the tier it was priced with and the cliff lands on the
  next turn.
- **The assign-time seed** (`seed_source_yield` in `bin/server.rs`) resolves the band's own tier on
  *both* arms, through the same two `EquipmentConfig` seams. It has to: the forecast-equals-actual
  invariant (`yield-forecast.md`) would otherwise promise a dry band a kitted haul or a bare-handed
  crew a basketful.
- **A pen harvest wears the SLED and the HUSBANDRY GEAR, and never the hunting kit.** A penned beast
  is slaughtered, not stalked — no fight, no spear to blunt — which is the same reason that branch
  passes no engagement bound to the quantiser. The two it *does* wear are charged on **different
  numbers**: see "A pen is collected on `pen_carry`" above.
- **An expedition never touches baskets.** A raid is a hunt (`ExpeditionMission` has no gather verb),
  so `advance_expeditions` resolves the sled and the hunting kit and nothing else.

**`forage_per_worker_biomass(capacity, seasonal)` takes a RESOLVED tier, not a config handle.** That
is the seam the basket tier rides; sites with no band to resolve against (the patch telemetry in
`snapshot/subsistence.rs`, a Field's managed collection cap) pass
`EquipmentConfig::equipped_reference(ForageCarry, ..)` — the item table's default tier, **not**
`labor_config`'s key, which is the bare-handed baseline. Same on the animal web for
`HerdTelemetryState::per_worker_biomass`. See "THE EQUIPPED CARRY RATES CAME OUT OF
`labor_config.json`".

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

- ~~The Crafter role and replenishment/upgrade are out of scope.~~ ~~What remains of #494 is the
  **count** and the quality **tier**.~~ **All three are wired**: replenishment (`crafting.md` → "The
  bench"), counts (batches, above) and tiers. **There is no Crafter role** — crafting always has a
  subject, so `set_bench` staffs it like a worked source rather than like a standing role. What
  remains of the arc is the **panel** (§7). ~~Nothing publishes a batch's count, tier or grade to a
  client yet.~~ **The wire is landed**: `PopulationCohortState.equipmentBatches` carries the count,
  the tier, the grade and the life wording, and `crafting.md` → "On the wire" is its rationale.
- **The partly-equipped party is #520 and is deliberately still open.** A party resolves uniformly
  off the serving batch: 16 hunters and 10 spears is 16 speared hunters. Counts were the blocker and
  are now landed.
- ~~The compose sheets offer no kit picker on a Scout or Warrior row.~~ **Wired.** The Band panel's
  WORKFORCE zone mounts the picker on each role CARD, over a line stating what the kit buys and the
  condition of the item behind it, and the card emits `assign_labor … <role> <n> kit <id>` on the
  pick — `.claude/rules/client/band-city-panel.md` → "The role cards carry the band's OTHER two kits".
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
reaches on its own, and resolving it through the roster would let a config edit panic the labor loop.

**What it is FOR changed when the band-wide roles gained a kit axis**, and the old justification is
worth stating so it is not quietly restored: it used to be *"a Scout or Warrior row has no kit to
resolve"*, and those rows now resolve their own job's default like every other. What survives is a
band **with no `LaborAllocation` component at all** — the fallback in `calculate_visibility`'s two
scout seams — and `HuntingParty::builtin_unequipped`, which wants every unequipped tier and every
neutral multiplier to come from the *same* resolution a live bare-handed party runs rather than from
a hand-built profile that could drift.

**An unstaffed singleton role is NOT one of those cases.** `LaborAllocation::kit_on` falls back to
the **job's default**, not to the empty kit, so a zero-worker Warrior row answers the same tier the
same row with one worker on it does. Both its consumers gate on the head-count first, so the choice
costs nothing — what it buys is that the two readings cannot differ.

**Nothing resolves a stat by naming an item.** `hunter_profile` asks the kit for the best *equipped*
`attack` among its live items; the carries ask whether any live item *supplies* their stat and fall
back to what the table declares. So a future bow is a config row, not a code change — and the only
place an item id is spelled in the sim is a test fixture.

**`none` is an ORDINARY roster member, not a sentinel.** Nothing branches on its id anywhere; it is a
kit whose `uses` list is empty, and every behaviour attributed to it falls out of that. A future
`fishing` kit with an empty `uses` would behave identically, which is the test of whether it has been
special-cased.

### Wear rides the SAME predicate that chose the tier

Every wear site is gated on the effective predicate its own tier came from. **There are eight, and
this list is the one an audit checks against** — three of them are outside `systems/labor.rs`'s
assignment loop entirely, which is exactly how a site gets missed:

| where | charges | gated on |
|---|---|---|
| `systems/labor.rs` — the gather | baskets, per biomass gathered | the crew's kit supplies `forage_carry` |
| `systems/labor.rs` — a pen harvest | the sled per biomass **hauled**, the husbandry gear per biomass **butchered** | each item's own live predicate |
| `systems/labor.rs` — a wild hunt | spears/traps per kill, the sled per biomass hauled | likewise, and independently |
| `systems/expeditions.rs` — the raid's take | the hunting kit and the sled | the party's launch-time kit |
| `systems/expeditions.rs` — the scout's roadside kill | likewise | likewise |
| `visibility_systems.rs` — `calculate_visibility` | wayfinding, per tile first revealed | only a **scout vantage's** first sightings, and only for the band that posted it |
| `systems/labor.rs` — `advance_predator_raids` | clubs, per fight resolved | `warrior_count > 0` — nobody swung anything in a band with no warriors |
| `systems/crafting.rs` — `advance_crafting` | the bench tool, per **item completed** | the tool was live for this draw (`live_bench_tool`) — a bare-handed bench wears nothing, and it is the **only** site that charges `item_crafted` |

So a party using no component spends no durability on any of them.

**The bench's is the one charge that goes through `wear_item` rather than `wear_kit`**, and that is
not an inconsistency: `wear_kit` names a *quantum* and charges every item in a **kit** that wears on
it, and a bench has no kit. The tool is resolved from the **material** instead, so the pairing that
makes `wear_kit` safe — charge only what is actually serving — is kept by
`live_bench_tool` returning `None` for a tool the band does not own or has worn out.

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
from the band. `None` reads as the job's default, **on all four roles** — Scout and Warrior included
now that each has a job to default through. `assign_labor` stores the *resolved* choice, so a
replayed command lands on the kit it named rather than on whatever the default is today.

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

### Every pre-launch forecast is priced at the player's OWN kit and wear

There is no surface left that quotes one kit to everybody. The forecast query
(`core_sim/src/forecast_query.rs`) takes `kit_id` as an **argument** and resolves the party against
the **asking band's live `BandEquipment`**, so a band whose spears have run dry is quoted the attack
it actually has — intrinsic `1`, which against a Red Deer's `defense 1.0` is an effective **zero** and
no party of any size works. The launch feed line (`launch_forecast_party` + `launch_forecast_haul`),
the in-flight delivery ETA and both assign-time compose seeds (`hunt_source_yield_preview` /
`forage_source_yield_preview`) were already priced at the chosen kit.

**`defaultKitId` still rides that per-species quote, and it is what is left of the two retired
tables' machinery.** `snapshot/capture.rs` resolves one `QuotedParty` per *species × source axis*
(range / pen), memoized, so a herd row's published fight tier and the kit id it names are the same
answer by construction. What went with the tables is the two id fields that used to disclaim them —
the query takes the kit as an argument and echoes it on every row, so there is nothing left to
disclaim.

**This section used to say the opposite**, and the reason it could is worth keeping: the two per-herd
estimate tables were quoted at the hunt job's default kit over a *fresh* component set, published
`huntTripEstimatesKitId` / `denialEstimatesKitId` so a client could **refuse** to present them for
another selection, and were ~95% of snapshot capture — so a kit axis multiplied a cost that was
already dominant. Retiring the tables in favour of an on-demand query removed the cost and the
disclaimer together: measured, capture went from **49.51 ms to 3.15 ms** and the herd pass from
**46.22 ms to 0.06 ms** (`expeditions.md` → "What the query replaced, and what it cost"). A
disclaimer is what you publish when you cannot answer the question; the answer is better.

## Which kit a QUARRY wants is DERIVED, never authored

`default_kits.hunt` is one id for the whole job, and that could not express *"which kit this quarry
wants"*. A Rabbit Warren's compose sheet therefore opened on the **Stalking kit** — which works on a
rabbit and is **~4× worse** than `trapping`, because a rabbit's `wariness 0.75` loses a spear party
three animals in four to the retreat while the trap's `dispersion 0` keeps all of them. The player
was defaulted onto the wrong tool on exactly the quarry the roster has a right one for.

**`fauna::quarry_default_hunt_kit` scores the roster against the species** and publishes the winner
per herd (`HerdTelemetryState.defaultKitId`). It is reached through `fauna::herd_default_hunt_kit`,
which answers the **source axis** first — a corralled herd never reaches the score, see "A PEN is not
a scoring question" below. The score is §4.6's own per-hunter-turn ceiling,
`fauna::per_hunter_take_biomass`:

```text
min(engage_rate, (attack − defense) / durability) × (1 − clamp(wariness × dispersion, 0, 1)) × body_mass
```

composed from the resolver's own `combat::strike_damage` / `combat::units_brought_down` and the
retreat's own `fauna::stay_fraction`, so it is the take model read as a rate rather than a second
copy of it. It sees **no** carry tier, crew size, escapement floor or quantiser — every term it drops
is a property of the *band* or the *herd* rather than of the kit, which is what makes it a fair
comparison between two kits against one quarry.

> **A CONFIG PREDICATE (`mass < X && wariness > Y`) WAS REJECTED.** Those facts already exist twice —
> the trap declares `max_body_mass 1.0` and `dispersion 0`, the species declares `body_mass`,
> `wariness` and `defense` — and a third copy would drift on the first retune. It is the same mistake
> this file already records rejecting twice: `dispersion` multiplies the species' own `wariness`
> rather than reading a "jumpy" flag, and `max_body_mass` reads `body_mass` rather than a
> `size_class`, both explicitly to avoid a second authority. A mass/wariness threshold would also be
> **silently wrong on `defense`**, which is what actually zeroes a trap party on a Marsh Grazer.

### Scored at the FRESH tier — wear must not enter

Every kit is scored against `BandEquipment::default()`, so a herd's default is a property of
**quarry × roster**: a per-world constant per herd, which cannot reshuffle under the player as their
spears wear down. Scoring live would do exactly that — a dry `big_game` party falls to the bare
hand's `attack 1`, a 20× cut on a `defense 0` warren, enough to flip any margin. It is the same rule
the picker's greying follows. Pinned by
`kit_selection::a_herds_default_kit_does_not_move_when_the_band_wears_its_kit_to_dry`.

### A near-tie keeps the job default — `quarry_default_kit_margin`

The winner replaces `default_kits.hunt` only when it scores more than
`(1 + quarry_default_kit_margin) ×` the default's own score. Without a margin the published default
flips on a trivial retune and the player watches their sheet move for reasons they cannot see. **A
default that scores `0` is beaten by anything positive** — "better than nothing" needs no margin, and
a margin cannot be expressed against zero. Ties resolve to the **earliest roster entry**, because the
fold keeps only a strictly greater score.

Measured against the shipped roster at `0.25`, the whole outcome is a clean split at the trap's
`max_body_mass`:

| quarry | body mass | job default | best | ratio | published default |
|---|---|---|---|---|---|
| Wild Fowl | 0.13 | 0.455 | 1.300 | 2.86 | **`trapping`** |
| Rabbit Warren | 0.27 | 0.675 | 2.700 | 4.00 | **`trapping`** |
| Forest Grouse | 0.47 | 1.253 | 3.133 | 2.50 | **`trapping`** |
| Snow Hare Warren | 0.60 | 1.000 | 4.000 | 4.00 | **`trapping`** |
| Silt Catfish | 0.67 | 4.020 | 6.700 | 1.67 | **`trapping`** |
| Desert Gazelle and everything above | ≥ 3.3 | — | ties | 1.00 | `big_game` |

**Nothing is near the line.** The narrowest genuine win is the Silt Catfish's `1.67×`, well clear of
`1.25`; every large-game row is an exact tie, because a trap grants no attack past its bound and
`husbandry` / `none` carry no weapon at all, so all three score exactly what the bare hand does.
`kit_selection::a_narrow_win_keeps_the_job_default_until_the_margin_lets_it_through` sweeps the
lever against the roster's *own* narrowest win rather than against a literal, so a retune moves the
threshold with the game.

### A PEN is not a scoring question — the source axis answers first

`fauna::herd_default_hunt_kit` is the one seam every no-kit-named surface resolves through, and it
puts a **source-axis** test in front of the score:

> A corralled herd's default is the kit that supplies `EquipmentStat::PenCarry`; every other herd's
> is the score's winner.

**The scorer structurally cannot answer for a pen.** `per_hunter_take_biomass` prices a fight, and a
pen has no fight stage — so it scored a corralled Rabbit Warren exactly as it scored one on the
range and published `trapping`, a kit whose contribution at a pen is nil. A pen is collected on
`PenCarry`, and the only kit supplying it is the handling gear (see "A pen is collected on
`pen_carry`"). That is the same axis the picker's greying and `KitRoster.priced_source` already read
off the source (`.claude/rules/client/labor-ui.md`), so it is one rule with three readers rather
than a special case beside the score.

**The kit is DERIVED, not named** — `EquipmentConfig::kit_supplying(job, stat)` returns the earliest
hunt kit, in file order, whose live items declare the stat at the fresh tier. *"Nothing resolves a
stat by naming an item"* applies to kits too: `"husbandry"` is spelled nowhere in the sim, only in a
test fixture, so a roster that moves the handling gear to another kit moves the pen's default with
it. **No such kit ⇒ fall through to the score**, which is the honest answer: nothing on this roster
can work a pen properly, so the herd keeps whatever the range comparison chose rather than
publishing an empty selection.

**Wear does not enter here either.** `kit_supplying` resolves against `BandEquipment::default()`,
because *which* kit supplies a stat is a property of quarry × roster, not of how worn one band's
gear is — the same rule the score follows and the picker's greying follows.

Capture-side this is a **second per-species quote table** (`penned_parties` beside `quoted_parties`
in `snapshot/capture.rs`), not a per-herd resolution: the axis is a property of the herd but the
quote is a property of the species, so each map is still one memo per species and a herd row is
still one probe.

### It is resolved SIM-side, and every command boundary agrees with the wire

`handle_assign_labor`'s **no-kit-named** path resolves the *herd's* default for a `LaborTarget::Hunt`
(`default_kit_for_target` → `EquipmentConfig::resolve_kit_or`). If the client picked a display
default while the command still resolved `default_kits.hunt`, the sheet would open on Trapping and
the command would run Stalking — the same silent substitution *"an unknown id … is a command
failure, never a silent fall back"* exists to prevent, arriving through the **absent-token** door.
The named path is untouched and is still the only validated one. `LaborAssignment` stores the
*resolved* choice, so a replayed command is unaffected.

**The two raiding verbs resolve through the SAME seam** — `resolve_raid_kit` builds a
`LaborTarget::Hunt` naming the raid's herd and hands it to `default_kit_for_target`, rather than
carrying a second resolution. `send_hunt_expedition` and `send_denial_raid` took `default_kits.hunt`
on the absent token while the herd published its own, and that is the launch-sheet form of the same
defect: the client's sheet reads `defaultKitId` and both estimate tables beside it are priced at
that id, so the forecast the player committed from was **not** the one the party went out on. The
kit is still resolved before the party is drawn off the band, so a bad id refuses the launch outright.

`Expedition` stores the *resolved* choice and prices its whole life from it, so a raid launched on
the herd's default keeps that kit even after the herd is penned or the roster is retuned.

**`default_kits.hunt` stays the fallback and the answer for every surface with no quarry**: a
Forage / Scout / Warrior row, a herd id the registry does not carry, a species the roster cannot
resolve, a band's own `hunterAttack`, and `HuntingParty::builtin_equipped`. That is now the stated
reason `validate` rejects a mass-bounded attack in it — see the callout above, whose old reason
(*"the capture builds one party for every herd"*) the per-herd resolution retired.

The shipped surfaces are pinned end to end, each against the **published** id rather than against a
re-derivation, because the claim is that two surfaces agree and a re-derivation agrees with itself:

| test | pins |
|---|---|
| `kit_selection::a_warren_defaults_to_the_trap_and_a_deer_to_the_spear_on_the_wire` | the score, off the **encoded** envelope, with an `assert_ne!` liveness half |
| `kit_selection::a_corralled_herd_defaults_to_the_pen_kit_and_a_wild_one_of_the_same_species_does_not` | the source axis — the same species, penned and ranging, compared **to each other**, so a constant fails |
| `server::tests::a_hunt_row_with_no_kit_named_stores_the_kit_the_wire_published_for_that_herd` | `assign_labor`'s absent-token path |
| `server::tests::a_raid_with_no_kit_named_launches_on_the_kit_the_wire_published_for_that_herd` | `send_hunt_expedition`'s, with the same `assert_ne!` against the job default |

## On the wire

`PopulationCohortState` carries seven append-only kit fields (`sim_schema/schemas/snapshot.fbs`,
captured in `snapshot/population.rs` through the `BandKitLevers` bundle so the resolution happens in
one place):

| Field | Meaning |
|---|---|
| `kitItemConditions:[KitItemCondition]` | **One row per item the config carries** — `itemId` + `remaining` on the 0–100 scale, `0` = dry. It replaced three fixed floats (`huntingKitDurability` / `sledKitDurability` / `basketKitDurability`), which are **`(deprecated)` in the schema rather than deleted**: FlatBuffers field ids are positional, so removing one renumbers every field after it. **Driven by the CONFIG's item table, not the band's sparse ledger**, so an item is never missing from the list — but since the count slice an item the band does not **own** reads `0`, not full, and `remaining` is the condition left on the **serving batch** (the most-worn live one), which is what makes it a fuel gauge for the unit actually in hand. **`count` rides beside it** since the crafting wire stage, and it is what stops a client inferring ownership from a condition of zero: `remaining == 0` means *owns none*, never *"owns one that is dry"* — a batch with no units left is removed. Which of *worn out* / *never made* a zero is, is `equipmentBatches`' answer (`crafting.md` → "On the wire") |
| `equipmentBatches:[EquipmentBatchState]` | **One row per BATCH**, plus one `count: 0` row per config item the band owns none of — `itemId`, `tierId`, `grade`, `count`, `remaining`, and the **life wording in use quanta, never percent**. It is the crafting arc's field; the rationale, the `Worn out` / `Never made` split and the `BandEquipment::retired` tally it needed are in `crafting.md` → "On the wire" |
| `hunterAttack:float` | The band's resolved per-hunter `attack` (1 bare / 20 kitted) — the left side of the fight's gate against a herd's `HerdTelemetryState.defense` |
| `huntCarryPerWorkerBiomass:float` | The band's resolved per-worker **hunt** haul rate (40 sledded / 12 sledless) |
| `forageCarryPerWorkerBiomass:float` | The band's resolved per-**gatherer** throughput, *before* the tile's seasonal weight (8 with baskets / 1.6 bare-handed) |
| `kitTiers:[BandKitTiers]` | **What EVERY offered kit would grant this band, at its live wear** — one row per roster kit (`kitId` + the same **nine** tiers `KitOption` carries: `attack`, the two mass bounds, `huntCarryPerWorkerBiomass`, `forageCarryPerWorkerBiomass`, `penCarryPerWorkerBiomass`, `scoutVantageRange`, `dispersion`, `exposure`). See below: it is the resolved answer, and a client must not re-derive it |
| `penCarryPerWorkerBiomass:float` | The band's resolved per-**keeper** pen collection rate (40 with husbandry gear / 12 without). It shares the hunt haul's *equipped* rate — `labor_config.hunt.per_worker_biomass_capacity`, the number a pen harvest has always been capped by, which keeps its one home — but resolves through `EquipmentStat::PenCarry`, so a Hunt row on the stalking kit works the pen at the bare rate |
| `scoutVantageRange:float` | The sight range each posted vantage reveals at (2 with wayfinding gear / 1 without). **How far the vantages are posted is not a kit axis** — three `labor_config.scout.*` dials — and `calculate_visibility` rounds this to whole tiles |
| `warriorAttack:float` | The band's resolved per-**warrior** `attack` (1 bare / 6 with clubs) — the defending contingent's side of `advance_predator_raids`. The same stat and the same seam `hunterAttack` resolves through, quoted at a different kit |

**They are separate fields on purpose.** A band can be out of baskets with its sled untouched, and it
fights raids with clubs while it hunts with spears; a client that rendered any of them on another's
row would be repeating the defect the three-kit split corrected.
`HerdTelemetryState.perWorkerBiomass` and `ForagePatchState.perWorkerBiomass` both stay the *equipped
reference* rate: neither a herd nor a patch has a band to resolve a tier against.

**The last three were published per KIT before they were published per BAND**, and that gap is what
they close: `KitOption` has carried `penCarryPerWorkerBiomass` / `scoutVantageRange` / `attack` since
the roster expanded, so the picker could quote a fresh kit's numbers while no readout could state a
keeper's actual pen rate, a scout's actual reach, a warrior's actual tier, or the cliff when any of
them runs dry.


The kit selection adds the slots below, all append-only. The two retired ones —
`HerdTelemetryState.huntTripEstimatesKitId` / `denialEstimatesKitId` — are `(deprecated)` in the
schema; they disclaimed the estimate tables, which are gone.

| Field | Meaning |
|---|---|
| `SubsistenceSection.kits:[KitOption]` | **The roster, once per world** — `id`, `displayName`, `jobs`, `itemIds`, and the tiers each kit grants a party whose components are **fresh** (`attack`, `huntCarryPerWorkerBiomass`, `forageCarryPerWorkerBiomass`, plus the appended `penCarryPerWorkerBiomass` and `scoutVantageRange`), so the picker renders real numbers without a second copy of the TOE table |
| `SubsistenceSection.defaultHuntKitId` / `defaultForageKitId` / `defaultScoutKitId` / `defaultWarriorKitId:string` | What each verb runs on when the player names none — **and, for Hunt, only where there is no quarry to score against**; a herd names its own below. The last two arrived with the expanded roster; before it the band-wide roles had no kit axis and so no default to name |
| `PopulationCohortState.kitId:string` | Which kit the row's **hunt-job** tiers are quoted at — an in-flight party's **own** kit (one kit, so it covers *every* tier on that party's row), a resident band's **hunt job default** (a band has one kit per assignment and this row is per cohort). See "One choice per JOB" below for the three tiers it deliberately does **not** answer for on a resident band |
| `LaborAssignment.kitId:string` | The kit that row's yields are priced at, **resolved** — never "unspecified" and never `""`: a band-wide role publishes its own job's default now |
| `KitOption.itemIds:[string]` | **Which items the kit carries** — its `equipment.json` `uses` list verbatim, in config order (`big_game` → `["spears", "sled"]`). The tiers beside it are numbers and name no item, so without this a durability readout has to guess which component produced them — and the guess was `attack → "spears"`, which quoted a Trapping party the spears' condition. An **empty** list is a real answer (`none` carries nothing), never "unknown" |
| `HerdTelemetryState.defaultKitId:string` | **The kit THIS HERD wants** — what the hunt compose sheet opens on, and what `assign_labor … hunt <herd> <n>` **and both raiding verbs** resolve with no `kit` token. Derived at the fresh tier from the take score against the species, *except* for a **corralled** herd, which takes the kit supplying `EquipmentStat::PenCarry` (a pen has no fight to score — see "A PEN is not a scoring question"). Empty only for a species the roster cannot resolve, which falls back to `defaultHuntKitId`. See "Which kit a QUARRY wants is DERIVED" |

### `kitTiers` — the resolved per-band answer, because the derivation is impossible on the wire

`PopulationCohortState.kitTiers` publishes, per band, what **each** roster kit would grant it *right
now*. The world-level `SubsistenceSection.kits` stays — it is the picker's list and the fresh-kit
reference — and this is the same nine numbers resolved against the band's own `BandEquipment`.

**A client must not step a tier down for itself**, and this is the field that makes that unnecessary.
It is also the field that makes it *possible* to be right, because stepping down cannot be done from
the wire at all:

> Stepping a tier down needs the **axis → item** mapping, and that mapping is **per kit**: `big_game`
> supplies `attack` from `spears`, `trapping` supplies it from `traps`. `KitOption.itemIds` names
> what a kit carries but not what each item is *for*, and no rule over that list recovers it —
> set-cover and positional order both mis-assign, *"any item live"* keeps a kit at full tier with its
> weapon dry, and *"all items dry"* keeps it at full tier with only the sled left.

The live symptom of guessing: a band with **fresh traps and dry spears** repriced to the bare hand
under `trapping`. Same root cause as the pre-launch estimate tables this arc retired — a fact the sim
knows that the wire does not carry — and the same fix.

**One arithmetic, three call sites.** `EquipmentConfig::resolve_kit_tiers` is the single resolver:
`snapshot::kit_roster_states` calls it per kit over a **fresh** ledger, `snapshot::population_state`
calls it per kit over the **band's** ledger, and `forecast_query` resolves the same seams for the
party it prices. It was extracted rather than copied precisely because this field would otherwise
have been a third transcription of the same nine calls.

> **A kit axis that is resolved BESIDE that call is an axis one of the readings will lose.** The pen
> and the vantage were: `kit_roster_states` open-coded
> `pen_per_worker_biomass_capacity` / `scout_vantage_range` next to the resolver, and the per-band rows
> — built from the resolver's output alone — therefore went to the wire without them. A picker asking
> what the kit *under the cursor* would grant fell back to the roster's **fresh** tier for exactly
> those two, so a pen compose sheet read **40 per keeper** while the sim collected **12** with the
> handling gear dry, and a Scout role card read **2 tiles** of sight while `calculate_visibility`
> revealed at **1** with the wayfinding gear dry. Both wrong in the *reassuring* direction. Both are
> inside `ResolvedKitTiers` now, and both call sites read them from there.
>
> **`warriorAttack` is deliberately not a tenth number**: it is the same `attack` the row already
> carries, read through a different *kit*, so the warrior kit's own row answers it.

Size is bands × kits — a handful each — and it diffs out between frames when nothing wears.

Pinned by `kit_selection::a_bands_published_tiers_step_down_per_kit_by_which_item_that_kit_actually_uses`,
which wears one band's spears to the cliff, leaves its traps untouched, and asserts **both** that
`trapping` keeps its attack and that `big_game` loses it — the pairing, because asserting only the
first would pass on a sim that had stopped stepping tiers down at all. It also asserts the shared
**sled**'s haul tier is unchanged on both kits, which is what a naive "any item in this kit is dry"
rule would break.

Its twin
`::a_bands_published_pen_and_vantage_tiers_step_down_per_kit_at_the_item_that_supplies_them` does the
same for the two appended axes — handling gear and wayfinding gear worn to the cliff, `husbandry`'s
pen rate and `wayfinding`'s reach each falling to the bare tier while `big_game` (which supplies
neither) is unmoved and the shared **sled** keeps its haul tier. **Every assertion is paired against
the same row read BEFORE the wear**, because *"the pen rate is 12"* passes on a table that publishes
12 for everything and *"it is unmoved"* passes on a table that never moved.

### One choice per JOB, not one per tier — and `kitId` names only one of them

A resident band holds **one kit per assignment**, so a cohort row that carries six resolved tiers is
quoting **four** different kits at once. `population_state`'s `job_choice(job)` is the single seam:
an in-flight party's own kit if it has one, otherwise that **job's** default.

| tier | resolved through | rides the wire as |
|---|---|---|
| `hunterAttack`, `huntCarryPerWorkerBiomass`, `penCarryPerWorkerBiomass` | the **hunt** default | `PopulationCohortState.kitId` |
| `forageCarryPerWorkerBiomass` | the **forage** default | `SubsistenceSection.defaultForageKitId` |
| `scoutVantageRange` | the **scout** default | `SubsistenceSection.defaultScoutKitId` |
| `warriorAttack` | the **warrior** default | `SubsistenceSection.defaultWarriorKitId` |

**The pen is on the hunt row deliberately** — a pen is worked from a Hunt assignment, so it shares
the hunt default and `kitId` *does* answer for it.

**Pairing any of the other three with `kitId` reads the wrong kit's tier**, and each mis-pairing is a
concrete wrong number: a gathering rate off `big_game`, which carries no basket component; a vantage
off a kit with no wayfinding gear; a warrior's `attack` off the hunt kit's spears (20 instead of the
club's 6). There is deliberately **no** second per-cohort `*_kit_id` field — each of those three
defaults already rides the wire once per world, and the per-crew truth is the assignment row's own
`LaborAssignment.kitId`, so a per-cohort copy would be a third home for a fact that has two.

**These six answer at the JOB DEFAULT, and that is a different question from the one `kitTiers`
answers.** A readout with **no kit selected** — the band's gear line, a role card's own tier — wants
this band's tier at the kit it would actually use, which is what the flat fields are. A picker wants
what the kit **under the cursor** would grant, which is that kit's `kitTiers` row. Both are per band
and both are resolved sim-side; neither is derivable from the other, because the job default is one
kit and the picker offers all of them. `penCarryPerWorkerBiomass` and `scoutVantageRange` ride **both
tables** for exactly that reason, and the flat pair is not redundant with the rows beside it.

Pinned by `kit_selection::a_resident_bands_published_kit_answers_for_the_hunt_tiers_only` and its
twin `::a_resident_bands_appended_tiers_each_answer_for_their_own_jobs_default`, which compare the
numbers a client would actually mis-pair rather than the wording. The party side —
`::an_in_flight_partys_appended_tiers_are_all_quoted_at_the_kit_it_was_sent_with` — sends the party
out with the **husbandry** kit, the one roster entry whose three appended tiers all differ from the
job default each would otherwise resolve to, so a resolution reaching for a default fails all three.

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
