---
paths:
  - "core_sim/src/{materials_config,recipes_config,crafting,craft_suggestions,auto_craft}.rs"
  - "core_sim/src/systems/crafting.rs"
  - "core_sim/src/snapshot/crafting.rs"
  - "core_sim/src/data/{materials,recipes}.json"
  - "core_sim/tests/{materials,crafting,crafting_wire,bench_delivery,bench_parallel_orders,auto_craft}.rs"
---

# Materials and the bench — the stuff a craftable thing is made of, and how it gets made

Authoritative design: `docs/plan_crafting_and_materials.md` (§1 what a material is, §2 stock and the
yield edges, §3 recipes, §5 crafts and tools). This file is the as-built half: the store's shape, the
merge rule, the cross-config seams that keep an id honest, and the bench.

**See also:** `.claude/rules/core_sim/equipment.md` (the TOE this arc replenishes, and where the
bench tools live as items), `.claude/rules/core_sim/config-loading.md` (the loader rule both configs
follow), `.claude/rules/core_sim/intensification.md` (the ledger the crafts sit in, and the
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
goods:     BTreeMap<String, Scalar>                       // provisions, fodder
materials: BTreeMap<String, BTreeMap<BandKey, MaterialBatch>>
```

Provisions and fodder are interchangeable **scalars** — two units of grain are two units of grain. A
material is not, so it cannot share that map: a single pooled average would silently drag a mammoth
hide down to a hare pelt the moment the two met, which is the whole thing the characteristic vector
exists to prevent.

> **`TRADE_GOODS` was a third key here and is RETIRED (arc #527).** It was written by every take site
> — the Field harvest, the drawn-down forage take, the pen, the wild hunt, the expedition delivery —
> and **read by none**: there was no `take(TRADE_GOODS)` anywhere in the workspace. Beside every one
> of those credits sat a `credit_material_yield` banking the *same* take's concrete hide, bone and
> fibre, which is the account that survived. A flat scalar is the one representation that cannot tell
> a mammoth hide from a hare pelt, so a market built on it would have collapsed the distinction this
> whole model exists to keep. `docs/plan_contact_and_logistics.md` §Q5 carries the long form.

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

The material account is credited **on the same seam the provisions are**, off what the take **keeps** —
the whole kill (`killed_biomass`), since every hunt keeps its carcass (a camp kill lands it at the
camp, a posted one's porters ferry it). Four sites in
`systems/labor.rs` — the wild/pastoral hunt, the pen harvest, the rung-1/2 forage take, and the
rung-3 Field harvest.

- **Amounts are `Scalar` and accumulate.** A rate becomes a whole unit by crossing a threshold in the
  store, never by rounding per turn.
- **The band's `output_multiplier` applies**, exactly as it does to the two scalar currencies: a
  material is the third account of one harvest, not a parallel economy.
- **A mixed plant basket DECOMPOSES rather than averaging** (`forage::patch_material_yields`). Food
  and fodder are interchangeable numbers, so a basket averages them into one rate; a material carries
  a characteristic vector, and averaging two species' would invent a plant that is not growing
  there. So a mixed tile pays one credit per species, each keeping its own exact reading, and
  credits that land in the same band merge in the store — which is where merging belongs.
- **A Field reads its harvest in BIOMASS** (`forage::field_harvest_biomass`) rather than scaling off
  one of the scalar currencies, because a cash Field's provisions are `0` and there would be nothing
  to scale. Same `min(production, collection)` shape the other accounts run — and since arc #527 it
  is the **only** account a cash Field pays into at all.

**An EXPEDITION credits its own store and hands it over on arrival.** A raid's take is a yield edge
like any other, so both detached take sites (`systems/expeditions.rs` — the `Hunting` arm and the
scout's roadside kill) credit off the same `take.carried` the food comes off, into the **party's**
`LocalStore`. It banks there rather than as a scalar on the `Expedition` — which is what the retired
`carried_trade` did — because a material is a batch with a characteristic vector and there is nothing
to flatten it to. `LocalStore::drain_materials_into` moves it, **batch by batch**, in
`fold_party_into_band`, so a mammoth hide is never averaged into a hare
pelt on the walk home and the receiver's ordinary merge rule does the merging. It is also what the
feed line's *"returning EMPTY"* test reads (`systems::expeditions::materials_carried`): a wolf raid
comes home with no meat and a pack full of hides, and calling that empty is the food-only blindness
the yield-vector arc removed.

**The pack does not bound it.** Food is capped by `provisions_capacity` and the surplus is wasted;
materials ride home outside that cap, which is the rule the yield-vector arc already settled for
pelts (`hunt-yield-model.md`) and this follows it rather than inventing a second carry rule.

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
Thunder Mammoth (`0.92 / 0.10`) makes an **excellent** sled and **poor** halters and a Snow Hare
(`0.14 / 0.92`) does the reverse. Fibre splits the same way (baskets read `fineness`, traps read
`strength`), and bone does (spears and clubs read `density`, wayfinding reads `length`).

`validate` rejects a second `reads`, because one reading answers one question.

### ⛔ `reads` IS OVERLOADED — it also picks WHICH BATCHES ARE SPENT

`reads` says **two** things at once, and only one of them is about quality:

1. **the axis the output's grade is read from** (`RecipeDef::grade_for`), and
2. **the axis the input is spent WORST-FIRST on** (`systems::crafting::spend_axis`), which falls
   back to the material's **first declared axis** when the row names none.

The two are inseparable in the config because there is one word for both. **On a material-only
recipe only the second applies** — there is no grade to read — and that is exactly the case where
deleting `reads` looks harmless and is not: `hurdles` reads hide's `suppleness`, hide's first
declared axis is `toughness`, so dropping the word would silently move the bench onto a different
pile of hide. **The gate therefore lives on the grade, never on the config.**

### A MATERIAL OUTPUT CARRIES NO GRADE, so a material-only recipe quotes none

A grade is a property of a piece of **equipment**: `emit_outputs` stamps `BatchGrade` on the
equipment branch only, and `MaterialBatch` has **no grade field** — a material output states its own
`characteristics` verbatim instead. `validate_grades` has always rejected a recipe that declares
`grades` and outputs only materials (*"a material carries its own characteristics, so the grade
would be read by nothing"*), but a **grade is resolved whether or not the recipe declares one**, so
the readouts quoted a band anyway.

`RecipeDef::grade_for` returns `None` for such a recipe, gated on
`RecipeDef::outputs_only_materials()` — **the same named predicate `validate_grades` calls**, because
a second hand-written `.all(|output| output.material_id().is_some())` is how the loader and the wire
come to disagree about one recipe. The gate sits on `grade_for` rather than on either publish site
because every grade in the game is resolved through that one call: the offered row's preview
(`preview_grade` → `CraftOffer.outputGrade`) **and** the drawn pile the bench stamps from
(`DrawnInputs::grade` → `BandBench.outputGrade`, `last_output_grade`). Gating the offer alone would
have left a *running* hurdles job still quoting a grade.

- **`hurdles` is the game's only material-only recipe** (all twelve surveyed; every other one outputs
  equipment), so this had exactly **one** visible instance: the panel's make-note read
  `Hide, no tanning frame → poor` while every hurdle produced carried the recipe's hard-coded
  `stoutness 0.6 / span 0.6`. It now reads `Hide, no tanning frame` — `snapshot::crafting::invitation`
  already appends `→ {grade}` only for a non-empty one, so an empty grade leaves **no dangling
  arrow**.

## ONE QUALITY LADDER — a grade key IS a `characteristic_bands` name

The drawn reading never scales a resolved stat; it **selects** a grade, and a grade declares
absolutes. That is not a preference — `EquipmentEffect` has no representation for a multiplier
stacking on something else, which is what makes *flat until expiry, then a step down* structural
rather than remembered (`equipment.md` → "The three rules").

**And the grade it selects is simply the BAND.** A recipe declares **no `when` at all**: the output's
grade is the band of `min(material reading, tool quality ceiling)`, resolved through
`MaterialsConfig`'s own lookup, which is already total by construction. So the same four words rate a
hide's toughness on the panel's rail and rate the sled you make out of it. An earlier cut invented
`coarse / standard / fine`, which was a second vocabulary for one idea — and, load-bearingly, a
**second set of cut points free to drift** from the bands beside them. Deleting them is the same
choice this model already recorded twice: `dispersion` multiplies a species' own `wariness` rather
than reading a "jumpy" flag, and `max_body_mass` reads `body_mass` rather than a `size_class`.

- **A band a recipe does not declare INHERITS THE ONE BELOW IT** (`RecipeDef::grade_effects_for`), so
  a recipe wanting three steps writes three. **Declaration governs effects only** — the grade a batch
  is *stamped* with is always the band name, so a craft off excellent hide reads `excellent` even on
  a recipe that declares nothing there; it simply buys no stat on that item. **A recipe resolves no
  grade at all (`""`) in two cases**: it reads nothing, *or* its outputs are all materials (see
  "A MATERIAL OUTPUT CARRIES NO GRADE" above). This bullet used to read *"a recipe with **no
  `reads`** still resolves no grade"*, which named only the first.
- **`validate_against` rejects a grade key that is not a declared band**, and **a recipe whose lowest
  declared grade is not the FIRST band** — inheritance only ever looks down, so something has to
  answer for a reading of `0.0`. Both are cross-config, because the book does not carry the
  vocabulary.
- **A grade may only name a stat the recipe's OWN OUTPUT TIER declares**, and must restate that
  effect's bounds verbatim — its mass bounds, and on `build_work` its `branch` and `rung`: a grade
  *replaces* a number rather than adding one, so an excellent snare that dropped `max_body_mass`
  would quietly become a mammoth trap, and an excellent crook that dropped `branch: animal` would
  serve plant builds too (an absent branch serves every web in `EquipmentEffect::serves_build`).
  **The scan is one tier
  wide, not the whole ladder** — it walked `def.tiers.iter()` until issue #736, which was
  indistinguishable while every item had one tier.
- **Build gear is graded on the standard spread on both webs.** `crook`, `crook_wood`, `hoes` and
  `hoes_flint` grade `build_work` at poor ×0.75 / fair ×0.85 / good ×1.00 / excellent ×1.15 of the
  tier each makes (0.375 / 0.425 / 0.5 / 0.575 on `plain`, 0.525 / 0.595 / 0.7 / 0.805 on `flint`),
  so a builder's per-worker offset responds to craftsmanship like every other kit stat.
- **A recipe with no `grades` block at all is a real statement**, not a missing value: eight ship
  that way (`wayfinding`, `wayfinding_wood`, the two road tools `earthmoving` and `stone_dressing`,
  and the four bench tools), because their payload is *shared* rather than tier-bought (the
  wayfinding gear's vantage) or is a stat nothing yet grades (the road tools' rung-bound
  `build_work`, a bench tool's craft stats). `hurdles` is ungraded too, but not by choice: it outputs a
  material, and `validate` rejects grades on a material-only output. The old shape spelled absence
  as three empty rungs per recipe — inert config saying by convention what absence now says
  outright.

## A RECIPE NAMES THE TIER IT MAKES — which is what lets ONE ITEM have TWO recipes

`RecipeOutput::tier` (`recipes.json` → `_comment_output_tier`). A **tier** is what the *material*
buys (`equipment.md` → "Quality tiers"), and a **recipe** is what names the material — so a spear
knapped from stone and a spear pointed with bone are one item at two tiers, and the thing that knows
which is the recipe. Before this field the tier came from faction knowledge alone
(`ItemDefinition::craftable_tier`), which can answer **one tier per item per faction** and so could
never express two at once.

- ⛔ **REQUIRED on an item declaring more than one tier**, and that is the load-bearing half rather
  than tidiness. `craftable_tier` resolves the **last** satisfiable rung, so a bone recipe naming
  nothing would have started delivering `flint` spears the moment the flint tier shipped. Mandatory
  makes that unrepresentable rather than merely avoided.
- **Rejected on a MATERIAL output** — the mirror of `characteristics` being rejected on an equipment
  one. A `MaterialBatch` has nowhere to put a tier.
- **Optional on a single-tier item**, which is what leaves the shipped rows that gained nothing
  unchanged: `craftable_tier` has exactly one answer there.
- **`craftable_tier` and `EquipmentTier::requires_knowledge` both STAY.** They are the fallback for a
  single-tier item and the mechanism a knowledge-gated tier will need. On the shipped book no item
  with a choice reaches them, so the knowledge-gated path is covered by a fixture only.
- **Three readers, one accessor** (`RecipeDef::output_tier_id`): the bench's delivery
  (`systems::crafting::emit_outputs`), the offer's per-recipe readings (`snapshot::crafting::made_tier`,
  which `makes`, `lasts`, `ownedAtTier` and the invitation's unlock band all resolve through) and the
  grade anchor. A second `outputs.iter().find_map(…)` is how they come to disagree.

**On the wire the tier is never published as a word — it is what the recipe's readings are TAKEN
AT.** A bone spear's offer quotes `250 blows` and the knapped one's `175 blows` because each reads
its own recipe's tier, not the item's default and not the best tier the faction knows.
`crafting_wire::each_recipes_readings_are_taken_at_the_tier_that_recipe_makes` pins that on a
four-tier fixture, where those three answers all differ.

### The anchor is DERIVED from the bench material's bare hand

**The rung pinned to the shipped item is the band that the recipe's bench material's
`hand_working.quality_ceiling` falls in** (`recipes_config::anchor_band`) — not a literal grade name.
The grade resolved there, *after inheritance*, must agree with **the tier that recipe makes**
(falling back to the item's default where it names none) for every stat it declares. That states the
invariant that actually matters: **a bare-handed craft off the best material a band can work by hand
reproduces that tier exactly**, which is *"a tool run dry drops the band back to the rate the game
already ships at rather than into a spiral"*. On the shipped config every hand-workable material
carries `quality_ceiling 0.60`, so the anchor is **`good`** everywhere; a material with no
`hand_working` at all has no anchor and no check.

> ⛔ **IT IS THE RECIPE'S TIER, NOT `tiers[0]`, and the difference is only visible with two
> recipes.** `spears` fans around `20` and `spears_flint` around `26`; pinning both to the item's
> default tier would demand a flint spear hit exactly as hard as a bone one, which is the whole thing
> the second tier exists to say. **Note the deliberate overlap it produces**: a *poor* flint spear
> (20) is exactly a *good* bone one. Craftsmanship and material are different axes and are meant to
> trade against each other.

**The migration was not a rename.** The old seams (`0.00 / 0.45 / 0.75`) do not line up with the band
cuts, so `good` holds the shipped number and the other three fan around it at
**`poor ×0.75 · fair ×0.85 · good ×1.00 · excellent ×1.15`** — the design's own sled example,
`30 / 34 / 40 / 46` on a shipped `40`.

`recipes_config::the_anchor_grade_reproduces_the_shipped_item_and_the_others_bracket_it` pins it in
both directions and carries a liveness assertion — *"every anchor grade agrees with its item"* is
trivially true of a book whose grades declare nothing, so the test also asserts the shipped book
genuinely **brackets** its anchor (a rung strictly below and one strictly above). Inheritance itself
is covered by a **fixture** (`a_band_a_recipe_does_not_declare_inherits_the_one_below_it`), for the
same reason `materials.json`'s varieties and `equipment.json`'s bronze tier are: no shipped recipe
declares a partial ladder.

### A START-STOCKED UNIT *IS* AN ANCHOR-GRADE CRAFT, and the ledger says which

The anchor has a second consumer: **a spawn stamps every batch it stocks with the anchor grade of the
recipe that makes that item** (`RecipesConfig::anchor_grade_for_item`, joined through the recipe book — the book is where an
item's crafted facts are written). That is a
statement of fact rather than a display default: a spawn stocks the item's **default tier**
(`equipment.md` → *"`plain` is the opening tier"*) and `validate_grades_against_item` requires
the anchor grade to agree with the tier each recipe makes, so the shipped spear already
*performs* exactly as an anchor-grade craft — the wire simply was not saying so, and the Materials &
Crafting ledger rendered a bare `×1` beside rows reading `×3 good`, which a player cannot tell from a
chip that failed to draw. On the shipped config the anchor is `good` everywhere.

**The NAME only — the effects payload stays empty.** A start-stocked unit's stats come from the tier
and must keep coming from there; the grade name is a label `validate` already ties to those numbers,
not a second home for them. An empty payload resolves through `LiveItem::effect_entry` exactly as
`None` does, which is what makes *"the shipped opening is unchanged"* structural rather than
measured. An item no recipe makes keeps no grade — there is no crafted equivalent to claim — and
every shipped start-stocked item has one, so that arm is unreachable today.

**The seam is `BandEquipment::start_stocked_owned`, and it is deliberately separate from
`start_stocked`.** The latter is also the **fresh reference ledger** every kit-scoring and
roster-quoting surface prices against (`kit_supplying`, `quarry_default_hunt_kit`, the published kit
roster, a launch forecast), where a grade *label* answers no question being asked and the recipe book
and materials table it would have to carry are two configs those passes do not read. The owning call
site is the band spawn (`systems::worldgen::spawn_population_entity`, via the `StartKit` bundle). A
detached party is no longer stocked here: its kit is TAKEN from its home band (`server::issue_party_kit`,
`.claude/rules/core_sim/band-carry.md`).

Pinned as a *pairing* by
`crafting_wire::a_start_stocked_batch_carries_the_grade_a_bare_handed_craft_of_it_comes_out_at`: the
spawned sled and one the band's own bench makes bare-handed off richer hide than the bare hand can
reach carry the **same** grade — asserted against `anchor_grade_for_item`'s own answer rather than
the word `"good"`, or the test would pass a hard-coded stamp.

## A material with NO CRAFT is one nothing works — and it is not the same as unreachable

`MaterialDef::craft` is `Option<String>`. **Absent means nothing works this material**, which is the
same statement `hand_working: None` already makes one level down and one rung further out:

| | `hand_working: None` | `craft: None` |
|---|---|---|
| says | not by hand | not by anything, yet |
| refuses by | a bench rate of `0` | not appearing in `crafts_declared_by` at all |

There is **no *"you cannot craft that"* branch anywhere**, exactly as there is none for the bare-hand
rate. `crafting::crafts_declared_by` filters `None` out, and a recipe's `craft` and every
`requires_knowledge` entry are validated against precisely that list — so a recipe *cannot name* a
craftless material's absent craft, and `RecipesConfig::validate_against` rejects a recipe that works
one outright rather than falling through an empty-string comparison. Nothing gains a knowledge track:
the coded set in `crafting::craft_discovery_id` is untouched.

**The three that use it are UNCRAFTED, not UNREACHABLE, and the distinction is the whole licence.**
`materials.json`'s roster note bans an unreachable material because it is dead content the catalogue
publishes. `tobacco`, `tea` and `grape` each have a **producer today** — the flora roster's three
cash crops pay them on every harvest, a band banks them, the store holds them, the catalogue lists
them. What does not exist is the craft that would turn a leaf into a cure or a grape into wine. The
wire says so with an empty `MaterialDefState.craft`, the `displayName` convention.

> **`grape` is dominated on both its axes** — `potency 0.20 / keeping 0.15` against tea's
> `0.45 / 0.80` and tobacco's `0.85 / 0.75` — which sits badly against this file's own *"there is no
> best material"* principle. **That is recorded rather than resolved.** A grape's value is entirely
> on the far side of a craft (fermenting) that has not been built, so inventing a third axis today to
> balance the roster would be inventing a number to fit a rule. **Open item**: when the luxury system
> lands, either the axes change or the grape gains the one it is actually good on.
>
> The axes themselves (`potency` — how strong the effect; `keeping` — how well it stores) and every
> reading on them are **provisional placeholders** for a luxury/boost system that does not exist.
> They are stated as such in `materials.json`'s `_comment_roster` rather than dressed as tuned values.

## The craft is DERIVED and then written down

`RecipeDef::craft` must equal the craft of the material the recipe **reads** (or, for a recipe that
reads nothing, of its first input), and `validate_against` enforces it. It is written in the file
because that is where a reader needs it, not because it is a second decision.

That is also what keeps a "primary material" field out of the model: the material a recipe **reads**
*is* the material the bench works — whose tool is consulted, whose bare-handed rate applies, and
whose craft is practised and taught. One field, four consumers.

## A recipe caps how many crafters work one item

`RecipeDef::crafters_per_item` (required, validated `>= 1`) is how many hands can work **one item**
of the recipe at once. It is a fact about the object, not the band: **1** for a thing one person holds
and shapes (spears, clubs, hoes, snares, the crook, wayfinding gear, the awl, the billet, the axe,
wedges), **2** for a thing big enough that a second pair of hands holds while the first lashes,
stretches or weaves (the sled, baskets, hurdles, the tanning frame, the loom, the two road tool sets).
Recipes that make the same item carry the same value.

An order's crew cap is `crafters_per_item × items still to make`, so a batch works its items side by
side and twenty crafters never land on one spear (`docs/plan_labor_at_scale.md` → "The bench: several
orders at once"). The bench that staffs orders against it is #873's; the one-order bench above works
its order with the player's crew and does not read it.

## Knowledge gates a recipe only when the recipe says so

`requires_knowledge` is a **list** and is absent on every ordinary kit recipe. You can make a sled by
hand on turn one; doing it is what teaches Tanning.

**A bench tool is gated on the craft of the material it BOUNDS (works), and on nothing else.**
`validate_against` requires a recipe whose output is a bench tool (an item with `bounds_material`)
to carry `requires_knowledge` exactly `[craft(bounded material)]`: the tanning frame (bounds `hide`)
requires `["tanning"]`, the loom (`fibre`) `["weaving"]`, the bone awl (`bone`) `["bone_working"]`,
the billet (`stone`) `["knapping"]`. The crafts of the tool's *inputs* are not required, so the
player reads "Loom: Needs Weaving", the craft the loom serves. The list type stays for a
non-tool recipe, which is validated as before (each entry must be the craft of one of its own input
materials; none ships one).

```text
tool bounds M  ⇒  M declares hand_working  ⇒  craft(M) is learnable by hand before the tool exists
```

That is why the gate cannot deadlock, and it is enforced rather than assumed: `validate_against`
rejects a tool whose bounded material declares no `hand_working` (`metal`, which a bare hand cannot
work, could not carry a tool until that changes). Paired with the rule that a tool is never made from
the material it bounds (also validated), a band works the material by hand, learns the craft by
making things from it, and then builds the tool. **Tools are earned, never a prerequisite**, and
there is no opening move where everyone builds tools first.

The offer's refusal follows with no extra code: `CraftOffer.queueable` and the `Needs Weaving`
reason read `requires_knowledge` through `crafting::first_unknown_craft`. Pinned by
`recipes_config::{a_tool_gated_on_the_craft_of_the_material_it_bounds_validates,
validate_against_rejects_a_tool_gated_on_an_input_craft, validate_against_rejects_a_tool_missing_its_gate,
validate_against_rejects_a_tool_bounding_a_material_with_no_hand_working}` and
`crafting_wire::the_loom_offer_names_weaving_until_it_is_known_then_queues`.

---

# The crafts are knowledge, on the ladder's own ledger

`crafting.rs` holds four discovery ids beside the ladder's own —
`TANNING_DISCOVERY_ID` (2008), `WEAVING_DISCOVERY_ID` (2009), `BONE_WORKING_DISCOVERY_ID` (2010) and
`KNAPPING_DISCOVERY_ID` (**2017**) — registered in `start_profile_knowledge_tags.json` so they are
*mappable*, and in `intensification::discovery_id_for` (which delegates to
`crafting::craft_discovery_id` for anything it does not name itself) so the ladder's own validator
can see them. **None ships known**, the same rule every ladder knowledge follows.

> ### ⛔ KNAPPING IS NOT QUARRYING, and the two ids sit one apart
>
> `quarrying` (**2016**) is the *ladder* rung that gets stone out of the ground — an
> `extraction` gate on a crew standing on rock (`extraction.md`). `knapping` (**2017**) is the
> *bench* craft that works that stone once it is in the store, earned the way every craft is: by
> making something out of it. A faction can hold either without the other, and they are priced
> separately in the ladder's `lesson_costs`. The ids being adjacent is an accident of when each
> landed, not a relationship.
>
> **2011 is retired and is not reused.** It was `trailcraft`; 2012–2016 are roadbuilding, paving,
> woodcraft, conservationism and quarrying, so 2017 is the next free number and a gap is safer than
> a renumber.

**Crafting is the fourth teacher.** Hunting teaches Herding and Penning, foraging teaches Cultivation
and Seed Selection, keeping tamed herds teaches Penning — and crafting teaches its own crafts. **The
lesson is the RECIPE's craft**, so what is being made decides what is learned, and a band that cannot
reach bone never advances Bone-working.

**The lesson and the tool's wear are charged on the SAME quantum: per item completed.** They are two
lines beside each other in `advance_crafting` for exactly that reason — split across two sites they
drift, and a tool that lasts 25 items ends up teaching a craft in 30. Pinned by
`crafting::the_lesson_and_the_tools_wear_are_charged_the_same_number_of_times`, which counts the
items finished and asserts *both* charges equal that count.

**The dial is `intensification_ladder.json`'s `knowledge.craft_lesson_per_item` (4.0 **practice
units** per item, against that craft's own `knowledge.lesson_costs` entry of 20 → 5 items per craft),
a SIBLING of `learn_rate` rather than a reading of it.** A ladder lesson is charged per turn worked
and scaled by the crew's floor; a craft lesson is charged per item, and there is no floor to scale it
by and no turn to charge it on. It lives on the ladder rather than in `recipes.json` so that **every
knowledge pace in the game is tuned in one file**, which is the reason the ladder's other dials moved
there in slice 4.

> **A CRAFT IS PRICED IN THE SAME CURRENCY AS A RUNG'S LESSON, and goes through the same divisor**
> (`docs/plan_unit_costed_work.md` §4). `credit_craft_lesson` credits
> `LadderKnowledge::ledger_credit(craft, craft_lesson_per_item)` — literally the call a rung's
> `knowledge_accrual` makes — so a bench and a rung cannot come to disagree about what a lesson costs,
> and **the ledger stays normalized**: the cost is a divisor at the seam, never a widening of
> `DiscoveryProgressLedger`'s `0..1` unit.
>
> **It moved with the currency rather than being left alone.** It was `lesson_per_crafted_item` `0.2`
> — a fraction of a normalized threshold — and leaving it that way while its sibling became a *cost*
> is precisely the drift the slice-4 consolidation existed to prevent. `4.0 / 20` is the same 5 items:
> the crafts' pacing did not move. **`crafting::CRAFTS_WITH_A_DISCOVERY` is what makes the pricing
> exhaustive** — `LadderConfig::validate` walks it and refuses to load a ladder that leaves a craft
> unpriced, because a defaulted cost is a pace nobody chose.

---

# The bench

`BandBench` is a component on a band: `{ orders, last_output_grade, last_started, finished, auto,
auto_skipped, kit_orders, next_kit_order }`. `orders` is the queue, in queue order, of
`BenchOrder { recipe_id, count, made, items, priority, crew, crew_cap, lent_to_hunt, auto, kit_order }`.
**Every workable order is worked at once** (#873): each order has its own priority, the **sim**
staffs it in the labor passes, an item's inputs are set aside the turn a crafter starts it, and a
tool is held by one crafter. Design: `docs/plan_labor_at_scale.md` -> "The bench: several orders at
once". There is no worked order, no player crew (`bench_crew`, `BandBench::workers`, `benchable()`
are gone) and no bench-wide priority; `BandBench::crew_total()` (the sum of the orders' crews) is the
one reading of "how many hands are at the bench".

## The queue

- **Every order has a count** (`MIN_ORDER_COUNT` = 1, refused below at the command boundary). `set_bench`
  (52) and `clear_bench` (53) are reserved in `command.proto`.
- **An order is workable** when it has an item in flight, or can start one now (every input for one
  item on hand at the tiers resolved for it), its recipe is in the book and its material can be worked
  (a tool is live, or the material's `hand_working` rate is above zero). An order short of an input
  with nothing in flight is not workable and **takes nobody**.
- **Rank order** - every per-order walk (staffing inputs, set-aside, shortfall forecast, material
  income, muster) goes priority High to Low, then queue order (`BandBench::ranked`). `bench_raise`
  still reorders the queue; it is the tie-break within a priority.
- **An order carries its items in flight** (`BenchOrder::items`, oldest first): each is an
  `ItemInFlight { progress, drawn }`, `drawn` being what was set aside for that item with the grade
  fixed there (`DrawnInputs::at_tool` says whether the bench tool was in hand, and gates the wear
  charge). `DrawnMaterial` carries the batches actually withdrawn so they can go back exactly.
- **Cancelling returns what the order holds.** `remove_order` / `remove_kit` hand the order(s) back and
  the handler calls `BenchOrder::return_set_aside` - every item's batches go back through
  `LocalStore::deposit_material` at their exact readings (two equal readings blend to themselves, so
  the store is what it was, to the bit). The event detail carries `pile_returned=`.
- **A finished item** (`BandBench::finish_item`) counts against `made`; `retire_done_orders` removes an
  order whose `made` met its `count` (with the kit-order bookkeeping), once per turn after every item.
- **The queue rides `BandRecord::bench`** (`SAVE_FORMAT_VERSION` 68): the orders, their items, priorities,
  crews and caps. `lent_to_hunt` is `serde(skip)` and `0` at every turn boundary.

| verb | does | refused when |
|---|---|---|
| `bench_enqueue <f> <b> recipe <id> count <n>` | appends an order (Normal priority); restaffs the band | unknown recipe, unlearned craft, `count` 0 |
| `bench_order_count <f> <b> order <i> count <n>` | sets one order's count | no order `i`, `count` 0, `count <= made` |
| `bench_remove <f> <b> order <i>` | takes one order off, returning its set-aside | no order `i` |
| `bench_raise <f> <b> order <i>` | swaps the order with the one above it | no order `i`, `i` is the head, `i` is a kit part |
| `bench_priority <f> <b> order <i> high\|normal\|low` | ranks one order (a kit part ranks its whole kit block) | no order `i`, unknown level |

Every bench command restaffs the band so the snapshot shows the new crews at once. They are
`CommandEventKind::Craft` and band-addressed with a **required** handle in `xtask`'s command guard.

## Staffing - orders join the passes like builds

Design: `.claude/rules/core_sim/work-party.md` (the #871 passes). Each **workable** order becomes a
`StaffRowInput` (`is_bench`) built by `staffing::bench_inputs` from `systems::bench_plan`:

| field | value |
|---|---|
| `priority` | the order's |
| `max` | `crew_cap` = `crafters_per_item x (items in flight + items it could start now)`, bounded by `count - made` |
| `kit_items` | the recipe material's bench tool item, one person per unit (one tool arms one crafter); empty when the material has none |
| `bare_can_work` | the material's `hand_working` rate > 0 |
| `distance` | 0 (the bench stands at camp) |
| keep / hold / reach / useful take | nothing |

**"Could start now" is computed sequentially in rank order against a clone of the band's store**, so two
orders cannot both be credited with the same hide (an item that cannot be supplied takes nobody).
Approximation: the startable count is struck at hand-working amounts (a tool's efficiency only lowers a
cost).

- **Never pass 1.** The bench has no hold crew; keepers, builders and hold crews run without it.
- **Pass 2 opens with ONE crafter on every workable order, at any priority** (High to Low, then queue
  order), before any group's pass-2 dealing: kitted when the shelf can arm one, else bare where a bare hand
  works. A Low order still gets its first crafter, and the tool goes to the higher priority.
- **Then, per priority group, the group's bench orders are dealt with the group's builds** (one
  round-robin set at distance 0, ahead of the group's sources): pass 2 kitted heads up to the cap, pass 3
  kitted then bare up to the cap.
- **The settle trim** (`trim_one_hand`) takes a bench order's heads as it takes a build's of the same
  priority; `StaffedBand::bench` (one `BenchStaffed` per queue index) is written by
  `StaffedBand::apply_bench`, which the turn, `restaff_band` and the preview all call.
- **The pool no longer nets the bench** (`staffable_pool(allocation, available)`); `BandWorkforce::benched`
  is `crew_total()` and `idle()` still means what pass 3 left over. The bench is zeroed before the
  starvation shed and refilled after it, so the shed never takes a bench hand (`ShedSubject::Bench`,
  `ShedStep::StallBench` and `BandBench::shed_one_worker` are gone).

## `advance_crafting` - per order, per item

For each order in rank order (so higher-ranked orders set aside first):

1. Hands = crew less this order's muster loan (the loan comes off the bare hands first).
2. **Seat the hands on items**: in-flight items oldest first, each up to `crafters_per_item` hands,
   tool-holders first; then, while hands remain and `made + items < count`, **start one more item** - tiers
   resolved with the tool if an unseated tool-holder remains (else by hand), every input checked, exactly one
   item's inputs set aside (all or nothing, `draw_pass`). If it cannot be supplied, stop starting.
3. **Accrue** per item `Σ hands x progress_per_worker_turn x craft_speed` through
   `systems::rate_per_turn(at_tool, by_hand, ..)` - the one authority the wire's `ratePerTurn` reads: a
   tool-holder at the tool's speed, a bare hand at the material's `hand_working.rate` (`0` for a material with
   no `hand_working`, which is the whole refusal - there is no "cannot craft" branch).
4. **Complete**: an item reaching `work` emits its outputs (equipment parked on `BandBench::finished`, materials
   into the store), charges one wear to the bench tool **only if the item was set aside at the tool**, charges
   one lesson and counts against `made`. Overflow past `work` is dropped.
5. **An unstaffed order draws nothing**: its items keep pile and progress and resume when staffed again.

`bench_material_rate` (the wire's material income and the labor pass's shortfall Alert) is
`order_rate / work x output amount` summed over the workable, staffed orders - empty for an order that
cannot start an item whatever crew is stamped on it.

## A hunting-by-need muster lends per order

Each staffed order is its own muster donor (`MusterBench::orders`: index, priority, crew), ranked by its own
priority and, within a tier, lending **before that tier's rows** (a bench pays into no account; later order
first). The loan is recorded on `BenchOrder::lent_to_hunt` (`MusterPlan::from_bench_orders`; `from_bench` is
the sum), read by `advance_crafting` as `crew - lent` and cleared. It never removes an order or touches an
item. On the wire it is `BenchOrder.lentToHunt` and `BenchState.lentToHunt` (the sum).

## What a completed craft delivers

`BandEquipment::stock(item, count, tier, grade)` is the delivery seam — **a new batch, never a merge
into one already standing**, because *"the next ten are their own batch"* is what keeps a fresh craft
from averaging into a half-spent pile. It is what ends `equipment.md`'s *"start-stocked and NOT
craftable"*, and a second sled made while the first is fresh is now genuinely a second sled.

### ⛔ Finished equipment is PARKED on the bench and stocked at the top of the NEXT turn

`emit_outputs` does not call `stock`. It pushes a fully resolved `FinishedBatch { item, count, tier,
grade }` onto `BandBench::finished`, and `systems::deliver_bench_output` — registered
`.before(TurnStage::Influence)`, ahead of every `BandEquipment` reader in the turn — drains it into
the ledger in completion order.

**Why: the bench runs after the tool settlement.** `advance_crafting` follows
`advance_labor_allocation`, which settles the band's tools (`plan_pool_tools`, the take-row kit
budget), so a tool finished on turn N cannot be issued before turn N+1. Stocking it at completion put
it in the store a turn before anything could hand it out: the published pool cards and keeping rows
read short beside a ledger that already held the hoe. Parked, **the turn a tool first shows in the
store is the turn it is first issued** — the one-turn lag is kept (the tool did not exist while that
turn's work was done), and the store and the issue agree. A finished **bench tool** works from the
next turn: the re-draw after a completion resolves tiers off the ledger as it stands, which does not
yet hold the parked tool, and the next turn's delivery lands before the next turn's bench.

- **Resolved at completion, delivered with no lookup.** The tier and the grade's absolutes are
  copied into the `FinishedBatch` exactly as they were stamped onto the stocked batch before, so a
  recipe retuned or a bench re-tasked between turns cannot change what arrives.
- **A queue edit never drops it.** No queue editor touches `finished` (nor `last_started`) — the
  items are made; only the next one is cancelled. The shed's
  `shed_one_worker` touches nothing but the crew.
- **It rides `BandRecord::bench`** with the rest of the bench (`SAVE_FORMAT_VERSION` 16), so a save
  or a rollback taken between the finishing turn and the next keeps it.
- **A splinter does not take it.** Fission's kit divide moves units out of the parent's
  `BandEquipment`, which does not hold a parked batch yet; the parent's bench keeps it and the parent
  is stocked with it next turn.
- **Materials are NOT parked.** A material output goes straight into `cohort.stores`, because the
  next pass's `draw_pass` runs immediately after and may chain on it.

Pinned through the real schedule by
`bench_delivery::a_tool_finished_this_turn_is_stocked_and_issued_on_the_next` (the ledger and the
encoded `poolToe` both without the tool on the craft turn, both with it the turn after) and
`::a_job_change_between_turns_still_delivers_the_finished_tool`; the bench-only harness in
`crafting.rs` runs `deliver_bench_output` at the top of each of its turns, in the schedule's order.

Three things the batch carries, each resolved at the moment of the craft:

- **`count` is `RecipeOutput::amount`.** A pass of a recipe that makes three makes three. The seam
  used to deliver one item's worth of *condition* however many the row named — invisible because
  every shipped equipment recipe makes exactly one, which is why the fixture in
  `crafting::a_completion_delivers_the_recipes_whole_output_amount` states an `amount` of three.
  `validate_against` now rejects a fractional equipment `amount`: a ledger that counts things cannot
  bank half a spear.
- **`tier` is the one the OUTPUT ROW names** (`RecipeOutput::tier`), because the recipe is what names
  the material and the tier is what the material buys — so the bone row banks a `plain` spear and the
  knapped row a `flint` one out of the same item definition. A row naming none falls back to
  `ItemDefinition::craftable_tier` off the same `DiscoveryProgressLedger` and the same completion
  threshold `bench_enqueue` gates a recipe on; `validate_against` makes the declaration mandatory on any
  multi-tier item, so that fallback answers only where the item has exactly one tier and the answer
  is that tier.
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
| `src/data/materials.json` | **The materials table** (loader `materials_config.rs`, env override `MATERIALS_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks. **`characteristic_bands`** — the shared rating vocabulary, `[{ name, from }]` ascending: `poor 0.0 · fair 0.30 · good 0.55 · excellent 0.80`. Retuning these re-partitions every batch on the map. **`materials`** — id → `{ craft, characteristics[], hand_working?, varieties?, weight }`. **`weight`** is required on every material — biomass-equivalent mass per unit, what one unit weighs on a porter's back — and is what a far deposit's work party divides the hunt's haul carry by (`work-party.md`); every shipped value is PROVISIONAL (`_comment_weight`: wood 2.4, stone 3.0, hurdles 10.1 derived from its 4 wood + 0.5 hide). Shipped: **`hide`** (tanning; `toughness`/`suppleness`), **`fibre`** (weaving; `fineness`/`strength`), **`bone`** (bone_working; `density`/`length`), **`stone`** (knapping; `hardness`/`workability`, `hand_working { 0.5, 0.60 }` — crafted since issue #736, see "A RECIPE NAMES THE TIER IT MAKES"), each `hand_working { rate 0.5, quality_ceiling 0.60 }` — plus the three **uncrafted** luxury crops **`tobacco`** / **`tea`** / **`grape`**, which name **no `craft`, no `hand_working` and no `varieties`** and carry the provisional axes `potency`/`keeping` (arc #527; see "A material with NO CRAFT is one nothing works"), and the two the material half of build-and-upkeep added (`docs/plan_standing_upkeep.md` §4.9 item 12): **`wood`** (weaving; `hardness`/`pliancy`, `hand_working { 0.5, 0.60 }`) and **`hurdles`** (**no `craft` and no `hand_working`**, axes `stoutness`/`span`), which is a woven fence panel a pen spends on its build pile and its upkeep rate. **Clay and metal still have no producer** until the minerals arc, and an unreachable material is dead content the catalogue publishes — but the roster holds **two** kinds of thing that are not that: the luxury crops are *uncrafted* (a producer, no bench) and `hurdles` are *crafted but never an input* (a bench, no recipe takes them). **`wood` and `stone` are ordinary crafted materials now** — the forestry and extraction branches produce them (`extraction.md`), a bench works both, and the opening loadout is a head start rather than the only way in. ⛔ **Nothing reads `hurdles`' axes**, which is deliberate: a quality axis on a fence panel invites *a better fence contains better*, the containment-scaling item 12 defers. **`hand_working` absent means the material cannot be worked bare-handed at all** (rate `0`, which is how metal will refuse itself with no branch), and the bare-handed ceiling belongs to the **material**, not to the absent tool. **`varieties` are parsed, validated, and none ships** — named presets over the material's own axes (`copper`, `bronze`), exercised by a test fixture for the same reason the bronze equipment tier is. **`validate` rejects**: an empty material table; a band list that is empty, does not open at `0.0`, does not strictly ascend, or carries a seam outside `0..=1`; a material stating a **blank** craft (omit the key to say nothing works it — an empty string would put an unnameable craft in `crafts_declared_by`) or no characteristics; a duplicate characteristic on one material; a non-finite or negative `hand_working.rate`; a `weight` that is not finite and positive; a `quality_ceiling` outside `0..=1`; a variety that omits an axis the material declares or names one it does not, or states a reading off the range. **No material declares a start stock and there is no key for one** — the block and its whole mechanism are deleted, so every material is produced or picked (see "NOTHING IS STOCKED AT SPAWN" below); `exact_axes_fault` survives as the recipe output's material arm. **The root is open (`_comment*` keys) and `MaterialDef` is CLOSED** — a mistyped `hand_workng` would silently make a material unworkable, while a stray key at the root can only be prose. |

| `src/data/recipes.json` | **The recipe book** (loader `recipes_config.rs`, env override `RECIPES_CONFIG_PATH`, `validate` inside `from_json_str`, cross-config `validate_against(&materials, &equipment)` at the `build_headless_app` seam). Two blocks. **`crafting`** — `progress_per_worker_turn` (**2.0**). **`recipes`** — id → `{ label?, craft, work, crafters_per_item, requires_knowledge[]?, inputs[], outputs[], grades? }`, where `crafters_per_item` is the hands one item takes at once (1 or 2 on the shipped book; see "A recipe caps how many crafters work one item"), where `label` is the word that tells a recipe from its siblings (`Bone` / `Flint`, `Fibre` / `Withy`, `Hide` / `Framed`, `Bone` / `Wood`, `Bone` / `Stone` on the clubs) — **required** on a recipe whose output another recipe also makes and **rejected** on a sole one (see "AN ITEM OWNS ITS NAME"); the name itself is the item's (`equipment.json`). `grades` is keyed by `characteristic_bands` NAME and carries only `effects` (there is no `when`), an input is `{ material, amount, variety?, reads? }` and an output is exactly one of `{ equipment }` or `{ material, characteristics }`. Twenty-three ship: **twenty-two make EQUIPMENT and one makes a MATERIAL** — `hurdles` is the material (4 wood + 0.5 hide / work 3.5, reading hide's `suppleness`, so its bench is `tanning`; see "`hurdles` ARE A MATERIAL"). The **bone/hide/fibre kit** is (`sled` 2 hide + 2 fibre / work 8; `crook` 0.2 bone + 1 fibre / 5, the animal web's build tool, reading bone's `length`; `hoes` 0.2 bone + 1 fibre / 5, the plant web's build tool — the crook, the hoes and `hoes_flint` all grade `build_work` on the standard spread; `baskets` 2 fibre + 0.2 hide / 6; `traps` 3 fibre + 0.05 bone / 6; `spears` 0.1 bone + 1 fibre + 0.2 hide / 6; `clubs` 0.3 bone + 0.2 hide / 4; `wayfinding` 0.1 bone + 0.2 hide + 1 fibre / 4), the two **road tools** (`earthmoving` 1 wood + 1 stone / 8, reading wood's `hardness`; `stone_dressing` 1 wood + 1 stone / 8, reading wood's `pliancy`), and the four **bench tools** (`tanning_frame` 2 wood + 3 fibre + 0.1 bone / 12; `loom` 0.3 bone + 1 hide / 14; `bone_awl` 0.6 hide + 2 fibre / 10; `billet` 0.2 bone + 0.25 wood / 9 — an antler billet, stone's tool, and the one made of the material stone is supposed to be replacing). Issue #736 added **three KNAPPED rows** making the `flint` tier — `spears_flint` 0.1 stone + 0.5 wood + 1 fibre / 7 and `clubs_flint` 0.2 stone + 0.5 wood / 5 reading stone's `hardness`, `hoes_flint` 0.2 stone + 0.5 wood + 1 fibre / 6 reading its `workability` — and **five SUBSTITUTES** that make the same item at the same tier with the same grades off a different pile (`crook_wood` 0.5 wood + 1 fibre / 5; `traps_wood` 3 fibre + 0.25 wood / 6; `baskets_withy` 1 wood + 2 fibre / 6; `sled_framed` 1 hide + 1.5 wood + 2 fibre / 8; `wayfinding_wood` 0.25 wood + 0.2 hide + 1 fibre / 4). **Bone and hide amounts are sized to the object against what an animal yields, fibre against what a bare gatherer earns, and the bench runs at 2.0 because a spawn owns no kit** — see the file's `_comment_work_and_costs` for the measured income figures. **Bone is a byproduct you cannot scale and wood and stone are labour you can** (bone ~0.09/turn for the whole shipped band against ~0.4 stone and ~0.3 wood per *worker*-turn on a deposit), so nothing costs more than 0.3 bone while a knapped spear spends 0.1 stone and 0.5 wood without apology. **`validate` rejects**: a non-positive `progress_per_worker_turn`; an empty book; a non-positive `work` or `amount`; a `crafters_per_item` of 0; a recipe with no inputs or no outputs; the same material twice in one recipe's inputs; **more than one input carrying `reads`**; an output naming both or neither of `equipment`/`material`; an equipment output stating characteristics, or a material output stating none; a duplicate output; grades on a recipe that reads nothing or outputs only materials; a duplicate stat in one grade's effects; **a sibling recipe with no `label`, and a `label` on the only recipe making its output**. **`validate_against` additionally rejects**: an unknown material, item, variety or axis; a `craft` that is not the craft of the material the recipe reads; a non-tool `requires_knowledge` naming a craft no material declares or one that none of the recipe's own inputs is worked by; a tool recipe whose `requires_knowledge` is not exactly `[craft(bounded material)]`, whose bounded material declares no `hand_working`, or whose inputs include the material it bounds; **a fractional `amount` on an equipment output** (a batch's `count` cannot bank half a spear); **a grade key that is not a declared `characteristic_bands` name, and a lowest declared grade that is not the FIRST band**; **a grade effect that names a stat the recipe's OUTPUT TIER does not declare, drops that effect's mass bounds or changes its `branch`/`rung`, or — at the DERIVED anchor band — disagrees with that tier** (see "ONE QUALITY LADDER"); and, for the output tier itself, **a `tier` the output item does not declare, a `tier` on a material output, and an ABSENT `tier` on an item that declares more than one** (see "A RECIPE NAMES THE TIER IT MAKES"). |

The two **yield edges** are rows on the rosters that own them — `fauna_config.json`'s
`hunt_yield.materials` (`fauna.md`) and `flora_config.json`'s `yield.materials` (`flora.md`) — and
their authoring rationale rides in those files' own `_comment*` keys, next to the numbers. The four
**bench tools** are rows on `equipment.json` (`equipment.md`), for the same reason: they are stocked,
worn and counted exactly like a spear, which is what makes *"band-local, consumable"* free.

> **The tuning panel may stage `materials` and `recipes` (`ConfigOverrideKind`), but only NUMBERS.**
> `config_override.rs` validates a staged patch through the kind's own `from_json_str` and nothing
> else — it holds one config's JSON, so it cannot run either cross-config check. A patch that renamed
> a material would therefore install cleanly and panic the **next New Game**, which is precisely the
> failure that module exists to prevent. Band seams, costs, `work` and grade seams are safe; an id is
> not.

## `hurdles` ARE A MATERIAL, AND `wood` IS THE ROSTER'S ONE STOCKED-AT-SPAWN ROW

**The test is whether the thing comes home with you** (`docs/plan_standing_upkeep.md` §2.7). A hoe
goes to the next field, so it is **kit**. A fence **stays in the ground**, so it is **material**: it
is spent by the `animal:pen` rung's build pile and its upkeep rate, and no number of hands replaces
it. `hurdles` therefore left `equipment.json` for `materials.json`, and the recipe's output row went
from `equipment` to `material` — the **only** shipped recipe that makes a material.

- **`hurdles` declare no `craft` and no `hand_working`** — the luxury crops' shape. `craft` names the
  craft that works a material **as an input**, and nothing takes hurdles as an input: they are
  consumed by an *improvement*, not by a bench. They are **uncrafted, not unreachable**, and the
  distinction is the same one `tobacco` / `tea` / `grape` already rest on.
- **`wood` is new, and it is `weaving`** — the hurdles recipe *is* a weave, and minting a
  `wood_working` would have needed a discovery id, a `lesson_costs` entry and a bench tool, none of
  which that slice was. Its axes are the roster's own opposed pair: a **hard** wood resists and will
  not bend, a **pliant** one weaves into withies and splits into long lengths.
- ⛔ **HURDLES' PAIR IS NOW THE ONLY ONE WITH NO READER.** `stoutness` / `span` exist because
  `RecipeOutput::material` and `MaterialDef` **require** a material to be rated, and a quality axis a
  pen *read* would invite *"a better fence contains better"* — the containment-scaling §4.9 item 12
  defers. **Wood's `hardness` / `pliancy` ARE read**, by the two road tools, and stone's
  `hardness` / `workability` by the three knapped kit rows; this bullet covered all four until issue
  #736 and now covers two.

### THE HURDLES RECIPE KEEPS ITS HIDE, AND THE `craft` FOLLOWS THE `reads`

The recipe is **4 wood + 0.5 hide**, `work 3.5` (3.5 worker-turns a hurdle at the 0.5 bare hand-working rate and `progress_per_worker_turn` 2.0; 21 for a pen's six), reading hide's **`suppleness`** — and the hide is
**load-bearing**. It is the book's **only** reader of that axis, and `_comment_reads_pairs`' pillar is
that each material's two axes are split between two recipes wanting opposite ends. A wood-only hurdle
orphans `suppleness`, makes the Thunder Mammoth strictly the best hide, and deletes a decision the
crafting arc built on purpose. A hurdle is woven withies **lashed with hide thong**, so keeping it is
both true and what preserves the pillar.

> **⛔ AND THAT IS WHY THE RECIPE'S `craft` IS STILL `tanning`, NOT `weaving`.** `craft` is **derived**
> — `RecipeDef::bench_material()` is the input row carrying `reads`, and `validate_against` rejects
> any `craft` but that material's own. So *"the hurdles are a weave off wood"* is true of the physical
> object and false of the **bench**: reading `suppleness` makes hide the bench material, and hide's
> craft is Tanning. **The two cannot both be satisfied** — a `pliancy` reading would make it `weaving`
> and orphan hide's axis — and the pillar wins. Wood's own two axes therefore have no reader today,
> which is the same standing hurdles' two have.

### ⛔ NOTHING IS STOCKED AT SPAWN — every material is PRODUCED or PICKED

A spawned band's `LocalStore.materials` is **empty**, and there is no mechanism by which it could be
anything else. Every material on this roster reaches a player one of exactly two ways: something
*produces* it (a yield edge on a plant or an animal, or a recipe that outputs it), or the player
*picks* it in the turn-one opening loadout (`start_profiles.json` `opening_loadout`,
`.claude/rules/core_sim/campaign.md` → "The opening loadout").

**`StartKit.materials` was never a stock, and still is not.** It is the materials *table*, carried
into the spawn so an equipment batch can resolve its anchor grade through
`recipes.anchor_grade_for_item`. It deposits nothing.

**A `MaterialDef::start_stock` block existed briefly and was deleted, mechanism and all** — the type,
the field, its `validate` arm, and `worldgen::start_stocked_materials`. It seeded `wood` and `stone`
per worker at spawn, which is now the loadout window's job, and it was **removed rather than zeroed**:
a dial no material declares reads as a tuning value and behaves as nothing, which is the trap
`start_stock_fraction` had already sprung once ("read *none* and behaved *one of everything*",
`.claude/rules/core_sim/equipment.md`).

- ⛔ **`wood` and `stone` have NO PRODUCER**, so an opening pick is the only way any band ever holds
  either, and a faction that spends its stone has no road above a dirt one until one exists. Forest
  foraging (wood) and quarrying (stone, issue #583) each carry their own tracker item.
- **A picked unit arrives at `OPENING_MATERIAL_READING` (0.5) on every axis it declares** — the middle
  of the band table, resolved through this table's own `band_key` exactly as a yield edge's row is.
  The store stores; it does not interpret.
- `core_sim/tests/materials.rs` pins both directions: a spawned band holds nothing, and every material
  a ladder rung eats is either produced or on some profile's pick list — an unreachable pile is a
  build that stalls for ever with nothing reporting why.

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

**The three luxury crops are the standing counter-example, and it is flagged rather than fixed**:
tobacco (`0.85 / 0.75`) dominates tea (`0.45 / 0.80`) on potency and grape (`0.20 / 0.15`) on both.
See the callout under "A material with NO CRAFT is one nothing works" for why that is honest today.

`per_biomass` follows a physical rule rather than a per-row judgement: **a small animal is nearly all
skin and a big one is nearly all meat**, so hide runs from a snow hare's `0.021` to a mammoth's
`0.006`. Bone runs the other way on `length` (a mammoth tusk at `0.95`, a hare's at `0.07`).

## The shipped costs against the kit clock

`plan_early_game_labor.md` puts a kit item's life at **~15–20 turns**, and the costs are set so a
band keeps its kit alive on roughly a tenth of its material income and spends the rest on tools and
stock. Measured against the shipped ~17-worker band split half-gathering, half-hunting Red Deer — so
~8.5 workers each way, reaching ~68 biomass of reeds and hauling ~45 biomass of deer per turn:

| | per turn | a kit item costs | its life |
|---|---|---|---|
| **fibre** (reeds `0.02`/biomass) | ~1.4 | baskets 2 fibre + 0.2 hide → ~1.4 turns of fibre | ~18.5 turns |
| **hide** (deer `0.014`/biomass) | ~0.6 | sled 2 → ~3.3 turns | ~56 turns |
| **bone** (deer `0.002`/biomass) | ~0.09 | spears 0.1 → ~1 turn | ~42 turns |

**The `its life` column is the FULL band on that one job**, not the half that banks the material —
it is `equipment.md`'s kit clock, where all 17 gather or all 17 hunt. The two columns therefore
answer different questions on purpose: how fast a split band *earns* an item, against how fast a
committed band *spends* one.

Bench time is sized to be quick, not binding: `progress_per_worker_turn` is **2.0** because a spawn owns
no kit (`start_stock_fraction` 0.0) and every early item goes through the 0.5-rate bare bench, so four
hands work a `work: 8` sled off in two turns bare-handed (4 × 2.0 × 0.5 = 4 a turn), against ~3.3
turns of banked hide. Bone and hide amounts are sized to the object against what an animal yields (a
Red Deer, body_mass 15, gives 0.03 bone and 0.21 hide: a spear point is 0.1 bone, ~3 deers' worth);
fibre is sized against what a bare gatherer earns (~0.03 a turn), so the basket costs 2. **A craft is ~5 items** (the ladder's
`craft_lesson_per_item` against that craft's `lesson_costs` entry), so ~25–40 turns of deliberate
work — the same order as one rung of the
intensification ladder — and the tool it unlocks is a further ~28-turn investment (a tanning frame is
2 wood + 3 fibre + 0.1 bone) that then pays back over the 25 items it lasts.

---

# On the wire — the refusal is RESOLVED here and RENDERED there

`snapshot/crafting.rs` is the whole publication (`docs/plan_crafting_and_materials.md` §7). It exists
because of one asymmetry: the sim's **resolution** has no *"you cannot craft that"* branch — every
refusal is a zero — but a bench that silently does nothing is not an answer, so the reason has to be
**published**. It is the same split `KitRoster.kit_offer` already makes for a snare against a Red
Deer, and the same rule `kitTiers` enforces: **a client must never re-derive a reason, a shortfall
number, a grade or a step-down.**

## Five fields on `PopulationCohortState`, and what each answers

| Field | Answers |
|---|---|
| `materialBatches:[MaterialBatchState]` | *what have I got* — one row per (material, band key) batch: `amount`, plus a `CharacteristicReading` per axis carrying **both** the exact value and its band name, in the material's **declared** axis order |
| `bench:BenchState` | *what is on the bench* — `workers` (**the sim's** crew, `BandBench::crew_total`), `orders:[BenchOrder]` (queue order), `auto` / `autoSkipped`, `lentToHunt` (the sum of the orders' loans), `kitOrders` and `setAside:[DrawnInput]` (per material, summed over every order's items in flight). **Everything about an order is on its `BenchOrder` row**: `recipeId`, `count`, `made`, `progress` (summed over items in flight), `drawn` (at least one item in flight), `priority`, `crewAtTool` / `crewByHand` / `crewCap` / `crewPerItem`, `itemsInFlight`, `displayName`, `teaches`, `work`, `ratePerTurn`, `outputGrade` (the oldest item's), `benchTool`, `drawnInputs`, `lentToHunt`, `blockedReason` / `blockedSeverity` (`No spare hands this turn` / `neutral` for a workable order with no crafter; the offer's `Short n material` / `danger` for an order short of an input with nothing in flight) and `shortToFinish`. The retired worked-order scalars (`recipeId`, `progress`, `work`, `teaches`, `blockedReason`, `shortfalls`, `drawn`, `outputGrade`, `ratePerTurn`, `drawnInputs`, `blockedSeverity`, `priority`, `worked`) are deleted from `BenchState`. |
| `craftOffers:[CraftOffer]` | *what could I make* — **one entry per recipe, always**, which the ledger folds into one row per thing made; with `available`, a resolved `reason` + `severity`, the `shortfalls`, the `outputGrade` a draw would select, `group`, `outputItemId`, `onBench`, and the one-row-per-item fields (`recipeLabel` / `makes` / `lasts` / `suggested` / `ownedAtTier`) |
| `equipmentBatches:[EquipmentBatchState]` | *what have I got, and how long will it last* — one row per **batch**, plus one `count: 0` row per config item the band owns none of, so the ledger is never missing a row |
| `craftSuggestions:[CraftSuggestion]` | *what should I make next* — appended last on the cohort; see "Craft suggestions — what to make next, ranked by who is going without" |

**`craftOffers` is the field that keeps the refusal out of the client**, and the reason vocabulary is
the contract. `reason` and `severity` are what a client renders — **not `available`**:

| situation | `reason` | `severity` |
|---|---|---|
| a material is short | `Short 6.0 hide` — the **number**, never "cannot craft" | `danger` |
| the craft is unknown | `Needs Bone-working` | `danger` |
| no tool and the material cannot be hand-worked | `No loom` (the bounding tool's display name, lowercased) | `danger` |
| two reasons at once | joined with ` · ` | `danger` |
| buildable | `Hide + tanning frame → excellent` / `Fibre, no loom → good`, with `· Hide costs −20%` appended when the tool saves material | `neutral` |
| buildable and this is a **first tool** | `Unlocks excellent hide work` — the band **this tool's own `craft_quality_ceiling` falls in** (`snapshot::crafting::unlocked_band`), never the top of the ladder | `good` |
| buildable but the item is untouched | **`Not needed yet`** | `neutral`, dimmed |

> **"Not needed yet" reading differently from a shortage is the entire point of publishing the reason
> at all.** One is a shrug and the other is a problem; a client deriving both from a boolean cannot
> tell them apart. Pinned as a *pairing* by
> `crafting_wire::not_needed_yet_and_a_shortage_are_different_strings_and_severities`, which asserts
> the two rows differ in **both** the string and the severity — asserting one row's wording alone
> would pass on a wire that said the same thing everywhere.

### `blockedReason` IS ABOUT THE ITEM BEING MADE, so a drawn pile cannot be short

**While the bench's pile is `drawn`, a material shortfall is not a blocked reason.** The materials
were withdrawn at the draw and are in hand; what the store is short of speaks to the **next** draw,
and cannot stop the item in flight. It is the same rule `output_grade` follows one field down — the
grade the pile *fixed*, not the one the next draw would pick — and `bench_state` now follows it in
both places (`snapshot::crafting::NOTHING_SHORT_STOPS_A_DRAWN_PILE`).

Two things can stop a job whose pile is already cut, and both survive:

- **`No one at the bench`** (`workers == 0`) — as true of a drawn job as of any other, and the one
  blocked reason that publishes at `neutral`: the sim staffs the bench, so it is a prompt about spare hands.
- **A zero craft rate** — the bounding tool wore out mid-craft on a material that cannot be worked
  bare-handed, so progress genuinely cannot accrue. This is the real *"a running job is stopped"*
  case, and it names the tool (`No tanning frame`) rather than saying *"cannot craft"*.

A knowledge refusal cannot arise on a drawn job at all: a craft is never un-learned.

**The shortage leaves the bench entirely rather than softening.** The ledger's own offer row for that
recipe already publishes `Short 0.7 bone` and is the right home for *"could I start another"* —
putting it in both places is how one fact acquires two homes and starts disagreeing with itself.
**`BenchState::shortfalls` keeps being populated either way**: it is honest data about the next draw,
and the client does not render it as blocking.

An **undrawn** bench is unchanged in every respect — a shortage is exactly why it has not drawn, and
that is the state the field exists for. Pinned as a *pairing* by
`crafting_wire::a_drawn_pile_is_not_blocked_by_a_store_too_poor_to_draw_again` (one store, read twice:
silent while drawn with the offer row still saying `Short …`, blocked once the pile goes back on the
shelf — a one-sided assertion passes on a bench that never reports anything), with
`::a_drawn_job_whose_tool_ran_dry_still_publishes_its_refusal` holding the surviving case.

> **The per-order rows below carry what the next three subsections say of `BenchState`.** Where they
> name `BenchState::ratePerTurn`, `::drawnInputs`, `::shortfalls` or `blockedReason`, read the same field
> on the order's `BenchOrder` row (#873): the rate is `systems::order_rate` (tool-holders at the tool's speed,
> bare hands at the material's `hand_working.rate`, net of the order's loan), `drawnInputs` is the sum of the
> order's items' `DrawnInputs::withdrawn`, and an order with an item in flight is workable, so a shortage never
> blocks it.

### A WORKER-TURN IS NOT A WORKER'S TURN, so the bench publishes the rate itself

`BenchState::ratePerTurn` is `workers × progress_per_worker_turn × craft_speed`, resolved through the
same `systems::rate_per_turn` `advance_crafting` accrues with. **`craft_speed` is the tool-or-bare-hand
join**, and bare-handed it is the *material's* `hand_working.rate` — **0.5** on all three shipped
organics. So a bench reading `3.0 of 6 worker-turns` with two crafters is three turns from done, not
one and a half: two hands deliver `1.0` a turn. Nothing else on the wire carries that factor, and
`kitTiers`' rule is that a client never re-derives a tool join, so a readout without this field can
only guess — and the reasonable guess is wrong by the bare-handed rate.

**It is a TERM, and there is deliberately no turns-remaining field beside it.** The finish estimate is
`ceil((work − progress) / ratePerTurn)` — exact arithmetic over three numbers all on this wire, which is
the boundary `yield-forecast.md` draws (*"where a closed form exists the sim ships the terms and the
client evaluates it"*). Two homes for one fact is how they disagree.

**`0` is a state, not an absence**: no crew, no recipe, or a craft speed of zero — the same zero that
*is* the refusal, published beside the words (`No tanning frame`) that say what to build. Pinned as a
pairing by `crafting_wire::the_published_rate_is_the_turn_of_progress_it_promises_bare_handed_and_tooled`
(a bare-handed and a tooled band on the same recipe publish **different** rates and each matches its own
band's observed progress — a wire that published `workers` alone matches neither) with
`::a_bench_that_cannot_accrue_publishes_a_zero_rate` holding the three zeros, each against the same
bench lifted off it.

### `drawnInputs` NAMES WHAT A REMOVAL DESTROYS, and it is the withdrawal, not the recipe

`drawn:bool` says a pile exists; it cannot say what is in it, and a destructive action that cannot
state its own cost is the opposite of *"a refusal names its number"*. `BenchState::drawnInputs` is
`DrawnInputs::withdrawn` — **the amounts the store really lost**, one row per input material in the
recipe's own input order, empty on an undrawn bench.

**It is neither the recipe's `inputs` nor a shortfall's `required`.** The tool's material efficiency
sits between the book and the withdrawal, so a tooled sled cuts 4.8 hide against a book price of 6, and
`required` is a statement about the **next** draw rather than the pile in hand. Pinned as a pairing by
`crafting_wire::the_published_drawn_pile_is_what_the_store_actually_lost`, which asserts the rows
against the store's own before/after totals on a **tooled** bench (so the book's number would fail) and
reads the same bench before its draw publishing an empty list.

## THE LEDGER SPEAKS NO TIER WORD — a tier is what a recipe's readings are taken at

The ledger is **one row per thing made** — an item, or a material for a stock recipe — under three
heads, **Kit · Bench tools · Materials**, with the columns **Item · Owned · Costs · action**. There is
no Tier column and there are no tier heads: an item with two recipes carries both of its tiers in one
row, so there is nothing to sort it under. **The sim publishes no tier word on an offer.**
`CraftOffer` carries no `outputTierName`, `outputTierRank` or `ownedNote`.

**What the tier still decides is every number an offer quotes.** `makes`, `lasts`, `ownedAtTier` and
the invitation's unlock band are all read at the tier **this offer's own recipe** makes
(`snapshot::crafting::made_tier`, through `RecipeDef::output_tier_id`, falling back to
`ItemDefinition::craftable_tier` for a single-tier item), resolved sim-side for the reason `kitTiers`
exists: a client that re-derived a tier would be a second copy of a join it cannot make correctly.

**The Owned cell carries no tier word.** It states how many the band holds and at what grades;
*which tier* those units are is answered per recipe in the recipes popup, by `ownedAtTier` (see
"THE LEDGER ROW'S FOUR ANSWERS").

**`BandEquipment::retired` stays keyed by (item, tier)**, because that is the fact `wear_item` holds
when it destroys a unit and a per-tier record can always be summed. Its one reader is `retired_of`,
which sums it for `equipmentBatches`' `Worn out` wording. **Nothing in the sim branches on it** and
nothing may — it must not become a repair discount. The checkpoint carries it for free, because
`BandRecord::equipment` clones the whole component.

**Nothing here is authored.** `group` (`kit`/`tool`/`stock`) is derived from whether the output item
declares a `bounds_material`; a craft's display name is `crafting::title_from_id`
(`clay_working` → *Clay-working*, underscores to hyphens, first letter up), which is why there is no
`display_name` beside a craft in `materials.json`. **An item's player-facing name is the one thing
here that IS authored, and it is the item's own**: `ItemDefinition::display_name` in
`equipment.json`, read through `EquipmentConfig::item_display_name` (see "AN ITEM OWNS ITS NAME").

## AN ITEM OWNS ITS NAME, and a recipe carries only what tells it apart from its siblings

The ledger is **one row per item**, with the item's recipes behind a link, so the name a row reads
under must belong to the item — every recipe for it has to agree, and with two recipes for spears a
name written on each recipe is two homes for one fact. So `equipment.json` gives every item a
required `display_name` (`validate` rejects a blank one), and `RecipesConfig::item_display_name` is
**gone**: every caller reads `EquipmentConfig::item_display_name` (the item's name, or its id for an
id the table does not carry).

A recipe keeps one optional word, **`label`** — `Bone`, `Flint`, `Withy`, `Framed` — which is what
tells it apart from the other recipes making the same thing. **The rule is two-sided and
`RecipesConfig::validate` enforces both halves**: a recipe whose output has **siblings** (another
recipe making the same item, or the same material) must declare one, and the **only** recipe making
its output must not. An optional-everywhere label would let a sibling ship unlabelled — two
`Spears (…)` links a player cannot tell apart — and a label on a lone recipe is a word the ledger
never shows. Siblings are grouped by `RecipeDef::row_key` (the equipment id, else the first material
id), which is the same key the ledger rows and the suggestion rule group on.

- **`RecipeDef::row_name`** is the item's own name — or, for a material output, the material id
  through `title_from_id`, since a material authors no display name.
- **`RecipeDef::full_name`** is `row_name (label)` where a label exists, the bare row name otherwise.
  It is what a *running* job is called: `BenchState.displayName` reads `Spears (Flint)`, because the
  bench holds one recipe and the player has to know which.
- **On the wire the name moved**: `CraftOffer.displayName` and `RecipeDefState.displayName` are the
  **row's** name (`Spears` on both spears offers), and each carries the recipe's own word in an
  appended `label` / `recipeLabel` (`""` on a sole recipe).

## THE LEDGER ROW'S FOUR ANSWERS — `makes`, `lasts`, `suggested`, `ownedAtTier`

Four fields appended to `CraftOffer` (`snapshot::crafting`), each resolved sim-side because a
client re-deriving them would need the grade table, the tier table, the wear table and the bench's
history:

| Field | Answers |
|---|---|
| `makes:string` | **What one unit of this recipe's output does**, as the headline stat and its noun — `26 attack`, `8 carry`, `+0.7 build work`, `2 tile vantage`. Read at the offer's own `outputGrade` where the recipe grades that stat, else at the tier the recipe makes, else the item's unequipped side quoted against the equipped reference (`equipped_reference`, with the scout vantage and expedition sight baselines from labor and expedition config — the same references the role cards read). `EquipmentStat::readout_noun` is the noun; `reads_as_addition` (only `build_work`) is what earns the `+`. Whole values print without a decimal. `""` outside the **kit** group — a bench tool's craft stats and a material output have no single headline |
| `lasts:string` | **How long one FRESH unit at that tier lasts**, in the item's headline use quanta (`175 blows`, `2500 biomass gathered`, `25 crafts`) — the same `quanta_phrase` and reference-job division the life gauge uses, so the two cannot quote one item in two units. Published on every equipment output, tools included; `""` on a material |
| `suggested:bool` | **Which of a row's recipes the row offers first.** Exactly one offer per row key is `true` |
| `ownedAtTier:int` | **Units the band holds at the tier THIS recipe makes**, summed across batches, so the recipes popup can say *2* beside flint and *1* beside plain. **`-1` (`OWNED_AT_TIER_UNATTRIBUTED`) means "not attributable to this recipe"** — see below |

**`makes` and `lasts` are why the pair `20 attack · 250 blows` / `26 attack · 175 blows` reads as a
trade rather than an upgrade**, which is the whole design of the brittle flint tier. A *poor* flint
spear quotes `20 attack` — the overlap "The anchor is DERIVED" names, stated on the row where the
player chooses.

### `ownedAtTier` is −1 on BOTH offers of a substitute pair

A per-tier count is honest only when the tier identifies the recipe. `spears` / `spears_flint` make
different tiers, so each offer's count is its own. `baskets` / `baskets_withy` both make `plain`, so
*"2 at this tier"* would be the same two baskets claimed by both offers — and a batch records its
tier, not which recipe made it. So an item whose recipes declare **fewer distinct tiers than
recipes** publishes `-1` on every one of its offers (`CraftOfferPlan::counts_by_tier`, decided per
capture from the book), and the recipes popup then **omits its Owned column** rather than repeating
the row's own total on each recipe line — which would claim identical baskets came from different
recipes. The rule is by declared tier rather than by recipe count so a two-recipe item at two tiers
still counts. The `-1` is the default at both doors that construct an offer — the fbs `= -1` and the
hand-written `Default` — because a defaulted `0` would read as *"owns none at this tier"*. **Serde
takes no default for it**: a serialized `CraftOfferState` missing the field fails like any other
missing field, rather than decoding silently as *unattributed*.

### The suggested recipe: the first workable order's, else last started if it can be made, else the first that can

*(This is which **recipe** a ledger row offers — distinct from the craft suggestions, which say which
**item** to make next.)* `onBench` is true on the offer whose recipe is the order the bench row
describes — the first workable order in rank order, else the head.

`mark_suggested` runs after a band's offers are built and picks, **per row**:

1. the offer that is **`on_bench`**, if any;
2. else the recipe the band **last started** on that row, if it is `available`;
3. else the **first available** recipe in book order;
4. else the last started one, even unavailable;
5. else the first in book order.

**The running job comes first because its own draw makes it unavailable.** A flint-spear job takes
the store's stone when the pile is drawn, so on the next capture `spears_flint` reads short while the
bone recipe is affordable. Without the first rung the suggestion moved to bone, and the Spears row
read *On the bench* beside bone's costs while a flint spear was being made
(`crafting_wire::a_running_job_stays_the_suggested_recipe_after_its_draw_empties_the_pile`).

**The history is `BandBench::last_started`** — a `BTreeMap` from row key to recipe id, written
**only** by `bench_enqueue` (`record_started`), so a suggestion reflects a choice the player made and
never one the sim inferred. Removing orders **keeps** it — an emptied queue forgets its orders, not
the band's habits — and it rides `BandRecord::bench`, so it survives rollback and a save
(`SAVE_FORMAT_VERSION` 11; `save_round_trip::a_bands_last_started_recipes_survive_the_round_trip`).
The rule lives in the sim because *"available"* is the sim's resolution and a client choosing the
default would be the second copy of it; `crafting_wire` pins each of the five arms.

**The grade an offer quotes is the grade the bench will fix.** `systems::crafting::preview_grade`
runs the same two steps the draw runs, in the same order — the store's own worst-first spend order
(`LocalStore::preview_take_material`, which shares `spend_order` with `take_material`) and then
`min(reading, ceiling)` against the recipe's seams. Anything less shared would let the panel's grade
change the moment the player pressed Make.

## `equipmentBatches` — the life meter is a fuel gauge

`life` reads in the item's **own use quanta** and never in percent: a spear at 34% is exactly as
deadly as one at 100%, so a single percentage bar would draw a taper the model does not have. The
quantum's noun is `WearQuantum::noun` — a sled that wears per `biomass_hauled` reads *biomass
hauled*, a spear (and a club, which shares the quantum) per `strike` reads *blows* — resolved
**sim-side**, because a client mapping the enum to English would be a second copy of that table that
a new quantum would not update. A *count* quantum gets a count noun; a
*continuous* one keeps its own unit (`biomass hauled`), because a "biomass" is not a countable event
and a turns conversion would need a forecast of what the band is about to do.

> **AN ITEM ON SEVERAL QUANTA IS QUOTED ON ITS FIRST** (`ItemDefinition::headline_wear`, issue #515 —
> `equipment.md` → "An item may wear on SEVERAL quanta"). A gauge reads in **one** unit, so an item
> on two — a pile of hurdles was worked both at a slaughter and on a `Tame` — has to pick one:
> `≈12 gardens' worth` and `≈2500 biomass butchered` are the same condition counted two ways, and
> stating both would need the readout to know the usage mix it exists to let the player choose. The
> item **declares** its headline by writing that quantum first, exactly as `tiers[0]` declares the
> default tier. **The `sled` is the item on two quanta today**: it took `biomass_collected` when
> hurdles became a material, **appended**, so its row still reads the haul.

> #### THE BUILD QUANTUM IS THE ONE WHOSE UNIT IS NOT ITS NOUN
>
> Since improvements were priced in work (`docs/plan_unit_costed_work.md` §6.3),
> `WearQuantum::BuildProgress` is charged per **work unit** — one worker-turn at the food peak — while
> a player holding a hoe thinks in *builds*. `100 / 0.16 = 625` is arithmetically right and says
> nothing, so the gauge quotes it against a **named reference job**: the noun is *"gardens' worth"*
> and `quantum_units_per_noun` divides by the `plant:tended` rung's own `work_cost`, giving
> **"12 gardens' worth left"** — for the first item that **headlines** the build quantum. **No
> shipped item headlined it until the hoes landed**, so the arm is *also* exercised by
> `the_build_quanta_are_quoted_against_the_ladders_reference_job`; the **hoes** were the first item to
> lead with it, and the **crook** beside them leads with it too, so both publish rows that divide.
>
> **The reference is the LADDER's** (`intensification::REFERENCE_BUILD_RUNG`), carried on
> `BandCraftInputs::reference_build_cost` and resolved once per capture — so a retune of the rung
> moves the readout with it rather than leaving a literal behind. The conversion lives at the readout
> because `WearQuantum` knows nothing about rungs, and it is `1.0` for every other quantum, whose unit
> already *is* the thing being counted (a kill is a kill).
>
> **The free win this pays out:** because the charge is per work unit, **a bigger job eats more
> gear with no per-improvement authoring** — a 75-unit Sow burns 1.5× what a 50-unit Cultivate does,
> and a Steppe Runner's 250-unit `Tame` 5× a rabbit's.
>
> **So the gauge is accurate under one usage assumption, not unconditionally** — a band splitting its
> handling gear between a pen and a climb runs out sooner than the pen-only count says. That is the
> limit every rate-to-range conversion has; the alternative, a row per quantum, is a wire and readout
> change #515 deliberately did not make.

Five wordings: `Untouched` · `48 blows left` · `~1 blow left` · **`Worn out`** · **`Never made`**.

> ### `Worn out` and `Never made` both read `count 0`, and telling them apart needed STATE
>
> A batch that runs out of units is **removed** from `BandEquipment`, so *"the sled broke"* and
> *"we have never had a sled"* were the same empty ledger — and they are not the same sentence to a
> player. `BandEquipment::retired` is the readout's memory, incremented by `wear_item` — the one seam
> that destroys a unit — and summed here by `retired_of`. It is keyed by **(item, tier)** — see "THE LEDGER
> SPEAKS NO TIER WORD". **Nothing in the sim branches on it** and nothing may — it must not become a repair
> discount. The checkpoint carries it for free, because `BandRecord::equipment` clones the whole
> component.
>
> Deriving it from config instead (*"a start-stocked item at count 0 must have worn out"*) is right
> for kit items and **wrong for a tool the band built and then wore out**, which is exactly the case
> a bench introduces.

`lifeSeverity` (`healthy`/`warn`/`danger`) comes off `equipment.json`'s **`life_readout`** seams,
which are fractions of *one fresh unit's* quanta rather than absolutes — a spear's life is 250 blows
and a sled's is 5000 biomass, so any single absolute count would colour one of them permanently red.

> ### ONE LIFE FRACTION, TWO READERS — the panel row and the dock line
>
> `snapshot::crafting::batch_life_fraction` is the **only** producer of *"how much of one fresh unit
> is left"*: `(count × starting_durability − wear) / starting_durability`, **per batch**, at **that
> batch's own tier**, and deliberately **unclamped** — a stockpile of two hoes seven tenths through
> the first reads `1.30`, because the gauge is for the pile and not for the unit in hand.
> `equipmentBatches` colours a row with it, and `systems::labor::kit_life_fractions` rolls it up to the
> item's **worst batch** for the kit-life notification's edge gate, so the dock fires exactly when a row
> on the panel turns amber.
>
> A second reading of the same idea drifted three ways at once on ordinary ledgers: taken at the
> item's **default** tier rather than the batch's, **summing** `wear` across batches (two half-worn
> ones read `0` — a `danger` Alert on mostly-fresh gear), and clamped to `[0, 1]` (that `1.30`
> stockpile announced as `0.30`, *warn*, beside a panel reading *healthy*).
> `the_dock_and_the_ledger_read_one_kit_life` sweeps all three arrangements.

**`KitItemCondition` gained `count`, and `remaining == 0` no longer means "dry".** Since the count
slice an absent entry is NOT OWNED, so an item the band has none of reads `remaining 0` — the same
`0` a pre-count reader took for *"dry"*. `count` is the explicit ownership statement so nothing has
to infer ownership from a condition of zero; *worn out* versus *never made* is `equipmentBatches`'
job.

## A bench that can draw nothing promises nothing

`systems::crafting::bench_material_rate` is the **one producer** of the bench's per-turn material
output — `rate_per_turn / work × the output's amount`, struck through the same `rate_per_turn`
`advance_crafting` accrues with, so the projection and the accrual cannot describe different benches.
It is the forward half of a band's material income (`intensification.md` → "THE BAND'S LEDGER"), read
by the wire's `materialUpkeepIncome` row and by the material-shortfall Alert.

⛔ **It is empty unless the bench would actually bank this turn.** `advance_crafting` skips a bench
whose pile is neither **drawn** nor **affordable** (`pass_is_affordable`, the availability half of
`draw_pass` — factored out so the projection asks the question the draw will ask), so the three gates
are, in that order: a recipe the book still carries, a crew, and a pile it can cut. A rate published
without the third is income that never arrives: a band with `hurdles` queued and no `wood` banks zero
for ever while the ledger read ~`0.29`/turn, which drove `materialUpkeepIncome` above
`materialUpkeepNeed`, printed a runway of `∞` and left the caret untinted in exactly the state the
standing bill exists to announce. `a_bench_that_cannot_draw_its_pile_publishes_no_income` pairs the
projection against `advance_crafting`'s own progress on both halves.

**A pile already cut keeps the bench running** even where the store could not fund a *second* pass —
the gate is on the draw, not on the progress.

## The three per-world catalogues, plus the learned one

`SubsistenceSection` gains `materials` (id, craft, axes **in declared order**, hand-workability and
its two readings, the tool that bounds it), `characteristicBands`, `recipes` (the static half — the
band-relative half is `craftOffers`) and `craftKnowledge` (per faction per craft: `known`, `progress`,
`completionThreshold`). The first three are `Whole<…>` baselines like the kit roster and re-send only
on a world rebuild; **`craftKnowledge` is not**, because a craft is *learned*.

**`craftKnowledge` carries one row per craft for the viewer whether or not the ledger holds anything
for it.** A people that has learned nothing has no `DiscoveryProgressLedger` entry, and its crafts
read unknown at `0`. Keying the rows off the ledger's factions publishes no craft list at all on a
fresh world, since no start profile grants knowledge.

**The rating vocabulary rides ONCE, at the section, not per material** — a deliberate departure from
the spec's sketch. It is one vocabulary, and every published reading already carries its own band
*name*, so a copy per material row would be a second home for one fact and the row would never be
read.

**None of this goes in `equipmentConfigJson`.** That blob is the Workbench's designer catalogue with
no gameplay consumer; a gameplay readout gets a typed field of its own, or the blob becomes a second,
untyped wire contract.

## What it costs, and how it stays cheap

`craftOffers` is **bands × recipes**, so everything that is a function of the recipe alone is hoisted
into a `CraftOfferPlan` resolved **once per capture** (`plan_craft_offers`): the group, the bench
material, the tool that bounds it, the material's own word. Known crafts are memoized **per faction**,
not per band. Inside a band, `BenchTiers` is memoized **per material** — three resolutions for ten
recipes — and each offer costs one store total per input row plus one preview draw over one
material's batches. Nothing re-walks the item table or the recipe book per band.

The whole cohort row is diffed by `PartialEq` (`Indexed<u64, PopulationCohortState>`), so a band whose
store, bench and ledger are unchanged diffs out entirely. `equipment.md` records capture going from
49.51 ms to 3.15 ms when the estimate tables were retired; this arc adds no per-frame table.

## Craft suggestions — what to make next, ranked by who is going without

Design: `docs/plan_crafting_and_materials.md` §7, "Suggestions". **`craft_suggestions.rs` is a pure
function over a band's own state** — `craft_suggestions(lines, bench, recipes, equipment)` — so the
panel, the AI's Craft specialist (#668) and auto-craft (#779) read one list.
`band_tool_shortfall_lines(allocation, take_rows, equipment, wear)` builds its input, and
`band_suggestions` is the entry point that builds the lines and ranks them in one call.

**The sources are the consumers that draw on the band's stock each turn**, read off what the turn
**settled**, never re-derived:

| source | read off | `kind` / `job` on the wire |
|---|---|---|
| a standing pool | `LaborAllocation::last_pool_toe` (`poolToe`) | `pool` / the pool token |
| a site crew | `LaborAllocation::last_keeping_issued` — each `KeepingIssue` carries `required` beside the `units` issued | `site` / the row kind keeping it (`forage` / `hunt` / `extract`) + target keys |
| a take row | the row's kit lines from `craft_suggestions::take_row_gear` — the very lines `kitToe` is copied from; a camp-kill hunt row's lines cover only its kill-bearing items and its spear ask is its hold crew when bare hands cannot wound (`work-party.md`) | `take` / the row kind (+ target keys; none on `scout` / `warrior`) |
| a declared build | `LaborAllocation::last_build_toe` (`BuildToeLine { source, item, required, filled }`) — what the build's own builders claimed beside what the settlement gave them (`buildToe` on the wire) | `build` / the build's kind + target keys |
| the bench's own lined-up work (a **bench tool**, see "A bench tool that pays for itself") | `craft_suggestions::bench_tool_suggestions` — the bench queue plus the suggestions above it | `bench` / no job; the line's `material` is the material the tool bounds |

`KeepingIssue::required` exists for this: `keeping_issued` used to write only what was **paid**, so a
claim the settlement reached with nothing had no line at all. It now writes one line per claimed item
(`units 0` where unfilled) plus any unit paid against no claim; every reader that sums `units` reads
those zeros as the zeros they are.

- **A need row's crew is a second, hunt-turn pass.** A hunting-by-need row stands at zero hands, so
  the standing lines never see the kit its crew would claim. For a band whose need row has a **planned crew**
  (`LaborAssignment::planned_crew > 0`: the meal-sized crew the labor pass priced at its foot,
  **whether the row is sending or still waiting, and before the equipment cap**
  (`NeedAssessment::wanted_crew`) - a band short of spears still asks for the spears the wanted crew
  needs, however few hands it can equip and send, and **even a band holding none of the kit on a
  quarry bare hands cannot wound** (its `wanted_crew` is sized as if every hand were equipped -
  `work-party.md` -> "Hunting by need"; `mustered_take_gear` strikes the need row's claim with the
  kit in hand when the real one is nothing, so its `kitToe` line states the missing spears) - so the kit is flagged ahead of the hunt and the
  band can make it in time), the allocation is cloned, that crew is mustered through `LaborAllocation::muster_donors` (kit-carrying donors bring
  their kit - `work-party.md` -> "Hunting by need"), every row's claim and the item budget are struck
  again, and the take rows' lines are built a second time
  (`snapshot::population::hunt_turn_lines` -> `hunt_turn_shortfall_lines`, through the same
  `take_row_gear` and `band_tool_shortfall_lines`). `band_suggestions` takes that pass as its
  `hunt_turn` argument and ranks over both readings with `craft_suggestions_over`: **each item's
  shortfall is the larger of the two, never the sum** (the same people on different days); the
  winning pass's sources are listed and `queued_units` is netted as before. **The panel and
  auto-craft both call `band_suggestions` with the pass**, so a kit a going-out hunt crew is short of
  is in the list auto-craft works down: it queues it like any other top suggestion (the offer marked
  `suggested` for the item), and a need row with no planned crew adds nothing to either list. The
  turn's own kit split is unchanged. The Gear readout (`kitItemConditions` /
  `workersOnQuotedJob`) is deliberately **not** maxed: it states how far the gear reaches the rows
  as they are staffed, not what to make. Pinned by
  `hunt_by_need::{a_hunt_crews_missing_kit_is_suggested_though_the_standing_rows_are_kitted,
  auto_craft_queues_the_kit_a_hunt_crew_is_short_of,
  a_waiting_rows_planned_crew_is_equipped_ahead_of_the_hunt}` (each with its control) and
  `craft_suggestions::tests::the_larger_of_two_passes_wins_per_item_and_they_are_never_summed`.
  **The same mustered pass is published on the rows.** While a crew is going out
  (`muster_crew > 0`) the turn crews the need row before the kit settles, so the capture publishes
  every take row's `kitToe` from that mustered gear (`snapshot::population::mustered_take_gear`, the
  one computation `hunt_turn_lines` reuses, never derived twice): the need row's line is its crew's
  claim, the donors' and other hunt rows' lines what they hold that turn. A waiting row
  (`muster_crew == 0`, even with a `planned_crew`) keeps the standing reading, and `craftSuggestions`,
  `kitItemConditions` / `workersOnQuotedJob` and auto-craft are unchanged. Pinned on the encoded frame
  by `hunt_by_need::a_crew_going_out_publishes_the_shortage_on_the_rows_and_a_waiting_row_does_not`.
- **The score is workers going without** — `(required − filled).max(0) × workers_per_unit`, summed
  over sources.
- **The order** (`craft_suggestions::rank_of`): every **non-build** shortage first (a pool, a site
  crew, a take row), by workers going without, ties by item id; then **the declared builds**, by
  builders going without, ties by item id. An item short in more than one place is **one**
  suggestion: its count is the total (the queue netting applies to that total) and it ranks at its
  **earliest** position, so hoes short now at a site and later for a build rank with the non-build
  shortages.
- **Every declared build asks for its own crew's kit.** The settlement strikes, for each build, its
  in-flight rung's `pool_toe` × that build's builders through `ToeClaim::of`, and parks the lines on
  `LaborAllocation::last_build_toe` (cleared each turn, riding `BandRecord::labor`). The rung comes
  from the build's own quote (`BuildQuote::legs`, first leg). A build with no builders, or no quote,
  claims nothing.
- **Stock left after the settlement** is `BandEquipment::live_units` (the reading
  `settle_pool_tools` itself uses) less every unit the turn issued — the `filled` of the pools'
  (the builds', sites' and take rows' lines, all already in the line list
  (`craft_suggestions::build_lines`; nothing re-derives the settlement).
- **A declared build short of the GOODS it draws is a suggestion too** (`craft_suggestions::
  build_material_suggestions`, merged by `merge_build_material_suggestions` into the one ranking).
  The declared builds are walked in declaration order against the store: each build's **remaining** pile
  (`systems::remaining_build_materials`: pile x the share of its climb still owed, off the same legs
  the settlement draws - a pen 64% built owes 36% of its hurdles; a road or tile improvement is quoted
  its whole pile) is added to a running need, and the part the stock does not cover is that entry's
  shortfall. Every line is a `build` source carrying the build's source, so the job can be named. The suggestion is emitted only when a recipe makes
  the material; `count` is whole passes of that recipe covering the shortfall, less what the bench's
  queue owes; a holding under `MATERIAL_DUST_UNITS` is nothing. Its words are the sim's:
  `CraftSuggestion.displayName` (an item's name or a material's title, set for every suggestion) and
  `reason` (`for Corral Wild Boar` on a build-material suggestion, empty for a kit one).
  `CraftOffer.outputMaterialId` (appended) is the join key onto the offer, as `outputItemId` is for
  equipment, so `shortfalls` fill and a client finds the queueable craft; the panel's manual Queue sends
  `bench_enqueue` with that offer's `recipeId`, which the handler accepts (it checks the recipe exists
  and its craft is known, never what the recipe outputs). Auto-craft reads the same list (below).
- **A detached party is not a source.** `population_state` publishes an empty list for any cohort
  with an `Expedition`: it carries the kit it left with and is never resupplied. A far work party is
  a take **row** of its home band, so it counts.
- **A spent unit counts and a worn one does not** with no code for either: `filled` is what the
  settlement issued from stock, an expired unit is gone from stock, and a worn one is issued like any
  other.
- **The count is the WHOLE shortfall in units, never capped by affordability**, less what is
  already coming from the bench (`queued_units`): what the queue still owes — `(count − made) × the
  output amount` over every order whose recipe makes the item — **plus the units parked on
  `BandBench::finished`**. The parked ones count because the labor pass settles tools *before* the
  bench runs: on the turn an item completes the lines still read it missing while its order has
  already counted it made, so netting the queue alone raised the suggestion by the item just made
  (and kept it there when the order popped). Next turn it is delivered, issued, and gone from both. The shortfall is `ceil(Σ missing − WHOLE_UNIT_TOLERANCE)`, so float noise never asks
  for a second tool; a netted count at or below zero drops the suggestion.
- **Work a turn recovered** is stated only where the gear adds build or keeping work — on pool and
  site sources, `workers without × build_work_per_worker(item)`: the `build_work` **equipped** value
  at the item's **default tier** (tier layer beats item layer), the largest across the branches it
  serves. A take row's kit buys attack, carry or reach — food, not work — so its line reads `0` and
  the client states the people instead.
- **Numbers and join keys only.** The client owns the words, as with `craftOffers`, and joins on
  `itemId == CraftOffer.outputItemId` for makeability; an item no recipe makes is still published.
  A `build` source line carries the build's own keys — `job` is `BuildSource::kind()`, with
  `targetX/Y` / `faunaId` / `material` — which is how the client names the job ("2 builders, for
  Tame Wild Boar") from the site row it already holds.
- **Auto-craft needs nothing here**: it takes the top usable suggestion, so a declared build's tools are
  crafted ahead of the build needing them (`auto_craft::auto_crafts_the_tools_a_queued_build_will_need`).

### A bench tool that pays for itself

No consumer is ever short of a bench tool (it is a multiplier on the bench, not a claim on a worker),
so the shortage lines never suggest one. `craft_suggestions::bench_tool_suggestions` adds it by a
different rule, run after the kit and build-material suggestions are merged (the snapshot in
`snapshot::population` and auto-craft in `auto_craft::band_suggestions_and_offers` both call it, so
the panel and auto-craft read one list). For each tool `T` bounding a material `M`, **skipped when**
the band holds a live `T` (`EquipmentConfig::live_bench_tool`) or has `T` queued or parked
(`queued_units`):

- `W` = the bench work lined up on recipes that **read** `M` (`RecipeDef::bench_material() == M`):
  the queued orders' remaining work (`remaining() × work − progress`) plus, for every suggestion
  already on the list whose suggested recipe reads `M`, that recipe's `work` × the passes the count
  needs (`ceil(count / output per pass)`). The suggested recipe is the one the ledger marks
  `suggested` for the item (the offers), else the first in book order. The suggestions' work is
  struck before any tool is added, so one tool's own recipe never makes another look worthwhile.
- `r_bare` = `M`'s `hand_working.rate`; `r_tool` = `T`'s `craft_speed` at the tier its recipe makes
  (`ItemDefinition::tier_craft_stat`). A tool no faster than the bare hand is never suggested.
- Turns saved, crew-free: `S = W × (1/r_bare − 1/r_tool)`. Cost: `C = T's recipe work ÷ output per
  pass ÷ the rate the bench would work it at` (`systems::bench_tiers` speed for the recipe's own read
  material: its live tool, else the bare hand). The crew and `progress_per_worker_turn` scale both
  sides alike and cancel.
- **Suggest `T` iff `S − C > BENCH_TURNS_TOLERANCE` (1e-3, float noise only).** No config lever: every
  number is already on `recipes.json`, `equipment.json` and `materials.json`. `count` is `1`.
- **Placement:** directly ahead of the highest-ranked suggestion whose recipe reads `M`; when none
  does (the work is all queued) at the end of the list. The list is not re-sorted afterwards.
- **Knowledge is not filtered** — as for every other suggestion, a gated tool is published (the offer
  says `Needs Tanning`) and auto-craft passes over an unqueueable one.
- On the shipped book the tanning frame (hide 0.5 → 2.0, built at fibre's 0.5: 24 turns) pays for
  itself from three sleds of lined-up work (36 saved), not from two (24, a tie).

The suggestion carries `benchTurnsSaved` (`S`) and `benchBuildTurns` (`C`) and a source line of kind
`bench` whose `material` is `M`; `workersWithout` and `workPerTurn` are `0`.

Pinned by `craft_suggestions::tests::{a_tool_whose_turns_saved_exceed_its_cost_is_suggested,
a_tool_that_does_not_pay_for_itself_is_not_suggested,
suggested_work_on_the_material_counts_beside_the_queue, a_held_or_queued_tool_is_not_suggested_again,
the_tool_sits_ahead_of_the_first_suggestion_reading_its_material}`, off the encoded frame by
`crafting_wire::a_bench_tool_that_pays_for_itself_is_published_with_its_turns`, and through auto-craft
by `auto_craft::a_tool_that_pays_for_itself_is_auto_crafted_and_a_little_work_gets_none`.

### Coming up - the kit that wears out before a new one could be made (#778)

Design: `docs/plan_crafting_and_materials.md` -> "Coming up". The list is one list: an item that is
only wearing out is a `CraftSuggestion` like any other, with empty `sources`.

- **The rate is recorded at the one wear seam.** `BandEquipment::wear_item` adds the condition it
  charged (`usable_uses(uses) * wear.amount`, only when a batch in condition took it) to the newest
  slot of `BandEquipment::wear_window`, a `VecDeque<BTreeMap<item, f32>>` with one slot per turn.
  `systems::open_wear_slots` (registered `.after(deliver_bench_output).before(TurnStage::Influence)`,
  after it only because both write the ledger) opens each ledger's slot at the top of the turn and
  drops slots beyond `equipment.json` `wear_forecast.window_turns` (**10**, validated `>= 1`).
  `BandEquipment::wear_rate(item)` is the item's sum over the recorded slots divided by the slots
  recorded (never more than the window); a ledger with no slot, or an item never charged in the
  window, reads `0` and is not forecast. `wear_item` opens a slot itself when the window is empty, so
  a fixture driving the seam directly records without the system.
- **One consumption step.** `BandEquipment::consume_condition` is the step `wear_item` and the
  forecast share (charge the serving batch, retire units at the tier's durability, carry the
  remainder, drop an emptied batch). `wear_item` adds the `retired` memory and the window write around
  it; the forecast calls `charge_condition` (`pub(crate)`, no recording) on a **clone**, once per turn
  of the lead, so units are spent in the order the turns will spend them and the real ledger is never
  touched (`publishing_the_forecast_leaves_the_real_ledger_untouched`).
- **The horizon is the lead time** (`craft_suggestions::wear_forecast`). Recipe = the offer marked
  `suggested` for the item, so `band_suggestions` takes the offers (`ForecastInputs`, with the
  materials table) and both callers build them first (`snapshot::population` already had them;
  `auto_craft::band_suggestions_and_offers` now builds the offers before the list). Lead = the queued
  orders' remaining work (`remaining() x recipe.work - progress`, over each order's own rate; an
  order at rate `0` adds nothing) plus `recipe.work / per-pass output / rate` for the item, ceiled
  with `WHOLE_UNIT_TOLERANCE`, minimum one turn. A rate is `systems::rate_per_turn` at the bench's
  crew, **one crafter when the bench is unstaffed** (`UNSTAFFED_BENCH_QUOTE_CREW`). An item with no
  recipe, or whose recipe one crafter cannot advance, has no forecast.
- **Units coming up** = `short(live after) - short(live now)` where `short(live)` is
  `ceil(need - live - WHOLE_UNIT_TOLERANCE)` floored at zero and `need` is the sum of `required` over
  the item's lines (the larger of the standing and hunt-turn passes, `item_needs`). A spare unit
  nobody holds absorbs the loss; a unit already short is not counted twice; a bench tool, appearing
  on no line, is never forecast.
- **Merge and rank** (`suggestions_from`): count = (today's whole shortfall - queued, floored at 0) +
  (the forecast - whatever of the queue exceeds today's shortfall). `upcomingCount` is the second
  term. An item with no source rank in the new last tier, ordered by `expiresInTurns` (the unit in
  hand's remaining condition over the rate) then item id; every other entry ranks as before.
  `expiresInTurns` is **when the shortage starts**, not when the unit in hand runs out: the first
  turn of the clone's walk whose loss leaves more of `need` unheld than today, as `turns before it +
  the share of that turn the serving unit lasted` (fractional), so a spare unit defers it to the turn
  its own loss lands. It is the coming-up ranking key. The walk stops once no live unit is left (the
  queue ahead has no cap, so it must not run its whole length). `expiresInTurns` and `leadTurns` are
  `0` for an item with no forecast.
- **Auto-craft is untouched**: it reads the same list, so it reaches a coming-up item only when
  nothing above it is usable, and its stale-skip rule drops a skip once the item leaves the list.
  The one difference to know: auto-craft runs mid-turn, so the newest slot it reads may not yet hold
  that turn's later charges; the capture reads the finished turn.
- **Rides the save.** `wear_window` is on `BandEquipment`, which `BandRecord::equipment` clones whole
  (`SAVE_FORMAT_VERSION` 38). A fission splinter's ledger is built from `BandEquipment::default()`
  plus the units `take_units` moved, so it starts with no window.
- **Wire**: `CraftSuggestion.upcomingCount` / `expiresInTurns` / `leadTurns`, appended last.
- **Bench-tool wire**: `CraftSuggestion.benchTurnsSaved` / `benchBuildTurns` (floats, appended after
  `leadTurns`; native dict keys `bench_turns_saved` / `bench_build_turns`) and
  `CraftSuggestionSource.kind == "bench"` with `material` set. Unchanged `SAVE_FORMAT_VERSION`:
  suggestions are derived.

Pinned off the encoded frame by `core_sim/tests/wear_forecast.rs`: forecast equals actual
(`a_unit_forecast_to_wear_out_within_the_lead_really_does`, with its fresh-unit control), the spare
unit (`a_spare_unit_absorbs_the_forecast_loss`), the merged entry and its queue netting, the ranking,
the lead time (crew, one-crafter quote, queue ahead), no window, the rolling window, the schedule
opening a slot, auto-craft with and without a usable shortage above, the stale skip, the save round
trip and the splinter.

Pinned by `craft_suggestions::tests` (ranking and ties, work only where gear adds work, queue netting
to zero, fractional shortfalls, the build/non-build order and the
merge) and, off the encoded frame, by
`bench_parallel_orders`,
`::a_suggestion_does_not_rise_on_the_turn_its_item_is_made` and
`::a_detached_party_publishes_no_suggestions` (paired with its home band carrying the same lines).

## Kit orders — a whole kit queued as ONE grouped order (#780)

Design: `docs/plan_crafting_and_materials.md` §7, "Ordering a whole kit". A kit is a set of items for a
job (`equipment.json` `kits`); the ledger's **Make** on a kit row queues the player's count of whole
kits at once.

**The grouping is queue structure, not a second queue.** A kit order is the ordinary `BenchOrder`s
for its items, appended contiguously and each tagged `BenchOrder::kit_order` (the id; `0` =
`NOT_A_KIT_PART`), plus one `KitOrder { id, kit_id, kits, done }` on `BandBench::kit_orders`
(`next_kit_order` counts ids ever issued, so ids start at 1 and are never reused). Everything the bench
already does — the per-order items and the forecast — is untouched; a part is
worked like any order.

**The invariant is that a kit order's parts stay contiguous**, kept by the editors:

- `raise_order` on a **plain** order whose predecessor is a kit part jumps over that **whole** block
  (`unit_at` finds it); on a **kit part** it is refused (`BenchQueueError::KitPartCannotMove`) — the kit
  moves as one.
- `raise_kit(id)` moves the whole block above the unit directly ahead (a single order, or another
  kit's whole block); refused at the head.
- `remove_kit(id)` removes every part; drawn piles are lost exactly as a single `remove_order` loses
  them (the command's event says `piles_returned`). A single part may still be removed or have its count
  edited (the well's ✕ and the part stepper) — the block stays contiguous either way.
- **One helper drops a `KitOrder`**: `prune_kit_orders` retains only the kit orders some order still
  carries, and `remove_order`, `remove_kit` and `complete_item` all call it. A kit order is therefore
  never left without parts and never outlives them, however the last one went.
- `complete_item` banks a finished part's `made` into its `KitOrder::done` before the part leaves; a
  part still queued reports its own `made` (the grouped box's progress is `done` + the live parts').
- **Auto-craft queues at the END** of the queue, behind any kit block, so it never interleaves with one.
- **Persisted** (`BandRecord::bench`; `SAVE_FORMAT_VERSION` 44).

**Commands** (proto 85-87, `CommandEventKind::Craft`, band-addressed with a required handle in `xtask`'s
guard):

| verb | does | refused when |
|---|---|---|
| `bench_enqueue_kit <f> <b> kit <id> kits <n> recipe <id> [recipe <id> …]` | queues `n` of every item of the kit (one recipe named per item) as one kit order; each part writes `last_started`, as `bench_enqueue` does; the crew is untouched | the kit is unknown or carries nothing (`none`); `n` is below 1; a recipe fails `bench_recipe_refusal` (the same predicate `bench_enqueue` uses); a recipe's output item is not one the kit `uses`; two recipes make the same item; an item of the kit has no recipe. **Fails whole** — nothing queues on any failure |
| `bench_remove_kit <f> <b> kit_order <id>` | removes every part | no such kit order |
| `bench_raise_kit <f> <b> kit_order <id>` | raises the whole block | no such kit order; already at the head |

**The order is a COUNT OF KITS the player chooses, and nothing nets it down.** One kit is one unit of
each item in the kit's `uses`, so each part's count is `kits` (items, as `BenchOrder::count` always
is — a recipe that makes several per pass still counts items) and the **sim** sets it; the command
carries `kits` and one recipe per item, never a per-part count. How many people have or lack a kit
(`KitNeed`) is information the ledger shows beside the row and **never reduces or refuses the order**
(Ray: *"We shouldn't prevent a kit from being built. The number should be the number of new kits to
build, it shouldn't matter how many hunters are not kitted"*). The only refusals are the ones that
make the order meaningless: a kit that cannot be ordered, a count below one, a recipe that cannot be
queued or makes nothing the kit uses, an item made twice, or an item left without a recipe.
`KitOrder::kits` is the number ordered and is what the grouped box reads.

**`KitNeed` on the wire** (`PopulationCohortState.kitNeeds`, one per kit that carries something, config
order; `craft_suggestions::band_kit_needs` + `snapshot::crafting::kit_need_states`):

| field | definition |
|---|---|
| `people` | `LaborAllocation::workers_on_job` summed over the kit's `jobs` |
| `have` (per item) | `Σ filled` over the settled lines that serve **this kit's jobs**, plus the **spare** units — `live_units − Σ filled over ALL lines`, floored at zero |
| `claimed` (per item) | `Σ required` over the settled lines **on this kit** (`SupplySource::is_on_kit`): a take row is on the kit it CHOSE (`SupplySource::Take.kit`, so the sled shared by `big_game` and `trapping` is counted once, on the row's own), a pool by its job, a site by its keeping job, a declared build on the kit carrying the item whose jobs include `builders`. A kit nobody's row chose reads 0. Of the standing and the hunt-turn pass, the one with the larger shortfall (Make Next's rule) |
| `short` (per item) | `whole_shortfall(Σ (required - filled).max(0))` over those same lines - `ceil(missing - WHOLE_UNIT_TOLERANCE)`, the one formula Make Next starts its count from, **before** the bench's `queued` nets it. The reader nets `queued`. Pinned by `craft_suggestions::tests` |
| `queued` | `craft_suggestions::queued_units` — the very figure the suggestions net by (queue + parked) |
| `finished` (per item) | `craft_suggestions::parked_units` — whole units of the item on `BandBench::finished` (made this turn, stocked at the top of the next). A pure readout: `queued` already includes them (`queued_units` adds the same `parked_units` sum), so `queued − finished` is what the queue still owes. On the turn an item completes with its order popped `queued == finished == 1`; next turn both are `0` and the unit is in the equipment ledger. Pinned off the encoded frame by `bench_parallel_orders` |
| `recipes` | one per recipe whose output is the item, book order: `outputPerPass`, and `inputs` — each amount **per pass with the bench's material efficiency applied** (`systems::required`, the call the bench draws with) |
| `materialsFree` | `QueueForecast::left_of` — the stock left after every queued order's claims, the term `suggestion_shortfalls` strikes a suggestion against — one row per material the band holds or any recipe reads |

- **`have` excludes another job's units** because the picker answers *"how many of my hunters have a
  sled"*: a sled held by a woodcutter does not equip a hunter, and counting it would tell the player
  nobody needs a sled while the hunters walk without one. Attribution
  (`SupplySource::serves_any`): a `Pool(job)` serves a kit listing that job; a `Take { job }` serves it
  when `KitJob::as_str() == job` (the same spelling the row publishes); a `Site` line is attributed
  through its source's **keeping job** (`site_keeping_jobs`: patch → agriculture, herd → husbandry,
  road → roadwork, working → quarrywork or extraction), because a keeping job has no labor row.
- **A queued-build (`BuildQueue`) line never counts as a unit held** — it is a *forecast* of a later
  job's claim against the stock the settlement left, and no unit is in anyone's hands for it. It is
  still part of what is spoken for when the **spare** is struck (the literal Σ over all lines), so a
  unit reserved for the next build is not offered as spare.
- **The client does the picker's arithmetic** (passes = `ceil(make / outputPerPass)`, `need = passes ×
  amount`, `short = need − free`): linear and exact over published terms, the same standing as the
  estimate precedent in `yield-forecast.md` ("the client draws the curve, the sim states the take"). The
  sim publishes the terms, never a pre-chosen recipe or a shortage.
- Pinned by `components::tests` (contiguity, raise over a block, `raise_kit`, `remove_kit`, `done`
  accounting and the record dropping with its last part), `craft_suggestions::tests` (a woodcutter's
  sled is not the hunters'; a spare unit counts and a queued-build line does not; `queued`; `none` not
  listed), `server::tests::a_kit_order_queues_every_item_as_one_group_and_refuses_whole`, the command
  text and proto round trip in `command_text::tests`, `sim_schema`'s
  `whole_kit_orders_and_kit_needs_ride_the_wire`, and, off the encoded frame,
  `bench_parallel_orders`.

## Auto-craft — the bench works down the suggestion list on its own (#779)

Design: `docs/plan_crafting_and_materials.md` §7, "Auto-craft". `auto_craft.rs` is the whole
behaviour; `BandBench::auto` is the per-band toggle (`bench_auto <f> <b> on|off`, proto field 82),
`BandBench::auto_skipped` the items the player passed over, `BenchOrder::auto` the tag on an order
auto-craft queued. All three ride `BandRecord::bench` (`SAVE_FORMAT_VERSION` 33) and the wire
(`BenchState.auto` / `autoSkipped`, `BenchOrder.auto`).

- **It adds ONE order only when crafters are free** (`auto_craft::crafters_are_free`): the band has idle
  hands after the latest staffing (`BandWorkforce::idle() > 0`) **and** every workable order is already at
  its `crew_cap` (or none is workable). `auto_craft_fill` then queues the first usable suggestion at its
  whole netted `count` as one `auto` order and **restaffs the band**, so the new order takes the idle hands
  this turn. **It never queues a suggestion that is itself short of an input** (the suggested recipe must
  be drawable for ONE item, `systems::recipe_is_drawable`), so the makeable suggestion behind a short one is
  the one queued, and **never a suggestion whose row already has an order on the bench**, blocked or not.
  One order is queued per fill call; both schedule slots (before and after `advance_crafting`) stay.
  Pinned by `bench_parallel_orders::{auto_craft_adds_exactly_one_order_when_crafters_are_free,
  auto_craft_adds_nothing_while_crafters_are_not_free}`.
- **The suggestions are the snapshot's own.** `craft_suggestions::take_row_gear` settles the rows'
  gear and `band_suggestions` ranks it — the two functions `snapshot::population` calls — fed by
  `take_claims::with_world_sources` (between-turns, regrow-first, exactly the capture's reading).
  Nothing here re-derives a shortfall line.
- **The recipe is the offer the ledger marks `suggested` for that item** (`band_craft_state`'s
  `craft_offers`, `suggested` and `outputItemId == item` or `outputMaterialId == item`) — the recipe the panel's Queue button sends.
  *Usable* means that offer is `queueable` (the one knowledge predicate `first_unknown_craft`); an item
  no recipe makes, or one whose craft is unlearned, is passed over. It writes no `last_started`: that
  records what the player chose.
- **Build-material suggestions are in that list.** `band_suggestions_and_offers` merges
  `craft_suggestions::build_material_suggestions_in_world` (the same wrapper the snapshot calls), so a
  pen short of hurdles is queued as an auto order at the suggestion's count, ranked where the
  suggestion ranks (a kit shortage outranks it). Pinned by
  an auto-bench fixture that queues the hurdles a pen build is short of, and one that skips it and
  stays skipped.
- **A bench tool that pays for itself is in that list too** ("A bench tool that pays for itself"),
  so a band with enough blocked or suggested work on a material auto-crafts the tool for it first
  (if its craft is known); a band with little work does not.
- **A queued auto order that later runs short WAITS** (the ordinary skip/waiting semantics) and no
  longer holds the bench: the next fill queues what can be made. Only the player's skip parks it.
- **It never picks a crew.** The sim staffs every order in the labor passes (`BandBench::crew_total`).
- **Skip** (`bench_auto_skip <f> <b>`, field 83) is valid only when no order is workable (`bench_plan`) **and** the
  head is `auto`; otherwise it is refused by name. It parks the head's row key (its recipe's
  `RecipeDef::row_key`: the equipment id, else the material id, so hurdles can be skipped) in
  `auto_skipped`, removes the head (a drawn order is workable, so a waiting
  head holds no pile and nothing is forfeited) and refills.
- **A skipped item returns only at the next fill** (when no order can progress). At each fill an item whose suggested
  recipe can draw ONE item from stock (`recipe_is_drawable`) leaves `auto_skipped`,
  and so does an item that is no longer in the suggestion list (nobody is going without it, so a
  stale skip must not survive); it never preempts a running order. Turning auto off clears the set and leaves every order alone.
- **When it runs:** the turn systems `advance_auto_craft` (before `advance_crafting`) and
  `advance_auto_craft_after_bench` (after it — so an order that completes this turn has its successor
  in the snapshot), both exclusive systems in the Population chain; and at the end of `bench_auto`,
  `bench_auto_skip` and `bench_remove`.
- **A detached party gets nothing** (it carries an `Expedition`; the snapshot publishes it no
  suggestions either). A world missing the crafting configs does nothing.
- **No config lever.** Every number is a count or a comparison the existing configs already own.

Pinned by `core_sim/tests/auto_craft.rs` (the fill, the wait, skip and its refusals, no preemption,
the player's order, toggle-off, successor-after-completion, the unlearned craft, the save round trip)
and `sim_schema`'s `the_benchs_auto_state_and_an_orders_auto_tag_ride_the_wire`.

## `queueable` — the Make button's gate is KNOWLEDGE ONLY

`CraftOffer.queueable` is true when every craft in the recipe's `requires_knowledge` is known — the
**one predicate** `crafting::first_unknown_craft`, which `bench_recipe_refusal` (`bench_enqueue`) and
the capture both call, so a `queueable` offer is never refused. Materials and tools do not gate the
queue: a short order is skipped and `shortToFinish` speaks for it. `available` stays "a pass can run
NOW" (readouts, `mark_suggested`). Pinned by `crafting_wire::a_short_offer_is_queueable_and_an_unlearned_one_is_not`
and `server::tests::a_known_recipe_queues_short_of_material_and_an_unlearned_one_is_refused`.

## The queue's FORECAST — `shortToFinish`, `CraftSuggestion.shortfalls` and the dock notice

`pass_is_affordable` only ever asks about ONE pass, so a six-sled order that can draw pass 1 said
nothing until it stalled. `systems::queue_material_shortfalls` looks at the whole queue and is the
**one function** the wire and the event read:

- **An order's need** is, per input material, the passes still to DRAW (`remaining()`, less one when
  the order holds its pile) × `required` at the band's current tiers. Orders are walked **in queue
  order** against a running copy of the store, each claiming `min(need, left)`, so a later order sees
  only what the earlier ones left. **Stock on hand only** — no inflow, no tier change from wear: the
  remedy it names is fetching more.
- **ONE material wording per order.** The `Short n material` parts of an order's `blockedReason` — and
  of the bench row's own, for the described order — are that order's `shortToFinish` numbers (whole
  remaining run, queue-order claims), via `snapshot::crafting::queue_aware_shortfalls`; non-material
  parts (`Needs X`, `No loom`) are unchanged. The per-pass shortfall only decides WHETHER material
  blocks (workability is untouched). The ledger's `CraftOffer.reason` stays per-pass: it describes
  one pass, not an order. Pinned by `bench_parallel_orders`.
- **`BenchOrder.shortToFinish`** is that order's rows (empty when covered). A **forecast, not a
  block** — `blockedReason` stays the only "why it is skipped" field.
- **`CraftSuggestion.shortfalls`** is the whole suggested count (`ceil(count / the recipe's per-pass
  output)` passes of the offer marked `suggested` on that item's row) against the stock **left after
  the whole queue's claims** — a queued suggestion lands at the queue's end. Filled after
  `mark_suggested` by `snapshot::crafting::suggestion_shortfalls`.
- **`CommandEventKind::BenchMaterialShort`** (`bench_material_short`, "Bench material short") —
  `systems::labor::announce_bench_material_short`, scheduled directly after `advance_crafting` so it
  judges the stock the draw left. Sums `short` per material across orders, fires staffed or not, and
  is edge-gated on the transient `LaborAllocation::bench_short_warned` (replaced each turn, excluded
  from equality, like `material_shortfall_warned`). Detail: `status=bench_short material= short=
  held=` plus `band=`.

Pinned off the encoded frame by `bench_parallel_orders` and
`::a_suggestions_shortfalls_are_struck_…`, the edge gate by `::the_bench_short_notice_is_edge_gated_…`,
the arithmetic by `systems::crafting::forecast_tests`.

## What is deliberately not wired

**The panel itself.** The Rust half publishes; `docs/plan_crafting_and_materials.md` §8 and the client
rules own the GDScript.

**A material output and an input `variety` are parsed, validated and shipped by nothing** — the alloy
shape, exercised by a fixture, exactly as the materials table treats `varieties` and `equipment.json`
treats the bronze tier. There is no producer for a material a recipe would make until the minerals
arc lands. `MaterialBatchState::varietyName` is therefore always `""` on the shipped roster, which is
a real answer rather than a gap.

**No `turns left` conversion for a continuous quantum.** A sled wears per biomass hauled, and turning
that into turns needs a per-band forecast of what the band is about to haul — a projection, not a
readout. It reads `N biomass hauled left` until something tracks that rate.
