# Plan: Pool TOE — a standing pool builds its own kit

**Status:** design. Decided with the maintainer: **one TOE per pool**, filled by the **existing High /
Normal / Low priority**, and **pools only** — hunters, gatherers, cutters, scouts and warriors keep
the kits they are sent out with.

## 0. The gap, in one line

**A standing pool is handed one kit, but its sites can need different tools — so "which tool does
this pool need?" has no single answer, and the code answers it by assuming every site on a web uses
the same one.**

---

## 1. Why one kit per pool breaks

The five standing pools — **Agriculture, Husbandry, Roadwork, Quarrywork, Builders** — each work many
sites out of one band-wide stock of tools. Today each pool resolves **one** kit, through a lookup that
asks *"which tool serves this web?"* without naming a rung.

That works only because of how the shipped tools happen to be configured:

| Tool | Serves | Tied to a rung |
|---|---|---|
| hoes | the plant web | no |
| crook | the animal web | no |
| earthmoving | routes | `route:dirt_road` |
| stone_dressing | routes, extraction | `route:paved_road`, `extraction:quarry` |

**Roads already break it.** A Roadwork pool keeping a dirt road and a paved road needs earthmoving
tools *and* stone-dressing tools. Quarrywork already holds sites on two branches (forestry and
extraction). Neither has one right kit.

**Farming would break silently.** A hoe works on every plant rung, so Agriculture's lookup finds it.
The first plant tool tied to a rung — a plough for Fields, say — refuses a lookup that names no rung.
The pool then resolves no kit, registers no demand for its tools, and the double-issue PR #665 closed
comes back with nothing flagging it.

**And the player picks what the pool should work out.** Keeping kits are picked per site and build
kits per queue entry (§3), when every one of those answers follows from the site's own rung.

---

## 2. The model

### 2.1 A pool's TOE is built from its sites

**Each site requires the tools that serve its own branch at its own rung.** A pool's **TOE** is one
line per tool: how many units its sites require, and how many the band filled.

**Required is one unit per hand** the pool puts on a site that tool serves (divided by the item's
`workers_per_unit`; no shipped item declares more than one).

> Agriculture has 4 hands on tended patches and 2 on a Field. If a Field wanted a plough as well as a
> hoe (hypothetical — no plough ships), its TOE reads **6 hoes, 2 ploughs**.

Because the requirement is asked **per site, at the site's real rung**, nothing ever asks *"what
serves the whole web?"* — so the silent failure in §1 cannot happen.

### 2.2 Tools are filled by the existing priority

**Tools are a scarce store, handled exactly as hurdles already are.** For each tool item, every
pool's claims on it are settled together by `settle_scarce_store` (`systems/labor.rs`): **High in
full, then Normal, then Low, and proportionally within a tier** when the remainder cannot cover it.

- A site's claim ranks at **that site row's** `SourcePriority`.
- A build's claim ranks at **its queue head row's** priority — the rule a build's materials follow
  today.

**Settlement is band-wide per tool; the per-pool TOE is the readout of it.** Stone-dressing is wanted
by Roadwork (paved roads) and Quarrywork (quarries); both pools' claims go into the one settlement,
and each pool's TOE shows its own share.

### 2.3 The order — what keeps it from being a loop

How many hands a site gets depends on how fast those hands work, which depends on tools; how many
tools a site requires depends on its hands. It resolves in one pass:

1. **Hands, as if fully equipped.** Split each pool's hands across its sites under the band's fund
   mode (Spread or Priority), at the rate each site's hands would work with its TOE lines filled.
2. **Requirement.** Each site requires one unit of each of its tools per hand from step 1. A pool's
   TOE is the sum over its sites.
3. **Fill by priority.** Settle each tool band-wide, as in §2.2.
4. **Rates from what was filled.** Each site's hands work at the coverage-weighted rate of the share
   it received — the existing `KitCoverage` seam. **Hands are not re-split.**

**When a band is not short of tools, this is identical to today:** step 1's rate is the rate today's
split uses on a fully equipped pool.

**When a band is short, the shortfall lands by priority** — on the sites that lost the settlement,
Low first — rather than the pool re-planning its hands around the missing gear. That is what the
on-screen promise *"when something runs short, the band spends it on high priority first"* means
applied to tools, and it keeps the two player levers separate: **the fund mode decides where hands
go, the priority decides where tools go.**

### 2.4 Builders fund one entry

The builders' claim is **the head entry's tool lines × the builders' head count**, ranked at the head
row's priority. Entries behind the head are dated, not worked, and claim nothing — as today.

### 2.5 What does not change

- **Take crews** (hunt, forage, extract) keep the kit the player picks and the pro-rata item budget
  from PR #665.
- **Scout and Warrior** keep their picked kits.
- **Detached expeditions** carry their own ledger and are rationed against nobody.
- **Wear** still draws on the most-worn batch with condition left.

**Pool tools and take/role tools are disjoint on the shipped roster.** `EquipmentConfig::validate`
refuses any item with a `build_work` effect appearing in a take or role kit, so the two allocations
can never contend for one stock.

---

## 3. What the player stops choosing

| Retired | Today | Replaced by |
|---|---|---|
| **Per-site keeping kit** | Work Inspector's Upkeep picker → `upkeep_kit` → `LaborAssignment::upkeep_kit` | the site's own tool requirement |
| **Per-build kit** | build queue kit picker → `build_kit` → `BuildQueueEntry::kit` | the head entry's tool requirement |

**A lever is lost, and it is stated rather than hidden.** Today a player can pick `none` for a site
to keep it bare-handed and spare the band's tools the wear. Under this plan a site marked **Low** is
served last when tools run short — but when tools are plentiful it is still equipped, and still wears
them. There is no longer a way to keep a site bare on purpose.

**This refines `plan_standing_upkeep.md` §2.5 rather than reversing it.** That section decided *"the
pool is workers and goods; the tool is the work site's."* The site still determines its tool; the
player no longer picks it.

**The four pool kits stay on the roster as starting-loadout bundles.** The Outfit screen mints items
from `tillage`, `hurdling`, `roadbuilding` and `paving` (`starting_loadout::expand_kits`), so they
remain the way a band starts with hoes, a crook or road tools. Their `jobs` lists stop naming
something a pool can be staffed with.

---

## 4. On the wire

- **Append** to `PopulationCohortState`:

  ```
  poolToe:[PoolToeLine];

  table PoolToeLine {
    pool:string;      // "agriculture" | "husbandry" | "roadwork" | "quarrywork" | "builders"
    itemId:string;    // equipment.json item id
    required:float;   // units the pool's sites require this turn
    filled:float;     // units the band's settlement gave them
  }
  ```

- **Fields that stop carrying meaning for pools** are left in place, because FlatBuffers fields are
  positional: a pool row's `LaborAssignment.kitId` publishes empty and its `kitWorkersHolding` equals
  its `workers` (the *nothing to be short of* reading, so no existing reader reports a shortfall);
  per-site `upkeepKitId` / `buildKitId` on patches, herds and workings publish empty. No fallback
  code.
- **`sim_ai` reads those per-site kit ids** (`instruments/observations.rs`, `SourceBuild.kit_id` and
  `SourceUpkeep`). It moves to `poolToe`. It never sends the retired commands.

---

## 5. On screen

- **Pool card.** The warning triangle shows on any shortfall — people, tools or both — with the
  reason in the tooltip (in flight on `worktree-pool-card-triangle`). The tooltip's tool reason
  becomes the pool's **short TOE lines**, in the client's existing `N of M` phrasing:
  **`4 of 6 hoes · 0 of 2 ploughs`**. A pool whose tools are all filled shows no tool line. This
  replaces `2 of 6 Tillage kits available` on pool cards only; take rows keep that sentence.
- **Work Inspector.** The Upkeep kit picker is removed.
- **Build queue.** The per-entry kit picker is removed.

---

## 6. Slices

1. **Sim.** Per-site tool requirement at the site's rung; band-wide priority settlement per tool;
   the four-step order; the builders' head claim; pools leave the pro-rata item budget (the pool arms
   of `LaborAllocation::row_kit` retire). Proves: a Roadwork pool keeping dirt and paved roads
   requires both tools; stone-dressing shared by Roadwork and Quarrywork settles High before Low; a
   pool that is not short is bit-identical to today; a rung-tied plant tool resolves per site instead
   of silently resolving nothing.
2. **Wire.** Append `poolToe`; publish the retired fields as §4 states; re-record the golden;
   decode-guard.
3. **Commands and config.** Retire `upkeep_kit` and `build_kit` end to end — runtime payloads,
   protobuf, command text, server handlers, the `command_guard` drives; add the `validate` guard;
   confirm loadout bundles still mint.
4. **Client.** Pool card TOE tooltip; remove the Upkeep and build-queue kit pickers; harness claims
   for short, filled and shared-tool pools.
5. **AI and docs.** Move `sim_ai` observations to `poolToe`; describe pool TOEs in the manual's Table
   of Equipment paragraph; update `.claude/rules/core_sim/equipment.md`; point
   `plan_standing_upkeep.md` §2.5 here.

---

## See Also

- `docs/plan_standing_upkeep.md` §2.5 — pools as band-level roles; the tool-per-site decision this
  refines
- `docs/plan_unit_costed_work.md` — why gear changes work per turn, never the work a job requires
- `.claude/rules/core_sim/equipment.md` → "ONE BAND, ONE SET OF GEAR" — the item budget take crews
  keep
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — the Table of Equipment paragraph
