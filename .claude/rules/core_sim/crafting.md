---
paths:
  - "core_sim/src/{materials_config,recipes_config,crafting}.rs"
  - "core_sim/src/systems/crafting.rs"
  - "core_sim/src/data/{materials,recipes}.json"
  - "core_sim/tests/{materials,crafting}.rs"
---

# Materials and the bench — the stuff a craftable thing is made of, and how it gets made

Authoritative design: `docs/plan_crafting_and_materials.md` (§1 what a material is, §2 stock and the
yield edges, §3 recipes, §5 crafts and tools). This file is the as-built half: the store's shape, the
merge rule, the cross-config seams that keep an id honest, and the bench.

**See also:** `.claude/rules/core_sim/equipment.md` (the TOE this arc replenishes, and where the
bench tools live as items), `.claude/rules/core_sim/config-loading.md` (the loader rule both configs
follow), `.claude/rules/core_sim/intensification.md` (the ledger the three crafts sit in, and the
`knowledge` block that paces them), `.claude/rules/core_sim/flora.md` and `fauna.md` (the two
rosters that carry the yield edge).

## A material is GENERIC, and the yield edge has ONE shape

There is no `deer_hide` — there is `hide`. A **source** states the characteristics of what *it*
yields, and it does so through `materials_config::MaterialYieldDef`, which is the **same type** on
`fauna_config.json`'s `hunt_yield.materials` and on `flora_config.json`'s `yield.materials`. A
deposit's will be the same type again.

That is why the type lives in `materials_config.rs` and not beside either roster. Nothing in this
model may be fauna- or flora-shaped, and a second definition is how that stops being true.

**Unlike the rate components beside it there is no global to fall back to.** `hunt_yield`'s
`provisions_per_biomass` reads `hunt.*` when omitted; a material list has nothing to fall back *to*,
because a material is a **thing** and there is no species-blind statement of which thing an animal
gives. An omitted list means *this source yields none*, which is a real answer.

## A characteristic VECTOR, and bands are the merge rule

A material declares its own **axes** (hide → `toughness`/`suppleness`) and a source states an exact
reading per axis. The shared vocabulary is four **bands** — `poor · fair · good · excellent` — and
they do three jobs at once, all three load-bearing:

1. **They are the merge key.** Same material, same per-axis band ⇒ one batch. Without it a band
   hunting deer for two hundred turns holds two hundred piles of hide.
2. **The exact reading survives the merge**, as the batch's amount-weighted average, and that is what
   crafting reads. Two `good` hides are therefore *not* interchangeable. **Never store only the
   band.**
3. **The band rates the AXIS, not the material.** "Excellent toughness" makes no claim about the
   hide. That is what lets ordinary quality words coexist with there being no best hide.

`MaterialsConfig::band_index` is total by construction: `validate` requires the first band to open at
`0.0` and the seams to strictly ascend, so every reading in `0..=1` selects exactly one band.

**Deriving a band needs the material's axis list, so `band_key` is a `MaterialsConfig` method** —
`LocalStore` stores, it does not interpret, and the key arrives at the store already resolved.

## `LocalStore` gained a second map; `goods` is untouched

```text
goods:     BTreeMap<String, Scalar>                       // provisions, fodder, trade goods
materials: BTreeMap<String, BTreeMap<BandKey, MaterialBatch>>
```

Provisions and trade goods are interchangeable **scalars** — two units of grain are two units of
grain. A material is not, so it cannot share that map: a single pooled average would silently drag a
mammoth hide down to a hare pelt the moment the two met, which is the whole thing the characteristic
vector exists to prevent.

- **`deposit_material`** is the merge rule. Amounts add; each axis becomes the amount-weighted
  average of the two, over **every axis either side names**.
- **`take_material(material, axis, amount)`** withdraws **worst-first on the named axis** — you spend
  the poor hide before the excellent one, the only ordering that does not silently burn the player's
  best stock on the first thing they make. A partial take leaves the batch's readings untouched (an
  average does not move when a uniform part of it is removed) and an emptied batch is pruned, so two
  stores holding the same materials still compare equal.
- **`take_material_batch(material, band, amount)`** is the supply network's move: the rating is
  already named, so nothing re-sorts.
- **`material_total`** is a sum over batches, never a cached field.

`BTreeMap` on both levels, for the reason `BandEquipment` is one: the checkpoint and any published
readout must iterate in a stable order.

**Checkpointing is free and deliberate.** `sim_state.rs` clones the whole `PopulationCohort`, so the
batch map — amounts *and* exact readings — rides both directions with no per-field code. Pinned by
`materials::material_batches_survive_a_checkpoint_round_trip`, which wipes the store between capture
and restore so "nothing happened" cannot pass.

## Pooling runs per RATING, never per material

`balance_supply_networks` is commodity-generic over `goods`; for materials it runs
`balance_commodity` once per **`(material id, band key)`** pair (`supply::MaterialKey`). So the
balancer never sees two ratings of one material as the same commodity, and pooling can never average
a mammoth hide into a hare pelt.

What moves keeps its exact characteristics: a sender's remainder is untouched, and the shipped half
arrives carrying the **senders' own amount-weighted reading** — one rating's worth of readings, which
the receiver then merges by the store's ordinary rule. Resolved off the same opening snapshot every
other flow reads, before any delta is applied.

Cost is one balancer run per rating rather than per material. That is the price of "not all hide is
the same", and it is bounded by how many *ratings* a network actually holds, not by how many batches
ever existed.

## Where a take credits it

The material account is credited **on the same seam the provisions are**, off what came **home**
(`take.carried`, never `killed_biomass`): you cannot tan a hide you left on the range. Four sites in
`systems/labor.rs` — the wild/pastoral hunt, the pen harvest, the rung-1/2 forage take, and the
rung-3 Field harvest.

- **Amounts are `Scalar` and accumulate.** A rate becomes a whole unit by crossing a threshold in the
  store, never by rounding per turn.
- **The band's `output_multiplier` applies**, exactly as it does to the three currencies: a material
  is the fourth account of one harvest, not a parallel economy.
- **A mixed plant basket DECOMPOSES rather than averaging** (`forage::patch_material_yields`). Food,
  fodder and trade are interchangeable numbers, so a basket averages them into one rate; a material
  carries a characteristic vector, and averaging two species' would invent a plant that is not
  growing there. So a mixed tile pays one credit per species, each keeping its own exact reading, and
  credits that land in the same band merge in the store — which is where merging belongs.
- **A Field reads its harvest in BIOMASS** (`forage::field_harvest_biomass`) rather than scaling off
  one of the three currencies, because a cash Field's provisions are `0` and there would be nothing
  to scale. Same `min(production, collection)` shape the other three accounts run.

**An EXPEDITION credits its own store and hands it over on arrival.** A raid's take is a yield edge
like any other, so both detached take sites (`systems/expeditions.rs` — the `Hunting` arm and the
scout's roadside kill) credit off the same `take.carried` the food and the pelts come off, into the
**party's** `LocalStore`. It banks there rather than as a scalar on the `Expedition` — which is what
`carried_trade` does for trade goods — because a material is a batch with a characteristic vector and
there is nothing to flatten it to. `LocalStore::drain_materials_into` moves it, **batch by batch**,
at the `Delivering` drop-off and in `fold_party_into_band`, so a mammoth hide is never averaged into
a hare pelt on the walk home and the receiver's ordinary merge rule does the merging.

**The pack does not bound it.** Food is capped by `provisions_capacity` and the surplus is wasted;
materials and trade goods both ride home outside that cap, which is the rule the yield-vector arc
already settled for pelts (`hunt-yield-model.md`) and this follows it rather than inventing a second
carry rule.

## The two cross-config seams

A material id is a `String`, so `validate` is the only thing between the file and a running sim —
the `UnknownItem` debt again (`equipment.md`). Both rosters are therefore reconciled against the
table **at load**, with the table passed in so it keeps one copy, exactly as the flora roster already
takes `forage.capacity_by_biome`:

- `FaunaConfig::validate_against_materials`, run by `load_fauna_config_from_env(&materials)`
- `FloraConfig::validate_against_materials`, run by `load_flora_config_from_env(&capacities,
  &materials)`

Both reject an unknown material, a `per_biomass` that is not finite and `> 0`, and — this is the
sharp one — a `characteristics` map that does not name **exactly** the axes the material declares.
Missing *or* extra: a defaulted axis is a silently wrong reading, and an invented one is read by
nothing.

**The materials table therefore loads FIRST of the three in `build_headless_app`.** The recipe book
loads **last** of all, because it is the only config that names ids from *two* other tables:
`RecipesConfig::validate_against(&materials, &equipment)` runs at that composition seam, and
`EquipmentConfig::validate_against_materials(&materials)` runs beside it for `bounds_material`.

---

# The recipe book

## A recipe reads ONE characteristic, and that is what makes "no best hide" real

`reads` names one axis on exactly one input. It is the whole of what separates two recipes over the
same material: the sled reads hide's `toughness` and the husbandry gear reads its `suppleness`, so a
Thunder Mammoth (`0.92 / 0.10`) makes a **fine** sled and **coarse** halters and a Snow Hare
(`0.14 / 0.92`) does the reverse. Fibre splits the same way (baskets read `fineness`, traps read
`strength`), and bone does (spears and clubs read `density`, wayfinding reads `length`).

`validate` rejects a second `reads`, because one reading answers one question.

## Continuous in, discrete out — and the STANDARD grade is the shipped number

The drawn reading never scales a resolved stat; it **selects** a grade, and a grade declares
absolutes. That is not a preference — `EquipmentEffect` has no representation for a multiplier
stacking on something else, which is what makes *flat until expiry, then a step down* structural
rather than remembered (`equipment.md` → "The three rules").

**A grade's payload was empty until the TIER owned these numbers, and now it is real.** The
objection was never to grades — it was that a grade may only declare a stat whose value does not
already live somewhere else, and while `attack` sat on the item and the two carry rates in
`labor_config.json`, writing them into `recipes.json` too would have given a shipped number a second
home to drift from *and* left the numbers inert. Quality tiers moved them onto the item's tier
(`equipment.md` → "Quality tiers"), so the grades declare absolutes against **that**:

- **The STANDARD grade is the shipped number, always.** `validate_against` rejects one that disagrees
  with the output item's default tier, so a standard-grade craft reproduces today's game exactly and
  the two rungs either side are what quality is worth — `coarse −15% · standard · fine +15%`, the
  design's own sled example (`34 / 40 / 46` on a shipped `40`).
- **A grade may only name a stat the item's TIERS declare**, and must restate that effect's mass
  bounds verbatim: a grade *replaces* a number rather than adding one, and a fine snare that dropped
  `max_body_mass` would quietly become a mammoth trap.
- **The empty payloads that remain are the items whose whole payload is shared** (husbandry gear's
  `pen_carry`, the wayfinding gear's vantage) or is a bench stat nothing yet grades.

`recipes_config::no_shipped_grade_declares_an_effect` is **replaced** by
`the_standard_grade_reproduces_the_shipped_item_and_the_others_bracket_it`, which pins the rule in
both directions and carries a liveness assertion — *"every standard grade agrees with its item"* is
trivially true of a book whose grades declare nothing, which is exactly the state it replaced.

Three rungs ship on every equipment recipe: `coarse 0.00 · standard 0.45 · fine 0.75`. The lowest
seam must be `0.00` and no two may be equal, so every reading selects exactly one — the same
totality rule `characteristic_bands` follows, for the same reason.

## The craft is DERIVED and then written down

`RecipeDef::craft` must equal the craft of the material the recipe **reads** (or, for a recipe that
reads nothing, of its first input), and `validate_against` enforces it. It is written in the file
because that is where a reader needs it, not because it is a second decision.

That is also what keeps a "primary material" field out of the model: the material a recipe **reads**
*is* the material the bench works — whose tool is consulted, whose bare-handed rate applies, and
whose craft is practised and taught. One field, four consumers.

## Knowledge gates a recipe only when the recipe says so

`requires_knowledge` is a **list** (`["weaving", "bone_working"]`) and is absent on every ordinary
kit recipe. You can make a sled by hand on turn one; doing it is what teaches Tanning.

> **It is a list because a tool has two parents.** The spec sketched a single string, which cannot
> express its own table: a tanning frame is made of fibre *and* bone, so it is gated on Weaving *and*
> Bone-working.

Every **tool** recipe carries one entry per material it is made from, and `validate_against` enforces
exactly that — **a required craft must be the craft of one of this recipe's own input materials**.
Paired with the rule that a tool is never made from the material it bounds (also validated), that
makes the deadlock *unrepresentable* rather than merely avoided:

```text
tool bounds M  ⇒  M is not an input  ⇒  craft(M) is not a required craft
```

So "metal needs a crucible, the crucible needs metalworking, nothing can start" cannot be written
down. **Tools are earned, never a prerequisite**, and there is no opening move where everyone builds
tools first.

---

# The crafts are knowledge, on the ladder's own ledger

`crafting.rs` holds three discovery ids beside the ladder's five —
`TANNING_DISCOVERY_ID` (2008), `WEAVING_DISCOVERY_ID` (2009), `BONE_WORKING_DISCOVERY_ID` (2010) —
registered in `start_profile_knowledge_tags.json` so they are *mappable*, and in
`intensification::discovery_id_for` (which delegates to `crafting::craft_discovery_id` for anything
it does not name itself) so the ladder's own validator can see them. **None ships known**, the same
rule every ladder knowledge follows.

**Crafting is the fourth teacher.** Hunting teaches Herding and Penning, foraging teaches Cultivation
and Seed Selection, keeping a pen teaches Foddering — and crafting teaches its own crafts. **The
lesson is the RECIPE's craft**, so what is being made decides what is learned, and a band that cannot
reach bone never advances Bone-working.

**The lesson and the tool's wear are charged on the SAME quantum: per item completed.** They are two
lines beside each other in `advance_crafting` for exactly that reason — split across two sites they
drift, and a tool that lasts 25 items ends up teaching a craft in 30. Pinned by
`crafting::the_lesson_and_the_tools_wear_are_charged_the_same_number_of_times`, which counts the
items finished and asserts *both* charges equal that count.

**The dial is `intensification_ladder.json`'s `knowledge.lesson_per_crafted_item` (0.2 → 5 items per
craft), a SIBLING of `progress_per_turn` rather than a reading of it.** A ladder lesson is charged
per turn worked and scaled by the crew's floor; a craft lesson is charged per item, and there is no
floor to scale it by and no turn to charge it on. It lives on the ladder rather than in
`recipes.json` so that **every knowledge pace in the game is tuned in one file**, which is the reason
the ladder's other two moved there in slice 4.

---

# The bench

`BandBench` is a component on a band: `{ recipe_id, workers, progress, drawn, items_completed,
last_output_grade }`. **One job at a time**, so no surface ever has to explain a queue.

**MAKE IS THE ASSIGNMENT.** `set_bench` puts the recipe up and draws idle workers onto it; there is
no Crafter role card and **no `LaborTarget` variant**. Scout and Warrior are standing roles with
nothing to point at, and crafting always has a subject, so it is staffed like a worked source. A
`LaborTarget::Craft` would also put a fictitious row on every per-source yield readout in the game.

**The crew comes out of the same pool `assign_labor` spends.** `band_idle_workers` is
`available_workers(working) − assigned_total − bench.workers`, and `handle_assign_labor` subtracts
the bench's crew from the headroom it clamps against. A band cannot staff the range and the bench
with the same people.

## `advance_crafting`, and why every refusal is a zero

Scheduled in the Population chain immediately after `advance_labor_allocation` — two reasons that
both have to hold: it draws on the materials **this turn's** take just delivered, and its crew came
out of the pool that pass spends.

1. No recipe ⇒ nothing.
2. Nothing drawn ⇒ **draw**. The availability test runs over *every* input before a single unit
   moves, which is what makes **"a short draw withdraws nothing"** true rather than "withdraws until
   it runs out". Each row is taken **worst-first** — on the read axis where the row names one, on the
   material's first declared axis otherwise (that fallback decides only *which pile*, never how much,
   and has to be deterministic rather than right).
3. **Accrue `workers × progress_per_worker_turn × craft_speed`.** `craft_speed` is the bounding
   tool's equipped value if the band has a live one, else the **material's** `hand_working.rate` —
   which is `0` for a material that cannot be worked bare-handed. **That zero is the entire refusal
   mechanism.** There is no *"you cannot craft that"* branch anywhere in the sim, exactly as
   `max(0, attack − defense)` refuses a hunt; the client renders the reasoned refusal.
4. On `progress >= work`: emit the outputs, charge one wear, charge one lesson, reset, re-draw.

**The grade is fixed at draw time and never moves** (`DrawnInputs`), which is what makes it *not* a
taper: a tool that runs dry mid-craft does not retroactively coarsen the thing on the bench, and a
pile that gets worse while the pass is in flight does not re-grade it.

**The overflow past `work` is dropped, not carried.** Progress beyond a completion was done on an
item whose materials have not been drawn yet, so there is nothing for it to have been spent on. The
consequence is the ladder's own `crew_scale` shape: over-crewing a bench buys less than
proportionally, and a `work: 8` recipe wants about four hands rather than sixteen.

**Swapping or clearing a job spends the pile already drawn.** The materials were cut for the thing
the player stopped making, and a `LocalStore` has no representation for a half-worked pile. The
command help says so rather than the sim pretending otherwise.

## What a completed craft delivers

`BandEquipment::stock(item, count, tier, grade)` is the delivery seam — **a new batch, never a merge
into one already standing**, because *"the next ten are their own batch"* is what keeps a fresh craft
from averaging into a half-spent pile. It is what ends `equipment.md`'s *"start-stocked and NOT
craftable"*, and a second sled made while the first is fresh is now genuinely a second sled.

Three things the batch carries, each resolved at the moment of the craft:

- **`count` is `RecipeOutput::amount`.** A pass of a recipe that makes three makes three. The seam
  used to deliver one item's worth of *condition* however many the row named — invisible because
  every shipped equipment recipe makes exactly one, which is why the fixture in
  `crafting::a_completion_delivers_the_recipes_whole_output_amount` states an `amount` of three.
  `validate_against` now rejects a fractional equipment `amount`: a ledger that counts things cannot
  bank half a spear.
- **`tier` is the best tier the faction knows** (`ItemDefinition::craftable_tier`), resolved off the
  same `DiscoveryProgressLedger` and the same completion threshold `set_bench` gates a recipe on. On
  the shipped roster that is always the one tier that ships known, so the opening makes exactly what
  it always made.
- **`grade` carries the drawn grade's ABSOLUTES, copied here rather than looked up later.** That is
  what makes *"the grade is fixed at craft time and never moves"* structural: a recipe retuned under
  a running world — or simply swapped off the bench — cannot re-grade a sled already in the band's
  hands. It is the same reason `DrawnInputs` carries its reading. `BandBench::last_output_grade` was
  a readout with no reader; it is now the same string every batch of that craft is stamped with.

## A BENCH TOOL's ownership was the FIRST honest reading, and the count slice generalised it

A tool could ask *"does the band own one"* before anything else could, for one reason: **nothing
stocks a tool at spawn**, so an absent entry for one could only mean nobody had made it. Reading it
as a free loom on turn 1 would delete *"tools are earned, never a prerequisite"* outright.

The count slice made that the **universal** reading — an absent entry is NOT OWNED for every item
(`equipment.md` → "The band carries BATCHES") — so `BandEquipment::owns` is gone and
`EquipmentConfig::live_bench_tool(material, wear)` is now the material lookup joined to the one
condition test, the way `KitChoice::item_live` joins the mask and condition for party gear. **The
caller passes the MATERIAL, never an item id**, so a roster that renames the loom moves the bench
with it. `start_stocked` stocks only what a kit `uses`, and `validate` forbids a kit naming a tool,
so *"tools are earned"* survives the flip by construction rather than by the old special case.

## Config files

| File | Purpose |
|---|---|
| `src/data/materials.json` | **The materials table** (loader `materials_config.rs`, env override `MATERIALS_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks. **`characteristic_bands`** — the shared rating vocabulary, `[{ name, from }]` ascending: `poor 0.0 · fair 0.30 · good 0.55 · excellent 0.80`. Retuning these re-partitions every batch on the map. **`materials`** — id → `{ craft, characteristics[], hand_working?, varieties? }`. Shipped: **`hide`** (tanning; `toughness`/`suppleness`), **`fibre`** (weaving; `fineness`/`strength`), **`bone`** (bone_working; `density`/`length`), each `hand_working { rate 0.5, quality_ceiling 0.60 }`. **Only the three organics ship** — wood, stone, clay and metal have no producer until the minerals arc and an unreachable material is dead content the catalogue publishes. **`hand_working` absent means the material cannot be worked bare-handed at all** (rate `0`, which is how metal will refuse itself with no branch), and the bare-handed ceiling belongs to the **material**, not to the absent tool. **`varieties` are parsed, validated, and none ships** — named presets over the material's own axes (`copper`, `bronze`), exercised by a test fixture for the same reason the bronze equipment tier is. **`validate` rejects**: an empty material table; a band list that is empty, does not open at `0.0`, does not strictly ascend, or carries a seam outside `0..=1`; a material with no craft or no characteristics; a duplicate characteristic on one material; a non-finite or negative `hand_working.rate`; a `quality_ceiling` outside `0..=1`; a variety that omits an axis the material declares or names one it does not, or states a reading off the range. **The root is open (`_comment*` keys) and `MaterialDef` is CLOSED** — a mistyped `hand_workng` would silently make a material unworkable, while a stray key at the root can only be prose. |

| `src/data/recipes.json` | **The recipe book** (loader `recipes_config.rs`, env override `RECIPES_CONFIG_PATH`, `validate` inside `from_json_str`, cross-config `validate_against(&materials, &equipment)` at the `build_headless_app` seam). Two blocks. **`crafting`** — `progress_per_worker_turn` (**1.0**). **`recipes`** — id → `{ display_name, craft, work, requires_knowledge[]?, inputs[], outputs[], grades? }`, where an input is `{ material, amount, variety?, reads? }` and an output is exactly one of `{ equipment }` or `{ material, characteristics }`. Ten ship: the seven kit items (`sled` 6 hide + 2 fibre / work 8; `husbandry_gear` 4 hide + 3 fibre / 7; `baskets` 5 fibre + 1 hide / 6; `traps` 6 fibre + 1 bone / 6; `spears` 1 bone + 2 fibre + 1 hide / 6; `clubs` 2 bone + 1 hide / 4; `wayfinding` 1 bone + 1 hide + 1 fibre / 4) and the three bench tools (`tanning_frame` 8 fibre + 2 bone / 12; `loom` 3 bone + 4 hide / 14; `bone_awl` 3 hide + 3 fibre / 10). **Costs are sized so MATERIAL, not bench time, is what binds** — see the file's `_comment_work_and_costs` for the measured income figures. **Bone is the scarce one by an order of magnitude** (0.0012–0.003 per biomass against hide's 0.006–0.022), so nothing costs more than 3 of it. **`validate` rejects**: a non-positive `progress_per_worker_turn`; an empty book; a non-positive `work` or `amount`; a recipe with no inputs or no outputs; the same material twice in one recipe's inputs; **more than one input carrying `reads`**; an output naming both or neither of `equipment`/`material`; an equipment output stating characteristics, or a material output stating none; a duplicate output; grades on a recipe that reads nothing or outputs only materials; a lowest grade seam that is not `0.00`; two grades sharing a seam; a duplicate stat in one grade's effects. **`validate_against` additionally rejects**: an unknown material, item, variety or axis; a `craft` that is not the craft of the material the recipe reads; a `requires_knowledge` naming a craft no material declares **or one that none of the recipe's own inputs is worked by**; a tool recipe whose inputs include the material it bounds; **a fractional `amount` on an equipment output** (a batch's `count` cannot bank half a spear); and **a grade effect that names a stat no tier of the output item declares, drops that effect's mass bounds, or — at the `standard` rung — disagrees with the item's default tier** (see "Continuous in, discrete out"). |

The two **yield edges** are rows on the rosters that own them — `fauna_config.json`'s
`hunt_yield.materials` (`fauna.md`) and `flora_config.json`'s `yield.materials` (`flora.md`) — and
their authoring rationale rides in those files' own `_comment*` keys, next to the numbers. The three
**bench tools** are rows on `equipment.json` (`equipment.md`), for the same reason: they are stocked,
worn and counted exactly like a spear, which is what makes *"band-local, consumable"* free.

> **The tuning panel may stage `materials` and `recipes` (`ConfigOverrideKind`), but only NUMBERS.**
> `config_override.rs` validates a staged patch through the kind's own `from_json_str` and nothing
> else — it holds one config's JSON, so it cannot run either cross-config check. A patch that renamed
> a material would therefore install cleanly and panic the **next New Game**, which is precisely the
> failure that module exists to prevent. Band seams, costs, `work` and grade seams are safe; an id is
> not.

## The shipped roster is authored so "there is no best hide" is REAL

The pairs are the point, not the rows:

| | tough / fine | supple / strong |
|---|---|---|
| Thunder Mammoth hide | **0.92** excellent | **0.10** poor |
| Snow Hare hide | 0.14 poor | **0.92** excellent |
| Wild Fowl down (fibre) | **0.90** fineness | 0.12 strength |
| Wild Aurochs sinew (fibre) | 0.30 fineness | **0.86** strength |
| Cotton (fibre) | **0.92** fineness | 0.35 strength |
| Mesquite bast (fibre) | 0.30 fineness | **0.70** strength |

Neither member of any pair is the upgrade. A **Grey Seal** (`0.66 / 0.74`) is the deliberate
exception that says the two axes are not opposed *by construction* — they just usually are.

`per_biomass` follows a physical rule rather than a per-row judgement: **a small animal is nearly all
skin and a big one is nearly all meat**, so hide runs from a snow hare's `0.021` to a mammoth's
`0.006`. Bone runs the other way on `length` (a mammoth tusk at `0.95`, a hare's at `0.07`).

## The shipped costs against the kit clock

`plan_early_game_labor.md` puts a kit item's life at **~15–20 turns**, and the costs are set so a
band keeps its kit alive on roughly a quarter of its material income and spends the rest on tools and
stock. Measured against the shipped ~16-worker band split half-gathering, half-hunting Red Deer:

| | per turn | a kit item costs | its life |
|---|---|---|---|
| **fibre** (reeds `0.02`/biomass) | ~1.3 | baskets 5 → ~4 turns | ~19.5 turns |
| **hide** (deer `0.014`/biomass) | ~0.6 | sled 6 → ~10 turns | ~60 turns |
| **bone** (deer `0.002`/biomass) | ~0.08 | spears 1 → ~12 turns | ~45 turns |

Bench time is deliberately **not** the binding constraint: four hands work a `work: 8` sled off in
four turns bare-handed, against ten turns of banked hide. **A craft is ~5 items** (the ladder's
`lesson_per_crafted_item`), so ~25–40 turns of deliberate work — the same order as one rung of the
intensification ladder — and the tool it unlocks is a further ~20-turn investment (a tanning frame is
8 fibre + 2 bone) that then pays back over the 25 items it lasts.

## What is deliberately not wired

**The snapshot wire** (§7) is the remaining stage. Equipment counts and quality tiers are landed
(`equipment.md`), but nothing publishes a batch, a bench or a craft offer to a client yet, so the
panel's reasoned refusals — *"Short 4.9 bone"*, *"Needs Clay-working"*, *"No loom"* — have no field to
ride on. The sim resolves none of them, by design: every refusal here is a zero, and the reason is a
publication.

**`BandBench::items_completed` is a readout with no reader**, and so is a batch's `count`. They exist
because the bench has to record them for the wear/lesson pairing to be testable at all; the client
half is what consumes them. `last_output_grade` **does** have a reader now — it is the grade stamped
onto the batch the pass delivered.

**A material output and an input `variety` are parsed, validated and shipped by nothing** — the alloy
shape, exercised by a fixture, exactly as the materials table treats `varieties` and `equipment.json`
treats the bronze tier. There is no producer for a material a recipe would make until the minerals
arc lands.
