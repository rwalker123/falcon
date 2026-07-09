# Plan: Early-Game Labor — the Band as a Labor Pool (Milestone 1)

Status: **Design approved, not yet implemented.** This is the authoritative spec for the
playable early game: a single small band, modeled as a **labor pool** whose working-age
population is partitioned across equipment-gated **roles** (a *Table of Equipment* / TOE
model), feeding and defending itself until population growth pushes it toward its first
place-bound structures. It **brings forward and concretizes Phase 2** of
`docs/plan_settlement_population.md` (labor pool + hybrid allocation), grounding it in the
first few turns, and it adds two concepts that arc did not have: **equipment (TOE)** and a
**carry-capacity population cap**.

## Motivation — why the current start is broken

Playtesting the Wildlife & Hunting arc exposed that the shipped opening is incoherent. A band
holds **exactly one task** (`reassign_band` — "orders replace orders"), and the start spawns
**four bands of ~900 people** (hardcoded in `spawn_profile_population`), each locked to a single
activity. But run the subsistence math to its conclusion:

> **A single food source, harvested sustainably, feeds about 10 people.** A deer herd's peak
> sustainable yield is ~0.3 food/turn (≈15 biomass regrowth × `provisions_per_biomass` 0.02),
> and per-capita draw is `consumption.per_capita_draw` 0.03 → 0.3 ÷ 0.03 ≈ 10 weighted mouths.
> (Integer rounding then drops that 0.3 to **0** — the literal starvation bug reported as
> "Issue 2".)

So a 900-person band on one herd is trying to feed 900 mouths from a source that tops out at
~10 — off by ~90×. **This is not a Sustain tuning bug.** It is that "one band = one task" was
always a placeholder that works for a 30-person scout party and collapses the instant the band
represents a real population. The fix is the model the game already wants: **the band is a
labor pool that draws subsistence from many sources at once.**

## The core realization

Every one of our design levers turns out to be an answer to the same question — *how* does a
population draw from many sources simultaneously — and they collapse into **one uniform rule**:

> Each role a band can perform has **two throughput tiers** — *unequipped* (bare hands, always
> available, low) and *equipped* (with the role's TOE gear, higher). Equipment is **consumable
> inventory** the band starts stocked with and cannot yet replace. You **allocate working-age
> labor** across roles; throughput is a function of `(workers assigned, equipment on hand)`.

That single mechanic covers foraging with/without baskets, hunting with/without spears, and
fighting with/without weapons. Build it once.

## The model

### The band is a labor pool
The allocatable labor **supply** is the **working-age** bracket only (children and elders are
dependents that eat but do not work — the existing `demographics_config.json` brackets). Of a
~30-person band (~55% working) that is ~16 workers to split across roles. Labor is the scarce
currency; *how you divide those ~16 people is the core turn-to-turn decision.*

### Roles (the labor demands)
Milestone 1 ships four roles. Each is a demand the player staffs from the labor pool:

| Role | Produces | Equipped by (TOE) | Unequipped tier |
|------|----------|-------------------|-----------------|
| **Foraging** | Food (baseline) | Baskets/containers → higher yield **and** carry capacity | Bare hands — much lower yield |
| **Hunting** | Food (draws down herds) | Spears/traps → higher take | Bare hands — weak take |
| **Scouting** | Vision / exploration | Wayfinding kit → range/speed | Bare hands — short range |
| **Warrior** | Defense (no food) | Weapons → combat strength | Rocks & fists — weak force |

Foraging and Hunting feed the band; Scouting and Warrior produce **no food**, so every worker
on them is a worker *not* feeding the band. **That opportunity cost is the whole game of the
opening:** put everyone on food and you're blind and undefended; scout/guard too much and you
starve.

### Equipment / TOE — consumable, start-stocked, not yet craftable
A TOE is the equipment set that lifts a role from its *unequipped* to its *equipped* tier.

- The band **starts kitted** for all four roles (starter stock in inventory).
- Equipment is **consumable**: it wears down with use (durability). **Performance is flat until
  expiry, then the role drops to the unequipped tier** — durability and performance are kept
  **orthogonal axes** (the future crafting system tunes them independently; coupling them now
  would be a modeling mistake).
- There is **no way to replace equipment in M1.** Running your kit dry — and feeling the drop to
  bare hands — *is* the pull into the Milestone 2 crafting economy (the Crafter role that
  produces TOEs). Equipment depletion is effectively the **pacing dial of the first act.**
- Equipment effects are **role-specific**, not a flat global multiplier: baskets raise forage
  yield *and* carry capacity; spears raise hunt take; weapons raise combat strength.

Because equipment is modeled as real depleting inventory from day one (just without a producer),
adding the crafting/production side later is additive — **least rework**.

### Carry capacity is the population cap
Growth is **not** capped by food production — it is capped by **carry capacity**, the band's
mobile food-storage limit:

> The band can only haul a food buffer sufficient for **N** people. Population is capped at N
> **regardless of how much food it can produce.** A band swimming in game can still be stuck at
> 20 because it cannot carry provisions for a 21st mouth on its back.

Two distinct constraints therefore govern the band:
1. **Survival** — food income ≥ consumption, or the band starves and shrinks (production).
2. **Growth cap** — population ≤ carry capacity; births stop at the cap (storage).

You plateau at whichever binds first, and for a nomad **carry capacity usually binds.** Raising
it is the only way to grow past the ceiling, via:
- **more containers/baskets** — equipment, but limited by the depleting kit; or
- **building place-bound storage** (drying racks → granary) — which is exactly the sunk-cost
  **tether that *is* sedentarization**.

So the nomad→settle transition becomes **mechanical, not scripted**: mobile carry is inherently
small, and the only way past your ceiling is to stop moving and build. That is the
emergent-settlement philosophy (`plan_settlement_population.md`) expressed as a hard number the
player runs into — and it feeds the existing `SedentarizationScore` readout.

### Growth self-limits (no forced gate)
Nothing *forces* the player to settle. Births run (per `advance_demographics`) until population
reaches the carry cap, then plateau. The plateau is set by `min(food-supportable, carry-cap)`,
and food-supportable is itself the **sum of sustainable yields of reachable herds + forage** —
so **where the band wanders determines its natural ceiling** (a game-rich valley supports a
bigger band than thin scrub). This makes the **already-shipped wildlife/forage density overlay a
strategic instrument for reading carrying capacity**, not just flavor. Settling is a *player
ambition* choice — break your ceiling — never a gate that unlocks a button.

### Fractional food (hard prerequisite)
At this scale **every yield is fractional** — a lone hunter's sustainable take is ~0.3
food/turn, and today's integer rounding silently zeroes it. **Food income and the larder must
accumulate fractionally.** This is step zero; nothing else in M1 works without it.

### The food ledger (the instrument)
Because growth self-limits at "income = consumption" against a carry cap, the player **must** be
able to see *why* they've plateaued. A per-band **food ledger** surfaces the flows:
`+forage, +hunt, +network transfers, −consumption, −(spoilage later) = net/turn → days to
empty`, plus current population vs carry cap. Most of these quantities are already computed;
this is primarily a surfacing job, and it is **load-bearing, not cosmetic** — it is the readout
that makes the entire equilibrium-and-settle loop legible (and would have let the tester
self-diagnose Issue 2).

### Minimal predator threat (so Warrior is live from turn 1)
Threats exist **from turn 1** — a role with nothing to fight gets designed blind. A **minimal
predator** pressures the band and its unguarded foragers/hunters, resolved against Warrior
strength (equipped vs bare-handed) → casualties or yield loss. This validates the combat tier as
we build it. Threat **variety** (barbarians, rival civilizations) is deferred; the *interface*
between Warrior and threats is the thing that is cheaper to get right now than to retrofit.

## Decisions & rationale

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **One** band to start, small (**~30**) | The band is a labor pool from turn 1; skip the throwaway "one task per band" model entirely. Split/merge into more bands later via migration. |
| 2 | Band = labor pool; **working-age only** allocatable | Clean abstraction; pool size tunes via existing `maturation_rate`/`aging_rate`/`elder_mortality_rate` (age children in sooner / elders out later). |
| 3 | **TOE from turn 1**, three+ roles kitted | No awkward interim; same engine at 30 people and 30,000. Reconciles the manual's Scout/Hunter/Guardian *bands* into *roles*. |
| 4 | Equipment = **consumable inventory**, start-stocked, no M1 replacement | Least rework (consumable is the real model); depletion is the pull into M2 crafting and the first-act pacing dial. |
| 5 | Durability **cliff**, not performance decay | Durability and performance stay orthogonal (crafting tunes each independently); switching to gradual decay later is a one-function change. |
| 6 | Foraging **free baseline**; baskets **upscale** it | A fresh band with no hunters mustn't instantly starve; equipment is an upgrade, not a gate. |
| 7 | **Carry capacity** caps population | Makes the growth ceiling physical and makes storage/settling the concrete way past it — mechanical sedentarization. |
| 8 | Growth **self-limits** (no forced settle) | Matches "no found-settlement action"; settling is ambition, not a gate. |
| 9 | **Fractional food** | Non-negotiable at sub-1-per-source scale; the literal Issue-2 fix. |
| 10 | **Minimal predator threat in M1** | Warrior needs a consumer to be designed right; cheaper now than retrofitting combat onto an untested role. |
| 11 | **Spoilage deferred** | Carry-cap gives storage its purpose without spoilage; spoilage matters only once storage lets food *sit* (M2), combined with time-in-storage. |

## Starting state (all config-driven — no hardcoded literals)

Replace the hardcoded `900` in `spawn_profile_population` with a **config lever** (start
profile / demographics config). Target opening values (dials, to be tuned live):

- **1 band, ~30 people**, split by the existing `initial_distribution` (≈30/55/15).
- **Carry capacity with headroom** — cap ≳ starting population (e.g. start 30, cap ~40) so there
  is *visible room to grow* before the first plateau; otherwise the loop never demonstrates
  itself.
- **Kit duration ~15–20 turns**, matched to the existing `startup.food_reserve_days` (20), so the
  starting kit and the starting food run down on a comparable clock.
- Starter TOEs: **Foraging (baskets), Hunting (spears/traps), Scouting (wayfinding), Warrior
  (weapons)**.

## Relationship to the Settlement & Population arc

This doc **realizes Phase 2** of `docs/plan_settlement_population.md` (labor pool + hybrid
allocation) at the earliest scale, and **extends** it:
- The four roles are the first concrete **labor demands**; the arc's tending/construction/
  knowledge demands slot into the same allocator later.
- **Carry capacity** and **storage structures** are the bridge into the arc's Phase 3
  **improvement catalog** (storage-class improvements raise the cap; they are the first
  place-bound, decay-tethered structures).
- **TOE / equipment** is a **new concept** the arc did not have. Its natural home is a future
  **Crafter** role + crafting economy (M2); for now it is capability-with-consumable-kit.
- **Spoilage** remains the deferred modifier that makes storage *tiers* matter (arc Phase 3+).

## Milestone breakdown

- **M1 — the labor-pool opening (this doc).** Fractional food; band-as-labor-pool with
  working-age allocation across Foraging/Hunting/Scouting/Warrior; equipped/unequipped tiers +
  consumable starter TOEs (durability cliff); carry-capacity population cap; food ledger; single
  ~30-person start via config. **Split candidates:** M1a data+sim (labor allocation, tiers,
  fractional food, carry cap), M1b client (allocation UI + food ledger + carry-cap readout).
- **M1-threats — minimal predators.** Predator pressure + Warrior combat resolution
  (equipped/unequipped). Folded into M1 because it is cheaper to build the Warrior↔threat
  interface now than to retrofit it. (Kept as a distinct work-slice so M1 can land without it if
  scope demands.)
- **Deferred (M2+).** Crafter role + crafting to replenish/upgrade TOEs; larder spoilage +
  storage tiers; richer threats (barbarians, rival civs); the arc's Phase 3 improvement catalog.

## Open tuning dials (to settle live)

Starting population; carry-capacity headroom; per-role equipped/unequipped throughput; equipment
durability (turns-to-dry); forage-vs-hunt yield balance; predator frequency/strength; the
demographic knobs (maturation/aging) that size the labor pool. All config, per the no-magic-
numbers convention.

## See Also

- `docs/plan_settlement_population.md` — the arc this realizes/extends (Phase 2 labor; Phase 3
  improvements/storage; emergent settlements). Carry capacity + storage are the bridge.
- `docs/plan_wildlife_hunting_overlay.md` — the herds/Sustain yields this subsistence model draws
  from; the density overlay that reads carrying capacity.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — §"Start of Game — Nomadic Default"
  (starting tribe → one band + TOE roles) and §Wildlife & Hunting (Sustain = sustain the
  *resource*, not the band).
- `core_sim/CLAUDE.md` — Population & Demographics (brackets, `advance_demographics`, larder),
  Fauna (`FaunaPursuit`, `FollowPolicy`, yields), config-loader pattern.
