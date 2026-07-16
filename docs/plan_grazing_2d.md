# Grazing Phase 2d — The Pen Economy: Pens Become Land

**Status:** implemented (PR #127). Completes the deferred `(e)` from
`docs/plan_grazing_2b.md` §7 ("Pens: `K_pen` becomes the fenced tiles' graze flow, retiring
`capacity_fraction`") and the "pen *extension*" and per-species husbandry `r` items flagged there.

**Read first:** `docs/plan_grazing_2b.md` (the two-coupled-stocks model and the escapement lesson),
and the "The husbandry yield ladder" section of `core_sim/CLAUDE.md`.

---

## 1. What 2d does

Through 2c, a pen is a special case: a **single tile** (`Herd.corralled_at`), a **frozen** carrying
capacity (`capacity_fraction × K_at_pen_time`), animals that **cannot graze** (they skip
`advance_herd_grazing`) and are fed **entirely from the keeper's larder**, and a husbandry `r` that is
a **flat constant per rung** (pastoral 0.25, pen 0.90) regardless of species.

2d makes a pen *a piece of land you fence*:

- **(a) Footprint.** A pen gains a `pen_radius`; its footprint is `hex_range_tiles(corralled_at,
  pen_radius)`. `pen_radius = 0` is today's single tile.
- **(b) K from the footprint.** A penned herd's carrying capacity becomes `Σ graze_flow / fodder` over
  its footprint — the identical seam wild/pastoral herds use (`ecological_carrying_capacity`). It is
  **recomputed each turn** (penned herds stop being frozen). `capacity_fraction` is **deleted**.
- **(c) Self-feeding.** The penned herd **grazes its footprint** (escapement-floored at 0.25, exactly
  like a wild herd), and the grass it eats **offsets the larder bill**. A pen on lush steppe feeds
  itself for free; a pen on scrub drains the granary. This is the whole point of the arc.
- **(d) Extension.** An `extend_pen` command grows `pen_radius` by one ring, worked off over turns via
  the existing corral build-ladder (a labor cost — no new materials economy). More rings = more K and
  more self-feeding, paid in keeper-turns.
- **(e) Per-species husbandry `r`.** Retire flat `pastoral 0.25 / pen 0.90`. Scale each species' own
  wild `r`, capped to the stable logistic band. A penned mammoth and a penned rabbit become different
  economies.

---

## 2. The self-feeding model

A **penned herd is a wild herd confined to its footprint, that the keeper may top up from the larder
and harvest.** Everything below reuses 2b machinery; nothing here is a new dynamical system.

### 2.1 Carrying capacity (unchanged seam, new range)

`K_pen = Σ_footprint graze_sustainable_flow(G_tile) / fodder_per_biomass`, over
`hex_range_tiles(corralled_at, pen_radius)`. This is `ecological_carrying_capacity` with the footprint
as its range. In `advance_herds`, the corralled branch stops skipping the K write — it computes K over
the footprint (still `next_pos = None`; a pen does not roam).

### 2.2 Grazing draw-down (un-skip penned herds)

`advance_herd_grazing` stops skipping corralled herds. A penned herd draws its footprint down with the
same `graze_take` + `overgraze_escapement_fraction = 0.25` floor as a wild herd — so it **cannot strip
its own land to zero**, and an overgrazed pen recovers (the 2b convergence proof already covers a
range-with-escapement; a pen is a stationary range). Let `footprint_intake` = the grass biomass the
herd successfully draws this turn.

### 2.3 The larder offset — the thesis, made literal

The keeper's feed bill is what the pasture *cannot* cover:

```
demand_grass    = fodder_per_biomass × biomass          // grass to fully feed the herd
pasture_fraction = clamp(footprint_intake / demand_grass, 0, 1)
larder_upkeep   = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)
```

- Footprint fully covers demand → `pasture_fraction = 1` → **larder_upkeep = 0** (free pen on good land).
- Footprint covers nothing (radius-0 pen on rock) → `pasture_fraction = 0` → full larder bill (today's
  behaviour, preserved as the worst case).

`pen.upkeep_per_biomass` (0.002) and `fodder_per_biomass` (per-species) are **unchanged** — the offset
reuses them. No new feed constant.

### 2.4 The net-positive invariant, reworked

`FaunaConfig::validate()` currently enforces `upkeep < r·p/(2+r)` for the flat pen `r` — "every pen is
net-positive." That guard was correct when a pen was an abstract flat rung. With per-species `r` it
**fails for slow breeders** (mammoth pen `r ≈ 0.12` → bound `0.0011 < 0.002` shipped), and with
self-feeding the real feed cost is *situational* (pasture-dependent), so an all-species static check no
longer models the system.

**Replace it with a best-case sanity floor:** the upkeep dial must leave the **fastest-breeding
species** profitable even when *fully larder-fed* (worst pasture):

```
u < r_pen(species with max wild_r) · p / (2 + r_pen(...))
```

With `r_pen(rabbit) = 0.75`: `0.002 < 0.75·0.02/2.75 = 0.00545` ✓. A slow breeder on poor pasture may
now run at a **loss by design** — that is a player's bad placement, not a config error. Document this
inversion of intent in the config comment.

---

## 3. Per-species husbandry `r`

Retire the flat `pastoral.ecology.regrowth_rate` and `pen.ecology.regrowth_rate`. In `herd_ecology`,
derive each managed rung from the herd's own wild `r`, capped:

```
r_pastoral = min(husbandry_regrowth_cap, wild_r × pastoral_gain)
r_pen      = min(husbandry_regrowth_cap, wild_r × pen_gain)
```

**Starting dials** (playtest levers — like the 0.16 consumption dial, expect to tune):

| lever | value | effect |
|---|---|---|
| `pastoral_gain` | 1.5 | mobile-tamed grows 1.5× its wild rate |
| `pen_gain` | 3.0 | penned grows 3× wild (protected, fed, bred) |
| `husbandry_regrowth_cap` | 0.75 | keep logistic `r` in the stable band (no overshoot/oscillation) |

Resulting pen `r`: rabbit `0.75` (capped, booms) · deer `0.30` · mammoth `0.12` (a long-haul
investment). Different herd types, different pen economies — the point of choosing the richer model.

**Balance note:** this re-tunes *every* managed herd's growth relative to today's flat rates (it lowers
broad pen growth from a uniform 0.90). Deliberate; measure and tune the gains/cap in playtest.

`wild_r` is `Herd.regrowth_rate` (already cached per-species at spawn). K still uses the **graze
layer's** own regrowth (`fauna.graze.ecology.regrowth_rate` — the *grass* growing), never the animal's
`r`; the two are orthogonal (grass ceiling vs. animal climb toward it).

---

## 4. Extension — `extend_pen`

A new command grows the fenced footprint by one ring, at a labor cost, reusing the corral build ladder.

- **Command:** `Command::ExtendPen { target_x, target_y }` (the pen's anchor tile), routed like
  `Command::Corral`. Validates: Herding known, the caller owns the penned herd at that tile, and a
  configured `pen_radius_max` is not yet reached.
- **Build:** accrues `corral_build_progress_per_turn` (0.04 → 25 turns/ring) on a per-ring progress
  meter; on completion, `pen_radius += 1`. Reuse `accrue_corral`'s pattern (a second progress field,
  `pen_extend_progress`, or reuse `corral_progress` re-zeroed per ring — server-dev's call, but keep it
  one mechanic).
- **Cost:** the extension is worked by the keeper band's labor (the forgone-yield model the corral
  build already uses); no materials resource is consumed. `pen_radius_max` (config, default **2** → up
  to 19 tiles) bounds it.

Fenced tiles that are water/glacier contribute 0 graze — fencing them simply adds no K, which correctly
models fencing partly-barren land.

---

## 4a. Husbandry ceiling — which species climb the ladder (slice 2d-δ)

Not every animal can be herded, and not every herdable animal can be penned: humans pen cattle, herd
reindeer, and only hunt deer and mammoth. The husbandry ladder is a **sequence** (wild → pastoral →
pen), so a species' reach is a single **ceiling**, not two independent flags — which also makes the
incoherent "pennable but not tameable" state unrepresentable.

Add a per-species `husbandry_ceiling` enum to each species def (`fauna_config.json`), values:

- **`wild`** — hunt-only. Domestication never accrues; the `domesticate` claim and the `corral`/
  `extend_pen` commands reject.
- **`pastoral`** — reaches the mobile-tamed rung but never the pen. Domestication works; `corral`/
  `extend_pen` reject ("{Species} cannot be penned").
- **`pen`** — the full ladder (today's universal behaviour).

**Default `pen`** (preserves current behaviour for any untagged/future species). Initial roster:

| species | ceiling | why |
|---|---|---|
| Mammoth | `wild` | megafauna windfall — hunt-only |
| Deer | `wild` | hunted, not herded |
| Steppe Runner (migratory) | `pastoral` | nomadic herding — follow, don't fence |
| Marsh Grazer (migratory) | `pastoral` | same |
| Boar | `pen` | → pig |
| Rabbit | `pen` | hutches |
| Fowl | `pen` | → poultry |

Cache the ceiling on `Herd` at spawn (mirrors `regrowth_rate`/`fodder_per_biomass`). Gate three seams:
domestication accrual (wild → no-op), the `domesticate` claim (wild → reject), and the `corral` /
`extend_pen` commands + the `Corral` follow-policy accrual (below `pen` → reject/no-op). Append a
`husbandryCeiling` wire field (append-only) so the client can hide the corral/extend affordance on
non-pennable herds and the whole domestication track on wild ones. No `validate()` combination guard is
needed — the single enum makes illegal states unrepresentable.

---

## 5. Config changes (`core_sim/src/data/fauna_config.json`, `husbandry` block)

- **Delete** `pen.capacity_fraction` (and its `_comment_capacity_fraction`).
- **Delete** the flat `pastoral.ecology.regrowth_rate` and `pen.ecology.regrowth_rate` **values**;
  replace with gains + cap (rewrite the `_comment_ladder` to describe the per-species-scaled ladder,
  not the retired flat `0.05 → 0.25 → 0.90`).
- **Add** `husbandry.pastoral_gain = 1.5`, `husbandry.pen_gain = 3.0`,
  `husbandry.husbandry_regrowth_cap = 0.75`, `husbandry.pen_radius_max = 2` — each with a
  no-magic-number comment.
- Rewrite `_comment_upkeep_per_biomass` and the validate() invariant text for §2.4's best-case floor.

`pen.upkeep_per_biomass`, `starve_shrink_rate`, `corralling_yield_fraction`,
`corral_build_progress_per_turn` are **unchanged**.

---

## 6. Wire surface (`sim_schema/schemas/snapshot.fbs`, `HerdTelemetryState`) — APPEND-ONLY

Append after the current last field (do **not** reorder; FlatBuffers slots are append-only):

- `penRadius:int` — footprint radius (0 = single tile).
- `penFootprintTiles:int` — count of in-bounds fenced tiles (server computes; the client must not
  reconstruct the closed-form disk count, which is wrong at map edges — see the 2b Hud lesson).
- `penPastureFraction:float` — `pasture_fraction` from §2.3 (share of feed covered by the footprint).
- `penExtendProgress:float` — `[0,1]` build meter for the in-flight ring (for a "Fencing N%" badge).

`carryingCapacity` (existing) already carries the recomputed K. The larder feed already ships via
`penUpkeep`; with §2.3 it now reflects the *offset* bill.

---

## 7. Client surface (`clients/godot_thin_client`)

- **Pen footprint highlight** on the map (mirror `_draw_herd_range_highlights`'s gold ring; a distinct
  "fenced" tint for the pen footprint tiles).
- **Herd drawer:** a feed-split readout — "Fed by pasture NN% · larder N.N food/turn" from
  `penPastureFraction` + `penUpkeep`; show `penRadius`/`penFootprintTiles`.
- **Extend affordance:** an `extend_pen` action on a selected pen (reuse the corral command wiring),
  with a "Fencing N%" badge from `penExtendProgress` (mirror the corral-build `_corral_label`).

---

## 8. Slice plan

Per-species pen `r` and self-feeding **share** the §2.4 invariant rework, so they land together.

- **2d-α — server core (one slice):** `pen_radius` field; K-from-footprint in `advance_herds`; un-skip
  penned grazing over the footprint; §2.3 larder offset; delete `capacity_fraction`; §3 per-species
  `r`; §2.4 invariant rework; append the §6 wire fields and populate them. Extend
  `tests/grazing_2b_convergence.rs` (or a new `grazing_2d_pen.rs`) to prove a penned herd converges at
  radius 0 and radius 1, and that a lush-footprint pen drives `larder_upkeep → 0`. Self-verify
  fmt+clippy+tests.
- **2d-β — server:** `Command::ExtendPen` + build ladder + validation (§4).
- **2d-γ — client:** §7 surface (footprint highlight, feed-split, extend affordance), verified via the
  ui_preview PNG harness.
- **2d-δ — husbandry ceiling (§4a):** per-species `husbandry_ceiling` enum; gate domestication accrual,
  the `domesticate` claim, and the `corral`/`extend_pen` commands + `Corral` policy accrual; append the
  `husbandryCeiling` wire field; client hides the corral/extend affordance on non-pennable herds and the
  domestication track on wild ones. Server + client.

The 19 corral balance tests must be re-read, not assumed: self-feeding changes `penUpkeep` on good
pasture, so tests asserting a fixed larder draw will need their fixtures pinned to a **barren footprint**
(radius 0 on low-graze tile) to preserve the "full larder bill" worst case they were written against.

---

## 9. See also

- `docs/plan_grazing_2b.md` §2 (coupled stocks), §7 (the deferred pen items this completes).
- `docs/plan_corral_managed_population.md` — the constant-**escapement** lesson §2.2 (why the footprint
  draw is escapement-floored, not constant-catch).
- `core_sim/CLAUDE.md` — "The husbandry yield ladder" (update its flat-rate description once 2d-α lands).
