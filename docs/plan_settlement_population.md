# Plan: Settlement & Population Economy

Status: **Design approved, not yet implemented.** This is the authoritative spec for the
game's core early/mid economy: a demographic population model, labor allocation, and a
knowledge-gated improvement catalog — the system from which settlements *emerge*. It
supersedes the discrete `found_settlement` / Founders-unit model and the `Camp`-entity
backlog (`TASKS.md` §Nomadic Start Prototype), and it is the destination of the
`SedentarizationScore` seam built by the Wildlife & Hunting Overlay.

## Motivation

Today a settlement is founded by a discrete command that consumes a `Founders` unit — but no
shipped start profile seeds one, so `found_settlement` is unreachable, and `Settlement` /
`TownCenter` are threadbare (a fog anchor, not even snapshotted). `FoundCamp` spawns no entity
at all. Meanwhile the `SedentarizationScore` (manual §"Organic Settlement — Sedentarization",
lines 64–72) measures "pressure to root in place" but leads nowhere actionable.

The deeper truth, arrived at in design: **there is no "found a settlement" action.** A
settlement/town/city is the *emergent, derived label* for a cluster of populated tiles — the
accumulated result of a population that builds place-bound **improvements it must attend**,
until the cost of leaving (abandoning built improvements + stored surplus) exceeds the benefit
of moving. Sedentarization is emergent from that accumulated *tether*, not a gate that unlocks a
build button.

## Core idea: settlements emerge from population + improvements

Three interlocking systems, all data-driven:

1. A **demographic population** that grows/ages/dies and supplies **labor**.
2. A catalog of **improvements** — typed, place-bound structures that cost stockpiles + known
   skills to build, that **house** people or **yield** resources, and that **tether labor**
   (they decay if not tended).
3. A **derived settlement view**: wherever populated tiles cluster, that *is* a
   camp/settlement/town/city — named by size, never spawned.

**Guiding principle (applies everywhere): build the general mechanism once; scale it later via
config — no new architecture for the late game.** A lean-to and a 2000-person arcology are the
same improvement engine with different tuning. A 400k town and a 5M city are the same population
mechanism with a bigger local pool and higher-occupancy dwellings. Localize from day one so the
early game and the late game are the same code at different scale.

## The model

### Population — a localized demographic model
Population lives per **location**: a **populated tile** (housed by that tile's dwelling
improvements) or a **band** (a mobile `PopulationCohort` with a position). Both carry the same
**three age brackets**:

| Bracket | Role |
|---------|------|
| **Children** | Dependents — fed and housed, no labor |
| **Working-age** | The **labor pool** — the only bracket that produces |
| **Elders** | Dependents again (optional small knowledge/culture bonus), then mortality |

Per turn, **fractional flows** (no literal ages — this is a strategy sim):
- **Births** → Children, scaled by food surplus + housing headroom + morale.
- `maturation_rate`: Children → Working-age.
- `aging_rate`: Working-age → Elders.
- `mortality`: removes Elders; plus scarcity/crisis/cold deaths across all brackets.

The **dependency ratio** `(children + elders) / working` is the core tension: overgrow and you
starve on mouths that can't yet work; a baby boom is a drain for many turns before it's a labor
windfall. This **subsumes today's crude `simulate_population` growth clamp** — same inputs
(morale, temperature penalty, food), now structured into births/aging/deaths.

**Migration** moves population (and its demographics) between locations — band↔tile, tile↔tile —
extending the existing `PendingMigration`. It is how bands settle, how colonists split off, and
how people urbanize toward the big city.

### Labor — working-age, hybrid-allocated, local
Working-age is the labor **supply**, local to each location. Competing **demands**:
**tending** improvements, **construction**, **military**, **knowledge work**. Allocation is
**hybrid** — auto-fill by priority, with per-demand player overrides. A shortfall starves the
lowest-priority demands: under-tended improvements lose condition, builds stall. **Labor is the
scarce currency**, and idle labor is the signal that you can grow or expand.

### Improvements — the atom; a config catalog by class
Typed, place-bound records on a tile; **multiple per tile** up to a **footprint** budget.
Grouped into **classes** (dwelling, storage, food/tending, defense, …); each type is pure
config:

| Field | Meaning |
|-------|---------|
| `class` / `type` | dwelling → {lean-to, tent, house, tenement, arcology}, storage → {nut cache, smokehouse, granary}, … |
| `footprint` | fraction of a tile consumed (tiles cap total footprint) |
| `occupancy` | (dwellings) housing capacity for settled population |
| `labor_draw` | working-age needed each turn to keep it running (attendance) |
| `build_cost` | stockpiles (provisions/materials) consumed to build |
| `yield` | what it produces per turn (food, storage capacity, defense, …) |
| `decay_rate` | condition lost per turn when under-tended (destroyed at 0) |
| `prerequisite` | a **knowledge tag** required to build it |

**Density = occupancy ÷ footprint**, so a 100-occupancy arcology consumes less of a tile than
100-occupancy of single-family homes. **Same engine at every scale** — a shelter and a megacity
tower differ only in config.

The **first catalog already exists in the manual**: per-biome **storage hooks** (fish racks,
shell middens, jerky racks, nut caches, smokehouses, permafrost pits — manual §Ecosystem Food
Modules), **corrals** (pastoral path, ties to the Phase E domesticated herds), and **tended
patches** (farming path). No invention needed for the first content — just categorize them.

### Building — player command + stockpiles + knowledge
`build <faction> <type> <x> <y>`: requires local labor + a stockpile cost + a **known
prerequisite** (a knowledge tag in `DiscoveryProgressLedger` — no `farming` → no tended patch;
no `dwelling` → no houses). It progresses over turns (reuse the harvest travel/work machinery),
then exists and must be tended. Building is where the currently-inert
`CapabilityFlags::CONSTRUCTION` bit finds its home, and where the improvement catalog becomes a
**tech-gated progression** for free.

### Settlements — derived clusters, never founded
A camp/settlement/town/city is the **derived label** over a cluster of populated tiles, tiered
by population and footprint. There is no founding command. **Decay** makes the sunk cost bite —
walk away and your granary rots, your patch goes feral. The discrete `found_settlement` /
`FoundCamp` model is **retired** (or kept only as an optional scenario-start alternative). The
`SedentarizationScore` is reframed as an emergent **readout of accumulated tether** from built
improvements, rather than an input gate.

## Cross-cutting touchpoints

- **ECS / turn loop** (`core_sim`): the demographic tick replaces/extends `simulate_population`
  (`TurnStage::Population`); a labor-allocation system; an improvement build/tend/decay system;
  a derived-settlement computation. Config: new `*_config.json` blocks (demographics rates,
  labor priorities, improvement catalog) following the `fauna_config` loader pattern.
- **Knowledge** (`DiscoveryProgressLedger`, knowledge tags, Great Discoveries): improvement
  prerequisites; first tags like `farming` / `dwelling` / `construction`; sets the
  `CONSTRUCTION` capability bit.
- **Schema** (`sim_schema`): per-tile improvements + demographics + labor + the derived
  settlement view, wired through the snapshot/delta like `factionInventory` /
  `SedentarizationState`.
- **Client** (`clients/godot_thin_client`): population/age readout, labor readout, tile
  improvements + a **build** affordance, and a settlement view.

## Implementation phases

Each phase is independently shippable — its own PR (or small PR-group), landed sequentially and
held until the prior merges (small, focused PRs, matching the Wildlife & Hunting Overlay cadence).

- **Phase 0 — Design doc (this document).** Capture the model; cross-link; seed `TASKS.md`.
- **Phase 1 — Demographic population.** ✅ **Shipped.** `PopulationCohort` (bands = the first
  "locations") gained the 3-bracket age structure (children/working/elders, fixed-point) +
  births/aging/deaths modulated by food/morale/environment (`advance_demographics`), replacing the
  old growth clamp. Rates in `demographics_config.json`. **Food is band-local from day one** — a
  per-cohort `food_store` larder filled by foraging/hunt/husbandry income and drained by per-capita
  consumption (deficit → scarcity deaths); provisions left the faction-global `FactionInventory`
  entirely. Inter-band sharing + storage-pit distribution are Phase 3. Brackets + larder persist in
  the snapshot (rollback); a per-faction age-structure + dependency-ratio HUD readout ships
  (`PopulationDemographicsState`, wired like `SedentarizationState`). Migration unchanged (the
  larder rides along with the cohort).
- **Supply network (Phase 1↔3 bridge).** ✅ **Shipped.** Bands are small logistics nodes: each
  band's food (and any commodity) lives in a commodity-keyed `LocalStore`, and
  `balance_supply_networks` (`supply.rs`, `TurnStage::Logistics`) connects same-faction bands within
  a configurable **reach** into supply networks that **auto-balance** stored goods per-capita each
  turn, **throughput-limited** with friction (`supply_network_config.json`). So you can specialize a
  gatherer band to feed a nearby scout band, while a band beyond reach lives off its own larder.
  "Logistics from turn 0, scaled by config" — the same engine grows into settlement/city
  distribution, and its connected-components pass is what Phase 4 uses to derive settlements. A
  future **trade policy** adds a consent gate + priced return flow on cross-faction edges (retiring
  the dormant `TradeLink`/`trade_knowledge_diffusion`). Client readout deferred.
- **Phase 2 — Labor pool + hybrid allocation.** Working-age → a local labor supply; a
  demand/allocation system (auto by priority + player override); client labor readout.
  **Brought forward and concretized for the early game by `docs/plan_early_game_labor.md`** —
  a single ~30-person band partitions its working-age pool across four equipment-gated **roles**
  (Foraging/Hunting/Scouting/Warrior), introducing two concepts this arc lacked: **equipment
  (TOE)** — consumable, tiered (equipped/unequipped), the seam a future Crafter role fills — and a
  **carry-capacity population cap** that storage-class improvements (Phase 3) lift, making the
  nomad→settle transition mechanical. The four roles are the first concrete labor demands; the
  tending/construction/knowledge demands slot into the same allocator.
- **Phase 3 — Improvement catalog + building + knowledge-gating.** The `Improvement` component +
  data-driven catalog + `build` command (stockpiles + knowledge prereq + labor over turns) +
  footprint (multiple per tile) + dwellings housing tile population + tending draw + decay. Sets
  the `CONSTRUCTION` bit. Snapshot + client. Likely splits (3a build+catalog, 3b tending/decay,
  3c dwellings/housing).
- **Phase 4 — Settlements as derived clusters.** Compute the settlement/town/city label from
  populated-tile clusters; tiering; retire discrete founding; rework `SedentarizationScore` into
  a tether readout. Client settlement view.

## Future arc — Borders & Government (deferred, documented)

A "cluster of populated tiles" is really a **territory with borders**. A city *is* its borders,
and its population is **who lives within them**; a 4M city vs a 400k town is how many people
live inside the respective borders. Borders are **fluid** and adjust over time with population
and influence reach. Territory + who administers it leads to **government / governance**
(settlement → town → city → nation), the natural continuation of this arc. This link is recorded
here and seeded in `TASKS.md`; the heavy lifting (border-claiming, territory mechanics,
government) is a **later arc**, out of scope for the phases above.

## Open items / defaults

- Age brackets: **3** (children / working / elders) from the start (mechanism-complete; avoids a
  later change). Rates are config.
- Population: **localized from day one** (per band / per populated tile), not a faction-global
  aggregate — same mechanism at every scale, and avoids a global→local refactor later.
- Labor allocation: **hybrid** (auto by priority + player override).
- Building: **player-commanded** improvements; **emergent** settlements. Gated on stockpiles +
  known skills.

## See Also

- `docs/plan_early_game_labor.md` — realizes/extends Phase 2 for the first few turns (band as a
  labor pool; TOE/equipment; carry-capacity population cap; the early-game roles).
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — §"Organic Settlement —
  Sedentarization" (lines 64–72), the era ladder (Forager → Pastoralist / Tended Patches →
  Agrarian Towns …, line 88), §Ecosystem Food Modules (the storage-hook catalog).
- `docs/plan_wildlife_hunting_overlay.md` — the domestication (`domesticated_count`) and
  `SedentarizationScore` seams this arc consumes; corrals tie to Phase E domesticated herds.
- `docs/plan_intensification.md` — the arc that *realizes* this catalog's food-tending class
  (tended patches / corrals) by adding forage depletion + cultivation (the plant-domestication
  transpose) as the pressure/response that feeds sedentarization.
- `core_sim/CLAUDE.md` — Campaign Loop → Sedentarization; the config-file + loader patterns.
- `docs/architecture.md` — cross-system data flow.
