---
paths:
  - "core_sim/src/materials_config.rs"
  - "core_sim/src/data/materials.json"
  - "core_sim/tests/materials.rs"
---

# Materials — the stuff a craftable thing is made of

Authoritative design: `docs/plan_crafting_and_materials.md` (§1 what a material is, §2 stock and the
yield edges). This file is the as-built half: the store's shape, the merge rule, and the two
cross-config seams that keep a material id honest.

**See also:** `.claude/rules/core_sim/equipment.md` (the TOE this arc replenishes),
`.claude/rules/core_sim/config-loading.md` (the loader rule `materials_config.rs` follows),
`.claude/rules/core_sim/flora.md` and `fauna.md` (the two rosters that carry the yield edge).

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

**Not yet credited: an EXPEDITION.** A detached party's larder folds back on arrival and is bounded
by `provisions_capacity`; a material has no such carry rule yet, so a raid's take pays food and trade
and no material. See "What is deliberately not wired".

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

**The materials table therefore loads FIRST of the three in `build_headless_app`.**

## Config files

| File | Purpose |
|---|---|
| `src/data/materials.json` | **The materials table** (loader `materials_config.rs`, env override `MATERIALS_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks. **`characteristic_bands`** — the shared rating vocabulary, `[{ name, from }]` ascending: `poor 0.0 · fair 0.30 · good 0.55 · excellent 0.80`. Retuning these re-partitions every batch on the map. **`materials`** — id → `{ craft, characteristics[], hand_working?, varieties? }`. Shipped: **`hide`** (tanning; `toughness`/`suppleness`), **`fibre`** (weaving; `fineness`/`strength`), **`bone`** (bone_working; `density`/`length`), each `hand_working { rate 0.5, quality_ceiling 0.60 }`. **Only the three organics ship** — wood, stone, clay and metal have no producer until the minerals arc and an unreachable material is dead content the catalogue publishes. **`hand_working` absent means the material cannot be worked bare-handed at all** (rate `0`, which is how metal will refuse itself with no branch), and the bare-handed ceiling belongs to the **material**, not to the absent tool. **`varieties` are parsed, validated, and none ships** — named presets over the material's own axes (`copper`, `bronze`), exercised by a test fixture for the same reason the bronze equipment tier is. **`validate` rejects**: an empty material table; a band list that is empty, does not open at `0.0`, does not strictly ascend, or carries a seam outside `0..=1`; a material with no craft or no characteristics; a duplicate characteristic on one material; a non-finite or negative `hand_working.rate`; a `quality_ceiling` outside `0..=1`; a variety that omits an axis the material declares or names one it does not, or states a reading off the range. **The root is open (`_comment*` keys) and `MaterialDef` is CLOSED** — a mistyped `hand_workng` would silently make a material unworkable, while a stray key at the root can only be prose. |

The two **yield edges** are rows on the rosters that own them — `fauna_config.json`'s
`hunt_yield.materials` (`fauna.md`) and `flora_config.json`'s `yield.materials` (`flora.md`) — and
their authoring rationale rides in those files' own `_comment*` keys, next to the numbers.

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

## What is deliberately not wired

Recipes, the bench, crafts-as-knowledge, tools, equipment counts and quality tiers, and the snapshot
wire are **later stages of the same arc** — this one builds the materials foundation and the two
yield edges only. Nothing publishes a batch to a client yet, and nothing consumes one.

**An expedition's take credits no material** (see "Where a take credits it"). Closing it needs a
carry rule for materials on a detached party and a fold-back on arrival, which is a design decision
rather than a transcription.
