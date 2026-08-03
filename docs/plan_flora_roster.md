# Flora Roster & Ecology — Named Plants, and the Yield Vector They Carry

**Status:** design. Opens the flora content arc (issue #202, *Flora Roster & Ecology*), the plant
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
- **rung 2+ — one FAVORED species, not a single one.** `Cultivate`/`Sow` commit the patch to one
  `species_id`, and that commitment **reweights the basket** rather than replacing it: Tended weeds
  the favored plant's share upward, Field forces it to 100%. The rest of the basket is displaced
  only as far as the rung actually displaces it, and the tile's `K` never moves. **How** it is
  priced is §4.3, which corrects two sketches this bullet carried in turn — first "capacity becomes
  share × the rung's multiplier", then the shipped concentration term that cut a committed tile's
  `K` outright and discarded the remainder.

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

### 4.3 What a rung changes — **the land owns `K`, and no rung lowers it either**

Settled during F2 planning, corrected in full after the S1/S2 model was measured (#433). This is the
plant-side statement of the principle the animal side already rests on
(`plan_grazing_foundation.md` §2: *"K is not a property of the species. It is a property of the
range."*), which the plant ladder never had:

> **A tile's production is CONSTANT across rungs 1–3.** The land owns `K`. No rung below 4 raises it
> — raising it is clearing, irrigation and manuring, i.e. Worked Land, by definition — and **no rung
> lowers it.** A tile always produces its full capacity, always made up of every plant its
> composition currently holds.

What a rung changes is only **which plants that constant production is made of**. A Field is not a
smaller patch growing one crop; it is the same tile producing the same amount, all of it one crop —
the degenerate case of the same basket, with a single member at 100%. Wild, Tended and Field are one
representation — a weighted composition over the tile's full `K` — at three settings, not three
different things.

**The three settings.**

- **Wild** — the basket as realized (§10). Every rate is the share-weighted average of the members'
  yield vectors.
- **Tended — weeding.** The favored species' share rises to `min(1.0, share × tended_weeding_gain)`.
  The increase is taken from the **least abundant remaining species first**, which may drive one to
  0%. Every surviving member still produces, at its own rate. `K` unchanged.
- **Field — planting.** The favored share is forced to `1.0` regardless of what it started at; the
  rest go to 0%. `K` unchanged — the tile's whole production is now that one crop.

> **Tended weeds what is already there. Field plants what isn't.**

That is the whole distinction, and it is what finally gives Tended a meaning of its own: it is
bounded by the tile's existing composition, where Field ignores it.

**Least abundant first, deliberately not lowest yielding.** Ranking the weeds by yield would require
the sim to compare a food rate against a trade rate — an exchange rate that does not exist in this
codebase (it is the Market arc's) and would be silently hardcoded here. `hay_grass` (0 food, 0
trade, 0.2 fodder) has no non-arbitrary rank at all. Abundance is currency-free, deterministic from
the composition alone, and independent of which crop was favored, so it survives the Market arc
unchanged.

**The invariant reaches rung 1, and that is the point.** A wild patch pays the share-weighted average
of **its own realized basket**, in every currency. It did not before: `patch_provisions_per_biomass`
fell back to the flat `forage.provisions_per_biomass` (0.05) for any uncommitted patch and never read
the tile's composition, and trade reached a wild patch only through `Deplete`'s equally species-blind
`market.trade_goods_per_biomass`. So *what was growing there* had no bearing on what the tile paid.
A single constant cannot be the average of a per-tile basket: the worked basket below averages
**0.0654** food/biomass (under-paid by 31%) while the full AlluvialPlain host table averages
**0.0378** (a tile realizing the other end was over-paid by 32%). Under the invariant,
`Sustain`-foraging wild ground that happens to hold cash crops **does** return trade goods — you
gathered them. `forage.provisions_per_biomass` survives as the empty-basket fallback and as the
rung-3 quality normalization baseline; it is no longer the wild rate.

**Two terms, and both of them pay.**

- **Weeding** changes *which plants* grow there — the composition above. It can only ever move share
  between the members of a basket the tile already holds, so on its own it is bounded by *the tile
  becomes 100% of its best plant* and **saturates there**: past that point no setting of
  `tended_weeding_gain` moves anything.
- **Conversion** — the rung's other half. A tended stand of a *known* plant is more edible and more
  harvestable per unit biomass, so the favored species' whole yield vector is multiplied by
  `tended_conversion_gain`. This is the debt correction 2 below recorded and S2 left unpaid.

**The conversion gain is on the FAVORED species' term only, and that is load-bearing.** Tending is
knowing *your* crop; the volunteers still standing in the field are still wild. A blanket multiplier
over the whole basket would pay ~`gain` for *any* commitment regardless of what was favored, which
erases the crop choice and with it the "committing must sometimes be worse" bar (§9). On the favored
term it **compounds** with weeding — favoring a plant that already dominates pays twice, favoring a
marginal one barely moves the number. It multiplies food, fodder and trade alike; no `role` branch.

**Why weeding alone could never have carried the rung.** Measured on the worked basket against
Cultivate's real cost (25 turns at `yield_fraction_while_building` 0.25, i.e. 0.75 × wild forgone per
turn), with no conversion gain the payback is **113 turns** at weeding gain 1.5 and **84 turns** at
gain 2.0 — where it **saturates**, so 3.0 and 4.0 buy nothing further. Nobody pays a 25-turn
investment for an 84-turn payback. That is not an argument against the model; it is this section's
own older conclusion arriving again — **"tending must pay in conversion, or it does not pay"** — and
it is why the two terms ship together.

**What the model buys, measured.** Realized basket `wild_emmer 0.50 / wild_tubers 0.39 / grapevine
0.11`, capacity 195 throughout, `tended_weeding_gain` 1.5, `tended_conversion_gain` 2.0:

| state | food/biomass | trade/biomass |
|---|---|---|
| wild, as it grows | 0.0654 | 0.0221 |
| Tended, favoring wild emmer | 0.1363 | 0.0088 |
| Tended, favoring wild tubers | 0.1093 | 0.0079 |
| Tended, favoring grapevine | 0.0618 | 0.0570 |
| Field of wild emmer | 0.0800 | 0.0050 |
| Field of grapevine | 0.0000 | 0.1600 |

Same tile, same turn, opposite directions: tending toward grain roughly doubles the food and gives
up 60% of the trade; tending toward the cash crop costs 5% of the food and pays **2.6×** the trade.
The grain commitment's payback is ~17 turns against a 25-turn build. Before this, that decision did
not exist — every commitment was a capacity cut.

**The consequence worth naming: cash and calories now compete INSIDE one tile's basket, not only
across tiles.** On a realization holding both a staple and a cash crop, weeding the staple upward
eats the cash crop — so tending a grain tile *lowers* its trade income, measurably. A tile that grows
both is no longer a tile that gives you both; it is a tile you have to point one way or the other.
That is the §6 land-use tension arriving a rung earlier than F4 could put it, and it is the reason
the rung-2 decision survives losing the old "tending might be worse than wild" trade.

**What the model costs: rung 2 is no longer sometimes-worse-than-wild in food terms.** The
conversion gain applies to whatever you favored, so a commitment to a plant with any real share
pays. The trade the player is making is no longer *wild vs. tended* — it is **which currency**, plus
the 25 turns and the place-pinning the build costs. §9's bar moved with it: the thing that must
sometimes be worse is a *crop choice*, not the rung.

**Two corrections to the shipped payoff model, both settled here rather than deferred:**

1. **`field_provisions_per_biomass` (0.02) was the right-shaped lever all along** — it is a
   *conversion* rate. An earlier F2 draft called rung 3's "currency change" a smell and proposed
   unifying it; **that was wrong and is withdrawn.** At rung 3 you control reproduction, so there is
   no wild stock left to over-skim, the policy axis honestly collapses, and a flat managed rate on
   the standing crop is correct. The `labor_config.rs` monotonicity guard is therefore policing a
   *legitimate* difference between two currencies, not an inconsistency — it stays, and it gains the
   conversion gain as a factor, evaluated at tending's saturated best case so the crop's own rate
   cancels from both sides and the check stays scale-free.
2. **The rung-2 conversion gain, recorded as a debt in S2 and paid here.** `tended_regrowth_gain`
   was retired to a neutral 1.0 on the correct reasoning that a growth boost double-counts
   competitor-removal — but nothing replaced it, and rung 2 was left with concentration alone, which
   this section had already proved could not pay. `tended_conversion_gain` is what should have
   landed then. Note `tended_provisions_per_biomass` was tried and retired before, for a reason that
   does **not** apply here: it turned rung 2 into a flat *managed rate*, collapsing the policy axis a
   rung before the animal side does. A conversion gain and a managed rate are separable — rung 2 pays
   a better rate while still drawing its stock down and still being over-farmable.

**`Deplete`'s markup is a policy, not a rung.** With every drawn-down harvest paying the basket's
trade vector, `market.trade_goods_multiplier` stops being *the* way a wild patch sells and becomes
what its name always implied — **sell harder**, a markup on goods you were already producing. It
therefore applies at rung 1 and rung 2 alike, so trade is credited once and from one rule at every
drawn-down rung. Rung 3 keeps its no-markup rule: a Field is never drawn down and has no policy axis.
`market.trade_goods_per_biomass` is retired — the basket vector is the rate. Every staple carries
`trade_goods_per_biomass` 0.005, so a staple-only basket's wild `Deplete` sale is numerically
unchanged; only baskets holding a cash crop move.

**Fodder at rung 1 is gated at the consumer.** The invariant reaches fodder too — a wild tile
realizing `hay_grass` pays hay on any harvest — but crediting it to a band with nowhere to put it
would hand out animal feed nobody bid for. So the **uncommitted** patch's fodder credit is gated on
the faction knowing **Foddering**, the same gate the pen's draw already reads, at the credit site
rather than in the rate seam. A **committed** patch is ungated — and the predicate is the
**commitment** (`patch.species`), not the rung, so the gate lifts on the first turn of a
`Cultivate`/`Sow` build, while the patch still stands at rung 1 and still converts at the wild
basket's rate: committing a patch to `hay_grass` *is* the bid, and the bid is placed when the crew
starts, not when the meter fills.

**Where Tended and Field coincide, they are allowed to.** On a tile whose favored crop is already
dominant, weeding reaches 1.0 and the two rungs' compositions become identical. What still separates
them is real — a Tended Patch draws its stock down, is policy-live and can be over-farmed, while a
Field pays a flat managed rate and never depletes — and the ladder does not owe every tile a distinct
rung-3 story. Sowing such a tile buys stability and a higher rate, not composition.

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

> **F4 landed (supply side).** The **trade account now routes per-species** exactly as fodder does:
> a Field's harvest credits `field_trade_goods` (biomass × the field rung's one rate dial ×
> `species_trade_rate / wild_provisions_rate`) to the faction **`trade_goods` stockpile** — a
> faction-level commodity, so unlike FOOD/FODDER it lands in `FactionInventory`, mirroring the Market
> wild-sale arm. **No `role` branch**: a staple's `field_trade_goods` is the negligible flat token, a
> cash crop's is dominant, all through one commodity-generic seam. The **wild Market path stays flat
> and unchanged** — `field_trade_goods` deliberately does **not** apply the Market
> `trade_goods_multiplier`, which is a Market-policy markup for wild commercial gathering, not a
> managed harvest. Four cash crops ship — cotton (0.20) / flax (0.15) / tobacco (0.18) / tea (0.16),
> playtest dials — hosted **honestly on the river valleys** (cotton/tobacco/flax on
> AlluvialPlain/Floodplain/RiverDelta, tea on the uplands): **per-tile realization (§10) is what keeps
> the staples dominant on their own realized tiles** (the local-share commit bar), so cash and grain
> share that ground without a global % table eroding wheat. The picker quote is `commit_trade_payoff`
> → `FloraShareInfo.sowTradePayoff`. **Client done**: the native reader decodes `sowTradePayoff` and
> the crop picker renders a cash-crop trade row (`FLORA_CROP_TRADE_ROW_FORMAT`). The cash **badge** was
> deliberately omitted for parity with fodder — no role-badge mechanism exists for either.

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
- **F4 — Cash crops. LANDED.** Trade-dominant vectors, the land-use tension, per-species trade rate
  replacing the flat one — the exact twin of F3's fodder work. `field_trade_goods` routes the vector's
  trade component to the faction `trade_goods` stockpile (`commit_trade_payoff` →
  `FloraShareInfo.sowTradePayoff`); the wild Market path stays flat. Four crops (cotton/flax/tobacco/
  tea). Client done: native `sowTradePayoff` decode + crop-picker trade row
  (`FLORA_CROP_TRADE_ROW_FORMAT`); the cash badge was intentionally omitted for fodder parity.
- **§10 — Per-tile realization. LANDED.** Split affinity (*what can grow*) from realization (*what is
  growing*): `tile_flora_composition` now realizes a seeded 2–4-species subset per tile, so two tiles
  of one biome differ. Dissolved the F4 dilution bind — cash crops re-hosted honestly on
  AlluvialPlain/Floodplain/RiverDelta (cotton/tobacco/flax) and the uplands (tea), and the commit bar
  reframed around the tile's local realized share. Client shows the varying basket for free (needs a
  two-tile ui_preview fixture).
- **F5 (mass-fill half) — LANDED.** 15 new species added to `flora_config.json` (kelp, sea_kale,
  wild_rice, cattail, chestnut, wild_orchard, sunflower, wild_pulses, mesquite, wild_fig, cloudberry,
  rock_tripe, alpine_herbs, cave_fungi, grapevine — grapevine a 5th cash crop), taking the roster
  18 → **33** so every non-zero biome now carries a **3–5 species basket** and per-tile realization
  (§10) has the breadth to vary tile-to-tile. Yields tuned to hold the per-realization commit bar:
  the wild-ceiling gathers sit at/below the 0.045 baseline (rock_tripe the 0.040 famine floor), and
  the crowded tended crops (cattail, wild_orchard, alpine_herbs 0.069; wild_fig 0.064) were lifted
  above their table starting rates because their best realized share caps ~0.5 against the dominant
  incumbents (reed/berry/shellfish/date_palm/pine_nut). grapevine's AlluvialPlain weight is 0.10 (not
  the spec's 0.15) so it is a *unique* minimum-weight there — a tie with wild_rice broke the
  realization aggregate-ordering guard. **The tile-card "what grows here" composition readout ships
  in this PR too** (one row per realized species + its share), so realization is visible on plain tile
  inspection. **The one client follow-up that remains is per-species flora icons** (there is no
  per-species flora icon lookup yet — display names ship as plain text and every basket row wears one
  generic plant glyph), tracked separately (#339); labels and the readout are done.

Each slice is independently shippable and independently measurable. F1 and F5 are content; F2–F4 are
the ones that need a playtest.

---

## 9. What must be measured, not assumed

The lesson of PR #119 and of grazing 2b: levers that pass every unit test can still be badly wrong.

1. **Every non-zero forage biome is named, and its basket reads sensibly on a real map** (F1). Look
   at the tile cards, not the table.
2. **F1 moved nothing.** Map-wide and per-start food capacity identical to pre-arc. If it moved, the
   normalization is wrong.
3. **A crop choice is a real trade** (#433) — *which* plant you favor must sometimes be worse than
   favoring another, and tending toward cash must cost real food. What is **no longer** the bar is
   "tending must sometimes be worse than wild": with §4.3's conversion gain, any commitment with a
   real share pays, and the decision the player makes at rung 2 is which currency plus whether the
   25-turn build and the place-pinning are worth it. If instead *every crop on a tile* pays about
   the same, the conversion gain has leaked off the favored term onto the whole basket and the crop
   picker is decorative.
4. **Fodder does not become the strategy** (F3). A fed pen must cost a field. Re-measure the herd
   ladder: `K_pen` gains a term, so every penned species' equilibrium moves.
5. **Cash crops are refused on thin ground** (F4) — if they are worth sowing everywhere they are
   priced wrong.
6. **Realization moves nothing at the wild rung** (§10) — map-wide and per-tile wild forage capacity
   identical to the pre-realization uniform basket (a tile's realized shares still sum to 1). And
   **two tiles of one biome show different baskets** — look at the tile cards, the point of the whole
   slice. The map-wide realized species mix must still track the affinity table within tolerance.
7. **Rung 1 stops being economy-neutral, within a bound** (#433). Once a wild patch pays its own
   basket's average instead of a flat constant, per-tile wild income moves in **both** directions —
   roughly −32% to +31% on the sampled baskets — and those deviations should very largely cancel in
   aggregate. So: **map-wide wild food income within ±5%** of the flat-rate total on the standard
   map, and the per-tile spread **recorded, not bounded**. A figure outside ±5% means the basket
   averages are wrong, not that the balance needs tuning — do not dial anything to hit the bar.
   **The bar needs a liveness assertion beside it or it passes when the feature is dead:** a
   `basket_rate` that silently fell through to the `0.05` fallback everywhere would score a *perfect*
   1.00 ratio. So the same measurement asserts the spread is non-degenerate — most food-bearing tiles
   differ from the flat rate by more than 1%, and the map's max/min basket rate is well above 1.

---

## 10. Per-tile realization — "what *can* grow" vs "what *is* growing"

> **LANDED.** `FloraConfig::realized_composition` / `realized_navigable_composition` +
> `tile_flora_composition(.., map_seed)`; dials `realized_species_min` (2) / `realized_species_max` (4)
> in `flora_config.json`. Cash crops re-hosted honestly (cotton/tobacco/flax on the river valleys, tea
> on the uplands). Pinned by `core_sim/tests/flora_realization.rs` (neutrality, variance, aggregate,
> bit-exact determinism) and the reframed `flora_roster.rs`/`flora_commitment.rs` commit-bar tests. See
> `core_sim/CLAUDE.md` → "Per-tile realization". Client render is free (needs a two-tile ui_preview
> fixture); "Sow reads affinity everywhere" stays deferred (§11).

**The problem F1–F4 left standing.** `FloraConfig::composition(terrain)` is a **per-biome** share
table, and *every tile of a biome publishes the identical basket*. So every alluvial plain is
byte-identical — wheat + tuber + hay at the same percentages — and none is a wheat tile while its
neighbour is a tobacco tile. The roster answers **"what can grow here"** and nothing answers **"what
is *actually* growing here"**, which should be a *subset*.

That uniformity is also the sole cause of the F4 cash-crop placement bind: adding tobacco to
AlluvialPlain dilutes wheat's share on **every** alluvial tile at once, so it trips the rung-2
commit bar (`flora_roster.rs`) everywhere simultaneously — wheat had only ~0.08 of weight headroom
on its one passing biome. That is a tuning artifact of a global % table doing a per-tile job, not a
real economic constraint.

**The model: split affinity from realization.**

| concept | function | meaning | keyed by |
|---|---|---|---|
| **affinity** | `FloraConfig::composition(terrain)` (existing, unchanged) | *what CAN grow here* | biome |
| **realization** | `tile_flora_composition(map_seed, tile, terrain, flora)` (extended) | *what IS growing here* | **tile** |

- **Realization is a seeded subset of the affinity roster.** For each tile, draw
  `k = clamp(realized_species_count, 1, hosted_count)` species from the biome's affinity roster by
  **weighted sampling without replacement** (probability ∝ affinity weight), then renormalize the
  picked species' weights into local shares. `k` is a small config range
  (`realized_species_min` 2 / `realized_species_max` 4, playtest dials; clamped to the number the
  biome actually hosts).
- **Deterministic, no stored state.** The draw's entropy is a pure hash
  `splitmix64(map_seed ^ FLORA_REALIZATION_SALT ^ fnv(tile.x, tile.y))` — no RNG stream, no
  `HashMap` iteration order (sort the roster before sampling), f32 sums ordered (the existing
  `build_composition` determinism discipline). A pure function of `(seed, tile, terrain, affinities)`
  ⇒ **deterministic under rollback for free** and **no snapshot/wire bloat** — realization is
  *derived*, exactly as "naming decomposes, it does not add" intends. Ties break by species key
  ascending.
- **`tile_flora_composition` stays THE seam** every caller reads (it already blends the navigable
  channel + underlying biome; realization applies to the biome basket inside it). It now takes the
  seed + tile. This one function feeds wild-gather display, rung-2 Cultivate legality + the
  concentration term, and the wire `ForagePatchState.composition`.

**Invariants — realization moves nothing at the wild rung** (the same discipline as F1's "F1 moved
nothing", now per-tile):
- **Per-tile neutrality holds by construction.** Realized shares renormalize to sum to 1, so
  `Σ share × capacity == capacity` on every tile — a tile still yields its **full biome capacity**
  gathered wild, just composed of different species. Wild forage income is byte-identical; only
  *which* species name a tile's basket changes. Pinned.
- **Map-wide aggregate ≈ affinity.** Averaged over a biome's tiles the realized composition
  approximates the affinity table (weighted sampling is ~unbiased), so the biome-wide species mix is
  unmoved. A **measured** test (tolerance), not a hard assert.
- **The commit bar becomes per-tile, and that dissolves the dilution bind.** A species is "worth
  committing" on a tile where it **realizes a high local share** — not where it is absent or
  marginal. With 2–4 species per tile instead of the whole roster, local shares are *large*, so a
  wheat-dominant tile clears the rung-2 bar for wheat and a tobacco-dominant tile clears it for
  tobacco. Adding cash crops to a biome no longer erodes the staples — each dominates its **own**
  realized tiles. `every_climbing_species_is_worth_committing_on_its_best_country` is reframed around
  the *realized* share (its best realization vs its worst), not the uniform biome share.

**Consequences this unlocks:**
- **Cash crops go on the land they'd grow on.** With dilution gone, host cotton/tobacco/flax on
  AlluvialPlain/Floodplain/RiverDelta (warm/temperate river valleys) and tea on
  RollingHills/MixedWoodland/HighPlateau (hill crop), at honest affinities. Realization spreads them
  — some alluvial tiles are wheat, some tobacco, some cotton — instead of every tile carrying a
  diluted slice of all of them.
- **Scouting means something.** *"What grows here"* is now a real per-tile question, and finding a
  tile where your cash crop dominates is a discovery worth acting on.

**Scoping for this slice.** `composition` = the **realized** basket (is-growing), read everywhere:
display, wild gather, Cultivate, and Sow-**upgrade** of an existing patch. `Sow`-from-nothing (the
create-a-patch-on-bare-ground case, which does not occur on a generated map — every food-bearing tile
already carries a patch) reads the **affinity** roster, since there is no realized basket to read. The
fuller **"carry seed to ground where it is not growing wild"** model — `Sow` reading affinity
*everywhere*, so you can plant cotton on a wheat tile — is a clean follow-up that needs a second wire
list (the affinity roster beside the realized one); it is deliberately **not** in this slice.
Thematically, this slice's line is: you **tend and sow what grows here**; making unwilling ground grow
a new crop is rung 4 (Worked Land).

**Config** (`flora_config.json`, new): `realized_species_min` (2) / `realized_species_max` (4),
validated `1 <= min <= max`. **Wire:** no schema change — `ForagePatchState.composition` simply
carries the realized subset now. **Client:** the crop picker already renders `composition`, so it
"just works" and shows what is growing on the selected tile, varying tile to tile — verify with
ui_preview that two tiles of the same biome show different baskets.

---

## 11. Deferred (tracked, not built)

- **`Sow` reads affinity everywhere** — carry seed to suitable ground where the crop is not *growing*
  wild (§10 scoping note). Needs a second wire list (the biome affinity roster beside the realized
  basket) so the Sow picker can offer "can grow here" while the Cultivate picker offers "is growing
  here."
- **Per-component wild stocks** — gather the berries out and leave the acorns (§4.2), now atop the
  per-tile realized basket (§10). Real, and it would make wild depletion selective; not foundational.
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
- The **Fauna Roster** arc (shipped) — the roster pattern this parallels.
