# Plan: Predators & the Danger of the Hunt

Status: **Design in progress — not yet implemented.** This is the authoritative spec for the
predator / dangerous-fauna layer: predators as ordinary fauna that *eat other fauna*, an
attack/defense/aggression rating web that makes the hunt itself dangerous, band casualties as a
new mortality path (the first consumer of the long-inert **Warrior** role), and a shared
prey-seeking movement primitive that scouting expeditions later adopt.

It supersedes and unifies two stub tasks that both pointed here:
- **M1-threats — minimal predators** (`docs/plan_early_game_labor.md` → *Minimal predator
  threat*; `TASKS.md` → Early-Game Labor). "Abstract pressure so Warrior is live."
- **(Optional) Predators / threat fauna** (`TASKS.md` → Fauna Roster). "Wolves/big cats need the
  predator-pressure model, not a `SpeciesDef` alone."

We do **not** build the throwaway abstract-pressure version. The herd/ecology/movement/snapshot
stack already carries almost everything a real ecological predator needs, so we build predators
as **carnivore herds** from the start and phase the delivery. What the two stubs asked for — a
live Warrior role and a casualty interface — is the *interface*, and it is identical either way.

## The core realization — there is no "predator" entity, only config

A predator is not a new kind of thing. It is an ordinary `Herd` (same `HerdRegistry`, same
logistic ecology, same whole-animal quantization, same movement dispatch, same snapshot) sitting
in a particular corner of a config space with **two kinds of knob, deliberately kept apart**:
**strength** (how hard it hits / how hard to kill — open-ended magnitude) and **behaviour** (what it
*does* — will it fight, will it come for you). "Predator" is just a name for one region of that space.

| Knob | Kind | Meaning | rabbit | deer | wild boar | mammoth | wolf pack |
|---|---|---|---|---|---|---|---|
| **`attack`** (`CombatStats`) | strength | damage output when it fights. **Open-ended, anchored to bare-handed human = 1.0** — no ceiling (a tank out-scales a mammoth). | 0 | low | low | **high** | high |
| **`defense`** (`CombatStats`) | strength | toughness / how hard to take down. Same open-ended scale. Drives predator-vs-prey resolution and the kill↔wound split. | low | low | mid | **very high** | mid |
| **`ferocity`** (`SpeciesDef`, 0..1) | behaviour | P(it **fights back** when attacked, vs flees). Scales how much of its `attack` a *hunt* actually faces. Default 0. | 0 | low | **mid** | **high** | high |
| **`aggression`** (`SpeciesDef`, 0..1) | behaviour | P(it **attacks you unprovoked**). Gates the predator-*raid* trigger. Default 0. | 0 | 0 | ~0 | **~0** | **high** |
| **`diet`** (`SpeciesDef`) | — | *what it eats* — `herbivore` (grazes) vs `carnivore` (eats prey biomass). The only knob that changes the food/carrying-capacity layer. | herb | herb | herb | herb | **carn** |

**Strength ≠ danger — this is the correction that shapes everything.** A big `attack` does *not* make
a thing dangerous *to you*; a mammoth is deadly to hunt but will never come for your camp, and a tank
is not "as deadly as a mammoth." Danger is **behaviour-weighted and derived, never stored** — and
there are honestly *two* of it:

- **Hunt-danger** ≈ `attack × ferocity` — how costly it is to go after it. Mammoth: max (huge attack,
  high ferocity). Rabbit: 0 (it flees). A cornered boar: real but modest.
- **Camp-threat** ≈ `attack × aggression` — how much it menaces you unprovoked. Wolf pack: high.
  **Mammoth: ~0** (it never initiates). This is what a threat *map* should paint — a mammoth must not
  light up your danger overlay.

Consequences that fall out, rather than being coded as special cases:

- **"Does it get eaten" is not a flag.** An animal is prey to a given predator iff that predator's
  `attack` clears its `defense`. Wolves can't crack a mammoth's defense, so a mammoth is simply not
  in the wolf's prey set — idea 7, for free.
- **A dangerous hunt is not a predator.** A mammoth is `(herbivore, high attack, high ferocity, ~0
  aggression)`: deadly to hunt, no camp-threat. Same casualty path as a predator, different behaviour.
- **"Runs vs fights back" is `ferocity`; "ignores you vs hunts you" is `aggression`** — two distinct
  probabilities, not one. A deer with a small `attack` still flees (`ferocity ≈ 0`), so it costs you
  nothing to hunt; a boar with the same `attack` *fights* (`ferocity` mid), so it does.
- **Pack vs solitary is config** — a wolf *pack* and a solitary big cat differ only in group
  `biomass` / `body_mass`, not in entity type.

This mirrors the moves the codebase already made — forage transposed from herds, cultivation from
husbandry. Predators are the trophic transpose of the grazer: an herbivore's carrying capacity
comes from `graze_sustainable_flow / fodder_per_biomass`; a carnivore's comes from
`prey_sustainable_flow / prey_per_biomass`. **Same seam
(`fauna::ecological_carrying_capacity`), different food layer.**

## What already exists (reused wholesale)

Grounding, so the phases below are edits to real seams, not green field:

- **Herds** — `core_sim/src/fauna.rs`, `Herd` in the rollback-persisted `HerdRegistry` (a flat
  `Vec<Herd>`, *not* ECS). Carries `biomass`, `carrying_capacity` (recomputed each turn),
  `body_mass`, per-species `regrowth_rate r`, `hunt_credit`, `ecology_phase`.
- **Logistic ecology** — `logistic_regrowth`, `net_biomass_delta` (with Allee/depensation
  collapse below `collapse_fraction·K`), MSY at `K/2`, constant-escapement harvest, extinction at
  `extinction_floor·K`. All in `fauna.rs`.
- **Grazing draw-down** — `advance_herd_grazing` already draws a *consumable layer* (the graze
  capacity of tiles in range) down as herbivores eat. **Predation is the same shape**, with prey
  herds as the layer.
- **Movement dispatch** — `advance_herds` reads a config-driven `RungMovement` primitive
  (`fixed | roam | drift_to_owner`); a new **`pursue`** primitive slots in. Greedy one-hex descent
  (`best_land_neighbor_toward`, `acceptable_steps`), no A*. Deterministic per-herd/per-turn
  `SmallRng` (hasher-independent tie-breaks are load-bearing for rollback — see `drift_order`,
  `GrazeRegistry::richest_patch`).
- **Prey-location data already published** — `HerdRegistry` positions + `HerdDensityMap` raster.
  A predator steering toward prey reads these; this is *also* the deferred "Grazing 2c" dynamic
  (move toward live food, not just fertile land) that no consumer has needed until now.
- **Hunting & Eradicate** — `LaborTarget::Hunt { fauna_id, policy }`; leash = `band_work_range +
  hunt_leash_tiles`. **Eradicate** already does "take the whole standing stock in one resolution,
  bypass the kill-credit bank, deliver no food" (`hunt_policy_rate` / `hunt_credit_ceiling`,
  `fauna.rs`). This is the player-kills-a-predator verb, verbatim — a predator is just a herd you
  are allowed to Hunt.
- **Warrior** — `LaborTarget::Warrior` is fully staffed and plumbed through
  command/snapshot/cancel but **literally inert** (`systems/labor.rs` resolution arm is an empty
  branch with the comment "the predator slice consumes Warrior strength"). This arc is that
  consumer.
- **Snapshot** — `HerdTelemetryState.sizeClass` / `huntable` and the `policy` / `species` fields
  are **free-form strings**, so a `carnivore` diet, a predator species, and any new policy need
  **no `.fbs` change**. (Consistent with the *no-back-compat-yet* rule — no old-snapshot
  fallbacks.)

## What is genuinely new

1. **`diet` + prey-derived carrying capacity.** A `Diet` enum on `SpeciesDef`; for carnivores,
   `ecological_carrying_capacity` sums *prey biomass flow in range* instead of graze flow.
2. **Predation draw-down** (`advance_predation`, new). Each turn every carnivore herd eats an
   **abstracted biomass fraction** from prey herds in range whose `defense` its `attack` clears.
   Mirrors `advance_herd_grazing`. Continuous draw — whole-animal quantization is reserved for the
   *player's* hunt, not the wolf's dinner (decision below).
3. **The rating web — strength + behaviour, kept apart.** `diet` on `SpeciesDef`; the shared
   **`CombatStats { attack, defense, range }`** (strength, open-ended); and the two behaviour
   probabilities **`ferocity`** (fights back) + **`aggression`** (initiates), 0..1 on `SpeciesDef`.
   Plus the resolution helpers (who can eat whom; the two *derived* dangers — hunt-danger
   `attack × ferocity`, camp-threat `attack × aggression`; casualty math). **No stored `danger`
   field.** Predation reads the *same* intrinsic `attack` combat does.
4. **A combat subsystem + a creatures roster + band casualties** — a new first-class
   `core_sim/src/combat/` module exposing `resolve_fight(payload) -> outcome` (placeholder resolver
   now, real one later; the *seam* is the deliverable), plus a 1-row `creatures` roster for the base
   human `CombatStats`. Casualties are a net-new mortality path (today only starvation / cold /
   elder exist) applied at the `death_fraction` seam in `core_sim/src/systems/population.rs`; the
   hunt/predator code is a thin adapter that composes `intrinsic ⊕ equipment` contingents, builds a
   fight, and applies `killed`/`wounded`.
5. **Shared prey-seeking movement primitive** — `relocate_toward_resource`: candidate tiles
   (hex disk, or a new ring iterator in `grid_utils.rs`) → score by resource presence (prey
   density for predators) → greedy step with a total, hasher-independent tie-break. Predator is
   consumer #1; scouting expeditions (which today stop at `AwaitingOrders` and wait for waypoints)
   adopt it later.

## Ecology: predator–prey dynamics without a runaway (idea 5)

No artificial "self-restraint" rule — restraint is emergent, consistent with the repo's
*emergent-not-quota* principle:

- A carnivore's **carrying capacity is prey-limited** — `K_pred` derives from prey biomass in
  range. Thick game → high K → the pack grows logistically toward it. Thin game → K falls → the
  pack declines and, past `extinction_floor·K`, **despawns** (idea 6: no game, they leave/die).
- **Functional response damps the crash.** Predation take scales with prey density, so as prey
  thins the pack takes less per turn and stops before zero — the discrete analogue of a
  Lotka–Volterra oscillation, riding the depensation machinery already in `net_biomass_delta`.
- The player's **Eradicate** on a predator herd is an additional control valve (idea 2), and
  predators competing with the player for the same deer (idea 1) is the whole point — the game you
  want is also feeding wolves.

Guard rails are tuning dials, not new mechanics: a modest predator `regrowth_rate`, a `prey_per_biomass`
conversion (the carnivore analogue of `fodder_per_biomass`), and a predation-rate ceiling.

## Casualties & danger (ideas 3, 4) — a combat subsystem, not a formula

**A predator encounter is nothing special — it is a *fight*, resolved by the same combat system
that will one day resolve TOE-vs-TOE, rival-civ raids, and every other battle.** So the casualty
math does **not** live in the hunt path as a bespoke formula. It lives in a new, first-class
**combat subsystem** behind one stable seam, and the hunt/predator code is a thin adapter that
builds a fight and reads the outcome. This is a DRY/SOLID call: combat is its own module with no
knowledge of fauna, bands, or labor; callers construct payloads and consume outcomes. New combatant
kinds (barbarians, armies, mechs) add *adapters*, never edits to combat.

### The contract — describe the forces, do NOT pre-compute their power

The load-bearing rule: **the caller describes *who is present* as a composition of units; combat
does all the aggregation, range-phasing, and attrition.** A caller must never hand combat a scalar
"this side has power N" — a single number cannot survive TOE (artillery is lethal at range and
near-useless in melee; 5 archers + 5 spearmen is two behaviours, not one total). Collapsing that to
a scalar hard-codes "range doesn't matter" into the *caller* and steals combat's actual job. So a
`Force` carries **contingents**, each a block of like units with a per-unit combat profile.

```rust
// core_sim/src/combat/  — a NEW subsystem module, not attached to fauna.rs / labor.rs.
// resolve_fight is a PURE function (deterministic, rollback-safe); encounter *detection*
// stays in the fauna/labor systems, which build the payload and apply the outcome.

pub fn resolve_fight(payload: FightPayload) -> FightOutcome;

pub struct FightPayload {
    pub sides:   Vec<Force>,             // ≥2 (today exactly 2); combat is agnostic to what they are
    pub terrain: Vec<TerrainContext>,    // hexes in play — structured, identity modifier for now
    pub seed:    u64,                    // caller-supplied, hasher-independent → rollback-stable
}

pub struct Force {
    pub id:          ForceId,            // maps the side back to its band / herd / faction
    pub posture:     Posture,            // Aggressor | Defender | Ambushed …
    pub contingents: Vec<Contingent>,    // the COMPOSITION — never a scalar
}

/// A block of like units fighting the same way. Humans: a squad with one loadout
/// ("5 × spear+shield"). Animals: the herd's fighting stock. Combat reads these; it is
/// never told an aggregate "power".
pub struct Contingent {
    pub kind:    ContingentId,           // maps casualties back (species, or role+equipment)
    pub count:   f32,                    // operators present — the whole-unit attrition quantum
    pub profile: CombatProfile,          // per-UNIT stats, supplied by the domain adapter
}

/// Combat's OWN neutral per-unit stat type. Domains adapt INTO it — fauna from
/// SpeciesDef.attack/defense, TOE from the equipment table — so combat depends on
/// neither. This is per-unit-type DATA, not an aggregate outcome-power.
pub struct CombatProfile {
    pub attack:  f32,
    pub defense: f32,
    pub range:   RangeBand,              // Melee | Ranged (artillery = Ranged, folds up close)
    // grows here — armor-vs-pierce, mobility, morale — resolver-internal, no caller change
}

pub enum RangeBand { Melee, Ranged }

pub struct FightOutcome {
    pub results:    Vec<ContingentResult>,  // per contingent, keyed (ForceId, ContingentId)
    pub victor:     Option<ForceId>,
    pub disengaged: bool,                   // loser withdrew (yield forfeited) vs annihilated
}

pub struct ContingentResult {
    pub force: ForceId, pub kind: ContingentId,
    pub killed:  f32,   // permanent — removed at the death_fraction seam
    pub wounded: f32,   // recoverable — transient capacity loss, returns to the pool
}
```

Why this is the right seam:
- **Per-unit `CombatProfile` is describing units, not computing outcomes.** Giving combat
  `{attack, defense, range}` per unit-type is data; giving it "side = 47" is the outcome. The first
  is required; the second is banned. Combat still decides who strikes in the ranged phase, how
  counts attrit, and who breaks.
- **Domains adapt into combat's neutral types** (dependency inversion, one direction:
  fauna/labor → combat). Fauna maps `SpeciesDef.attack/defense` → `CombatProfile`; the TOE/labor
  system maps a Warrior squad's equipment → one or more contingents. Combat imports neither.
- **TOE slots in with no contract change.** Equipment-vs-operators *is* the `Contingent`: `count`
  = the people, `profile` = what they fight with (bare hands today is just a low-stat `Melee`
  profile). When TOE lands, the labor system emits the breakdown ("10 warriors: 5 spear, 5 bow")
  as multiple contingents — the caller reports *composition*, combat resolves it. Range-phasing
  ("archers back, spearmen close") and training (a spearman firing a bow badly, folded into the
  emitted profile) are resolver-internal / adapter-internal upgrades. Crew ratios (a ballista needs
  2 operators) are *noted, not built* — if ever needed, `Contingent` gains an operators/equipment
  split.

### Where the stats live — a wolf and a human are the same combatant

The `CombatProfile` a contingent hands to combat is **composed**, never a special-cased blob. The
unifying model:

> **A combatant = an intrinsic creature ⊕ an equipment loadout.** `CombatProfile = intrinsic
> CombatStats ⊕ equipment modifiers`.

- wolf = creature(wolf) ⊕ nothing
- bare human = creature(human) ⊕ nothing
- spearman = creature(human) ⊕ [spear, shield]
- war elephant = creature(elephant) ⊕ [howdah, armor, crew]

An armored elephant is **structurally identical** to a human with a shield — a creature plus
equipment. So the storage split is *intrinsic vs. equipment*, **not** *animal vs. human*:

1. **One shared value type — `CombatStats { attack, defense, range }`** (combat's neutral stat
   struct). The *same* struct describes a wolf's body and a human's body. This is the DRY core.
2. **Intrinsic stats live with the creature, in its own domain:**
   - **Animals → `SpeciesDef`** (embed `CombatStats`). The wolf's `attack` there is the **same**
     number `advance_predation` reads for "who can it eat" — intrinsic combat stats and predation
     stats are one thing; splitting them would be the real duplication.
   - **Humans (and future non-fauna units) → a small `creatures` roster** holding the same
     `CombatStats`. The "person" base lives here — **not** `fauna_config.json` (a human is not
     wildlife to spawn/graze/hunt) and **not** `combat_config.json` (that file is resolver *tuning*
     — severity constant, attrition curve — not creature identity).
3. **Equipment stats → a separate equipment/TOE table** (spear, shield, sling, armor, howdah),
   wielded by any unit with operators. Armor on an elephant reads the *same* table as a shield on a
   human — that is the consistency guarantee.
4. **Combat owns the algorithm and the neutral types, not the stat data.** Adapters compose
   `intrinsic ⊕ equipment → CombatProfile` and hand it over; predation and combat are both mere
   *consumers* of the creature's intrinsic `attack`. `range` is intrinsic for animals (a wolf is
   Melee) and usually set by equipment for humans (a bow makes them Ranged) — the composition
   handles both.

A future war-mount falls out for free: a tamed elephant/horse is already a `SpeciesDef` creature
(its intrinsic stats exist), and it becomes a unit by adding equipment — no special path. Likewise
a wolf→dog war unit reuses its `SpeciesDef` stats ⊕ a harness.

### Death vs. wounded — modelled in the outcome from day one

Casualties are **not** binary dead. Every fight returns `killed` (permanent — removed at the
`death_fraction` seam) and `wounded` (survives, returns to the pool at reduced capacity while
recovering). This is deliberate design, not bookkeeping:
- Low-level predator harassment becomes **attritional and recoverable**, not a binary death-spiral —
  the right pacing for turn-1 threats.
- **Warriors and equipment shift the kill↔wound split, not just the total.** Bare-handed against a
  wolf → mostly killed; spears + a shield wall → mostly wounded, few dead. That is the legible,
  satisfying reason to equip, and it is exactly "a human never beats a wolf bare-handed" — bare
  hands don't lose the *count*, they lose the *severity*.

Phase 0 **applies `killed`** (real pop removal) and starts `wounded` as the lightest possible
effect (a short recovery-capacity dip) — or carries-and-defers its application if injury state is
too much for the first slice. Either way the field is in the contract now, so the real recovery
model is additive.

### The placeholder resolver (swap later, keep the contract)

`resolve_fight`'s **internals are a deliberate placeholder** that nonetheless consumes the *real*
shapes: per-contingent count × attack vs opposing defense attrition, seeded RNG for variance,
splitting each side's casualties into `killed`/`wounded` by a severity factor (better
Warrior/equipment profiles push the split toward wounded). It **ignores `range` and `terrain`** for
now. When the real combat system lands — ranged pre-phase then melee, terrain cover, morale/break —
only the function body changes; **every caller and the payload/outcome contract stay put.** That is
the whole point of building the seam first.

### Two triggers, one seam — but different combatants

Both triggers call `resolve_fight`; what differs is **who stands on the band's side**, and that
distinction is load-bearing:

- **Player-initiated (the hunt):** a band Hunts a herd with `attack` > 0 (mammoth, ox, or any
  predator) → the band-side contingents are **the hunters on that herd**, and their safety is *their
  own* — answered by **equipping them (TOE)**, never by a guard. A hunting party that goes after a
  mammoth defends itself with spears; you do not dispatch the camp's border patrol to escort it, and
  there is no "assign Warriors to a hunt" affordance because the concept doesn't fit. As TOE lands,
  the hunters' `CombatProfile` improves (bare hands → spears) and the same hunt comes home whole — no
  seam change, that is the whole point of intrinsic ⊕ equipment.
  - **Both the resident-band hunt and the detached hunting *expedition* resolve here** — same seam,
    same person-vs-beast contingents. The **expedition is bloodier**: far from home, no relief, tired
    and hungry. Phase 0 models that crudely with a flat `expedition_danger_multiplier` (scales the
    fight's lethality). The principled version — a **combat-modifiers layer** (proximity-to-home,
    fatigue, supply, terrain) that tilts a fight toward one side, and which is *also* how a local hunt
    earns its **home-advantage discount** — is deferred (see below); the flat multiplier is a stopgap
    it supersedes.
- **Predator-initiated (the raid):** a carnivore herd with `aggression` > 0 in range of a band raids
  the band / its unguarded foragers → **this** is what **Warriors** defend against, the band-wide
  guard doing its actual job (band as Defender). It requires a carnivore that initiates, which does
  not exist until Phase 1 — so **Warrior's first live consumer is Phase 1, not the hunt.**

> **Warriors never enter a hunt.** Warrior is a band-wide standing posture (border/camp patrol);
> hunt danger is the hunting party's own, mitigated by *its* equipment. Folding the band's Warrior
> head-count into a hunt would let a border-patrol assignment silently make a mammoth hunt safer —
> the wrong model. The hunt's only levers are the hunting party: its numbers, and (via TOE) its gear.

Outcome application: `killed` people are removed from the band cohort's working-age bracket;
`wounded` are computed and surfaced but **mechanically inert** in the first slices (recovery is its
own later slice); forfeited yield is the take you lose when driven off; the event narrates in the
command feed. Applies to **any** dangerous encounter — mammoth danger and wolf danger are the same
code path, differing only in the contingents' profiles.

## Staging — each phase independently testable

Order matches the approved staging; every phase ships something a tester can exercise, however
basic.

- **Phase 0 — The combat seam + ratings + dangerous-hunt casualties.**
  Stand up the `core_sim/src/combat/` subsystem: the composition contract (`FightPayload` / `Force`
  / `Contingent` / `CombatProfile` / `FightOutcome` / `ContingentResult`, shaped for TOE + range +
  death/wound) and `resolve_fight` with a **placeholder** per-contingent attrition resolver (own
  module, DRY/SOLID, no fauna/labor knowledge). Introduce the shared **`CombatStats { attack,
  defense, range }`** value type; embed it in `SpeciesDef` alongside `aggression` + `diet` (defaults
  keep every existing species byte-identical: `attack` 0, `aggression` 0, `defense` low, `range`
  Melee, `diet` herbivore); add a **1-row `creatures` roster** holding the base human `CombatStats`.
  Wire the **existing** Hunt path as a thin adapter — hunting a high-`attack` animal (start with
  mammoth/ox — *no predator species yet*) builds a fight whose **band side is the hunters on that
  herd** (the person profile), resolves it, and applies `killed` (working-age bracket) + inert
  `wounded`, narrated on a `hunt_danger` command-feed line (no `.fbs` change). **Warrior is NOT
  wired in** — a hunt's danger is the hunting party's own; its mitigation is the hunters' equipment,
  which arrives with **TOE** (see `docs/plan_early_game_labor.md` → Equipment/TOE). **No equipment
  table yet** — the hunters' loadout composes to identity (bare hands), so Phase 0 *lands the
  casualties*; the equip-to-survive payoff is a TOE-arc consequence, not this slice. Warrior stays
  inert until Phase 1's raid path.
  **Testable:** hunt a mammoth (attack 8) → a `hunt_danger` feed line + working-age population drops;
  hunt a deer (attack 0) → nobody dies. `resolve_fight` gets its first caller.

- **Phase 1 — Carnivore herds (diet + prey-limited K + predation draw-down) + the raid trigger.**
  Add the `Diet` enum; make `ecological_carrying_capacity` sum prey flow for carnivores; add
  `advance_predation` (abstracted biomass draw from prey herds the predator's `attack` clears).
  Seed one predator species (wolf pack) in `fauna_config.json`. Movement can stay on the existing
  `roam` primitive for this phase (predators wander like any herd) — dynamic pursuit is Phase 2.
  **This is where Warrior goes live:** a carnivore with `aggression` > 0 in range of a band raids
  it (band as **Defender**), and the band-side contingent is its **Warriors** — the second
  `resolve_fight` trigger, and the Warrior role's first real consumer.
  **Testable:** spawn wolves near deer; watch deer biomass drawn down, wolf biomass track prey
  (grow when fed, decline and despawn when the deer are gone); a wolf pack near an under-guarded
  band costs it people, and staffing Warriors cuts the losses. Predator–prey oscillation visible in
  telemetry.

- **Phase 2 — Shared prey-seeking movement.**
  Extract `relocate_toward_resource` (+ a `pursue` `RungMovement` primitive) scoring candidate
  tiles by prey density (`HerdDensityMap`). Predators now actively follow the game and relocate
  when local prey thins (ideas 6, 8). Deterministic tie-breaks preserved.
  **Testable:** move a deer herd across the map; the wolf pack tracks it rather than idling on
  empty ground.

- **Phase 3 — Client legibility.**
  Threat/casualty events in the command feed; a predator presence overlay (predators are huntable
  herds already carried in telemetry); Warrior strength & danger readout on the band panel; the
  income/loss line showing yield forfeited to raids. Consumes only free-form snapshot fields.
  **Testable:** the player can *see* a predator, read the danger of a hunt, and watch warriors
  change the outcome — the loop is legible (the *ui_preview* harness verifies the HUD).

**Deferred (own slices, noted so the interface is built to accept them):**
- **Combat-modifiers layer.** A set of situational factors that tilt a `resolve_fight` toward one
  side — **proximity to home** (a local hunt is safer near camp; an expedition, far away, is
  deadlier), **fatigue**, **supply/provisions**, **terrain cover**, later **morale**. Fed into the
  fight (a modifier on the side's effective strength or the fight's lethality), it **replaces the
  flat `expedition_danger_multiplier`** and gives the local hunt its home-advantage discount. This is
  where "distance/tiredness/hunger make a hunt more dangerous" becomes a real model rather than a
  constant. The combat contract already carries `TerrainContext` and `Posture` as hooks for it.
- **Predator domestication → dogs.** Wolf `(carnivore)` climbing a husbandry ladder to a tamed
  companion — historically the *first* domestication, and it fits the existing
  `husbandry_ceiling` grammar. A dog is then a Warrior/Scout *multiplier* rather than a food herd —
  a new consumer of the tamed-animal seam. See *The Intensification Ladder*
  (`docs/plan_intensification_ladder.md`) for the rung engine it would extend.
- **Scouting expeditions adopt `relocate_toward_resource`** to auto-explore toward unrevealed
  value instead of stopping at `AwaitingOrders` (`docs/plan_exploration_and_sites.md`).
- **Richer threats** — barbarians / rival civs reuse the same casualty interface (already deferred
  in M1-threats).

## Decisions & rationale

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Predators are `Herd`s, no parallel entity** | They *are* animals; behaviour overlaps grazers because prey behave like grazers. Reuses ecology, movement, quantization, snapshot, determinism discipline for free. |
| 2 | **"Predator" is emergent from 4 config knobs**, not a category | `diet` / `attack` / `aggression` / `defense` span the space; mammoth-danger and wolf-danger, pack vs solitary, runs-vs-fights all fall out as config corners. |
| 3 | **`defense` does double duty** (resist predation *and* endanger hunters) | One rating ties the trophic web and the mammoth-danger case together; no separate `danger` field. |
| 4 | **"Gets eaten" derives from attack ≥ defense**, no `is_prey` flag | Idea 7 (wolves can't take mammoth) is a comparison, not a table. |
| 5 | **Predation draws prey biomass down abstractly (continuous)** | Smooth predator–prey dynamics; whole-animal quantization stays reserved for the *player's* hunt (the lumpy-windfall feel is a player-facing mechanic, not a wolf's). |
| 6 | **Predator carrying capacity is prey-limited; restraint is emergent** | No gamey "don't overkill" rule; prey-limited K + functional response + depensation give the oscillation and the die-out. Matches *emergent-not-quota*. |
| 7 | **Casualties resolve through a first-class combat subsystem** (`resolve_fight`), never a bespoke hunt formula | A predator encounter is just a *fight* — same resolver as future TOE/rival-civ battles. Combat is its own module with no fauna/labor knowledge; callers build payloads (DRY/SOLID, open/closed). Placeholder resolver now; the *seam* is the deliverable, so the real model drops in without touching callers. |
| 7a | **Callers describe composition, never an aggregate power scalar** | A scalar can't survive TOE (artillery lethal ranged / useless in melee; archers + spearmen ≠ one total). `Force` carries `Contingent`s with per-unit `CombatProfile`s; combat owns aggregation, range-phasing, attrition. |
| 7b | **A combatant = intrinsic creature ⊕ equipment; one shared `CombatStats`** | Wolf and human are the same combatant; an armored elephant = creature + equipment, identical to human + shield. Intrinsic stats live with the creature (animals → `SpeciesDef`, humans → a `creatures` roster) — the wolf's `attack` is the same one predation reads; equipment lives in its own table; combat composes. Split is *intrinsic vs equipment*, not *animal vs human*. |
| 7c | **Death vs wounded modelled from day one** | `killed` (permanent) + `wounded` (recoverable capacity dip) per contingent; the hunting party's *equipment* (and, secondarily, its numbers) shifts the kill↔wound *split*, not just the count — the legible reason to equip a hunt, and why bare hands lose to a mammoth on severity. |
| 7d | **Warriors do NOT escort a hunt; equipment does** | Warrior is a band-wide standing guard (border/camp patrol); a hunt's danger is the hunting party's own, answered by equipping the hunters (TOE). Folding band-wide Warriors into a hunt would let a border-patrol assignment silently make a mammoth hunt safer. So Phase 0's hunt path reads only the hunters on that herd; **Warrior's first live consumer is the Phase 1 predator-*raid* trigger** (band as Defender), not the hunt. |
| 8 | **Build the ecological version, not abstract M1 pressure** | The herd stack makes real predators barely more work than a fake; the *interface* (Phase 0) is identical, so nothing is wasted and nothing is retrofitted. |
| 9 | **Movement primitive shared from the start** (Phase 2 extract) | The user requires scouting to reuse it; designing `relocate_toward_resource` as shared avoids a rewrite. |
| 10 | **Defaults keep every existing species byte-identical** | `attack`/`aggression` default 0, `defense` low, `diet` herbivore — no-back-compat rule; the roster is unchanged until a species opts in. |
| 11 | **Combat-strength magnitude is open-ended, anchored to *bare-handed human = 1.0*** | `attack`/`defense` are relative — only ratios carry meaning (mammoth 8 vs human 1 = 8:1), like the graze table. No ceiling; `f32` scales as far as history does (bronze, iron, tanks). The single discipline is the shared anchor so the whole future roster — animals, equipped humans, mounts — stays comparable on one scale. |
| 12 | **Strength and behaviour are separate; "danger" is DERIVED, not a stored `attack`** | *(Corrected — the first cut stored `danger = attack`, which is wrong: strength ≠ danger.)* Four fields — `attack`/`defense` (strength, open-ended) + `ferocity`/`aggression` (behaviour, 0..1). There is **no stored `danger` field**; two dangers are derived: **hunt-danger** `≈ attack × ferocity` and **camp-threat** `≈ attack × aggression`. A mammoth is max hunt-danger, ~0 camp-threat. `ferocity` (fights back vs flees) is its own axis, distinct from `attack`. |
| 13 | **Danger is DISPLAYED as an open/relative scale, never fixed words** | *(Corrected — the first cut bucketed into Harmless/Minor/Dangerous/Deadly, which can't survive the roster: a mammoth and mechanised infantry can't both be "Deadly".)* Like the Elevation view: the panel shows the raw **components as relative bars** (`attack`/`defense` normalised to the map's range, `ferocity`/`aggression` as native 0..1), no danger word; the map overlay paints **camp-threat**, relative-normalised so it auto-rebases as stronger units arrive (no ceiling to guess). |

## Open tuning dials (ship, measure, retune — playtest)

- `attack` / `defense` / `aggression` scales and the casualty formula's `engagement_scale` and
  `warrior_strength` curve.
- `prey_per_biomass` (carnivore food conversion) and predator `regrowth_rate`.
- Predation-rate ceiling / functional-response shape (how fast a pack can crash a herd).
- Predator spawn abundance & `host_biomes` (predators seat where their prey seats).
- Raid frequency / unguarded penalty (how punishing turn-1 predator pressure is).

## See Also

- `core_sim/CLAUDE.md` — the husbandry yield ladder, herd ecology seams, movement dispatch.
- `docs/plan_wildlife_hunting_overlay.md` — the hunting overlay & harvest policies predators reuse.
- `docs/plan_early_game_labor.md` — the Warrior role & TOE equipment tiers this arc activates.
- `docs/plan_intensification_ladder.md` — the rung engine a future wolf→dog domestication extends.
- `docs/plan_exploration_and_sites.md` — scouting expeditions, the second consumer of the shared
  movement primitive.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §Wildlife & Hunting — the player-facing
  narrative to extend once this is approved.
