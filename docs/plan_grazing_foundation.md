# Grazing — the Foundation Layer

**Status:** design. Foundational — most of the fauna model is downstream of this.

**Supersedes:** the "Phase 2 — grazing" stub in `docs/plan_corral_managed_population.md`, which framed
this as a corral feature. It is not. The corral is one *consumer* of a layer the whole game rests on.

---

## 1. The insight: we do not eat the same things

The sim has **one** vegetal stock — `ForagePatch.biomass`, on `FoodModuleTag` tiles, gathered by
foragers at `provisions_per_biomass`. That stock is implicitly **human-edible**: seeds, nuts, tubers,
fruit.

**Livestock eat grass and browse — cellulose humans cannot digest at all.**

That is not flavor. It is *the entire economic basis of herding*. A pastoralist does not convert food
into food at a favorable exchange rate; they convert a resource that is **worthless to humans** into
meat and milk. Model it and the fudge at the heart of the corral evaporates. Refuse to model it and
the pen is forever an embarrassing food multiplier with an apologetic comment above it.

So the land carries **two stocks, on two food webs**:

| | `forage_biomass` (exists) | `graze_biomass` (**new**) |
|---|---|---|
| Who eats it | humans (Forage assignments) | **animals** (herds, penned and wild) |
| Where it is | `FoodModuleTag` tiles | **any vegetated land**, by biome |
| What it is | seeds, nuts, tubers, fruit | grass, browse, forbs |

They are **different stocks on different tiles**. A temperate forest is rich in nuts and poor in graze
(the canopy shades out ground cover); a prairie steppe is the reverse; a salt flat is neither. **Your
best farm is usually not your best pasture** — the agropastoral split, and a genuine spatial decision
the game does not currently have.

---

## 2. The consequence: carrying capacity belongs to the LAND

Today a herd's `carrying_capacity` is a **per-species constant** (`fauna_config.json` `biomass[1]` —
Red Deer 1200). It is identical on rich steppe and on bare rock. The land the animal is standing on
has **no bearing on how many animals it can support**, which is, stated plainly, backwards.

> **`K` is not a property of the species. It is a property of the range.**

```
K_herd = (sustainable graze yield across the herd's range) / fodder_per_biomass
```

This is not a formula being invented — it is **what carrying capacity means**. And it is the *same*
formula for a wild herd, a pastoral herd, and a penned one. They differ only in **which tiles are the
range**: a wild herd's home range around its position, a pen's fenced tiles.

### 2.1 What falls out for free

Every one of these is **emergent**, not special-cased. That is the test of a foundation:

- **Herds size themselves to their land.** Rich range → high `K` → the herd grows. Growing → eats more
  → draws the graze down → `K` falls → it stabilizes. A real equilibrium, not a config constant.
- **Overgrazing is a spiral.** Exceed what the pasture's *flow* supports and the herd eats into the
  *stock*, degrading the pasture, lowering next turn's flow, supporting fewer animals. The classic
  pastoral failure mode, for free.
- **Herds compete.** Two herds sharing range draw from the same tiles. No competition system needed.
- **Migration is animals walking toward food.** A herd whose range is eaten bare must move. Add a
  seasonal term to graze and herds migrate seasonally — **because the grass moves**, not because a
  route was baked at worldgen. Migratory routes stop being fixed anchor lists.
- **Herds spawn where the food is.** `game_density` becomes a *reading* of the graze layer rather than
  an independent raster that happens to sit beside it.
- **A dead tile cannot hold a pen.** No graze → `K_pen = 0` → the pen supports zero animals. There is
  no "is this tile worth penning?" check anywhere; the ecology simply says no.
- **Extending a pen has a mechanical reason.** More pasture → more fodder flow → higher `K_pen`. You
  fence more land because you are running out of grass.
- **The larder can never be the strategy.** Larder feeding becomes a **lossy supplement** covering a
  shortfall. It carries animals through a bad patch, but since `K_pen` comes from *grazing*, it cannot
  grow the herd past what the land supports. Hauling human food to livestock is exactly as wasteful as
  it should be.

### 2.2 What it retires

- `pen.capacity_fraction` (the arbitrary `1.0` shipped in Phase 1) — **replaced by real ecology.**
- Species `biomass[min, max]` as *carrying capacity* — it demotes to a **spawn size**.
- The fixed migratory `route` anchor list — demoted, then removed (§6, Phase 2c).

A foundation that only *deletes* levers is the right smell.

---

## 3. The gap this exposes: `regrowth_rate` is global

`EcologyConfig.regrowth_rate` is **one number (0.05) for every species**. A rabbit warren and a
mammoth herd rebuild at the same rate, which is badly wrong: small animals breed *fast*.

Once `K` comes from the land, `r` is the **only** thing left that distinguishes species ecologically —
and it is currently a constant. So this arc must make **`regrowth_rate` per-species**.

That is not scope creep; it is the other half of the same correction. And it retroactively fixes a
finding from PR #117 — *"small game cannot provision an expedition under any policy"* — which was
never a truth about rabbits, only an artifact of giving them a mammoth's growth rate. A rabbit warren
*should* be a fast-renewing, high-MSY food source. Under a real `r` it becomes one.

**Per-species levers after this arc:** `regrowth_rate` (fast rabbit → slow mammoth),
`fodder_per_biomass` (metabolic demand per unit biomass; smaller animals run hotter per kg), and the
existing `host_biomes` (what range the species can use at all). `biomass[min,max]` becomes spawn size.

---

## 4. The graze layer (the atom)

Mirrors `ForageRegistry` exactly — the pattern is proven and rollback-persisted.

- **`GrazeRegistry`** — per-**land-tile** `{ biomass, carrying_capacity, ecology_phase }`, reusing the
  shared `EcologyState` record (as `ForageState`/`HerdState` do), keyed by tile coord.
- **Capacity by biome** — a config table over the 37 biomes (prairie/savanna high, forest low under
  canopy, desert marginal, glacier/rock/water **zero**). A per-biome lever, not a formula.
- **Regrowth** — pure logistic (grass has no Allee collapse) toward capacity, with a **reseed floor**
  like `ForagePatch` (grass reseeds; graze is never permanently dead). Its own `regrowth_rate`, tuned
  well above fauna's — grass regrows fast.
- **Persistence** — round-trips through the rollback snapshot exactly like `ForageRegistry`.
- **Wire** — a per-tile graze readout + a **pasture overlay channel**, so the distribution is
  *visible* before anything depends on it.

**Overgrazing recovers, slowly.** The reseed floor means eaten-out ground comes back over many turns
rather than dying forever. Permanent degradation (desertification) is a later lever, not this arc.

---

## 5. The one formula, three consumers

```rust
/// The fodder flow a set of tiles sustainably yields, and the biomass it therefore supports.
fn range_carrying_capacity(tiles, graze, species) -> f32 {
    let flow: f32 = tiles.map(|t| sustainable_yield(t.biomass, t.capacity, graze.ecology)).sum();
    flow / species.fodder_per_biomass
}
```

- **Wild herd** — range = the tiles within `graze_range_tiles` of its position.
- **Pastoral herd** — the same (it roams; it grazes what it walks over).
- **Penned herd** — range = the pen's **fenced tiles** (§6, Phase 2d).

Each turn a herd **demands** `fodder_per_biomass × biomass` and draws it from its range's graze,
depleting it. A shortfall means the animals go hungry: the herd shrinks (the Phase-1 starvation path
already exists — it simply gets a *second*, more common cause). For a **penned** herd only, the keeper
may cover the shortfall from the larder at a **deliberately lossy** conversion.

---

## 6. Phasing

Each phase is independently shippable and independently *measurable*. Phase 2b is a large rebalance —
it must not ride in the same PR as the layer it depends on.

- **2a — The graze layer.** `GrazeRegistry`, per-biome capacity, regrowth, persistence, wire, and a
  **pasture map overlay**. **Nothing consumes it yet.** Ship it, look at a real map, and confirm the
  distribution is sane *before* betting the fauna model on it.
- **2b — Herds eat; `K` becomes ecological.** The formula above for wild + pastoral + penned. Per-species
  `regrowth_rate` and `fodder_per_biomass`. Retires `capacity_fraction` and species-`K`. **The big
  rebalance** — measure it in a live campaign, as PR #119 did, and expect to retune.
- **2c — Migration follows graze.** Roaming biases toward fodder; an eaten-out range pushes a herd to
  move. Seasonal graze → seasonal migration. Retires the baked anchor routes.
- **2d — Pen extension, with a build cost.** The pen owns a *set* of tiles; fencing more land is a
  build (the 25-turn pen's mechanic, per tile or per ring). Design the pen's range as a tile-set from
  **2b** so this is an extension, not a refactor.
- **2e — Client.** Pasture quality on the tile card, the pen's fenced area on the map, overgrazing
  warnings, and gating the Corral rung on the pasture that would actually feed it.

---

## 7. What must be measured, not assumed

The lesson of PR #119: *the first levers passed every unit test and every ladder check and were still
badly wrong; only a live campaign caught it.*

1. **The graze distribution on a real map** (2a) — before anything reads it. Is prairie actually
   pasture? Is forest actually poor? Look at the overlay.
2. **Herd sizes after 2b.** Species `K` is gone; every herd resizes. Some will vanish, some will
   balloon. Measure the whole species table across a real map, not one herd.
3. **The hunting economy re-balances itself.** MSY is `r·K/4`; **both** terms just changed, for every
   species. Every number in PR #119's ladder moves. Re-measure the ladder, and re-measure the
   early-game campaign.
4. **Overgrazing converges.** An overgrazed range must reach a stable smaller herd on degraded ground,
   not oscillate and not crash to zero.

---

## 8. Deferred (tracked, not built)

- **Trampling** — heavy grazing degrading a tile's *human* forage. Real, and a nice tension between
  pasture and farm on the same ground. Not foundational.
- **Desertification** — permanent degradation past a threshold, rather than the recoverable reseed
  floor.
- **Grazer vs browser** — species eating *different* vegetation (grass vs shrub vs canopy), so range
  quality is species-specific rather than one `graze_biomass` scalar.

---

## See Also

- `docs/plan_corral_managed_population.md` — the flow-based husbandry ladder this rests on; its
  "Phase 2 — grazing" stub is superseded by this document.
- `core_sim/CLAUDE.md` → Fauna & Wild Game (ecology, `regrow_biomass`, herd movement), Depletable
  Forage (the `ForageRegistry` pattern this mirrors).
- `docs/plan_intensification.md` — the ladder graze ultimately feeds.
