# Rivers — a real drainage network

Status: **BUILT — sim and client both landed.** Follow-up arc to `docs/plan_rivers.md` (edges +
classes + navigable water), which built the river *model* but left the *network* it expresses unbuilt.
The authoritative as-built spec now lives in `core_sim/CLAUDE.md` → Rivers; this document is the
design rationale plus the **as-built deltas** recorded at the bottom. The shader head-taper gate
(§A) shipped with it: the client popcounts the `river_channel` exit bits (1 = head, ≥2 = mid-chain)
rather than reading `inflow != 0`.

**The next arc is fluvial erosion in the heightfield** — see the closing section here, the "known
limitation" note in `core_sim/CLAUDE.md` → Rivers, and `TASKS.md`.

## The problem, measured

`generate_hydrology` routes water by a priority-flood **cost** field expanding from the sea, where a
step costs `slope_penalty + 0.01 * step_len`. That cost is dominated by *distance to the nearest
coast*, so the flow tree it produces is close to a distance transform, not a river network.

Baseline census, shipped config, default 80×52 earthlike, 6 seeds (`CENSUS_SEEDS`):

| Metric | Aggregate |
|---|---|
| Land-corner accumulation | **p50 = 2**, mean 2.78, p95 = 8, p99 = 17, max 69 |
| Corners with **3** upstream contributors | **0** (zero, across all six maps) |
| Corners with **2** (a confluence) | 489 / 12,046 = **4.1%** |
| Edge discharge | p50 = 2, mean 4.95, p95 = 18, max 31 |
| Class histogram | 91.7% Minor / 8.3% Major |
| Navigable | 8 segments / 31 hexes **total across all six maps** |
| Strahler (per segment) | o1=8, o2=29, o3=8, o4=2 |

**A median corner drains two corners.** Not one map in six contains a single corner where three
tributaries meet. Rivers run roughly parallel and straight at the sea; they do not gather.

That narrow dynamic range is also why the class thresholds are small absolute magic-ish numbers
(`river_class_major_min_discharge = 16`, `river_class_navigable_min_discharge = 32`) sitting on no
physical scale, and why navigable rivers are short and rare.

The class system works. It has nothing interesting to express.

## Root cause

The pipeline descends a **cost-to-sea** field instead of the **landscape**. Dendritic drainage is
steepest descent on a *depression-filled elevation surface*; descending distance-to-sea cannot produce
it, because the thing being descended is not the terrain.

## The design

Five changes, in dependency order. The corner graph, the canonical-edge model, the class vocabulary,
and the wire format are all **kept** — this replaces the routing and the extraction, not the model.

### 1. Fill elevation, then descend it

Replace the corner **cost** field with a corner **filled-elevation** field.

- Priority-flood (Barnes) on the corner graph: seed the heap with every **sink** corner at its own
  elevation, pop lowest-first, and for each unvisited neighbour set
  `filled[n] = max(elev[n], filled[popped] + epsilon)`.
- The `epsilon` gradient means every non-sink corner ends up **strictly above** the corner it was
  flooded from, so a strict descent to a sink always exists — including across the flats of a filled
  depression, which is exactly where a naive fill would stall.
- Flow direction = **steepest descent on the filled surface** over the corner's 3 neighbours.

`epsilon` becomes a named config lever (`fill_epsilon`), not a bare literal.

### 2. Break ties on flats deterministically

Pure steepest-descent on a filled surface produces parallel artificial channels on plateaus and filled
flats, because every corner on the flat picks the same direction. The brief suggests D-∞ or a
stochastic tie-break; with only **3** neighbours per corner, D-∞ is awkward and buys little.

Instead: add a tiny **seeded, deterministic jitter** to corner elevation before filling — a hash of
`(world_seed, corner_index)`, amplitude `flat_jitter` (a config lever), chosen `>> fill_epsilon` but
`<<` real relief so it can never reorder genuine terrain. It breaks flat ties into a natural branching
pattern and is fully reproducible.

### 3. Weight accumulation by precipitation

Today every corner seeds `1`. Instead seed it with its **rainfall**, so discharge is a real water
budget: wet highlands feed big rivers, deserts don't.

`MoistureRaster` is **already a Bevy resource**, inserted in `spawn_initial_world` before
`generate_hydrology` runs — no new plumbing. Corner precipitation = mean of its 3 hexes' moisture.

```
seed(corner) = (base_runoff + moisture_weight * moisture(corner)) / CORNERS_PER_HEX
```

Dividing by `CORNERS_PER_HEX` makes discharge read directly as **precipitation-weighted upstream
drainage area, in hex-equivalents** — a physically meaningful, self-documenting unit.
`base_runoff` is a floor so an arid basin still trickles rather than producing a map with no rivers at
all. Both are config levers. Discharge becomes `f32` (sim-internal; it does not cross the wire).

### 4. Extract the network as a tree, not N independent paths

This is the biggest deletion. Today the code traces N unrelated rivers with min-spacing rejection and
a 3-pass acceptance loop — machinery that exists to *manufacture* a river set the flow field failed to
produce. With real accumulation, the network **is** the answer:

1. **Channel corners** = `accumulation >= channel_min_discharge`. Because accumulation is monotone
   non-decreasing downstream, the channel corners plus their descent links form a **forest of trees
   rooted at outlets, by construction**.
2. **Headwaters** = channel corners with no channel corner draining into them.
3. Walk headwaters in **descending discharge** (ties by index — deterministic). Each walks downstream,
   emitting edges, until it reaches either a sink/termination **or a corner already claimed by an
   earlier river** — which is its **confluence**, and where it stops.

Because the biggest branch is walked first, it claims the trunk all the way to the sea, and every
smaller tributary is truncated exactly where it joins something bigger. That is the **main-stem
decomposition**: "the Missouri joins the Mississippi." Each `RiverSegment` stays a *path* (so
`navigable_hexes` stays a chain and `river_channel` stays a path mask), edges are never painted twice,
and confluences become first-class instead of accidental.

Strahler order is then computed on the **real channel tree**, where it is actually defined — replacing
the current per-tile computation on the hex flow field.

### 5. Re-express the class thresholds

With discharge now meaning "precipitation-weighted drainage area in hexes," the thresholds sit on a
**physical, map-size-independent** scale. A river draining 300 wet hex-equivalents is a big river on a
80×52 map and on a 256×192 map alike; a bigger map simply has more of them and longer ones. So they
stay **absolute** (not a fraction of the map max, which one giant basin would skew) — the *values*
change, the keys don't:

- `river_class_major_min_discharge`
- `river_class_navigable_min_discharge`
- `channel_min_discharge` (new — the network-extraction threshold)

`river_density` survives, re-expressed as a **multiplier on the channel threshold**
(`effective = channel_min_discharge / river_density`) — one knob for "how wet does this map read."

## What gets deleted

All of this exists only to compensate for the missing network:

- The entire **hex flow field** (`flow_dir`, `flow_accum`, hex `cost`, `upstream`/`downstream`, the
  square-8 priority flood) — ~300 lines, and **nothing outside `hydrology.rs` reads it** (verified).
- `SourceCategory` (Glacier / LakeOutlet / Runoff / Fallback), `climb_headwater`, the four head-list
  builders, and their sort/dedup bookkeeping.
- `too_close_to_existing_head`, `MAX_SPACING_RELAXATION_PASSES`, `SPACING_RELAXATION_FACTOR`, the
  multi-pass acceptance loop and its last-resort fallback.
- Preset levers: `river_min_spacing`, `river_land_ratio`, `river_min_count`, `river_max_count`,
  `river_accum_threshold_factor`, `river_accum_percentile`, `river_min_accum`,
  `river_source_percentile`, `river_source_sea_buffer`.

Headwaters stop being a hand-picked category and become **emergent**: accumulation crosses the channel
threshold where the ground is high and wet, which is where headwaters are.

## Two decisions (settled)

### A. `river_inflow` semantics — a real network forces this (and a small client change)

The client-scope check came back **negative on one point**. `river_inflow` today *means* "this hex is a
navigable chain HEAD," and the shader keys its head-taper off `inflow != 0`, tapering **every** armed
direction on that hex (`terrain_blend.gdshader:1214-1223`). The sim only ever sets it on
`navigable_hexes.first()`, so the assumption holds today.

**A real drainage network joins tributaries to trunks *mid-chain*** — that is the entire payoff — and
both encodings break the current shader:

- Set inflow on a mid-chain trunk hex → the shader reads it as a head and **pinches the full-width
  trunk to the tributary's width at the hex centre**: a visible hourglass in mid-channel.
- Don't set it → the tributary's edge band ends at a **vertex** while the trunk's arms only reach the
  edge-*midpoints*, so the tributary **dead-ends with a gap** short of the trunk it feeds.

**Decided:** redefine `river_inflow` as *"a tributary hands over to the channel at this vertex"* — true
of any navigable hex, not just the head — and fix the shader to gate the head-taper on **"this hex has
no upstream channel exit"** rather than on `inflow != 0`, leaving the spur unconditional. The wire
format is unchanged (same field, same bits, same corner convention); only the *meaning* widens and the
taper's gate moves. That makes this arc **sim + one small shader slice**, not sim-only.

### B. Lakes — flow-through, not terminal sinks

**Decided: ocean is the only true sink.** Today a corner is a sink if **any** of its 3 hexes is water,
so an `InlandSea`/lake terminates flow, and lake-outlet rivers are *faked* by the `lake_heads` source
category (which this arc otherwise deletes).

Instead, only a corner touching an **ocean** hex (`WATER` *without* `FRESHWATER`) is a sink. Lakes and
inland seas are ordinary low corners: the fill raises them to their lowest saddle and they **spill**,
so the whole upstream catchment carries **through** the lake and out a genuine outlet. Depression
filling is precisely the mechanism for this — real outlet rivers, and a big river below a big lake,
fall out for free, replacing the `lake_heads` hack with the real thing.

Two consequences to build:
- **Deltas become per-transition, not per-terminus.** A river now both *enters* a lake and *leaves* it,
  so the delta scan finds **every land→standing-water transition** along the river's hex path (each
  still gentle-coast gated) rather than only the final one. A lacustrine delta and the ocean delta are
  different tiles on the same river.
- **No river edges inside a water body.** An edge whose *both* flanking hexes are water is not emitted:
  the river visibly enters the lake and re-emerges below it, which is what it should look like.

`TerminationClass::Lake` stops meaning "the river ends here."

## Navigable rivers get longer and more numerous — that is the point

`docs/plan_rivers.md` flagged landmass bisection as a risk. **It is not one, and this is a goal, not a
side effect.** Navigable rivers today are too short and too rare (8 segments / 31 hexes across six
whole maps). Movement has no impediments at all yet, so a navigable river blocks nothing; crossing tech
is a future slice that will arrive with the movement rules it belongs to. The Mississippi cuts the
United States roughly in half, and that is a feature of the map, not a defect.

So: **no landmass-connectivity test, and no tuning against bisection.** Tune
`river_class_navigable_min_discharge` for a map that *reads* right — a handful of genuine great rivers
with real length — and report the census.

## Verification — the numbers are the deliverable

Re-run the same census and compare against the baseline table above. Success means:

- **Discharge distribution** gains a long tail — max up by a large factor, p50 ≪ p99 ≪ max.
- **Confluences become the dominant structure** — the 3-contributor bucket, currently **exactly zero**,
  must be populated.
- **Strahler follows Horton's laws** — order counts fall off geometrically (~3–5× per order). Asserted
  on shape, not on exact values.
- **Class histogram** re-tuned and reported (% Minor / Major / navigable per map), with navigable
  rivers **materially longer and more numerous** than the 8-segments/31-hexes baseline.
- Export a map and **look at a river**: does it gather tributaries and widen?

Plus `cargo fmt`, `cargo clippy -D warnings`, `cargo test`. Determinism is guarded by
`integration_tests/tests/determinism.rs` — no `HashMap` iteration order, no unseeded RNG; the flat
jitter is a pure hash of `(world_seed, corner_index)`.

Performance: a priority flood over `2·W·H` corners is ~98k corners / ~295k edges at 256×192. Worldgen
runs once at Startup; this is a few ms.

## Invariants preserved

- Corner graph, canonical edges, corner-index wire contract, `HEX_CORNER_LAYOUT`.
- Class vocabulary **stays Minor/Major + `NavigableRiver` terrain** — class 3 renders nothing, silently
  (the client's river texture array caps at 3 layers), so no third edge class.
- `RiverDelta` stamping stays **gentle-coast gated** (`shelf.coast_height_threshold`) —
  `integration_tests/tests/shelf_ratio.rs` depends on it.
- World Viability Contract + starting-area placement.
- Wire format unchanged: `riverEdges` / `riverInflow` / `riverChannel` + `NavigableRiver`.


---

## As built — where reality differed from this design

Three things in the design above turned out to be wrong or impossible, and are recorded here rather
than quietly worked around.

### 1. The 3-contributor bucket cannot be populated. It is not evidence of anything.

The verification criterion "the 3-contributor bucket, currently **exactly zero**, must be populated"
is **structurally impossible**, and its being zero in the baseline was never a symptom of poor
concentration. A corner has exactly 3 neighbours; on a strict descent tree one of them *is* that
corner's own downstream, and a strictly-lower neighbour can never route back into it. So a **non-sink
corner has at most 2 contributors, always**, on any flow field whatsoever.

**The 2-bucket is the confluence bucket**, and it is what moved: **4.1% → 11.7%** of land corners
across the 6-seed sweep. That is the number the "confluences become the dominant structure" claim
should have been written against, and it is what
`hydrology_earthlike::the_drainage_network_has_confluences_and_obeys_hortons_laws` asserts (along with
Horton's law of stream numbers, on the *whole* drainage tree — threshold-independent, so it survives
any later re-tuning of the class thresholds).

### 2. `TerminationClass` fell out entirely.

The design said "`TerminationClass::Lake` stops meaning 'the river ends here'". With ocean-only sinks
and per-transition deltas, it stopped meaning *anything*: nothing decides where a river stops (the
tree does) and nothing gates a delta on it (the land→water transition does). The enum,
`termination_class_for`, `stronger_termination` and the `RiverSegment.termination` field are deleted
rather than carried as a dead field.

### 3. The "navigable rivers are a path, not a blob" invariant had to move to the channel mask.

The old bound — *no navigable hex has 4+ navigable **neighbours*** — measures the wrong thing once
chains follow the real river course. **A hex chain that turns 60° puts hex `k` adjacent to hex `k+2`**:
the three hexes at a bend are mutually adjacent, unavoidably. So a bending chain with a tributary
merging at the bend **touches** 4 navigable hexes while remaining a perfectly good path.

The invariant now rides the **`river_channel` exit mask** — which is what the renderer and a future
movement system actually consume: a mid-chain hex links to **2** channel neighbours, an endpoint to
**1**, a confluence to **3**; 4+ is a 2D water body. Terrain adjacency is still bounded, at the
geometric ceiling a chain can reach (2 chain links + one bend skip-adjacency + one merging tributary).

### And one thing the design did not anticipate: the landscape, not the router, is now the limit.

The heightfield is multi-octave noise + smoothing with **no fluvial erosion**, so it has no carved
valleys to capture neighbouring drainage, and every ocean-touching corner is a sink (~578 outlets over
~2050 interior land corners on an 80×52 map). Where the landmass is big enough the network gathers
properly — seed 4 reaches a **546 hex-equivalent** basin, an 8× dynamic-range gain on the baseline's
~69 — but a fragmented map (seed 1: max 16) simply has no big river to find. **Fluvial erosion in the
heightfield is the natural follow-up arc**, and it is what would make every map read like seed 4.


---

## As built — the shipped thresholds, and why

Tuned from a **45-cell sweep** (`hydrology_earthlike::drainage_threshold_sweep`, `#[ignore]`d), not
guessed. `river_density = 1.0` so the levers read at face value.

| Lever | Shipped |
|---|---|
| `river_channel_min_discharge` | **3.0** |
| `river_class_major_min_discharge` | **12.0** |
| `river_class_navigable_min_discharge` | **25.0** |

Per 80×52 earthlike map: **20.7 rivers**, **72.7% Minor / 27.3% Major**, **5.0 navigable segments /
22 navigable hexes**, navigable present on **5 of 6** census seeds. Against the pre-rewrite baseline
(1.3 navigable segments and 5.2 navigable hexes per map) that is **~4× the navigable count and ~4×
the navigable water**.

**The count goal is met. The LENGTH goal is not, and no threshold can meet it.** Probing *below* the
sweep range:

| `navigable_min` | segments (6 seeds) | hexes | mean run | **max run** |
|---|---|---|---|---|
| 8.0 (a trickle) | 92 | 337 | 3.7 | **13** |
| 15.0 | 46 | 193 | 4.2 | **13** |
| 25.0 (shipped) | 30 | 132 | 4.4 | **10** |

Dropping the threshold 3× adds 62 segments and moves the longest river from 10 hexes to 13. **The
threshold buys COUNT, not LENGTH**: chain length is bounded by the distance from the threshold
crossing to the sea, and on these coastlines that is 4–10 hexes. The Mississippi is not a tuning
problem — it is an **erosion** problem. See below and `core_sim/CLAUDE.md` → Rivers → "Known
limitation".
