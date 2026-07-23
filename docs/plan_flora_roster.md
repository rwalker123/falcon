# Flora Roster & Ecology — Named Plants, and the Yield Vector They Carry

**Status:** design. Opens the flora content arc (`TASKS.md` → *Flora Roster & Ecology*), the plant
twin of the shipped fauna roster.

**Rests on:** `docs/plan_grazing_foundation.md` (the two food webs, and the two per-biome capacity
tables), `docs/plan_intensification_ladder.md` (the rung grammar both webs climb),
`docs/plan_grazing_2d.md` (the pen economy fodder must interlock with).

---

## 1. Why this exists

Fauna is a concrete roster: `fauna_config.json` holds named species, each a hand-tuned stat block
(`host_biomes`, `body_mass`, `fodder_per_biomass`, `regrowth_rate`, `husbandry_ceiling`), and adding
a species is a config edit, not code. **Flora has no named species at all.** Plants are three
abstractions with no identity:

| layer | where | what it says |
|---|---|---|
| `forage.capacity_by_biome` | `labor_config.json` (38 biome rows) | *how much* human-edible biomass a tile carries |
| `FoodModule` (10 variants) | `food.rs:14` | *what kind of gathering* a tile offers — not which plant |
| the ladder rungs | `intensification_ladder.json` | wild → tended → field, with no per-crop identity |

So you can name the aurochs you herd but not the plant you sow. That asymmetry is the itch, and it
is not merely cosmetic: **three things the game wants next cannot be said in the abstract model at
all.**

- **Fodder.** There is no hay. Pens self-feed off the live `GrazePatch` (2d), which chains herd size
  directly to standing pasture. A *storable* feed crop is what historically decoupled the two.
  `plan_intensification_ladder.md` §2 already names Fodder as the animal ladder's **rung 4** and
  calls it "the moment your fields feed your herds" — but the plant half of that coupling does not
  exist, so the rung cannot be built. This doc supplies it.
- **Cash crops.** Tobacco, cotton, sugar, tea. Today both food webs sell into **one undifferentiated
  scalar** (`trade_goods_per_biomass` × `trade_goods_multiplier` 4.0, `labor_config.rs:174`). Trade
  goods have no *kind*, so a trade-good economy has nothing to be about.
- **A tile's food signature.** "190" is not a place. Hazel, acorn and berry under a canopy is.

### 1.1 What is already shipped (recon, 2026-07-22) — read before planning work

- **The plant ladder is complete through rung 3.** `intensification_ladder.json` ships
  `wild → tended` (`Cultivate`, Cultivation 2003) `→ field` (`Sow`, Seed Selection 2005), and
  `ForagePatch` carries both meters (`cultivation_progress`, `field_progress`, `forage.rs:145-161`).
  `Sow` is already gated to naturally food-bearing ground. **This arc adds no rungs** — it gives the
  existing rungs something specific to be about.
- **Discovery ids 2001–2006 are taken** (`nomadic_wayfinding`, `portable_forge`, `cultivation`,
  `herding`, `seed_selection`, `penning`). Next free is **2007**.
- **`fodder_per_biomass` is a herd's demand *rate*, not a crop** (`fauna.rs:218`). Nothing in the sim
  stores feed.
- **The rung engine reads config, not code.** `RungDef` already drives verbs, gates,
  `earns_knowledge` and behavior primitives from JSON — so most of what this arc needs is data.

---

## 2. The design tension, and the ruling that resolves it

The abstract model existed to avoid the 4X ritual: *settle turn 1, plant wheat, every game.* A fake
decision (see *scarcity-drives-the-real-decision*). Named crops threaten to reintroduce it as a
checklist you fill regardless of terrain.

> **Ruling — naming decomposes, it does not add.** A roster entry says what a tile's existing
> capacity *is made of*; it never adds capacity on top. `MixedWoodland`'s 190 becomes named shares
> (hazel + acorn + berry) that still sum to 190.

The consequence is the whole point:

- **At rung 1 (wild), names are descriptive and cost nothing.** You gather the tile's whole basket.
  No new decision, no balance change, no tech-tree. A forager does not choose a crop; they eat the
  woods.
- **At rungs 2–3, names become the decision.** `Cultivate` and `Sow` commit a patch to **one**
  species. Which one is worth the land depends on that species' affinity for *this* biome and on
  what its yield vector pays — and those differ tile to tile, because the affinity table and
  `capacity_by_biome` differ tile to tile.

So a crop earns its place by biome affinity + scarcity + the ladder, exactly as the task demanded.
There is no crop that is right everywhere, and on thin ground there is no crop worth the labor at
all.

**Corollary — not every plant climbs.** Mirroring the manual's "not every animal climbs," each
species declares a **`cultivation_ceiling`** (`wild` | `tended` | `field`). An oak's mast is a wild
harvest forever; you do not sow an oak forest on a five-turn horizon. That single ceiling makes the
incoherent "sowable but not tendable" state unrepresentable — the same reason `husbandry_ceiling` is
one ceiling and not two flags (`fauna_config.rs`, Grazing 2d-δ).

---

## 3. The spine: every plant carries a yield vector

The three "roles" the task names — staple / fodder / cash — are **not three subsystems.** They are
three characteristic *shapes* of one per-species output vector:

```
yield: {
  provisions_per_biomass:   f32,   // human food   — the shipped forage path
  fodder_per_biomass:       f32,   // animal feed  — NEW, §5
  trade_goods_per_biomass:  f32,   // trade value  — differentiates today's flat scalar, §6
}
```

A harvest of `B` biomass pays `B × yield.*` into three different accounts. `role` survives only as a
**display tag** derived from which component dominates — never as a branch in the sim. Modeling it
as a vector rather than three categories is what gives the future Market / yield-vector arc a real
data surface to land against, instead of a fourth thing to invent (the task's requirement in its own
words: *"they feed the command yield-vector's trade-good dimension"*).

**Today's behaviour is the degenerate case:** every existing patch behaves as a single implicit
species whose vector is `{provisions_per_biomass: <labor_config value>, 0, <flat trade rate>}`. So
slice 1 is provably a no-op on the economy.

---

## 4. Schema — `flora_config.json`

Mirrors the `fauna_config.rs` loader pattern exactly (baked-in `include_str!` builtin + optional
file/env override, `validate()` on load, heavy `_comment_*` prose on every block — this repo's JSON
carries its own rationale).

```jsonc
{
  "species": {
    "hazel": {
      "display_name": "Hazel",          // player-facing; embeds the client icon keyword
      "plural": "hazel",
      "adjective": "hazel",
      "role": "staple",                 // DISPLAY TAG ONLY — derived from `yield`, never branched on
      "cultivation_ceiling": "tended",  // wild | tended | field  (the husbandry_ceiling twin)
      "host_biomes": {                  // biome -> AFFINITY WEIGHT (not a capacity)
        "MixedWoodland": 0.45,
        "TemperateForest": 0.30
      },
      "yield": {
        "provisions_per_biomass": 0.30,
        "fodder_per_biomass": 0.0,
        "trade_goods_per_biomass": 0.005
      },
      "regrowth_rate": 0.10             // per-species r, as fauna got in grazing 2b
    }
  }
}
```

### 4.1 Biome affinity — terrain-keyed weights, normalized per tile

**Decision: key affinity on `TerrainType` (the 38 biomes), not on `FoodModule` (the 10 buckets).**
Fauna keys `host_biomes` off `FoodModule` because an animal *ranges* over a region; a plant *is* its
tile. The buckets are too coarse to say "this wants floodplain silt, not any wetland," and
`capacity_by_biome` already proves the 38-row shape is workable. `FoodModule` is untouched — it
still decides what *kind* of gathering a tile offers and its `seasonal_weight`.

A tile's composition is then derived, never authored per-tile:

```
share(species, tile) = weight(species, tile.biome) / Σ weights of all species hosting tile.biome
patch_capacity(species, tile) = share × forage.capacity_by_biome[tile.biome]
```

Weights are **relative, not absolute** — normalization is what makes the decomposition ruling
structural rather than a tuning promise: the shares sum to 1 by construction, so the tile's total can
never drift from `capacity_by_biome`. Adding a species to a biome *dilutes* the others; it does not
inflate the tile.

**`validate()` must reject:** any biome with non-zero `capacity_by_biome` that no species hosts (a
tile whose food has no name), a species with an empty `host_biomes`, a non-positive weight, and an
all-zero `yield` vector.

**A navigable hex has TWO capacity terms, so it has two baskets** (found during F1 — it silently
broke the sum on a whole class of tiles). A navigable river's forage capacity is *not* its
`capacity_by_biome` row — that row is **vestigial and bypassed** (`labor_config.json`'s own
`_comment_navigable_river`). It is `capacity_for(underlying) + navigable_river_forage_bonus`: the
valley the channel cut, **plus** a fishery. Composition must mirror that structure exactly —
weight the underlying biome's basket by `capacity_for(underlying)`, weight the `NavigableRiver`
basket by the bonus, merge duplicate species, renormalize. Decomposing only the underlying term
leaves the fishery **unnamed**, which is precisely the nameless food the coverage validator forbids,
arriving through a path the validator cannot see.

This is what makes the `NavigableRiver` host row *mean* something rather than being dead metadata:
it is **what the channel itself yields**, as distinct from the ground it flows over — which is why
`river_fish` hosts it alone. `forage::tile_flora_composition` is **the** seam (the twin of
`tile_forage_capacity`); no sim or snapshot path may call `FloraConfig::composition` on a raw
terrain.

### 4.2 What the patch carries

`ForagePatch` gains a composition. Two shapes were considered:

- **rung 1 — a mix.** The wild patch holds the tile's basket: `Vec<(species, biomass, capacity)>`,
  or (cheaper) one biomass scalar plus the derived share table, since nothing draws the components
  down independently at rung 1.
- **rung 2+ — a single species.** `Cultivate`/`Sow` commit the patch to one `species_id`. The rest
  of the basket is displaced — *that is the cost of tending*, and it is why committing a rich mixed
  tile to one crop is a real trade rather than a free upgrade. **How** it is priced is §4.3, which
  corrects the sketch this bullet originally carried ("capacity becomes share × the rung's
  multiplier" — that model is wrong, and §4.3 shows why).

**Recommendation: the cheap shape.** Store one `Option<FloraSpeciesId>` on `ForagePatch` (`None` =
wild mix) and derive the mix for display from the affinity table. A wild patch's biomass stays one
scalar, so the roster touches no ecology math and the rollback snapshot grows by one optional id.
Per-component wild stocks (differential depletion — gather the berries out and leave the acorns) is
a **deferred** enrichment, noted in §9.

**That field lands in F2, not F1** (corrected during F1 planning). In F1 nothing writes it — every
patch is `None` — so shipping it early would be dead snapshot plumbing with a rollback path no test
can exercise. F1 derives composition from the biome alone and adds no per-patch state; `Cultivate`/
`Sow` introduce the field in F2, in the same slice that first sets it.

---

### 4.3 What committing costs and what it buys — **K belongs to the land**

Settled during F2 planning, after the §4.2 sketch was worked through and found wrong. This is the
plant-side statement of the principle the animal side already rests on
(`plan_grazing_foundation.md` §2: *"K is not a property of the species. It is a property of the
range."*), which the plant ladder never had:

> **The land owns `K`. Rungs 1–3 redistribute it; only rung 4 (Worked Land) may raise it.**

Farming does not make ground more productive — clearing, weeding and sowing *point the ground's
existing productivity at you*. Raising `K` itself is irrigation, terracing, manuring: rung 4, by
definition. This finally gives rung 4 a mechanical identity instead of "a later, bigger step".

**Two terms, and only the second can pay.**

- **Concentration** — committing pushes one species toward monopolising the tile's human-edible `K`,
  bounded by how suited it is: `concentration = min(1.0, share × concentration_gain(rung))`.
  A plant that is already most of the basket can fill the tile; a marginal one cannot without
  inputs — which is, again, rung 4.
- **Conversion** — the rung's *real* payoff. A tended stand of one known plant is more edible and
  more harvestable per unit biomass: `provisions_per_biomass` rises. This is the yield vector §3
  already ships.

**Why concentration alone cannot be a rung payoff** (the error §4.2 originally encoded): it is
capped at `K`, and a *wild* patch already yields the **whole basket** — the full `K`. So committing
an alluvial tile to Wild Emmer (share 0.56) at any concentration gain yields `≤ 1.0 K` against
wild's `1.0 K`, and tending would be a strict **downgrade**. Concentration can only ever redistribute
what wild already gave you. **Tending must pay in conversion, or it does not pay.**

So the trade is:

```
tended pays   tended_regrowth_gain × concentration × species_rate    vs.   wild pays   1.0 × base_rate
```

**The `tended_regrowth_gain` term is not decoration — it dominates, and omitting it was a real bug.**
S1 first shipped this inequality *without* it (and a sweep test that "verified" the published ratio on
a **capacity** basis, where `r` cancels — so the test shared the code's wrong assumption and passed
vacuously). Every Cultivate ratio on screen was understated by exactly the gain (2.0): a delta tile
showed `Sustain +0.64` / `Cultivate +1.28` in its policy chips while the crop row for the same tile
claimed `0.9×`, i.e. that tending *loses*. Corrected, that crop is `1.8×`.

The rule this produced, now in `core_sim/CLAUDE.md`: **assert a published quote against the payoff
functions themselves, never against a re-derivation of their arithmetic** — and a test for a
correctness bug must be shown to *fail* against the unfixed code before it is trusted.

**What the corrected numbers say — and it is a finding for S2, not a crisis.** With the gain in,
*almost everything is worth tending*: best-country ratios run 2.3–2.7×, and only two rows lose
anywhere at all (berry_scrub 0.70 and wild_tubers 0.97, both on RollingHills). So the claim above —
that a minor-share commitment is a **structural loss** — is **too strong as stated**: the gain swamps
the concentration penalty. What survives is weaker but still real: *within one tile* the spread is
large and the ranking matters (RollingHills offers hazel 1.35, wild_emmer 1.20, wild_tubers 0.97,
berry_scrub 0.70 — best to worst is nearly 2×, and the bottom of the list does lose).

This is precisely the double-count §4.3 predicted and handed to **S2**, now measured rather than
suspected: `tended_regrowth_gain` 2.0 was carrying weight that concentration now carries explicitly.
S2 decides whether the gain comes down until a marginal commitment genuinely loses again, or whether
"every crop pays, but some pay far better" is the honest model. Do **not** settle that here — S1
deliberately moves no payoff dial. `forage.provisions_per_biomass` (0.05) stays the **wild**
rate — you gather the whole basket at the basket's average — so **rung 1 remains exactly as F1
shipped it**, and the roster stays economy-neutral until you actually commit a patch.

**Two corrections to the shipped payoff model, both settled here rather than deferred:**

1. **`field_provisions_per_biomass` (0.02) was the right-shaped lever all along** — it is a
   *conversion* rate. An earlier F2 draft called rung 3's "currency change" a smell and proposed
   unifying it; **that was wrong and is withdrawn.** At rung 3 you control reproduction, so there is
   no wild stock left to over-skim, the policy axis honestly collapses, and a flat managed rate on
   the standing crop is correct. The `labor_config.rs` monotonicity guard is therefore policing a
   *legitimate* difference between two currencies, not an inconsistency — it stays.
2. **`tended_regrowth_gain` (2.0) is half right and needs a retune, not a deletion.** Freed from
   competitors a stand genuinely does regrow faster toward its own ceiling, so the lever is not
   fake — but once concentration is modelled *explicitly*, part of what the 2.0 was silently
   standing in for is double-counted. **S2** retunes it, and restores a rung-2 conversion gain.
   Note `tended_provisions_per_biomass` was tried and retired before, for a reason that does **not**
   apply here: it turned rung 2 into a flat *managed rate*, collapsing the policy axis a rung before
   the animal side does. A conversion gain and a managed rate are separable — rung 2 can pay a
   better rate while still drawing its stock down and still being over-farmable.

**Staging** (each slice measured before the next):

- **S1** — species commitment + concentration + the yield vector as the conversion rate. Rung payoff
  dials **untouched**, so any balance movement is attributable to the roster alone.
- **S2** — retune rung 2 with competitor-removal now explicit (`tended_regrowth_gain` down; restore
  a conversion gain without collapsing the drawdown axis).
- **S3** — *a question, not a task.* Revisit rung 3's currency only if S1/S2 data says it is wrong;
  the expectation after correction 1 is that this slice never happens.

---

## 5. Fodder — the coupling, both halves

This is the arc's load-bearing piece and the one place it reaches into the animal web. Design it as
a coupling, not a lever (`plan_intensification_ladder.md` §2's own instruction).

**Two decisions settled at F3 planning** (they resolve a contradiction the earlier draft carried —
the §5 formula said fodder raised the pen's *ceiling*, while properties 2–3 described it paying the
*feed bill*; those are different mechanisms and the doc asked for both without saying so):

> **Fodder is delivered graze-flow.** Hay is grass you grew and stored; it enters the pen economy at
> **exactly the point graze does**, so it raises `K_pen` **and** pays down the lossy larder bill in
> one term — because it *is* feed. And **the whole loop ships in one slice** (F3): the fodder crop,
> the store, the `Foddering` capability, and the pen's draw, measured together against the pen
> economy's existing invariants.

### 5.1 The plant half — grow the hay

A **fodder crop** is an ordinary Field of a species whose yield vector is **fodder-dominant**
(`fodder_per_biomass > 0`, `provisions_per_biomass ≈ 0`). Sowing it needs **no new plant knowledge**
— `Sow` already exists and is gated on Seed Selection. Its harvest does **not** credit provisions; it
credits a **fodder store**, which is a **second commodity key** in the band's existing `LocalStore`
(the commodity-keyed larder from the population arc, already snapshot-persisted): `FODDER = "fodder"`
beside `FOOD = "provisions"`. No new resource type, no new persistence path — the store round-trips
for free, and the supply network can already balance any commodity (deferred: whether fodder *should*
flow over it — v1 keeps it band-local).

The fodder Field is otherwise a normal rung-3 Field: same `Sow` site rule (rich, watered ground),
same build, same feral-if-abandoned. It just pays in hay. So **your best cropland is now contested**:
grain (calories) *or* hay (herd ceiling) from the same scarce sowable tile — the §4.3 land-use
tension, extended to the animal web.

### 5.2 The animal half — `Foddering`, and one augmented flow

**`Foddering`** is a faction **capability knowledge** (discovery id **2007**, next free), earned by
**running a pen** — the `animal:pen` rung's `earns_knowledge` (`null` today) becomes `foddering`, so
*you learn to hay a herd by keeping one*. It is **not** a new ladder rung with a verb or a build
meter (a pen already exists; foddering only unlocks the store-draw), and it is never start-granted.
Once known, a pen automatically draws hay when a fodder store is in reach.

**The model — fodder is a flow that supplements the footprint, drawn before the larder.** Today
(`fauna::advance_herd_grazing` + the corral feed branch):

```
demand          = fodder_per_biomass × biomass
footprint_intake= graze the fenced footprint yields           (from GrazeRegistry)
pasture_fraction= clamp(footprint_intake / demand, 0, 1)
larder_upkeep   = upkeep_per_biomass × biomass × (1 − pasture_fraction)   ← the LOSSY human-food bill
```

F3 inserts hay **between** the footprint and the larder:

```
shortfall       = max(0, demand − footprint_intake)
fodder_draw     = min(shortfall, band FODDER store, [faction knows Foddering])   ← hay covers the gap
fed_by_land+hay = footprint_intake + fodder_draw
larder_upkeep   = upkeep_per_biomass × biomass × (1 − fed_by_land+hay / demand)  ← same bill, smaller
```

and the **ceiling** reads the same augmented flow (`ecological_carrying_capacity`, the one `K` seam):

```
K_pen = (footprint_graze_flow + fodder_delivery_rate) / fodder_per_biomass
```

where `fodder_delivery_rate` is the hay the store can sustain per turn (store-limited; in steady
state = your fodder Fields' output rate, since inflow = outflow). So **one term does both jobs**: hay
raises `K` (the herd grows) *and* it is subtracted from the larder bill before the lossy path (the
pen stops draining bread) — because delivered hay and grazed grass are the same quantity, `fodder`.

### 5.3 The four properties, re-checked against the resolved model

1. **The land still decides — but now it is *your fields' land too*.** The pen's ceiling stops coming
   from its own tile and starts coming from your farming (`K_pen` reads your fodder output). This
   makes the plant ladder a hard **prerequisite** for a big pen, which is the coupling the whole arc
   was reaching for.
2. **Hauling human food to livestock stays wasteful — untouched.** `larder_upkeep` is the *same*
   deliberately-lossy provisions bill; hay is drawn *before* it, so growing hay *shrinks* it but never
   makes it a better deal. Feeding a pen bread is exactly as bad as today; feeding it hay is the point
   of having grown hay. (`fodder` and `provisions` are separate `LocalStore` keys — a fodder crop
   fills one, the population eats the other; they never convert.)
3. **"A dead tile cannot hold a pen" is deliberately RELAXED — a feedlot is real.** The literal
   grazing-foundation statement is overturned (accepted at planning): with hay as flow, a pen on thin
   or barren footprint *can* be carried by delivered fodder — that is a drylot, historically exactly
   what hay is *for*. What keeps "the land decides" honest is **not** a dead-tile block but that the
   hay must be **grown on real farmland** (a fodder Field needs rich, watered, sowable ground) and
   **delivered within the band's work range**. Land-relevance moves from the pen's tile to your
   fields' tile — mixed farming, not an exemption.
4. **Convergence must be proven, not assumed.** `K → biomass → demand → fodder_draw → flow → K` is a
   coupled loop, exactly like the graze loop grazing 2b-ii had to gate with a convergence test. F3
   owes the same: a store-limited `fodder_delivery_rate` (bounded by what the store holds and the
   field output that fills it) so the loop settles rather than runs away. **Ship a convergence test
   before betting the pen economy on it.**

**Delivery.** v1: a fodder Field within the **owning band's work range** feeds the pens that band
keeps (the keeper band draws its own `FODDER` store). Spatial by construction — you cannot hay a pen
from across the map. Routing fodder over the supply network is a follow-on, deliberately deferred.

**Overwintering** falls out for free: the store is a **stock**, so a fodder buffer carries a herd
through a seasonal graze trough with no seasonal special-case.

### 5.4 What F3 must not break

The pen economy is tightly validated and this slice reopens it — re-measure, do not assume:
- **The net-positive floor** (`FaunaConfig::validate`, grazing 2d §2.4) — fodder lowers the larder
  bill, which can only *help* a pen's net, but the floor's derivation reads `upkeep × biomass ×
  (2 + r)/4`; confirm the `(1 − fed/demand)` factor doesn't invalidate its scale-free argument.
- **The convergence gate** (grazing 2b-ii) — the new flow term joins the coupled loop; the existing
  convergence tests must still pass and a fodder-specific one must be added.
- **`pen_fed_fraction` / starvation** — a pen with hay in the store is *fed*; the starvation shrink
  and its one-turn-lag flag must read the hay-inclusive fed fraction.
- **Rollback** — the `FODDER` store rides `LocalStore` (already persisted); nothing new to plumb, but
  the herd's per-turn `fodder_draw` (if cached like `footprint_intake`) is transient, not persisted.

---

## 6. Cash crops — differentiating the trade scalar

`trade_goods_per_biomass` is currently one flat rate on each web. A cash crop is a species whose
yield vector is trade-dominant and whose `provisions_per_biomass` is **zero or near it** — the
tension is structural, not a penalty: a cash Field occupies food-bearing ground (rung 3's
`site_requirement`) and pays no calories. Calories *or* cash from the same scarce good tile.

Because rung 3 is gated to the rare rich-and-watered country, cash crops compete for **exactly the
land the game already made scarce** — so they inherit the scarcity that makes the choice real rather
than needing their own.

**Per-kind trade goods** (tobacco vs cotton vs sugar as *distinct* goods, with distinct demand) is
where the yield vector meets a Market that can price them. This arc **ships the supply side and the
data shape**; it does not build market pricing. When the Market/yield-vector arc lands, it reads
`yield.trade_goods_per_biomass` per species and can extend the vector to a per-good map without
re-cutting the schema.

---

## 7. What this does *not* change

Stating the blast radius, because the roster looks bigger than it is:

- **No new rungs, no new verbs on the plant side.** `Cultivate` / `Sow` already exist.
- **No change to `capacity_by_biome`.** The decomposition ruling forbids it. Retuning the human web
  is a food-economy edit and must not ride in on a roster PR (the same rule the fauna arc adopted for
  the abundance cap).
- **No change to `FoodModule` or `seasonal_weight`.**
- **No worldgen change.** Composition is derived from biome; nothing new is stamped on the map.
- **One new discovery id (2007, `Foddering`)**, needing a `start_profile_knowledge_tags.json` mapping
  and appearing in **no** start profile's `starting_knowledge_tags` — nothing is start-granted.

---

## 8. Phasing

Per the arc plan: **spec the whole roster in this doc, hand-implement a couple to prove the stat
block carries its weight, then mass-fill.**

- **F1 — Schema + loader + decomposition (no economic change).** `flora_config.json`, `FloraDef`,
  `validate()`, the derived share table, and the wire/tile-card readout. No per-patch state (see
  §4.2). *Verification: the economy is provably unmoved* — every F1 species carries **today's flat
  yield values verbatim** (`provisions_per_biomass` 0.05, `trade_goods_per_biomass` 0.005), and the
  shares sum to the same capacity, so nothing can move by construction. The vector is *parsed and
  validated only* in F1 — the same "ship the shape, read it later" discipline the ladder's
  `feeding`/`harvest` primitives used. Wire the tile card so you can *see* what grows where before
  anything depends on it (graze 2a's discipline: ship the layer, look at a real map, then bet on it).

  **Coverage forces breadth before depth.** `validate()` rejecting an unnamed non-zero biome means
  F1 cannot ship "a couple of species" — it must cover all 32 non-zero biomes or the game has
  nameless food. So F1 ships a **complete but coarse** roster (~12 broad families, each hosting many
  biomes); F5 refines it into the fine-grained one. The strict validator is the right trade: it is
  the same "zero must be stated" discipline `capacity_by_biome` already enforces, and a permissive
  "unnamed remainder" would quietly become permanent.
- **F2 — The rungs get a subject.** `Cultivate`/`Sow` select a species; the yield vector drives the
  harvest; the displaced basket is the cost of committing. *This is the first slice that moves
  balance* — measure it in a live campaign.
- **F3 — Fodder, both halves.** Fodder store, the fodder Field, `Foddering` (2007), the `K_pen`
  term. Measure: a pen on thin pasture must be *survivable but expensive*, never free.
- **F4 — Cash crops.** Trade-dominant vectors, the land-use tension, per-species trade rate replacing
  the flat one.
- **F5 — Mass-fill + client.** The full roster across all non-zero biomes, icons, labels, tile-card
  composition readout.

Each slice is independently shippable and independently measurable. F1 and F5 are content; F2–F4 are
the ones that need a playtest.

---

## 9. What must be measured, not assumed

The lesson of PR #119 and of grazing 2b: levers that pass every unit test can still be badly wrong.

1. **Every non-zero forage biome is named, and its basket reads sensibly on a real map** (F1). Look
   at the tile cards, not the table.
2. **F1 moved nothing.** Map-wide and per-start food capacity identical to pre-arc. If it moved, the
   normalization is wrong.
3. **Committing a patch is a real trade** (F2) — tending a rich mixed tile to one crop must
   sometimes be *worse* than leaving it wild. If it is always an upgrade, the displaced basket is
   priced too cheaply and rung 2 is a free lunch again.
4. **Fodder does not become the strategy** (F3). A fed pen must cost a field. Re-measure the herd
   ladder: `K_pen` gains a term, so every penned species' equilibrium moves.
5. **Cash crops are refused on thin ground** (F4) — if they are worth sowing everywhere they are
   priced wrong.

---

## 10. Deferred (tracked, not built)

- **Per-component wild stocks** — gather the berries out and leave the acorns (§4.2). Real, and it
  would make wild depletion selective; not foundational.
- **Fodder over the supply network** — hay hauled beyond a band's work range (§5).
- **Per-good trade demand** — tobacco vs cotton as distinct priced goods; belongs to the Market arc
  (§6).
- **Rung 4 plant side (Worked Land)** — irrigation/clearing making unwilling ground farmable. Owned
  by `plan_intensification_ladder.md`, not this arc; the roster is orthogonal to it.
- **Seasonality of composition** — a biome's basket shifting across the year. `seasonal_weight`
  already exists on `FoodModule`; per-species seasonality is a later enrichment.

---

## See Also

- `docs/plan_grazing_foundation.md` — the two food webs and the two capacity tables this decomposes.
- `docs/plan_intensification_ladder.md` — the rung grammar; §2's rung-4 Fodder note is *supplied* by
  §5 here.
- `docs/plan_grazing_2d.md` — the pen economy (`K_pen`, footprint, larder) fodder extends.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §2a — the player-facing ladder
  vocabulary this roster speaks.
- `TASKS.md` → *Fauna Roster* — the roster pattern this parallels.
