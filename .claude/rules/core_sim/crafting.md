---
paths:
  - "core_sim/src/{materials_config,recipes_config,crafting}.rs"
  - "core_sim/src/systems/crafting.rs"
  - "core_sim/src/snapshot/crafting.rs"
  - "core_sim/src/data/{materials,recipes}.json"
  - "core_sim/tests/{materials,crafting,crafting_wire}.rs"
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

The material account is credited **on the same seam the provisions are**, off what came **home**
(`take.carried`, never `killed_biomass`): you cannot tan a hide you left on the range. Four sites in
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
to flatten it to. `LocalStore::drain_materials_into` moves it, **batch by batch**, at the
`Delivering` drop-off and in `fold_party_into_band`, so a mammoth hide is never averaged into a hare
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
- **A grade may only name a stat the item's TIERS declare**, and must restate that effect's mass
  bounds verbatim: a grade *replaces* a number rather than adding one, and an excellent snare that
  dropped `max_body_mass` would quietly become a mammoth trap.
- **A recipe with no `grades` block at all is a real statement**, not a missing value: five ship that
  way (`crook`, `hoes`, `wayfinding`, and the three bench tools), because their payload is *shared*
  rather than tier-bought (the wayfinding gear's vantage, a build tool's `build_work`) or is a
  bench stat nothing yet grades. The old shape spelled that as three empty rungs each — fifteen inert
  config rows saying by convention what absence now says outright.

### The anchor is DERIVED from the bench material's bare hand

**The rung pinned to the shipped item is the band that the recipe's bench material's
`hand_working.quality_ceiling` falls in** (`recipes_config::anchor_band`) — not a literal grade name.
The grade resolved there, *after inheritance*, must agree with the output item's default tier for
every stat it declares. That states the invariant that actually matters: **a bare-handed craft off
the best material a band can work by hand reproduces the shipped item exactly**, which is *"a tool
run dry drops the band back to the rate the game already ships at rather than into a spiral"*. On the
shipped config all three organics carry `quality_ceiling 0.60`, so the anchor is **`good`**
everywhere; a material with no `hand_working` at all has no anchor and no check.

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
recipe that makes that item** (`RecipesConfig::anchor_grade_for_item`, joined the way
`item_display_name` joins — the book is where an item's crafted facts are written). That is a
statement of fact rather than a display default: a spawn stocks the item's **default tier**
(`equipment.md` → *"flint is today's spear, verbatim"*) and `validate_grades_against_item` requires
the anchor grade to agree with that tier for every stat it declares, so the shipped spear already
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
and materials table it would have to carry are two configs those passes do not read. The two owning
call sites are the band spawn (`systems::worldgen::spawn_population_entity`, via the `StartKit`
bundle) and the detached party's outfit (`server::outfitted_party_equipment`).

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

`BandBench` is a component on a band: `{ recipe_id, workers, progress, drawn, items_completed,
last_output_grade }`. **One job at a time**, so no surface ever has to explain a queue.

**THE BENCH IS THE ASSIGNMENT.** `set_bench` puts the recipe up; there is no Crafter role card and
**no `LaborTarget` variant**. Scout and Warrior are standing roles with nothing to point at, and
crafting always has a subject, so it is staffed like a worked source. A `LaborTarget::Craft` would
also put a fictitious row on every per-source yield readout in the game.

**THE PLAYER STAFFS THE BENCH; THE SIM NEVER DOES.** `set_bench` chooses the *job*, never the crew.
The crew comes out of the same pool `assign_labor` spends — a crafter is a hunter who is not hunting
— and dividing the band is this game's core turn-to-turn decision (`docs/plan_early_game_labor.md`),
so this is the one place the sim must not take a labor decision off the player. There is no correct
number for it to pick either: **all idle** wastes hands, because `advance_crafting` finishes one item
per pass and zeroes the meter rather than carrying the overflow, so everything accrued above `work`
in the crossing turn is dropped; **any other number** is the sim guessing at intent.

**So a `workers` of zero means "do not change the crew".** `workers` rides a proto3 scalar, which
cannot tell an absent field from an explicit `0`, and the client sends neither number — pressing
**Make** stages the job and nothing else. `sim_runtime`'s `BENCH_CREW_UNSPECIFIED` is the shared
spelling of that `0`, read by the text parser and the handler alike, and it resolves two ways:

| bench state | `set_bench` with no crew named |
|---|---|
| idle (no job) | the recipe is staged with **crew 0**; the player sets it on the stepper |
| already running a job | the new recipe is staged and the **crew already there stays put** |

The second row is not the sim choosing a crew, it is the sim declining to *un*-choose one:
`BandBench::set_job` overwrites `workers`, so applying the proto's `0` dismissed hands the player had
placed — the exact case `BandWorkforce::benchable()` (pool − assigned, deliberately *not* netting the
bench) exists to preserve. **`bench_crew <n>` is the verb that names a crew, zero included**, so it is
how a bench is stood down without taking the job off it, and no reachable intent is lost.

Pinned as a pairing by `server::tests::a_set_bench_with_no_crew_named_recruits_nobody` (an idle bench
stages at 0 and the band's idle count is untouched; a crew *named* is applied to the head — the second
half is what stops "leave the crew alone" from becoming "ignore the crew") and
`::swapping_the_job_on_a_running_bench_keeps_its_crew`.

**A bench awaiting its crew is a PROMPT, not a fault**, and the wire says so:
`BenchState.blockedSeverity` carries the same `danger` / `neutral` / `good` vocabulary as
`CraftOffer.severity`, `""` when nothing blocks. *"No one at the bench"* is the normal state one click
after Make, so it publishes at `neutral`; a shortage, an unlearned craft and a zero craft rate are
problems and publish at `danger`, as does any joined reason with one of them in it. Without the field
a client renders every `blockedReason` in one alarm colour and the expected state reads as an error.
**The client must not re-derive it** — the same rule `reason`/`severity` already follow. Pinned as a
pairing by `crafting_wire::a_crewless_bench_is_a_prompt_and_a_short_bench_is_a_fault`: one severity
stamped everywhere passes neither half.

**The crew comes out of the same pool `assign_labor` spends**, and **`BandWorkforce`
(`components.rs`) is the ONE place that says so.** A band's people are spent on exactly two things —
the `LaborAllocation` and the bench — so it resolves `{ pool, assigned, benched }` off the three
components and answers the three questions anyone asks:

| reading | value | who reads it |
|---|---|---|
| `idle()` | `pool − assigned − benched` | **the published `PopulationCohortState.idleWorkers`**, and every "n idle of m" readout downstream of it |
| `assignable()` | `pool − benched` | `handle_assign_labor`, as the ceiling `LaborAllocation::set_assignment` clamps against (that helper nets out the other assignments itself) |
| `benchable()` | `pool − assigned` | `set_bench` / `bench_crew` — a band's own crew stays put while its job is swapped, so it is idle **plus** the crew already there |

**The bench is netted out exactly once, and that is the point.** It was subtracted at each command
site and *not* at the publish site, so a band with four hands at the bench published them as idle:
the Band panel's workforce zone, the compose sheet's available-worker count and the turn orb's
attention model all over-reported, in the *reassuring* direction — the player was told they had hands
free that were already busy, and a compose sheet sized against it could not be staffed. Two
authorities over one number is how they drift, so a second subtraction must not be added beside this
one. Pinned by the liveness pair `server::tests::a_bench_crew_is_missing_from_the_published_idle_count`
(fewer published idle with a crew at the bench, restored when the job is cleared) and
`::the_published_idle_count_is_what_assign_labor_will_staff` (the published number is exactly what the
command path staffs — without it, a sim that stopped publishing idle at all would pass the first).

### The shed can take the bench's crew, so a running job with nobody on it now exists

The bench spends the band's people, so when a band cannot field what it is holding the **shedding
order** takes hands off it like anything else — `LaborAllocation::normalize` ranks it in step 5 beside
the worked rows and stalls it at step 5b (`yield-forecast.md` → "The crafting bench is a candidate in
the walk" has the ordering and why the bench is not a learner). Three consequences land here:

- ⛔ **The shed must never call `clear_job`.** It is `*self = Self::default()`, so it **forfeits the
  drawn pile** — the materials are dropped rather than returned to the store, and the grade the draw
  fixed is re-fixable against different stock. `BandBench::shed_one_worker` takes one hand and leaves
  the recipe, the progress, `items_completed`, the last grade **and** the drawn pile alone, so
  re-crewing **resumes**. That matches this system's own rule for a pass it cannot advance: *the
  player chose this job, and silently emptying their bench is a worse answer than a job that makes no
  progress.*
- ⛔ **An unstaffed bench draws nothing** (`AN_IDLE_BENCH`). `advance_crafting` runs `draw_pass`
  **before** the workers term is used anywhere, so a bench at zero crew would withdraw a pass's
  inputs every turn for work it can never do — a famine quietly draining the material store into a
  bench nobody is at. The gate is on the **draw**, not the pile: a bench that had already cut its
  materials keeps them (they are the player's, and the job is still theirs) and simply banks no
  progress, which `rate_per_turn(0, …)` gives for free. Pinned by
  `crafting::an_unstaffed_bench_draws_nothing_from_the_store` — and that fixture stocks **both** of
  the sled's inputs, because a draw is all-or-nothing and a fixture short of one of them withdraws
  nothing whatever the code does.
- **`BandBench::priority` is the bench's own mark**, set by `bench_priority <faction> <band>
  high|normal|low`. Its own verb rather than a `work_priority` token: every other bench command is
  addressed `<faction> <band>` with no source, and `work_priority`'s grammar reads a lone token as a
  **herd id**, so `work_priority <f> <b> bench low` would be ambiguous with a herd named `bench`. It
  rides `BandRecord::bench` with the rest of the bench, because a checkpoint that forgot it would
  silently re-rank the band's work on rollback, and it is **on the wire** as
  `BenchState.priority` — the *same* `SourcePriority` enum a worked row carries, reused rather than
  duplicated, because it is one property of one kind. Captured **live** off the bench (as
  `LaborAssignment.priority` is), so an edit lands on the command's own recapture and the client needs
  no optimistic overlay. **It is published on an IDLE bench too**: a rank is a standing statement
  about the bench rather than about the job on it, `bench_priority` is settable with nothing on it —
  the moment a player is most likely to state it — and a capture that dropped the mark there would
  make the control look like it had done nothing. `crafting_wire::the_benchs_rank_reaches_the_client_running_or_idle`
  pins all three arms off the encoded envelope.

**An unmarked bench is the first thing a short band thins**, because a craft pays into no food, fodder
or material account — a statement about the accounts, not a judgement about crafting. That is the
right default in a famine and is exactly what the mark exists to override.

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
3. **Accrue `workers × progress_per_worker_turn × craft_speed`**, through `systems::rate_per_turn` —
   **the one authority**, because the wire publishes the same product as `BenchState::ratePerTurn`
   and a second copy of it would be free to drop the `craft_speed` term. `craft_speed` is the bounding
   tool's equipped value if the band has a live one, else the **material's** `hand_working.rate` —
   which is `0` for a material that cannot be worked bare-handed. **That zero is the entire refusal
   mechanism.** There is no *"you cannot craft that"* branch anywhere in the sim, exactly as
   `max(0, attack − defense)` refuses a hunt; the client renders the reasoned refusal.
4. On `progress >= work`: emit the outputs, charge one wear, charge one lesson, reset, re-draw.

**The grade is fixed at draw time and never moves** (`DrawnInputs`), which is what makes it *not* a
taper: a tool that runs dry mid-craft does not retroactively coarsen the thing on the bench, and a
pile that gets worse while the pass is in flight does not re-grade it.

**`DrawnInputs` also carries `withdrawn`** — one `DrawnMaterial` per input row, in the recipe's own
order, holding **what the store actually lost**. That is not the recipe's stated `amount`: the bench
tool's `craft_material_efficiency` sits between them, so a tanning frame cuts 4.8 hide for a sled the
book prices at 6. It is a *record of the withdrawal*, taken by summing the draws `take_material`
returned, because worst-first spends one row across as many piles as it needs.

**The overflow past `work` is dropped, not carried.** Progress beyond a completion was done on an
item whose materials have not been drawn yet, so there is nothing for it to have been spent on. The
consequence is the ladder's own `crew_scale` shape: over-crewing a bench buys less than
proportionally, and a `work: 8` recipe wants about four hands rather than sixteen.

**Swapping or clearing a job spends the pile already drawn.** The materials were cut for the thing
the player stopped making, and a `LocalStore` has no representation for a half-worked pile. What is
lost is nameable rather than merely warned about: it is `BenchState::drawnInputs`, straight off
`DrawnInputs::withdrawn`.

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
| `src/data/materials.json` | **The materials table** (loader `materials_config.rs`, env override `MATERIALS_CONFIG_PATH`, validated inside `from_json_str` so every load path is covered). Two blocks. **`characteristic_bands`** — the shared rating vocabulary, `[{ name, from }]` ascending: `poor 0.0 · fair 0.30 · good 0.55 · excellent 0.80`. Retuning these re-partitions every batch on the map. **`materials`** — id → `{ craft, characteristics[], hand_working?, varieties?, start_stock? }`. Shipped: **`hide`** (tanning; `toughness`/`suppleness`), **`fibre`** (weaving; `fineness`/`strength`), **`bone`** (bone_working; `density`/`length`), each `hand_working { rate 0.5, quality_ceiling 0.60 }` — plus the three **uncrafted** luxury crops **`tobacco`** / **`tea`** / **`grape`**, which name **no `craft`, no `hand_working` and no `varieties`** and carry the provisional axes `potency`/`keeping` (arc #527; see "A material with NO CRAFT is one nothing works"), and the two the material half of build-and-upkeep added (`docs/plan_standing_upkeep.md` §4.9 item 12): **`wood`** (weaving; `hardness`/`pliancy`, `hand_working { 0.5, 0.60 }`, and the roster's **only `start_stock`** — see "`MaterialDef::start_stock`" below) and **`hurdles`** (**no `craft` and no `hand_working`**, axes `stoutness`/`span`), which is a woven fence panel a pen spends on its build pile and its upkeep rate. **Stone, clay and metal still have no producer** until the minerals arc, and an unreachable material is dead content the catalogue publishes — but the roster now holds **three** kinds of thing that are not that: the luxury crops are *uncrafted* (a producer, no bench), `hurdles` are *crafted but never an input* (a bench, no recipe takes them), and `wood` is *stocked but unproduced* (an opening pile, no producer — deliberately, and the one owed tracker item this slice leaves behind). ⛔ **Nothing reads `wood`'s or `hurdles`' axes**, which is also deliberate: a quality axis on a fence panel invites *a better fence contains better*, the containment-scaling item 12 defers. **`hand_working` absent means the material cannot be worked bare-handed at all** (rate `0`, which is how metal will refuse itself with no branch), and the bare-handed ceiling belongs to the **material**, not to the absent tool. **`varieties` are parsed, validated, and none ships** — named presets over the material's own axes (`copper`, `bronze`), exercised by a test fixture for the same reason the bronze equipment tier is. **`validate` rejects**: an empty material table; a band list that is empty, does not open at `0.0`, does not strictly ascend, or carries a seam outside `0..=1`; a material stating a **blank** craft (omit the key to say nothing works it — an empty string would put an unnameable craft in `crafts_declared_by`) or no characteristics; a duplicate characteristic on one material; a non-finite or negative `hand_working.rate`; a `quality_ceiling` outside `0..=1`; a variety that omits an axis the material declares or names one it does not, or states a reading off the range; **a `start_stock.per_worker` that is not finite and `> 0`** (a stocked nothing is a block that should not be there, which is what an absent `start_stock` already says), **a `start_stock` naming an axis the material does not declare or omitting one it does** (through `exact_axes_fault`, the one home of that rule, shared with the recipe output's material arm), and **a `start_stock` reading outside `0..=1`**. **The root is open (`_comment*` keys) and `MaterialDef` is CLOSED** — a mistyped `hand_workng` would silently make a material unworkable, while a stray key at the root can only be prose. |

| `src/data/recipes.json` | **The recipe book** (loader `recipes_config.rs`, env override `RECIPES_CONFIG_PATH`, `validate` inside `from_json_str`, cross-config `validate_against(&materials, &equipment)` at the `build_headless_app` seam). Two blocks. **`crafting`** — `progress_per_worker_turn` (**1.0**). **`recipes`** — id → `{ display_name, craft, work, requires_knowledge[]?, inputs[], outputs[], grades? }`, where `grades` is keyed by `characteristic_bands` NAME and carries only `effects` (there is no `when`), an input is `{ material, amount, variety?, reads? }` and an output is exactly one of `{ equipment }` or `{ material, characteristics }`. Eleven ship: **ten make EQUIPMENT and one makes a MATERIAL** — `hurdles` is the material (4 wood + 2 hide / work 7, reading hide's `suppleness`, so its bench is `tanning`; see "`hurdles` ARE A MATERIAL"), and the kit items are (`sled` 6 hide + 2 fibre / work 8; `crook` 1 bone + 2 fibre / 5, the animal web's build tool, reading bone's `length`; `hoes` 1 bone + 2 fibre / 5 — **ungraded**, deliberately, so plant and animal build gear stay consistent until both gain grades together (issue #561); `baskets` 5 fibre + 1 hide / 6; `traps` 6 fibre + 1 bone / 6; `spears` 1 bone + 2 fibre + 1 hide / 6; `clubs` 2 bone + 1 hide / 4; `wayfinding` 1 bone + 1 hide + 1 fibre / 4) and the three bench tools (`tanning_frame` 8 fibre + 2 bone / 12; `loom` 3 bone + 4 hide / 14; `bone_awl` 3 hide + 3 fibre / 10). **Costs are sized so MATERIAL, not bench time, is what binds** — see the file's `_comment_work_and_costs` for the measured income figures. **Bone is the scarce one by an order of magnitude** (0.0012–0.003 per biomass against hide's 0.006–0.022), so nothing costs more than 3 of it. **`validate` rejects**: a non-positive `progress_per_worker_turn`; an empty book; a non-positive `work` or `amount`; a recipe with no inputs or no outputs; the same material twice in one recipe's inputs; **more than one input carrying `reads`**; an output naming both or neither of `equipment`/`material`; an equipment output stating characteristics, or a material output stating none; a duplicate output; grades on a recipe that reads nothing or outputs only materials; a duplicate stat in one grade's effects. **`validate_against` additionally rejects**: an unknown material, item, variety or axis; a `craft` that is not the craft of the material the recipe reads; a `requires_knowledge` naming a craft no material declares **or one that none of the recipe's own inputs is worked by**; a tool recipe whose inputs include the material it bounds; **a fractional `amount` on an equipment output** (a batch's `count` cannot bank half a spear); **a grade key that is not a declared `characteristic_bands` name, and a lowest declared grade that is not the FIRST band**; and **a grade effect that names a stat no tier of the output item declares, drops that effect's mass bounds, or — at the DERIVED anchor band — disagrees with the item's default tier** (see "ONE QUALITY LADDER"). |

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
- ⛔ **NOTHING READS EITHER PAIR'S AXES, AND THAT IS DELIBERATE.** `stoutness` / `span` on hurdles and
  `hardness` / `pliancy` on wood exist because `RecipeOutput::material` and `MaterialDef` **require** a
  material to be rated. A quality axis a pen *read* would invite *"a better fence contains better"* —
  the containment-scaling §4.9 item 12 defers — so no effect is wired to any of the four.

### THE HURDLES RECIPE KEEPS ITS HIDE, AND THE `craft` FOLLOWS THE `reads`

The recipe is **4 wood + 2 hide**, `work 7`, reading hide's **`suppleness`** — and the hide is
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

### `MaterialDef::start_stock` — the roster's one spawn-stocked row

**`StartKit.materials` was never a stock.** It is the materials *table*, carried into the spawn so an
equipment batch can resolve its anchor grade through `recipes.anchor_grade_for_item`; **nothing in
`StartKit` deposited a material batch**, and a spawned band's `LocalStore.materials` was empty. So the
material half of the standing upkeep **added the path** rather than calling one.

`MaterialStartStock { per_worker, characteristics }` is a `#[serde(default)]` block on `MaterialDef`,
and **only `wood` declares one** (`per_worker 6.0`). It is seeded by
`worldgen::spawn_population_entity`, off the **same floored worker count** the equipment stock is
sized against, so *"a party's worth"* means one thing in that function; the readings must state
**exactly** the material's declared axes, through the same `exact_axes_fault` a yield edge's row goes
through.

- **The lever lives on the MATERIAL rather than in a start profile** — the roster is where a material
  is described, one home per fact.
- **`per_worker` is a CONFIG LEVER, not a constant**: until forest foraging lands it is the only thing
  between a band and its first pen, so it has to be tunable without a rebuild.
- **It is seeded at the SPAWN rather than in `apply_starting_inventory_effects`** because it needs no
  demographic split — the party's own worker count is the whole of it — and the nearby comment saying
  brackets and larder are seeded at Startup stays true.
- ⛔ **Wood has no producer, deliberately**, and that has its own tracker item. It is the one row on
  this roster that is *unproduced* and still reachable.

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
band keeps its kit alive on roughly a quarter of its material income and spends the rest on tools and
stock. Measured against the shipped ~17-worker band split half-gathering, half-hunting Red Deer — so
~8.5 workers each way, reaching ~68 biomass of reeds and hauling ~45 biomass of deer per turn:

| | per turn | a kit item costs | its life |
|---|---|---|---|
| **fibre** (reeds `0.02`/biomass) | ~1.4 | baskets 5 → ~4 turns | ~18.5 turns |
| **hide** (deer `0.014`/biomass) | ~0.6 | sled 6 → ~10 turns | ~56 turns |
| **bone** (deer `0.002`/biomass) | ~0.09 | spears 1 → ~11 turns | ~42 turns |

**The `its life` column is the FULL band on that one job**, not the half that banks the material —
it is `equipment.md`'s kit clock, where all 17 gather or all 17 hunt. The two columns therefore
answer different questions on purpose: how fast a split band *earns* an item, against how fast a
committed band *spends* one.

Bench time is deliberately **not** the binding constraint: four hands work a `work: 8` sled off in
four turns bare-handed, against ten turns of banked hide. **A craft is ~5 items** (the ladder's
`craft_lesson_per_item` against that craft's `lesson_costs` entry), so ~25–40 turns of deliberate
work — the same order as one rung of the
intensification ladder — and the tool it unlocks is a further ~28-turn investment (a tanning frame is
8 fibre + 2 bone, and the bone is what paces it) that then pays back over the 25 items it lasts.

---

# On the wire — the refusal is RESOLVED here and RENDERED there

`snapshot/crafting.rs` is the whole publication (`docs/plan_crafting_and_materials.md` §7). It exists
because of one asymmetry: the sim's **resolution** has no *"you cannot craft that"* branch — every
refusal is a zero — but a bench that silently does nothing is not an answer, so the reason has to be
**published**. It is the same split `KitRoster.kit_offer` already makes for a snare against a Red
Deer, and the same rule `kitTiers` enforces: **a client must never re-derive a reason, a shortfall
number, a grade or a step-down.**

## Four fields on `PopulationCohortState`, and what each answers

| Field | Answers |
|---|---|
| `materialBatches:[MaterialBatchState]` | *what have I got* — one row per (material, band key) batch: `amount`, plus a `CharacteristicReading` per axis carrying **both** the exact value and its band name, in the material's **declared** axis order |
| `bench:BenchState` | *what am I making* — `recipeId` (`""` = idle), crew, `progress` against `work`, `teaches` (the recipe's craft), `itemsCompleted`, whether the pile is `drawn` and the grade it fixed, `blockedReason` with its `blockedSeverity`, the `ratePerTurn` a turn adds, and the `drawnInputs` a clear would destroy |
| `craftOffers:[CraftOffer]` | *what could I make* — **one row per recipe, always**, with `available`, a resolved `reason` + `severity`, the `shortfalls`, the `outputGrade` a draw would select, `group`, `outputItemId`, `onBench`, and the three ledger fields below (`outputTierName` / `outputTierRank` / `ownedNote`) |
| `equipmentBatches:[EquipmentBatchState]` | *what have I got, and how long will it last* — one row per **batch**, plus one `count: 0` row per config item the band owns none of, so the ledger is never missing a row |

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
  blocked reason that publishes at `neutral`: the player staffs the bench, so it is an instruction.
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

### `drawnInputs` NAMES WHAT A CLEAR DESTROYS, and it is the withdrawal, not the recipe

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

## TIER IS A GROUP HEAD, NOT A COLUMN — and the note is the disagreement

The ledger's columns are **Item · Owned · Rebuild costs · action**; there is no Tier column, because
a column spends its width saying `flint` on every row for the whole early game while a **head** says
it once and can **fold away**. Three appended `CraftOffer` fields carry it, and all three are
resolved sim-side for the reason `kitTiers` exists: a client that re-derived a tier, a grade or a
wording would be a second copy of a join it cannot make correctly.

| Field | Answers |
|---|---|
| `outputTierName` | **The head** — `ItemDefinition::craftable_tier`, the best tier this *faction* knows, so it is resolved per band rather than in the per-capture `CraftOfferPlan`. `""` on a material (stock) recipe |
| `outputTierRank` | that tier's index in the item's own `tiers` list. **Heads order by rank descending** — newest first — because there is no other honest ordering for two heads and alphabetical would put Iron above Bronze |
| `ownedNote` | **the cell's news, rendered verbatim** — `""` unless what the band *has* disagrees with what it could now *make* |

**The head is what a row would be MADE at; the Owned cell is what the band HAS, and the disagreement
is the readout.** A Clubs row under **Bronze** whose cell says *carrying flint · poor* is telling the
player something worth knowing. Two wordings, resolved by `snapshot::crafting::owned_note`:

- units in hand at a tier **below** `craftable_tier` → `carrying <tier> · <grade>`. Several such
  batches name the **worst** grade — naming the best is the one a player would be told about last,
  and a row that flattered its stock would say the opposite of what they need to act on. An
  **ungraded** batch makes no quality claim at all, so it sorts ahead of every graded one and reads
  simply `carrying flint`. Since the start-stock stamp (below) nothing on the shipped roster is
  ungraded, so that arm answers only for a batch some future path banks without a grade.
- no units at all and a set retired at an older tier → `last <tier> set wore out`. **The tier is read
  out of `BandEquipment::retired_tiers_of`, never inferred from `craftable_tier`'s neighbour** — of
  several it names the highest-ranked one still below what the band can now make, the set it lost
  most recently.

A tier's word goes through `crafting::title_from_id` like every other id (`equipment.json` authors no
display name), lowercased for mid-sentence use.

> ### `retired` IS KEYED BY (ITEM, TIER), because the readout names the tier out loud
>
> `BandEquipment::retired` is a `BTreeMap<String, BTreeMap<String, u32>>`, written by `wear_item` —
> the one seam that destroys a unit, and the one place that already holds the tier it is destroying,
> so the key costs a `clone` and no lookup. `retired_of` sums it for the caller that only asks
> *whether* anything broke (`equipmentBatches`' `Worn out` wording); `retired_tiers_of` is the
> readout's join.
>
> **An item-wide tally could only ever INFER the tier**, and *"the rank below what I can now make"*
> is right only while nothing has three tiers: with iron beside bronze and flint it names **bronze**
> for a flint set that actually wore out. A published string asserting the wrong tier is worse than
> saying nothing, and it is the same defect class the derived anchor exists to avoid — a value taken
> from a coincidence of the shipped config rather than from the fact it is claiming.
>
> **Nothing in the sim branches on `retired`** and nothing may — it must not become a repair
> discount. That constraint is what keeps the per-tier key cheap. The checkpoint carries it for free,
> because `BandRecord::equipment` clones the whole component.

**On the shipped one-tier roster `ownedNote` is `""` on every offer**, which is why it is covered by
a **three-tier fixture**
(`crafting_wire::the_owned_note_is_published_only_when_the_band_carries_something_older`) — the same
treatment `a_tier_switches_an_items_attack_without_touching_its_shared_effects` gets, one tier
deeper. **Two tiers cannot tell the two rules apart**: *"the tier that wore out"* and *"the tier below
craftable"* agree there, so a two-tier fixture passes either implementation. The fixture retires a
**flint** set with bronze standing between it and the iron the band can now make, and is pinned as a
**pairing** — an upgraded row against an un-upgraded one on the same frame — so a wire that emitted
the same note everywhere cannot pass.

**Nothing here is authored.** `group` (`kit`/`tool`/`stock`) is derived from whether the output item
declares a `bounds_material`; a craft's display name is `crafting::title_from_id`
(`clay_working` → *Clay-working*, underscores to hyphens, first letter up), which is why there is no
`display_name` beside a craft in `materials.json`; and an **item**'s player-facing name is
`RecipesConfig::item_display_name` — the book already writes it, and `equipment.json` carries none.

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
> that destroys a unit — and summed here by `retired_of`. It is keyed by **(item, tier)**, because
> the craft ledger's `ownedNote` names the lost tier out loud; see "`retired` IS KEYED BY
> (ITEM, TIER)". **Nothing in the sim branches on it** and nothing may — it must not become a repair
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
