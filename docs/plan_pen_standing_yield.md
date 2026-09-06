# Standing yield — a kept herd chooses its output

**Issue:** [#630](https://github.com/rwalker123/falcon/issues/630) · **Status:** spec, for implementation

A kept herd produced only meat, so its food rate had to be tuned up against a Field to feel worth
building at all — `pen_density` was retuned species-by-species against the Field's 12.48 food/turn
line for exactly that reason. Milk, eggs and wool are the renewable half: you harvest without
killing, so the standing stock keeps compounding and the animal pays over its whole life rather
than once. This spec adds that half.

---

## 1. The model

**A kept herd commits a fraction of itself to standing output.** `f ∈ [0,1]` on the `Herd`:

- **Meat take** = the existing sustainable take `× (1 − f)`.
- **Standing yield** = the species' per-head rates `× head count × f × rung_fraction`.

At `f = 1` the herd is taken from not at all, so it rides at `K` and the surplus births are
self-limiting — **the cull IS the meat take**, and moving `f` below 1 is how you perform it. No
separate culling mechanism exists or is needed.

`f` defaults to `0.0`, which is today's all-meat behaviour byte-for-byte. Nothing changes until a
player commits.

### One fraction, not one per output

A herd that gives milk *and* wool gives both at `f`. Only meat trades off, because only meat is
paid for by killing the animal — a shorn sheep is still milked. Splitting `f` per output would
model a tradeoff that does not exist.

---

## 2. Config — `standing_yield`, the living twin of `hunt_yield`

**Nothing here is milk-shaped or wool-shaped.** `hunt_yield` says what a *dead* animal pays;
`standing_yield` says what a *live* one pays per head per turn, in the same row shape, drawing on
the same generic materials table. Milk and eggs are both just `provisions`; wool, down and cashmere
are all just `fibre` — which `materials.json` already defines as *"bast, sinew, grass and **wool** —
anything twisted or woven"*, axes `fineness` / `strength`. **No new material is added.**

```jsonc
"wild_sheep": {
  "hunt_yield":     { "materials": [ /* what a dead one pays, unchanged */ ] },
  "standing_yield": {
    "provisions_per_head": 0.00168,
    "materials": [
      { "material": "fibre", "per_head": 0.00588,
        "characteristics": { "fineness": 0.85, "strength": 0.30 } }
    ]
  }
}
```

- `standing_yield` is **optional**. An absent block means the species has no renewable option, with
  no "this species can't" branch anywhere — the same shape an absent `craft` or an absent
  `hand_working` already has. `boar`, `rabbit` and `snow_hare` omit it: pigs and lagomorphs give
  neither milk nor fleece.
- Both sub-fields are optional within it, so a species may pay food only (`aurochs`), fibre only, or
  both (`wild_sheep`).

### The rates are DERIVED, not invented

**`per_head = k × per_unit_biomass_rate × r × body_mass / 4`**

This falls out of setting a full-standing herd's output to `k ×` what the same herd's meat line
pays. Since meat/turn `= rate × r × K/4` and head `= K / body_mass`, **`K` cancels** — the per-head
rate is a pure function of the species' own breeding rate and body size, and is *independent of how
big the pen is*. That is what makes it survive a `pen_density` or `capacity_by_biome` retune without
re-derivation, and it is why a hand-tuned table would have been wrong.

For food the biomass rate is the global `provisions_per_biomass = 0.02`, so it reduces to
`per_head = k × 0.005 × r × body_mass`. For a material it is that material's own `per_biomass` on
the species' `hunt_yield` row.

**`k` is the fiction, and it is the only judgement call.** `k < 1` on food is the whole balance
point: a full-milk herd pays *less per turn* than a full-meat one and pays it forever, into a stock
that keeps growing.

| species | output | `k` | why | **`per_head`** |
|---|---|---:|---|---:|
| aurochs | provisions | 0.65 | cattle are *the* dairy animal | **0.0351** |
| crag_goat | provisions | 0.65 | goats are very milky for their size | **0.00429** |
| crag_goat | fibre | 1.50 | combed cashmere — real, but less than a fleece | **0.00248** |
| wild_sheep | provisions | 0.30 | milked, but the fleece is the point | **0.00168** |
| wild_sheep | fibre | 3.00 | a fleece every turn dwarfs one carcass' sinew | **0.00588** |
| fowl | provisions | 0.70 | hens lay near-daily; the most efficient converter | **0.000159** |
| fowl | fibre | 1.00 | down, and it is ~1.0 fibre/turn at flock scale | **0.0000455** |
| steppe_runner | provisions | 0.45 | mare's/reindeer milk — pastoral-only species | **0.00477** |
| marsh_grazer | provisions | 0.45 | as above | **0.00423** |
| boar / rabbit / snow_hare | — | — | no `standing_yield` block at all | — |

Fibre characteristics — shorn fibre is **finer and weaker** than the sinew a carcass gives, so these
are not the hunt row's readings: sheep `fineness 0.85 / strength 0.30`, goat `0.92 / 0.22`
(cashmere: exceptionally fine, exceptionally weak), fowl down `0.95 / 0.08` (the finest and weakest
thing on the roster). This is the materials table's own "there is no best material" working as
intended — a fleece and an aurochs' sinew (`0.30 / 0.86`) are opposite corners, and the weaver
wanting a bowstring still has to go hunting.

**These are playtest dials.** The `k` column is the one to retune; the `per_head` column is
arithmetic and must be re-derived, never nudged.

---

## 3. Pastoral yields too, at a lower rate

**`husbandry.pastoral_standing_fraction: 0.4`** — a global, not a per-species dial, because the
reason a roaming herd yields less is structural (it is milked opportunistically, not twice daily)
rather than species-specific. `rung_fraction` is `1.0` at `animal:pen` and this at `animal:pastoral`;
a wild herd yields nothing.

This is load-bearing rather than a nicety: `steppe_runner` and `marsh_grazer` carry
`husbandry_ceiling: "pastoral"` and **can never be penned**, so a pen-only gate would hand the two
migratory species nothing at all. It is also the first thing that makes the mobile rung worth
*staying* on rather than a waypoint to the pen — the steppe economy was milk.

---

## 4. The commitment costs work

**`SetHerdOutput { herd, fraction }`**, modelled on `ExtendPen`: the command queues a build job that
banks work against the herd's own rung, and the new `f` takes effect when the meter completes.

**Every commitment costs, including the first.** A herd reaching the pen is all-meat and pays to
become a dairy herd, exactly as it pays again to go back. Re-sorting a herd, keeping and raising the
females and drying off is months of real work — but it is not rebuilding the fence, so:

**`husbandry.output_recommit_work_fraction: 0.333`** of the herd's *current rung's* `build.work_cost`
— **25 work** at `animal:pen` (75), **16.67** at `animal:pastoral` (50). Derived from the rung rather
than invented, on the precedent `route:paved_road` set when it took `animal:pen`'s own pile-to-rate
ratio, so a rung retune carries it.

---

## 5. Implementation surface

| what | where |
|---|---|
| `StandingYieldDef` + `SpeciesDef::standing_yield` | `core_sim/src/fauna_config.rs` (beside `HuntYieldDef`) |
| resolve seam (the `hunt_yield_for` / `hunt_materials_for` twin) | `core_sim/src/fauna_config.rs` |
| the rates, per species | `core_sim/src/data/fauna_config.json` |
| `pastoral_standing_fraction`, `output_recommit_work_fraction` | `HusbandryConfig` + `fauna_config.json` |
| `Herd::standing_output_fraction` | `core_sim/src/components.rs` |
| `BuildJob::SetHerdOutput` → `RungKey` | `core_sim/src/components.rs` |
| the take split and the payout | `core_sim/src/systems/labor.rs` (the pen/pastoral herd arm) |
| material crediting | reuse `materials_config::credit_material_yield` — **the one seam**, unchanged |
| command + proto plumbing | `core_sim/src/bin/server.rs` (follow `ExtendPen` end to end) |
| head count | the **existing** seam `fauna::herd_herders_needed` reads — never a new `biomass / body_mass` |

### Contracts and edge cases

- **One row per herd.** The standing yield folds into the herd's existing `SourceYield`, it does not
  add a second source row — a herd is one source, and a second row would double-count in
  `food_income`. The row must carry the meat/standing split so the client can itemize it without
  arithmetic.
- **`wasted` goes to zero as `f` → 1.** Carry waste is a property of hauling meat home; there is
  none on milk. At `f = 1` a herd wastes nothing.
- **Materials are continuous.** A material store is fixed-point micro-units (`Scalar`), so a draw of
  `0.0000455` subtracts exactly that and the stock crosses whole units by itself. **Do not round the
  per-turn payout** — a rate becomes an event on a whole-unit crossing, never by rounding per turn.
  The pen's `hurdles: 0.05` upkeep has run below one unit a turn since it shipped, so this seam is
  exercised rather than new.
- **Validation:** `provisions_per_head` and every `per_head` finite and `≥ 0`; a `standing_yield`
  row naming a material the table does not carry is rejected by the same cross-config check
  `hunt_yield` already has; `pastoral_standing_fraction ∈ [0,1]`; `output_recommit_work_fraction > 0`.
- **No back-compat.** No shipped saves or clients, so no missing-field fallback code.

### Out of scope

- **The client half** — the commit UI and the itemized readout. The sim publishes the split; the
  client consumes it in a follow-up.
- **Knowledge gating.** No new discovery. `penning` already gates the pen and a `shearing` rung would
  exist to hold one boolean. *Milk earning pasteurisation and wool earning thread* — standing yields
  as knowledge **sources**, the way working a pastoral herd already earns `penning` — is the more
  interesting shape and is a separate conversation.
- **Pens competing with Fields for good land.** Investigated and **not a real defect**: seven biomes
  graze well and grow no field crop at all (PeriglacialSteppe 150, PeatHeath 135, Tundra 100,
  AlpineMountain 65, KarstHighland 60, BorealTaiga 40, SeasonalSnowfield 25), which is exactly the
  "land crops cannot use" the issue thought was missing; and a pen spans up to 19 tiles
  (`pen_radius_max: 2`) against a Field's one, so they are not the same kind of object. The one real
  overlap is `fodder_delivery_rate` — a foddered pen eats hay, and `hay_grass` hosts on the crop
  land — which is the honest shape of a feedlot and is left alone.
