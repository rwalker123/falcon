# Rivers — edges, classes, and navigable water

Status: **in progress** (model + render). Movement effects are a deliberate follow-up.

## Why this arc

Rivers were the cheapest possible thing the engine could get away with: a worldgen-only
decoration. `hydrology.rs` traced them through **hex centers** on a square-grid 8-neighbour
flow field, shipped them as polylines with a per-segment constant `width`, and `MapView`
drew them with `draw_polyline` — a flat blue 3px line, ignoring the `width`/`order` the
server already sent. They had **zero runtime gameplay effect**.

Three problems, in the order they matter:

1. **A river through a hex center cannot carry a crossing penalty.** When movement lands,
   the side you enter a river hex from is what matters — only the side the river is on
   should cost. A centerline can't express that.
2. **All rivers are the same river.** A trickle and the Mississippi rendered identically
   and meant identically.
3. It was ugly.

## The model: two kinds of thing

The central decision — and the thing to understand before reading any of the code — is that
**a stream and a great river are not the same kind of object**, and modelling them as one
was the mistake.

| | Minor / Major | Navigable |
|---|---|---|
| Lives on | a **hex edge** | a **hex** (`TerrainType::NavigableRiver`) |
| Is | a line *between* tiles | a body of water you are *in* |
| Blocks | nothing — a crossing cost only | yes, like ocean/lake: needs a boat |
| Rendered by | an edge pass in `terrain_blend.gdshader` | a **channel pass** in the same shader |

A navigable river *is* a body of water, so making it a `TerrainType` means every existing
water **mechanic** applies for free rather than being reimplemented as a special edge rule:
blocking falls out of the water-hex rule that already exists, and `RiverDelta` / `InlandSea`
are already precedents for a `must_have`, solver-protected water biome.

**Its RENDER is the one thing that could not be free.** Rendering it through the ordinary
water path put it in the land↔water **shore pass**, and it came out a hex-shaped puddle with
a sandy beach and surf — pixel-for-pixel the read of an `InlandSea` lake, and nothing like a
river; it also ballooned to fill the whole hex, when a great river is a *channel* far narrower
than that. So the hex is now rendered as a silty **BANK** (its `blend_class` is the render-only
`"flat"`, which drops it out of the shore pass and blends the bank into neighbouring land) with
a **wide channel** painted across it, running from the hex centre out to the edges it actually
connects through. Sim-side nothing changed. See `clients/godot_thin_client/CLAUDE.md` → Rivers.

Minor and Major stay on edges, because that is where a crossing penalty can live.

## Generation — the corner graph

To put rivers on edges, hydrology routes on the **dual** of the hex grid: the corner lattice.

Every hex corner is shared by exactly 3 hexes, so there are exactly **2 corners per hex**
(`V = 6F/3 = 2F`). A corner is indexed `(hex_x, hex_y, slot)` with `slot ∈ {TOP, BOTTOM}`.

Each corner has exactly 3 neighbour corners, and **every corner→corner step traverses
exactly one hex edge**. So a downhill walk on corners *is* a chain of hex edges —
contiguous by construction, with no conversion step and no ambiguity about which edge the
water is on.

Corner elevation is the **mean** of its 3 hexes. Mean rather than min is what makes this
work: it puts a corner low exactly where the trough between two low hexes is, so rivers
settle into valleys and run *between* hexes rather than over them.

The pipeline otherwise keeps its existing shape — priority-flood cost field from the sea,
flow accumulation, headwater source categories, min-spacing/min-length acceptance,
termination classes — all ported to corners rather than redesigned.

This also incidentally fixes the square-grid/hex mismatch that `core_sim/CLAUDE.md` flagged
as known-deferred: the corner lattice derives from the real odd-r hex adjacency.

### Class, and where the river stops being an edge

Edge discharge is the corner flow accumulation at the **upstream** corner of the step, so it
is monotonically non-decreasing downstream and a river **grows**: Minor in the headwaters,
Major through the middle, and — on the biggest drainages only — Navigable near the mouth.
This is the thing the old per-segment constant `width` could not express.

When discharge crosses `river_class_navigable_min_discharge`, the river stops emitting edges
and switches to the **existing hex-center tracer** (`trace_river_path`, reused unchanged),
stamping `NavigableRiver` on each hex to the sea. The first navigable hex is the lower of the
two hexes flanking the last **emitted** edge, so the edge chain and the hex chain always share
an **edge** and join without a gap.

The "emitted" is load-bearing. The threshold is crossed *on* an edge that is therefore never
emitted, and it is tempting to anchor the hex chain on that edge — but it and the last emitted
edge meet at a corner, and **three hexes meet at every corner**. Anchoring on the un-emitted edge
could pick the third hex, the one the edge chain never touches: the two chains then shared only a
*point*, the first navigable hex carried no `river_edges` bits, and the tributary visibly
dead-ended at the trunk. Anchoring on the last emitted edge makes the shared edge true by
construction.

## The gameplay primitive

```rust
// Tile
pub river_edges: u16,   // 2 bits per odd-r direction: class = (river_edges >> (2*dir)) & 0b11
```

Both hexes flanking an edge carry it: edge `(H, d)` sets dir `d` on `H` and `opposite(d)` on
the neighbour. The future movement system asks one question —
`H.river_class_on_side(d)` — "what do I cross entering `H` across direction `d`?"

**Nothing reads this yet.** It ships populated and unread, so the movement PR is purely
additive.

`RiverClass` is `None = 0 | Minor = 1 | Major = 2`. Value 3 is reserved; **Navigable is
deliberately not a member** — it is a `TerrainType`.

## Where the tributary meets the trunk

```rust
// Tile
pub river_inflow: u16,  // 2 bits per hex CORNER: class = (river_inflow >> (2*corner)) & 0b11
```

An edge river runs *along* a side, corner to corner: it does not stop mid-edge, **it stops at a
vertex** — and that vertex is where the water leaves the edge model and enters the navigable hex.
`river_edges` records *sides*, so it cannot say this: a trunk hex can flank three river edges (the
tributary ran along three of its sides on the way in), which leaves two candidate chain-ends
between them. A renderer keyed off the edge mask alone would guess — drawing an arm from the hex
centre to every flanked side's midpoint, three arms, and a hex that fills with water.

So the sim states the terminus. On the **first navigable hex only**, at the corner the edge chain
ended on, with the class of the **last emitted edge**. A river that was navigable from its first
step emitted no edges, has no tributary, and reports `0` — no invented inflow.

Corner `i` is the vertex at screen angle `60*i + 30`, **+y down** (the client's `_hex_points`):
`0` lower-right, `1` bottom, `2` lower-left, `3` upper-left, `4` top, `5` upper-right. In the sim's
`(hex, TOP|BOTTOM)` corner model that is `TOP(SE(H))`, `BOTTOM(H)`, `TOP(SW(H))`, `BOTTOM(NW(H))`,
`TOP(H)`, `BOTTOM(NE(H))`; side `dir` spans corners `{dir - 1, dir}`. Both tables are unit-tested
exhaustively against the corner model — a wrong table puts *every* tributary on the wrong vertex.

Two tributaries can end at the **same** vertex of the same trunk hex (three hexes meet at a corner,
so a river down either bank converges there — a confluence at a corner, present on real seeds). One
slot holds one class, so the **wider** wins: `Major` over `Minor`, and order-independent.

## Wire format

The per-tile mask plus the `NavigableRiver` terrain **fully determine the render**, so the
old `HydrologyOverlay` polyline overlay was deleted rather than ported — keeping it would
have been a second, parallel copy of state the tiles already carry.

The entire wire change is two new fields:

```
// TileState
riverEdges:ushort;
riverInflow:ushort;
```

`HydrologyState` still keeps its `Vec<RiverSegment>` server-side (the worldgen tag solver
reads it to bias wetland/fertile placement), but that never leaves the sim.

## Rendering

Rivers are drawn **in `terrain_blend.gdshader`**, not as a mesh or a polyline.

The decisive reason is alignment: a smoothed spline ribbon can drift off the edge it
represents, and for a feature whose entire point is *"the side the river is on is the side
that costs"*, the water must be drawn exactly where the penalty applies. What you see is
what you cross.

The shader already has every primitive needed:

- **Signed distance to the shared edge** between a hex and each of its 6 neighbours — the
  same machinery the beach/foam shore pass rides.
- **World value-noise boundary perturbation** — the trick that already makes the treeline
  bumpy and the surf irregular. Here it both *warps* the band (a capped meander) and
  *varies its width along its length*. Note the meander is capped by the design itself: the
  river is edge-locked, so a warp large enough to erase the lattice read would also detach
  the water from the edge the crossing cost applies to. What actually stops an edge river
  from reading as a honeycomb is a **thin** band with a **varying width** and ragged banks —
  see `clients/godot_thin_client/CLAUDE.md` → Rivers.
- Taking the **min distance over a hex's river-carrying edges** rounds the joins at corners
  for free, so a 120° turn softens with no spline math.
- FoW tinting, LOD suppression, and the pan/zoom-anchored map-space UV all come along free.

Each hex paints the half-band on its own side of the edge, and since both flanking hexes
carry the edge in their mask, the two halves meet symmetrically. No cross-hex sampling.

River water art follows the **existing canopy/peaks precedent** exactly: a new
`textures/rivers/` dir, a `Texture2DArray` built by a `_build_river_texture_array()` cloned
from `_build_canopy_texture_array()`, and a new per-hex splatmap (RG8, 12 bits = 6 edges ×
2-bit class). The file's existence *is* its registration.

## Config levers

Worldgen (`simulation_config.json` → `hydrology`, and per-preset in `map_presets.json`):
- `river_class_major_min_discharge`
- `river_class_navigable_min_discharge`
- `river_navigable_enabled`

Render (`terrain_config.json` → `rivers`): band widths per class (`minor_width` / `major_width`,
plus `navigable_width` — the CHANNEL half-width, wider than Major but well short of filling the
hex), softness, meander, width variation, texture scale, LOD floor, flow speed. The organic levers
(softness / meander / width-variation / bank-noise / flow-speed) are **shared** by the edge and
channel passes rather than duplicated per class.

## Known consequence — navigable rivers bisect landmasses

A one-hex-wide navigable river cuts a continent in two, and **there is no crossing
technology yet**. Nothing breaks today (rivers have no movement effect, so this is currently
cosmetic), but when movement lands a great river becomes a hard wall until boats exist — it
can strand a starting band on the wrong side or wall off half a landmass.

This is a design item for the movement PR, not a bug in this one. The levers already exist:
tune `river_class_navigable_min_discharge` so only a handful of rivers per map qualify, and/or
add fords, bridges, or a crossing tech. The `river_navigable_enabled` kill switch exists so
the feature can be turned off wholesale if it proves hostile.

## See also

- `core_sim/CLAUDE.md` → Worldgen Pipeline (Hydrology)
- `clients/godot_thin_client/CLAUDE.md` → Terrain Texture System (Edge Blending)
