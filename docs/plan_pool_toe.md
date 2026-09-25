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

**Tools are a scarce store of COUNTABLE objects, and the unit of a tool is a PERSON.** For each tool
item the settlement runs in two stages (`systems/labor.rs`):

1. **Whole units between pools** — one bid per `(pool, priority tier)` group, summed over that
   group's sites, served by `settle_scarce_tools`: **High in full, then Normal, then Low**, a group
   wanting `ceil(its bid)`, and — because two pools are different people and cannot pass one hoe
   between them — a short tier apportioned **in whole units by largest remainder on the raw bid**.
2. **Continuously within a pool** — the group's whole allocation split across its own sites pro-rata
   by each site's `required`. One pool's hands are one crew carrying their tools from site to site,
   so a fractional unit there states *"this hand works here part of the time and brings its tool"*.

The `settle_scarce_store` beside it keeps splitting the *continuous* stores (pen hay, material
upkeep, build materials) pro-rata.

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
   mode (Spread or Priority), at the rate each site's hands would work with its TOE lines filled
   **from the band's own ledger**.

   > ⛔ **"AS IF FILLED" MEANS UNCOVERED, NOT AT THE ROSTER'S FRESH TIER — and the literal reading
   > was tried and rejected.** `fully_equipped_keeper_rate` reads the band's ledger for each tool's
   > **tier and condition** (a spent tool is not a filled line) and then applies **no coverage** —
   > so a band that owns none of a tool plans at the **bare** rate, and a band that owns one worn
   > one plans at that worn tier.
   >
   > Planning at the roster's fresh tier instead — what "as if its lines were filled" says if read
   > literally — would quote every pool a rate its band cannot reach, and at the shipped
   > `start_stock_fraction` of `0.0` that cut every pool's keeping supply by a third, permanently,
   > on a band that owns no tools at all.
   >
   > The uncovered part is what keeps the pass from being a loop: the requirement is struck **from**
   > the hands this rate produces, so a rate that already knew how many hands the stock could arm
   > would be the very circularity this four-step order exists to cut.
2. **Requirement.** Each site requires one unit of each of its tools per hand from step 1. A pool's
   TOE is the sum over its sites.
3. **Fill by priority.** Settle each tool band-wide, as in §2.2.
4. **Rates from what was filled.** Each site's hands work at the coverage-weighted rate of the share
   it received — the existing `KitCoverage` seam. **Hands are not re-split** — a site the settlement
   left short works its own hands slower; no site ever loses a hand to a site that was served.
5. **The hands nobody took go to the work still owed.** Step 1 caps each site at the hands its
   *plan* wanted, so a pool whose sites need fewer hands than it has leaves the remainder standing.
   Those hands — and only those — are put on whatever each site is still short of, **bare**.

   > ⛔ **A SITE OWING N UNITS OF WORK IS OWED N UNITS OF WORK.** Whether they arrive as one keeper
   > holding a tool worth 1.5 or as two keepers holding nothing is the band's business, not the
   > site's. Steps 1–4 plan the hands at the rate the tools *would* buy and then never ask whether
   > the work arrived, so a site the band-wide settlement reached with nothing worked below its
   > planned rate and fell short — while keepers the plan had no use for stood idle. Measured, one
   > `Quarrywork` pool of three keepers holding one chisel against a `High` and a `Low` quarry: the
   > Low working was supplied `0.7` of the `2.1` it owed, and `1.6` keepers did nothing.
   >
   > **The deficit** is `demand − delivered`, per site, off the rate step 4 resolved. **The idle
   > hands** are `keepers − Σ what the sites ASKED FOR` — step 1's own needs, not the hands it
   > handed out. The two are the same number whenever anything is actually idle, because that is
   > exactly when `distribute_upkeep_pool`'s coverage clamps at `1.0` and each share *is* its need;
   > taking the difference against the shares instead leaves a committed pool reporting ~1e-7 of a
   > spare keeper out of float, and step 5 would spend it. Struck against the needs it is negative
   > there and clamps to none, exactly, with no tolerance to tune.
   >
   > They are split across the deficits by the **same**
   > `distribute_upkeep_pool` under the **same** fund mode, over the same claim order the first
   > split used — which is what keeps *"the fund mode decides where hands go, the priority decides
   > where tools go"* true of the top-up as well.
   >
   > **A site takes a top-up hand only where a hand carrying nothing delivers something** — the
   > bare rate must be above zero. On the shipped roster a bare hand always banks
   > `PER_WORKER_OUTPUT`, so the condition is inert today; it is stated because *"only send the idle
   > keeper if it can actually contribute with no kit"* is the rule, not because the case ships.
   >
   > ⛔ **THIS IS NOT THE RE-SPLIT STEP 4 REFUSES, AND THE DIFFERENCE IS WHY THERE IS STILL NO
   > LOOP.** Step 4's refusal is untouched: **no site loses a hand**. Step 5 assigns only hands the
   > split never assigned to anybody, so every site's supply is `>=` what step 4 alone paid it — it
   > is monotonic, not a re-plan. And **a top-up hand claims no tool**, so it cannot move the
   > requirement step 2 struck or the settlement step 3 made from it. The circularity the order
   > exists to cut is *hands → tools → hands*; a bare hand is outside it, so there is no fixed point
   > to converge on.
   >
   > **It therefore does not grow the published requirement.** `poolToe.required` states the
   > **geared** plan's hands and nothing else, because that is the tool line a reader is being asked
   > about.
   >
   > **Wear is billed on the geared half alone.** A top-up hand was issued no tool, so charging the
   > site's kit for its hours would run gear down against work it took no part in.
   >
   > **What is left over after this step is what the wire publishes** as
   > `PopulationCohortState.poolCrew[].idleKeepers` — the idle hands *minus* the part of them step 5
   > just placed, per keeping pool, clamped at none. A pool with a head count and no sites therefore
   > publishes its whole head count, and a pool step 5 fully employed publishes nothing. The
   > pre-top-up figure is deliberately not the published one: it counts keepers the sim has working.

**When a band is not short of tools, this is identical to today:** step 1's rate is the rate today's
split uses on a fully equipped pool, and step 5 finds no deficit to fill. **A pool whose plan wants
every hand it has is likewise untouched** — the coverage binds from below, nothing is idle, and the
shortfall it works under is ordinary scarcity of people rather than of tools.

**When a band is short of TOOLS, the shortfall lands by priority** — on the sites that lost the
settlement, Low first — rather than the pool re-planning its hands around the missing gear. That is
what the on-screen promise *"when something runs short, the band spends it on high priority first"*
means applied to tools, and it keeps the two player levers separate: **the fund mode decides where
hands go, the priority decides where tools go.**

**What a site that lost the settlement is short of is TOOLS, not necessarily WORK.** Step 5 fills
what it can out of hands nobody took, so the ranking decides who works *geared* and the leftover
hands decide how much of the rest gets done at all. A pool short of both — every hand already
planned onto a site — has nothing spare and the deficit stands.

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

- **Pool card.** The shortfall is stated in **work units**, and the tool is named only as a
  **cause**, never counted or listed (issue #716). A tool count under a keeper stepper read as a
  head count (`0 of 1 hoe` → *"with a hoe I need 1 worker"*), and a list of tool names grows with
  every tier the roster adds. Two marks:
  - **⚠ — the work falls short.** The hover's work-units sentence, followed by `Short of tools.`
    when the pool's TOE is also short.
  - **ⓘ — the work is covered but the TOE is short:** `Tools would get more done per worker —
    short of tools.` Every pool tool is productivity, not a requirement, so a tool shortfall alone
    loses no work; it is shown anyway because bare hands stop being viable early in a game.

  A pool whose tools are all filled shows no tool line. This replaces `2 of 6 Tillage kits
  available` on pool cards only; take rows keep that sentence.
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
