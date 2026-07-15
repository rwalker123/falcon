# Grazing Phase 2b — Herds Eat, and Carrying Capacity Becomes Ecological

**Status:** design, pre-implementation. The implementation plan for
`docs/plan_grazing_foundation.md` §2 ("carrying capacity belongs to the LAND"). Phase 2a (the graze
layer) shipped inert in PR #119; 2b makes the fauna model consume it.

**Read first:** `docs/plan_grazing_foundation.md` (the vision), and the "The Graze (Pasture) Layer" +
"Fauna & Wild Game" + "The husbandry yield ladder" sections of `core_sim/CLAUDE.md`.

---

## 1. What 2b does, and why the seam is small

Today a herd's `carrying_capacity` is the species' `biomass[1]` — a **constant**, set once at spawn,
identical on rich steppe and bare rock (Red Deer = 1200 everywhere). 2b makes it a function of the
graze the herd's range actually produces.

The blast-radius map found the model is **already built for this**: every capacity read funnels
through one resolver, **`fauna::herd_capacity(herd, fauna)`**, and every curve helper
(`sustainable_yield`, `peak_regrowth`, `net_biomass_delta`, `managed_yield_biomass`,
`classify_ecology_phase`) already takes capacity as a *parameter*. Nothing outside `herd_capacity`
recomputes K from `biomass[1]`. So 2b is:

- **(a)** give a herd a **range** (the tiles it grazes — it already roams a footprint; §4),
- **(b)** rewrite `herd_capacity` to derive K from the graze on that range (§2), threading
  `&GrazeRegistry` into the ~15 call sites that resolve capacity,
- **(c)** add a graze **draw-down** — the herd eats its range down each turn, mirroring `forage_take`
  (§2, §6),
- **(d)** make `regrowth_rate` and a new `fodder_per_biomass` **per-species** (§5) — neither exists
  today; `regrowth_rate` is one global 0.05 for every animal,
- **(e)** retire `pen.capacity_fraction` — its single reader, `pen_capacity`, becomes the pen tiles'
  graze flow (deferred to 2d; §7).

---

## 2. The model: two coupled stocks

This is the heart of 2b, and the part to get right. There are now **two** biomass stocks in a
relationship, not one regrowing toward a constant:

- **Graze** `G` — grass on each tile (`GrazePatch.biomass`), regrows logistically toward the tile's
  biome capacity `G_cap` (Phase 2a, already shipped).
- **Herd** `B` — animals, regrows toward a carrying capacity `K` that is *derived from the graze*.

**The carrying capacity is the sustainable fodder flow the range yields, converted to animals:**

```
K_herd = ( Σ over range tiles: sustainable_yield(G_tile, G_cap_tile, graze.ecology) ) / fodder_per_biomass
```

`sustainable_yield` is the graze's MSY (already shared, `fauna.rs`). `fodder_per_biomass` is a new
per-species metabolic constant: how much fodder one unit of animal biomass needs per turn. So `K` is
"how many animals this range's *sustainable grass surplus* can feed."

**And the herd eats.** Each turn a herd of biomass `B` demands `fodder_per_biomass × B` fodder and
draws it from its range's graze (a `forage_take`-style primitive, §6), lowering `G`. Then graze
regrows.

### 2.1 The equilibrium, and why it is the whole point

Trace the feedback:

- **`B < K`** — the herd eats *less* than the range's sustainable flow. Graze is under-grazed, regrows
  toward full, `K` rises, herd grows. →
- **`B = K`** — the herd eats *exactly* the sustainable flow. Graze holds at its MSY point
  (`G ≈ G_cap/2`), `K` is stable. The herd sits at carrying capacity, holding its pasture at the most
  productive grazing intensity. This is the definition of carrying capacity, and it falls out — it is
  not a number anyone set.
- **`B > K`** (overgrazed — e.g. two herds sharing range, or a herd that walked onto poorer ground) —
  the herd eats *more* than the flow, drawing graze *below* its MSY point. `F` falls → `K` falls →
  the herd shrinks. Because graze reseeds (2a's floor), this is **recoverable**: the herd settles at a
  smaller size on degraded ground, and the ground heals if grazing eases. **The overgrazing spiral,
  emergent.**

Everything the foundation doc promised — herds sizing to their land, competing on shared range,
overgrazing, migration toward food — is this one feedback loop. Nothing is special-cased.

### 2.2 THE central risk: convergence, not oscillation

**This is a coupled consumer–resource system, and those oscillate if you build them carelessly. It is
the same class of trap that bit the corral** (`docs/plan_corral_managed_population.md` §3 — constant-*catch* MSY
was unstable below K/2 and I had to switch to constant-*escapement*). The design's own §7 names it:
*"an overgrazed range must reach a stable smaller herd, not oscillate and not crash to zero."*

Why it is *probably* stable here (but must be **measured**, not assumed):

- Graze is **fast** (`r = 0.40`), the herd is **slow** (wild `r = 0.05`). This is the "fast resource,
  slow consumer" regime — the resource equilibrates quasi-statically to the herd's demand each turn,
  which damps rather than oscillates. Predator–prey oscillation needs *comparable* timescales.
- The reseed floor (2a) stops graze crashing permanently to zero.

Where it could still bite:

- **Fast species.** A rabbit's per-species `r` might be 0.3–0.5, approaching graze's 0.40 — the danger
  zone. Small game is exactly where to watch.
- **Discretization overshoot.** The herd's demand `fodder × B` is a *constant-catch* draw on the graze.
  If a large herd eats a big fixed chunk in one turn, `G` can overshoot downward. The corral lesson:
  cap the per-turn draw so it cannot take a patch below a floor in one step, and prefer an
  escapement-style formulation where a draw can't drive the resource past the point that would reverse
  the sign of next turn's flow.

**Implementation gate:** a convergence test is the *first* thing written and the last thing trusted —
run the coupled system forward for many turns from several starting states (under-, at-, over-grazed;
one herd and several sharing range) and assert it reaches a stable fixed point within a tolerance,
never oscillates beyond a small band, never crashes to zero on recoverable ground. No slice ships
until this passes.

---

## 3. What K becomes, concretely, and what it retires

| | today | 2b |
|---|---|---|
| Wild / pastoral herd K | `biomass[1]` (species constant) | `Σ_range sustainable_yield(graze) / fodder_per_biomass` |
| Penned herd K | `capacity_fraction × biomass[1]` | pen tiles' graze flow / fodder (2d) |
| `regrowth_rate` | one global `0.05` for every animal | **per-species** |
| `fodder_per_biomass` | does not exist | **per-species** (new) |
| species `biomass[1]` | doubles as K | demotes to **spawn size** only |
| `pen.capacity_fraction` | scales pen K | **deleted** (2d) |

`biomass[min,max]` stays as the spawn-size sampler; it stops being carrying capacity.

---

## 4. The herd's range — it already exists

I do **not** need to invent a range; the roam footprint is already in the data (confirmed with the
human): small game sits on ~1 tile, big game roams a couple, migratory loiters a cluster then migrates
far. That footprint is exactly the grazing range.

- **Wild game** (`Big`/`Small`) — the tiles within a small hex radius of `current_pos`. The radius maps
  from the existing per-species footprint: `route_len == 1` (small game) → range 0 (just its tile);
  bigger `route_len` / `loiter_radius` → radius 1–2. Reuse the existing cadence levers rather than add
  a new one where possible.
- **Migratory** — its range is the **current loiter cluster** (`loiter_radius` around the current
  anchor), *not* the whole route. It eats the cluster it is loitering in, migrates on, and the grass
  regrows behind it while it is away — **which is why it migrates** (2c makes that causal; in 2b the
  route is still baked, but a migratory herd already only *occupies* its current cluster, so K keying
  off the cluster is correct even before 2c). This is the one real modeling lever for how large the
  biggest herds get; I'll set it to the current cluster, measure the resulting mammoth K, and flag it
  if absurd.

**Mechanics:** no `hex_disk`/`hex_range` helper exists — enumerate range tiles by scanning a bounding
box around `current_pos` and filtering on `grid_utils::hex_distance_wrapped(center, t) <= R`
(wrap-aware, horizontal only, matching the map topology). Add a small `hex_range_tiles` helper to
`grid_utils` so the herd, the pen (2d), and anything later share one definition.

### 4.1 Movement must become graze-aware — animals avoid barren ground

Today roam is **pure geometry**: `advance_herd_roam` wanders toward a geometric anchor and `build_route`
lays a jittered spiral, both blind to graze. Once K comes from the range's graze, that produces a
**bug the K change exposes**: a herd that wanders onto barren land (glacier, rock, desert — `G_cap = 0`)
has `K = 0` and starves on ground it never should have set foot on. Grazers don't do that. So 2b makes
movement graze-aware, in two matched pieces:

- **Roam avoids barren, prefers graze** (all mobile herds). When `advance_herd_roam` picks its next
  ≤1-hex step (and when a migratory herd wanders within its loiter cluster), it **won't step onto a
  zero-graze tile**, and among grazeable candidates it biases toward richer graze. A herd hemmed in by
  barren simply doesn't cross it. This is a small change to the step selection — score candidate steps
  by `GrazeRegistry` capacity, exclude zeros — and it is *required* for K-from-graze to be sane, so it
  lands in 2b, not deferred.
- **Migratory anchors sit on fertile ground** (the human's point: *"they'd follow the most fertile land
  on their migrations"*). `build_route` currently drops anchors on a geometric spiral; instead, bias
  anchor placement toward high-graze tiles, so a migratory route **connects fertile patches** and a
  herd loiters where the grass is. This is a spawn-time change to route generation — the routes are
  still baked in 2b, but they are baked along fertility.

**What stays deferred to 2c — the *fully dynamic* migration.** 2b makes routes *start on* fertile land
and keeps herds *off* barren land; it does **not** yet make a herd *leave* because it ate its cluster
down and *head for the greenest uneaten pasture*. That causal "migrate toward greener grass, driven by
what's been eaten" is the emergent endpoint (2c), and it needs the eaten-graze state 2b introduces. The
staging: **2b — routes follow fertility and herds avoid barren (placement/avoidance); 2c — herds chase
receding grass (dynamics).** Doing avoidance now and dynamics later keeps each independently
measurable, and stops 2b's K model from producing herds stranded on dead ground.

---

## 5. The two new per-species levers

Added to `SpeciesDef` (`fauna_config.rs`), both `#[serde(default)]`. The lowest-ripple path (per the
blast-radius map) is to **cache them onto `Herd` at spawn**, exactly as `carrying_capacity` is cached
today — so `regrow_biomass` / `herd_capacity` / `hunt_forecast` read a herd field instead of
re-resolving `SpeciesDef` at 15 call sites.

- **`regrowth_rate`** — per-species, replacing the read of the global `fauna.ecology.regrowth_rate` for
  *wild* herds (pastoral/pen ecologies keep their own `r`). Fast small game, slow megafauna. This is
  the lever that **retroactively fixes the PR #117 finding** — "small game can't provision an
  expedition" was never a fact about rabbits, only an artifact of giving them a mammoth's 0.05 breeding
  rate. Under a real per-species `r`, a rabbit warren becomes the fast-renewing, high-MSY resource it
  should be. Starting values (to measure): rabbit/fowl ~0.30–0.40, deer/boar ~0.10, mammoth/migratory
  ~0.04.
- **`fodder_per_biomass`** — per-species metabolic demand. Smaller animals run hotter per kg, but the
  absolute numbers are a free scale we set by *what K we want each species to reach on typical range*,
  then measure. This is the denominator that turns "grass flow" into "animals," so it and the graze
  `capacity_by_biome` scale jointly — only their ratio matters. Set by back-solving from a target K
  (e.g. "a Red Deer herd on good steppe should cap near ~1200, its old constant" as a *starting anchor*,
  then let the measured campaign move it).

**Design intent:** the *ratios* between species carry the meaning (a rabbit warren is small and fast;
a mammoth herd is huge and slow); the absolute scale of `fodder × graze_capacity` is chosen so the
resulting herd sizes land where the hunting economy expects. Measure and retune, exactly as the corral
levers were.

---

## 6. Turn order — where eating happens

Today (Logistics chain): `advance_herds` (roam + `regrow_biomass`) → `advance_forage_regrowth` →
`advance_graze_regrowth` → … So **graze currently regrows before anything could eat it**.

2b inserts a **graze draw-down** — the herd eats its range. The clean placement, mirroring how forage
and hunt draw-downs already sit relative to their regrowth:

1. Herd eats its range (draws `G` down) — **before** `advance_graze_regrowth` regrows it, so the
   eaten state is what regrows (a herd can't eat grass that regrew the same turn it ate).
2. `advance_graze_regrowth` regrows the drawn-down graze.
3. `regrow_biomass` grows the herd toward `K` derived from the *current* (eaten) graze.

The exact ordering of "herd grows" vs "graze regrows within the turn" is a discretization choice that
the convergence test (§2.2) will settle — get it wrong and it oscillates. The draw-down primitive
itself mirrors `forage_take`: `GrazeRegistry::patch_mut(tile)`, subtract the herd's demand across its
range tiles (proportionally when demand exceeds what's there), clamp to available, mark the eaten
flow. Multiple herds sharing a tile draw order-independently (compute all demands, then apply — the
migration-pass discipline).

---

## 7. Migratory herds, pens, and spawn — scope for 2b

- **Migratory** (2b): K keys off the current loiter cluster (§4). The baked route stays; making
  migration *causal on eaten-out graze* is **2c**, deferred.
- **Pens** (2d, deferred): `K_pen` becomes the fenced tiles' graze flow / fodder, retiring
  `capacity_fraction`. A pen doesn't roam, so its "range" is its penned tile(s) — and 2d's pen
  *extension* (fencing adjacent tiles, with a build cost) is *why* you'd grow the range. **2b keeps the
  pen on `capacity_fraction` unchanged** so the corral arc's shipped balance is untouched until 2d
  deliberately revisits it — one rebalance at a time.
- **Spawn-where-the-food-is** (deferred, likely 2c): `spawn_short_range_game` currently rolls a
  food-module abundance table; replacing that with a graze reading is a separate seam
  (`spawn_short_range_game:860`) and a separate measurement. Not 2b.

2b's scope is deliberately **wild + pastoral mobile herds only**. Pens and spawn placement are named
here so the seam is designed for them, not built now.

---

## 8. Slice plan (each independently measurable)

- **2b-i — the range helper + graze draw-down + graze-aware movement, inert on K.** Add
  `hex_range_tiles` to `grid_utils`; add the `forage_take`-style graze draw-down (wild/pastoral herds
  eat their range each turn); make roam **avoid barren and prefer graze** (§4.1) and `build_route` drop
  migratory anchors on **fertile** ground. **K stays the species constant** this slice — so herds eat,
  graze responds, and animals stop wandering onto dead ground, but carrying capacity hasn't moved yet.
  Ship it, look at the pasture overlay under live herds (does grazing visibly draw down range and
  recover? do herds stay on grass?), and confirm no balance change to the hunting economy (K unchanged).
- **2b-ii — K becomes ecological.** Rewrite `herd_capacity` to derive K from the range's graze flow;
  add per-species `regrowth_rate` + `fodder_per_biomass` (cached on `Herd`); thread `&GrazeRegistry`
  through the capacity call sites. **The big rebalance.** The convergence test (§2.2) gates it. Measure
  the whole species table's new K distribution on a real map, and re-measure the hunting economy /
  early-game campaign the way the corral was measured.
- **2b-iii — client + telemetry.** Surface a herd's range and its derived K / grazing pressure so the
  player can *see* why a herd is the size it is (a herd's range ring, a "K vs range graze" readout,
  overgrazed-range warning). Wire fields as needed. Per the standing rule, the sim slices don't reach a
  player without this.

---

## 9. What must be measured, not assumed

1. **Convergence** (§2.2) — the coupled system reaches a stable fixed point from every starting state,
   for every species (especially fast small game), one herd and several on shared range. *No slice
   ships until this holds.*
2. **The new K distribution** — every herd resizes off the land. Measure the whole species table across
   a real map: what does a Red Deer herd cap at on steppe vs forest vs tundra? Do mammoths stay huge?
   Do any species vanish or balloon?
3. **The hunting economy re-balances itself** — MSY is `r·K/4`, and *both* terms just changed for every
   species. Re-measure PR #119's ladder and the early-game campaign; expect to retune `fodder` and the
   graze scale.
4. **Small game is finally viable** — under a real per-species `r`, a rabbit warren should become a
   fast, high-MSY resource. Verify the PR #117 "small game can't provision an expedition" finding
   *reverses*.
5. **Overgrazing recovers** — an overgrazed range reaches a stable smaller herd on degraded ground and
   heals when grazing eases; it does not desertify permanently (that is a deferred lever).
6. **Herds stay on grass** (§4.1) — no herd ends a turn on a zero-graze tile it roamed onto, and
   migratory routes visibly track fertile corridors rather than crossing barren ground. Look at it on a
   real map, not just a test.

---

## See Also

- `docs/plan_grazing_foundation.md` — the vision and the two-food-web split this implements.
- `docs/plan_corral_managed_population.md` — the constant-escapement lesson (§2.2) and the
  measure-in-a-live-campaign discipline this arc reuses.
- `core_sim/CLAUDE.md` — Fauna & Wild Game (`herd_capacity`, `regrow_biomass`, the ecology curve),
  The Graze (Pasture) Layer (`GrazeRegistry`, the draw-down pattern).
