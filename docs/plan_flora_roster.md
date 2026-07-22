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

### 4.2 What the patch carries

`ForagePatch` gains a composition. Two shapes were considered:

- **rung 1 — a mix.** The wild patch holds the tile's basket: `Vec<(species, biomass, capacity)>`,
  or (cheaper) one biomass scalar plus the derived share table, since nothing draws the components
  down independently at rung 1.
- **rung 2+ — a single species.** `Cultivate`/`Sow` commit the patch to one `species_id`; its
  capacity becomes that species' share **times the rung's multiplier**. The rest of the basket is
  displaced — *that is the cost of tending*, and it is why committing a rich mixed tile to one crop
  is a real trade rather than a free upgrade.

**Recommendation: the cheap shape.** Store one `Option<FloraSpeciesId>` on `ForagePatch` (`None` =
wild mix) and derive the mix for display from the affinity table. A wild patch's biomass stays one
scalar, so slice 1 touches no ecology math and the rollback snapshot grows by one optional id.
Per-component wild stocks (differential depletion — gather the berries out and leave the acorns) is
a **deferred** enrichment, noted in §9.

---

## 5. Fodder — the coupling, both halves

This is the arc's load-bearing piece and the one place it reaches into the animal web. Design it as
a coupling, not a lever (`plan_intensification_ladder.md` §2's own instruction).

**The plant half (this arc).** A fodder crop is an ordinary Field of a species whose yield vector is
fodder-dominant. Sowing it needs **no new plant knowledge** — `Sow` already exists and is already
gated on Seed Selection. Its harvest does not go to provisions; it accumulates in a per-faction
**fodder store** (a feed larder, distinct from the provisions larder).

**The animal half (animal rung 4, `Foddering`, discovery id 2007).** Today:

```
K_pen = footprint_graze_flow / herd.fodder_per_biomass          (grazing 2d)
```

With foddering, the pen may draw its shortfall from the fodder store:

```
K_pen = (footprint_graze_flow + delivered_fodder_flow) / herd.fodder_per_biomass
```

Three properties this must preserve, all inherited from `plan_grazing_foundation.md`:

1. **The land still decides — it is just *different* land.** Hay is graze flow you *grew*, on your
   fields, and delivered. The pen's ceiling stops coming from its own tile and starts coming from
   your farming. That is mixed farming, and it makes the plant ladder a **prerequisite** for the
   animal ladder's top rung.
2. **Hauling human food to livestock stays wasteful.** The existing larder-shortfall path is
   *deliberately lossy* (§2.1: "the larder can never be the strategy"). Fodder is **not** that path
   reopened — it is a separate store that only a fodder crop fills. Feeding a pen wheat must remain
   as bad a deal as it is today; feeding it hay is the whole point of having grown hay.
3. **A dead tile still cannot hold a pen** — but a *fed* pen on thin pasture now survives, at the
   cost of a field elsewhere. That is precisely the historical decoupling, and it is a genuine
   land-use decision rather than an exemption.

**Delivery.** v1: a fodder Field within the owning band's work range feeds pens in the same range —
the simplest rule that keeps the coupling *spatial* (you cannot hay a pen from across the map).
Routing fodder over the supply network is a follow-on, deliberately deferred.

**Overwintering.** Because the store is a stock and graze flow is seasonal, a fodder buffer carries a
herd through the trough — the historical function of hay. This falls out of the store being a stock;
no seasonal special-case is needed.

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
  `validate()`, the derived share table, `ForagePatch.species`. A handful of species only.
  *Verification: the economy is provably unmoved* — a live campaign's food numbers must match
  pre-arc, since the shares sum to the same capacity. Wire the tile card so you can *see* what grows
  where before anything depends on it. (Same discipline as graze 2a: ship the layer, look at a real
  map, then bet on it.)
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
