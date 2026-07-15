use bevy::prelude::*;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet, VecDeque},
};

use crate::{
    components::Tile,
    grid_utils::{hex_neighbor, HEX_CORNER_COUNT, HEX_DIRECTION_COUNT},
    heightfield::ElevationField,
    map_preset::{
        default_river_base_runoff, default_river_channel_min_discharge,
        default_river_class_major_min_discharge, default_river_class_navigable_min_discharge,
        default_river_fill_epsilon, default_river_flat_jitter, default_river_moisture_weight,
        MapPresetsHandle,
    },
    mapgen::WorldGenSeed,
    resources::{MoistureRaster, SimulationConfig, TileRegistry},
    terrain::terrain_definition,
};

use sim_runtime::{RiverChannel, RiverClass, TerrainTags, TerrainType};

// ---------------------------------------------------------------------------
// The corner graph
//
// Minor/Major rivers run along hex EDGES, not through hex centers, so a movement system can
// charge a crossing penalty on exactly the side the river is on. The dual of "flow along edges"
// is "route between corners": every corner-to-corner step traverses exactly one hex edge.
//
// On a pointy-top odd-r grid every corner is shared by exactly 3 hexes, so V = 6F/3 = 2F — two
// corners per hex. Index a corner as `(hex_x, hex_y, slot)` with `slot ∈ {TOP, BOTTOM}`.
// ---------------------------------------------------------------------------

/// Corners per hex: `V = 6F/3 = 2F`. Also the number of corner steps it takes to advance roughly
/// one hex downstream (a corner step covers one hex *side*, so a river zig-zags ~2 sides per hex).
const CORNERS_PER_HEX: usize = 2;

/// A corner step covers one hex side, which advances the river about half a hex-center distance.
/// So a river spanning `N` hexes is roughly `N * CORNER_STEPS_PER_HEX` edges long. Used to convert
/// the hex-denominated `river_min_length` config lever into a corner-graph step budget.
const CORNER_STEPS_PER_HEX: usize = CORNERS_PER_HEX;

/// Hexes meeting at a single corner (the reason `V = 2F`).
const HEXES_PER_CORNER: usize = 3;

/// The corner slot at the *top* vertex of a hex — shared with its NW and NE neighbours.
const CORNER_TOP: u8 = 0;
/// The corner slot at the *bottom* vertex of a hex — shared with its SW and SE neighbours.
const CORNER_BOTTOM: u8 = 1;

/// Odd-r direction indices, matching `grid_utils::HEX_NEIGHBOR_OFFSETS` (clockwise from E).
const DIR_E: u8 = 0;
const DIR_SE: u8 = 1;
const DIR_SW: u8 = 2;
const DIR_NW: u8 = 4;
const DIR_NE: u8 = 5;

/// Every hex edge has two representations — `(H, d)` and `(neighbour, opposite(d))` — and exactly
/// one of them has `dir ∈ {E, SE, SW}`. Storing that one gives each edge a single canonical key.
const CANONICAL_DIR_COUNT: u8 = 3;

/// Hex `H`'s six corners, in the **client's** corner order (`grid_utils::HEX_CORNER_COUNT`: index
/// `i` is the vertex at screen angle `60 * i + 30`, +y down), each expressed in the sim's corner
/// model as `(step from H to the hex that owns the corner, that hex's slot)` — `None` meaning `H`
/// owns it itself:
///
/// | `i` | vertex | owner |
/// |---|---|---|
/// | 0 | lower-right | `TOP(SE(H))` |
/// | 1 | bottom | `BOTTOM(H)` |
/// | 2 | lower-left | `TOP(SW(H))` |
/// | 3 | upper-left | `BOTTOM(NW(H))` |
/// | 4 | top | `TOP(H)` |
/// | 5 | upper-right | `BOTTOM(NE(H))` |
///
/// (`TOP(SE(H))` is shared by `{SE(H), NW(SE(H)) = H, NE(SE(H)) = E(H)}` — the vertex between `H`,
/// its E and its SE neighbour, i.e. `H`'s lower-right corner. The rest follow the same way.)
///
/// This table is the wire contract behind `Tile::river_inflow`; getting it wrong would put every
/// tributary on the wrong vertex, so it is exhaustively unit-tested rather than merely asserted in
/// a comment.
const HEX_CORNER_LAYOUT: [(Option<u8>, u8); HEX_CORNER_COUNT] = [
    (Some(DIR_SE), CORNER_TOP),
    (None, CORNER_BOTTOM),
    (Some(DIR_SW), CORNER_TOP),
    (Some(DIR_NW), CORNER_BOTTOM),
    (None, CORNER_TOP),
    (Some(DIR_NE), CORNER_BOTTOM),
];

/// Histogram slots for the class telemetry — one per `RiverClass` discriminant (None/Minor/Major).
const RIVER_CLASS_HISTOGRAM_SLOTS: usize = 3;

/// A strictly-positive floor on the extraction threshold. `river_density` divides
/// `river_channel_min_discharge`, so a pathological config could otherwise drive the threshold to
/// zero (or below) and make *every* corner a channel.
const CHANNEL_MIN_DISCHARGE_FLOOR: f32 = 1e-6;

/// Uniform precipitation used when the `MoistureRaster` is missing or mis-sized — a "rains the same
/// everywhere" world, so hydrology degrades to plain drainage-area accumulation rather than failing.
const DEFAULT_UNIFORM_PRECIP: f32 = 1.0;

// --- splitmix64, the deterministic hash behind the flat-tie jitter (no RNG, no HashMap) ---
/// splitmix64's increment (the odd 64-bit "golden gamma").
const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// splitmix64's first mix multiplier.
const SPLITMIX_MIX_A: u64 = 0xBF58_476D_1CE4_E5B9;
/// splitmix64's second mix multiplier.
const SPLITMIX_MIX_B: u64 = 0x94D0_49BB_1331_11EB;
/// First xor-shift distance in splitmix64's finalizer.
const SPLITMIX_SHIFT_A: u32 = 30;
/// Second xor-shift distance.
const SPLITMIX_SHIFT_B: u32 = 27;
/// Third xor-shift distance.
const SPLITMIX_SHIFT_C: u32 = 31;
/// Bits of hash output mapped onto the unit interval — `f32` carries 24 significand bits, so taking
/// the top 24 gives every representable value in `[0, 1)` exactly once.
const HASH_UNIT_BITS: u32 = 24;

/// The opposite odd-r direction (`E ↔ W`, `SE ↔ NW`, `SW ↔ NE`).
#[inline]
fn opposite_dir(dir: u8) -> u8 {
    (dir + CANONICAL_DIR_COUNT) % HEX_DIRECTION_COUNT as u8
}

/// splitmix64 — a pure, deterministic 64-bit mixer. No state, no RNG, no allocation: the same
/// `(world_seed, corner)` always produces the same jitter, on every machine and every run.
#[inline]
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(SPLITMIX_GAMMA);
    z = (z ^ (z >> SPLITMIX_SHIFT_A)).wrapping_mul(SPLITMIX_MIX_A);
    z = (z ^ (z >> SPLITMIX_SHIFT_B)).wrapping_mul(SPLITMIX_MIX_B);
    z ^ (z >> SPLITMIX_SHIFT_C)
}

/// A deterministic hash of `(world_seed, index)` into `[0, 1)`.
#[inline]
fn hash01(world_seed: u64, index: usize) -> f32 {
    let bits = splitmix64(world_seed ^ splitmix64(index as u64));
    (bits >> (u64::BITS - HASH_UNIT_BITS)) as f32 / (1u32 << HASH_UNIT_BITS) as f32
}

/// Grid geometry the corner graph is walked on. Every hex step goes through
/// `grid_utils::hex_neighbor`, so horizontal wrap is honored throughout.
#[derive(Debug, Clone, Copy)]
struct HexGrid {
    width: u32,
    height: u32,
    wrap_horizontal: bool,
}

impl HexGrid {
    #[inline]
    fn neighbor(&self, pos: UVec2, dir: u8) -> Option<UVec2> {
        hex_neighbor(
            pos.x,
            pos.y,
            dir as usize,
            self.width,
            self.height,
            self.wrap_horizontal,
        )
        .map(|(x, y)| UVec2::new(x, y))
    }

    #[inline]
    fn tile_index(&self, pos: UVec2) -> usize {
        (pos.y * self.width + pos.x) as usize
    }

    #[inline]
    fn corner_index(&self, pos: UVec2, slot: u8) -> usize {
        self.tile_index(pos) * CORNERS_PER_HEX + slot as usize
    }

    #[inline]
    fn corner_parts(&self, corner: usize) -> (UVec2, u8) {
        let slot = (corner % CORNERS_PER_HEX) as u8;
        let tile = (corner / CORNERS_PER_HEX) as u32;
        (UVec2::new(tile % self.width, tile / self.width), slot)
    }

    #[inline]
    fn corner_count(&self) -> usize {
        (self.width as usize) * (self.height as usize) * CORNERS_PER_HEX
    }

    /// The three hexes meeting at a corner, or `None` for a **border corner** — one whose 3 hexes
    /// are not all on the map (the top/bottom map edge, or left/right when wrap is off). Border
    /// corners are excluded from routing entirely.
    fn corner_hexes(&self, pos: UVec2, slot: u8) -> Option<[UVec2; HEXES_PER_CORNER]> {
        let (a, b) = if slot == CORNER_TOP {
            (DIR_NW, DIR_NE)
        } else {
            (DIR_SW, DIR_SE)
        };
        Some([pos, self.neighbor(pos, a)?, self.neighbor(pos, b)?])
    }

    /// Which of hex `pos`'s six corners the corner `corner` is, in the client's screen-space corner
    /// order (`grid_utils::HEX_CORNER_COUNT`) — or `None` if `corner` is not one of them.
    ///
    /// Bridges the two corner models: the sim owns corners as `(hex, TOP|BOTTOM)` (two per hex,
    /// each shared by three hexes), while the renderer indexes the six vertices *of one hex*. See
    /// `HEX_CORNER_LAYOUT` for the table, and `local_corner_index_is_a_bijection_on_every_hex` /
    /// `hex_edge_corner_indices_match_the_corner_model` for its proof.
    fn local_corner_index(&self, pos: UVec2, corner: usize) -> Option<u8> {
        HEX_CORNER_LAYOUT
            .iter()
            .position(|&(dir, slot)| {
                let owner = match dir {
                    Some(dir) => self.neighbor(pos, dir),
                    None => Some(pos),
                };
                owner.map(|owner| self.corner_index(owner, slot)) == Some(corner)
            })
            .map(|index| index as u8)
    }

    /// The canonical form of the hex edge `(pos, dir)`: the representation whose `dir` is one of
    /// `{E, SE, SW}`. An edge only exists if **both** of its hexes are on the map, so a step off the
    /// map edge yields `None` in either representation.
    fn canonical_edge(&self, pos: UVec2, dir: u8) -> Option<(UVec2, u8)> {
        let far = self.neighbor(pos, dir)?;
        if dir < CANONICAL_DIR_COUNT {
            Some((pos, dir))
        } else {
            Some((far, opposite_dir(dir)))
        }
    }

    /// The (up to) 3 corners adjacent to `corner`, each with the hex edge the step traverses.
    ///
    /// `TOP(H)` is shared by `{H, NW(H), NE(H)}`; its neighbours are `BOTTOM(NW(H))` across edge
    /// `(H, NW)`, `BOTTOM(NE(H))` across `(H, NE)`, and `BOTTOM(NE(NW(H)))` across `(NW(H), E)` —
    /// the vertical edge between NW and NE. `BOTTOM(H)` is the mirror image.
    fn corner_neighbors(&self, corner: usize) -> [Option<CornerStep>; HEXES_PER_CORNER] {
        let (pos, slot) = self.corner_parts(corner);
        let (near_a, near_b, far_slot) = if slot == CORNER_TOP {
            (DIR_NW, DIR_NE, CORNER_BOTTOM)
        } else {
            (DIR_SW, DIR_SE, CORNER_TOP)
        };

        let mut out = [None, None, None];
        // The two edges of `pos` itself that meet at this corner.
        out[0] = self.step_across(pos, near_a, far_slot);
        out[1] = self.step_across(pos, near_b, far_slot);
        // The third edge belongs to the neighbour hex: it is the `E` edge joining the two hexes
        // that flank `pos` at this corner, and it lands on the far slot of the hex beyond them.
        if let Some(side) = self.neighbor(pos, near_a) {
            if let Some(beyond) = self.neighbor(side, near_b) {
                out[2] = self.corner_step(beyond, far_slot, side, DIR_E);
            }
        }
        out
    }

    /// A step from a corner of `pos` across `pos`'s own edge in direction `dir`, landing on
    /// `far_slot` of the neighbour.
    fn step_across(&self, pos: UVec2, dir: u8, far_slot: u8) -> Option<CornerStep> {
        let neighbor = self.neighbor(pos, dir)?;
        self.corner_step(neighbor, far_slot, pos, dir)
    }

    /// Build a step onto `(corner_pos, corner_slot)` traversing edge `(edge_pos, edge_dir)`,
    /// rejecting border corners and off-map edges.
    fn corner_step(
        &self,
        corner_pos: UVec2,
        corner_slot: u8,
        edge_pos: UVec2,
        edge_dir: u8,
    ) -> Option<CornerStep> {
        self.corner_hexes(corner_pos, corner_slot)?;
        let (hex, dir) = self.canonical_edge(edge_pos, edge_dir)?;
        Some(CornerStep {
            corner: self.corner_index(corner_pos, corner_slot),
            hex,
            dir,
        })
    }

    /// The corner step from `from` to its adjacent corner `to` (the hex edge the two share), or
    /// `None` if they are not neighbours in the corner graph.
    fn step_between(&self, from: usize, to: usize) -> Option<CornerStep> {
        self.corner_neighbors(from)
            .into_iter()
            .flatten()
            .find(|step| step.corner == to)
    }
}

/// One corner→corner move: the corner arrived at, and the (canonical) hex edge crossed to get
/// there.
#[derive(Debug, Clone, Copy)]
struct CornerStep {
    corner: usize,
    hex: UVec2,
    dir: u8,
}

/// One side of one hex carrying a river. Canonical: `hex` is the representation whose `dir` is in
/// `{E, SE, SW}`, so an edge appears exactly once no matter which of its two hexes traced it.
#[derive(Debug, Clone, Copy)]
pub struct RiverEdge {
    pub hex: UVec2,
    /// Odd-r direction, `0..6` (`grid_utils` convention).
    pub dir: u8,
    pub class: RiverClass,
    /// Corner flow accumulation at the **upstream** corner of this step, in **precipitation-weighted
    /// hex-equivalents of drainage area** (see `CornerField::accumulate`) — monotonically
    /// non-decreasing downstream, so `class` never shrinks toward the mouth. Sim-internal: it does
    /// not cross the wire.
    pub discharge: f32,
}

/// **A tributary hands over to the channel at this vertex.**
///
/// An edge river runs *along* a side, corner to corner, so it does not end mid-edge — it ends at a
/// **vertex**, and that vertex is where the water leaves the edge model and enters the navigable
/// hex. The per-tile `river_edges` mask cannot express that (a hex may flank three river edges,
/// leaving two candidate chain-ends), so the sim states it, exported as `Tile::river_inflow`.
///
/// Two hand-overs exist, and both are recorded here:
/// - a river that **outgrows the edge model itself** hands over at the head of its own navigable
///   chain, and
/// - an **edge-only tributary that lands on a navigable trunk** hands over at a vertex of that
///   trunk hex — **mid-chain**. That is new with the drainage network (before it, tributaries could
///   only meet a trunk at its head), and it is why `river_inflow` no longer means "chain head": it
///   means "a tributary arrives at this vertex".
#[derive(Debug, Clone, Copy)]
pub struct RiverInflow {
    /// The navigable hex the tributary hands its water to — its own chain head, or a mid-chain hex
    /// of the trunk it joins.
    pub hex: UVec2,
    /// Corner index `0..HEX_CORNER_COUNT` **on `hex`**, in the client's screen-space corner order
    /// (see `HEX_CORNER_LAYOUT`).
    pub corner: u8,
    /// Class of the last edge the chain emitted — the tributary's own width where it arrives.
    pub class: RiverClass,
}

#[derive(Debug, Clone)]
pub struct RiverSegment {
    pub id: u32,
    /// Strahler order of the river's most downstream channel corner, computed on the **real channel
    /// tree** (see `DrainageNetwork::strahler`).
    pub order: u8,
    /// The hex edges the river runs along, upstream → downstream.
    pub edges: Vec<RiverEdge>,
    /// The `NavigableRiver` hex chain the river becomes once its discharge crosses
    /// `river_class_navigable_min_discharge`. Empty unless the river went navigable.
    pub navigable_hexes: Vec<UVec2>,
    /// Where this river's edge chain hands over to a navigable channel (its own, or the trunk it
    /// lands on). `None` when it emitted no edges (nothing to hand over) or when it never reaches a
    /// navigable channel at all.
    pub navigable_inflow: Option<RiverInflow>,
}

impl RiverSegment {
    /// Every hex the river touches, upstream → downstream: both hexes flanking each edge, then the
    /// navigable tail. Not deduplicated — callers that need an ordered walk (delta placement) scan
    /// it in sequence.
    pub fn touched_hexes(&self, grid_width: u32, grid_height: u32, wrap: bool) -> Vec<UVec2> {
        touched_hexes(
            &self.edges,
            &self.navigable_hexes,
            &HexGrid {
                width: grid_width,
                height: grid_height,
                wrap_horizontal: wrap,
            },
        )
    }
}

/// The discharge thresholds that turn corner flow accumulation into a per-edge `RiverClass` — and,
/// past the top threshold, into a `NavigableRiver` hex chain.
///
/// Discharge is **precipitation-weighted upstream drainage area in hex-equivalents**, a physical,
/// map-size-independent unit — so these are **absolute** values, not fractions of the map maximum.
/// A river draining 300 wet hex-equivalents is a big river on an 80×52 map and on a 256×192 map
/// alike; a bigger map simply has more of them.
#[derive(Debug, Clone, Copy)]
struct RiverClassThresholds {
    major_min: f32,
    navigable_min: f32,
    navigable_enabled: bool,
}

impl RiverClassThresholds {
    /// The class of an edge carrying `discharge`, or `None` when the river has outgrown the edge
    /// model entirely and must become a `NavigableRiver` hex chain.
    fn classify(&self, discharge: f32) -> Option<RiverClass> {
        if discharge < self.major_min {
            Some(RiverClass::Minor)
        } else if discharge < self.navigable_min || !self.navigable_enabled {
            Some(RiverClass::Major)
        } else {
            None
        }
    }
}

/// The water biomes — a body of water you are *in*. `NavigableRiver` and `RiverDelta` are
/// deliberately **not** here: a navigable river is the river itself, not a body it drains into, and
/// a delta is depositional land.
fn is_water_terrain(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::InlandSea
    )
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HydrologyState {
    pub rivers: Vec<RiverSegment>,
}

impl HydrologyState {
    /// Per-tile "this hex is on a river" mask — a hex flanks at least one river edge, or is part of
    /// a navigable river's hex chain. The single definition of river-adjacency, shared by the tag
    /// solver's terrain nudges and its regression tests.
    pub fn river_tile_mask(&self, width: u32, height: u32, wrap_horizontal: bool) -> Vec<bool> {
        let mut mask = vec![false; (width as usize) * (height as usize)];
        for river in &self.rivers {
            for pos in river.touched_hexes(width, height, wrap_horizontal) {
                if pos.x < width && pos.y < height {
                    mask[(pos.y * width + pos.x) as usize] = true;
                }
            }
        }
        mask
    }
}

// ---------------------------------------------------------------------------
// The tile layer the corner graph reads
// ---------------------------------------------------------------------------

/// The per-tile facts hydrology routes against: which hexes are water, and which of those are the
/// **ocean** (the only true sink).
struct TileWorld<'a> {
    grid: HexGrid,
    /// Elevation-derived water mask, used only where a tile carries no terrain yet.
    seamask: &'a [bool],
    terrain: &'a [Option<(TerrainType, TerrainTags)>],
}

impl TileWorld<'_> {
    /// A hex you are *in* the water of: the ocean, or a lake / inland sea.
    fn is_water(&self, idx: usize) -> bool {
        self.seamask[idx]
            || self.terrain[idx]
                .map(|(terrain, _)| is_water_terrain(terrain))
                .unwrap_or(false)
    }

    /// **The ocean, and only the ocean.** `WATER` *without* `FRESHWATER` — so lakes / inland seas
    /// are not sinks: the depression fill raises them to their spill point and the catchment drains
    /// *through* them (a real outlet river, rather than the old faked "lake outlet" source).
    fn is_ocean(&self, idx: usize) -> bool {
        match self.terrain[idx] {
            Some((terrain, tags)) => {
                is_water_terrain(terrain) && !tags.contains(TerrainTags::FRESHWATER)
            }
            // Preset-less fallback: no terrain has been stamped, so elevation is all we have.
            None => self.seamask[idx],
        }
    }

    fn is_water_hex(&self, pos: UVec2) -> bool {
        self.is_water(self.grid.tile_index(pos))
    }
}

/// The levers that shape the flow field itself (all config, no bare literals).
#[derive(Debug, Clone, Copy)]
struct FlowConfig {
    /// The drainage gradient the depression fill lays across a filled flat: every non-sink corner
    /// ends up **strictly** this much above the corner that flooded it, so a strict descent to a
    /// sink always exists — including across the flats of a filled depression, where a naive fill
    /// would stall.
    fill_epsilon: f32,
    /// Amplitude of the deterministic elevation jitter applied before filling. Must be `>>
    /// fill_epsilon` (so it decides ties the fill cannot) and `<<` real relief (so it can never
    /// reorder genuine terrain). Without it, pure steepest descent on a plateau picks the same
    /// direction for every corner and carves artificial parallel channels.
    flat_jitter: f32,
    /// Per-hex runoff floor, so an arid basin still trickles rather than producing a map with no
    /// rivers at all.
    base_runoff: f32,
    /// How hard rainfall drives discharge: a hex contributes `base_runoff + moisture_weight ×
    /// precipitation` to its drainage.
    moisture_weight: f32,
}

// ---------------------------------------------------------------------------
// The corner flow field: jittered elevation → priority-flood fill → steepest descent → accumulation
// ---------------------------------------------------------------------------

/// The corner-graph flow field. Everything downstream of it (channel extraction, classes, the
/// navigable hand-off) reads only `filled` / `downstream` / `accumulation`.
struct CornerField {
    grid: HexGrid,
    /// `false` for border corners (not all 3 hexes on the map) — excluded from routing.
    valid: Vec<bool>,
    /// A corner touching an **ocean** hex: the only true sink (see `TileWorld::is_ocean`).
    sink: Vec<bool>,
    /// Mean of the corner's 3 hexes' elevation samples, plus the deterministic flat-tie jitter.
    /// **Mean, not min**: it puts a corner low exactly in the trough between two low hexes, so
    /// rivers settle into valleys instead of hugging a single low tile.
    elevation: Vec<f32>,
    /// Depression-filled elevation (Barnes priority flood + epsilon). `INFINITY` = unreachable from
    /// any sink. This is **the landscape rivers descend** — not a cost-to-sea distance transform.
    filled: Vec<f32>,
    /// Steepest descent on `filled` — which, on a regular lattice where all 3 corner steps are the
    /// same length, is simply the lowest filled neighbour. `usize::MAX` for sinks and unreachable
    /// corners.
    downstream: Vec<usize>,
    /// Precipitation-weighted upstream drainage area, in **hex-equivalents** (see `accumulate`).
    accumulation: Vec<f32>,
    /// Every routable corner, sorted by `filled` DESCENDING (ties by index ascending) — a
    /// deterministic topological order of the drainage tree: a corner always precedes its
    /// downstream.
    topo_order: Vec<usize>,
}

impl CornerField {
    fn build(
        tiles: &TileWorld,
        elevation_field: &ElevationField,
        moisture: Option<&MoistureRaster>,
        world_seed: u64,
        flow: &FlowConfig,
    ) -> Self {
        let grid = tiles.grid;
        let count = grid.corner_count();
        let mut valid = vec![false; count];
        let mut sink = vec![false; count];
        let mut elevation = vec![f32::INFINITY; count];
        let mut filled = vec![f32::INFINITY; count];
        let mut accumulation = vec![0.0f32; count];
        let mut heap = BinaryHeap::new();

        let precip = PrecipField::new(moisture, grid);

        for y in 0..grid.height {
            for x in 0..grid.width {
                let pos = UVec2::new(x, y);
                for slot in [CORNER_TOP, CORNER_BOTTOM] {
                    let Some(hexes) = grid.corner_hexes(pos, slot) else {
                        continue;
                    };
                    let idx = grid.corner_index(pos, slot);
                    valid[idx] = true;

                    let mean_elev = hexes
                        .iter()
                        .map(|h| elevation_field.sample(h.x, h.y))
                        .sum::<f32>()
                        / HEXES_PER_CORNER as f32;
                    // A pure hash of (world_seed, corner) — reproducible, allocation-free, and the
                    // only thing that keeps a plateau from carving parallel artificial channels.
                    let jitter = flow.flat_jitter * (hash01(world_seed, idx) - 0.5);
                    elevation[idx] = mean_elev + jitter;

                    let mut is_ocean = false;
                    let mut precip_sum = 0.0f32;
                    for hex in hexes {
                        is_ocean |= tiles.is_ocean(grid.tile_index(hex));
                        precip_sum += precip.sample(hex);
                    }

                    // Seed the corner's own runoff. Dividing by `CORNERS_PER_HEX` makes the summed
                    // accumulation read directly as *precipitation-weighted upstream drainage area
                    // in hex-equivalents* — the unit the class thresholds live in.
                    let mean_precip = precip_sum / HEXES_PER_CORNER as f32;
                    accumulation[idx] = (flow.base_runoff + flow.moisture_weight * mean_precip)
                        / CORNERS_PER_HEX as f32;

                    if is_ocean {
                        sink[idx] = true;
                        filled[idx] = elevation[idx];
                        heap.push(HeapEntry {
                            key: elevation[idx],
                            idx,
                        });
                    }
                }
            }
        }

        let mut field = Self {
            grid,
            valid,
            sink,
            elevation,
            filled,
            downstream: vec![usize::MAX; count],
            accumulation,
            topo_order: Vec::new(),
        };
        field.priority_flood(heap, flow.fill_epsilon);
        field.build_topo_order();
        field.derive_downstream();
        field.accumulate();
        field
    }

    /// **Barnes priority flood, with an epsilon gradient.** Pop the lowest unfinalized corner, and
    /// raise each neighbour to at least `filled[popped] + fill_epsilon`. Every non-sink corner
    /// therefore ends up **strictly above** the corner that flooded it, so a strict descent to a
    /// sink always exists — that is what carries water across the flats of a filled depression
    /// instead of stalling in it. Corners no sink can reach keep `filled = INFINITY`.
    fn priority_flood(&mut self, mut heap: BinaryHeap<HeapEntry>, fill_epsilon: f32) {
        while let Some(HeapEntry { key, idx }) = heap.pop() {
            if key > self.filled[idx] {
                continue; // a stale heap entry, superseded by a lower route
            }
            for step in self.grid.corner_neighbors(idx).into_iter().flatten() {
                if !self.valid[step.corner] {
                    continue;
                }
                let candidate = self.elevation[step.corner].max(self.filled[idx] + fill_epsilon);
                if candidate < self.filled[step.corner] {
                    self.filled[step.corner] = candidate;
                    heap.push(HeapEntry {
                        key: candidate,
                        idx: step.corner,
                    });
                }
            }
        }
    }

    /// Routable corners in descending `filled` order (ties by index ascending). Because a corner's
    /// downstream is strictly lower, this is a topological order of the drainage tree — and it is
    /// deterministic, which is what accumulation and Strahler both depend on.
    fn build_topo_order(&mut self) {
        let mut order: Vec<usize> = (0..self.filled.len())
            .filter(|&idx| self.valid[idx] && self.filled[idx].is_finite())
            .collect();
        order.sort_by(|a, b| {
            self.filled[*b]
                .partial_cmp(&self.filled[*a])
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        self.topo_order = order;
    }

    /// Flow direction = **steepest descent on the filled surface**. All three corner→corner steps
    /// cover exactly one hex side, so "steepest" is simply "lowest filled neighbour"; ties break by
    /// corner index ascending, so the tree is deterministic.
    fn derive_downstream(&mut self) {
        for idx in 0..self.filled.len() {
            if !self.valid[idx] || !self.filled[idx].is_finite() || self.sink[idx] {
                continue;
            }
            let mut best: Option<(f32, usize)> = None;
            for step in self.grid.corner_neighbors(idx).into_iter().flatten() {
                if !self.valid[step.corner] || !self.filled[step.corner].is_finite() {
                    continue;
                }
                let candidate = self.filled[step.corner];
                if candidate >= self.filled[idx] {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some((best_fill, best_idx)) => {
                        candidate < best_fill || (candidate == best_fill && step.corner < best_idx)
                    }
                };
                if better {
                    best = Some((candidate, step.corner));
                }
            }
            // The epsilon fill guarantees a strictly lower neighbour exists for every reachable
            // non-sink corner (it is the one that flooded this one), so `best` is always `Some`.
            self.downstream[idx] = best.map(|(_, corner)| corner).unwrap_or(usize::MAX);
        }
    }

    /// Sum each corner's own runoff downstream. The seeds were laid in `build`, so what this adds is
    /// only the upstream contribution — walking the deterministic topological order means every
    /// contributor is complete before its downstream is read.
    ///
    /// The result is **precipitation-weighted upstream drainage area in hex-equivalents**: a corner
    /// whose accumulation is 300 drains the runoff of 300 fully-wet hexes.
    fn accumulate(&mut self) {
        for i in 0..self.topo_order.len() {
            let idx = self.topo_order[i];
            let down = self.downstream[idx];
            if down != usize::MAX {
                self.accumulation[down] += self.accumulation[idx];
            }
        }
    }

    fn is_routable(&self, corner: usize) -> bool {
        self.valid[corner] && self.filled[corner].is_finite()
    }
}

/// Per-hex precipitation in `[0, 1]`, read from the worldgen `MoistureRaster`.
struct PrecipField<'a> {
    moisture: Option<&'a MoistureRaster>,
    grid: HexGrid,
}

impl<'a> PrecipField<'a> {
    fn new(moisture: Option<&'a MoistureRaster>, grid: HexGrid) -> Self {
        let expected = (grid.width as usize) * (grid.height as usize);
        let usable = moisture.filter(|m| {
            m.width == grid.width && m.height == grid.height && m.values.len() == expected
        });
        if usable.is_none() {
            tracing::warn!(
                target: "shadow_scale::mapgen",
                present = moisture.is_some(),
                "hydrology.moisture_raster_unusable: falling back to uniform precipitation"
            );
        }
        Self {
            moisture: usable,
            grid,
        }
    }

    fn sample(&self, hex: UVec2) -> f32 {
        match self.moisture {
            Some(raster) => raster.values[self.grid.tile_index(hex)].clamp(0.0, 1.0),
            None => DEFAULT_UNIFORM_PRECIP,
        }
    }
}

// ---------------------------------------------------------------------------
// The drainage network: channel corners, their tree, and Strahler order
// ---------------------------------------------------------------------------

/// The channel network carved out of the flow field.
///
/// Accumulation is monotone non-decreasing downstream, so the channel corners **plus their descent
/// links form a forest of trees rooted at outlets, by construction** — there is nothing to reject,
/// space, or count-target. Sinks are deliberately not channel corners: they are in the ocean.
struct DrainageNetwork {
    channel: Vec<bool>,
    /// Channel corners draining into each corner, sorted by (accumulation DESC, index ASC) — so the
    /// first is always the main stem.
    contributors: Vec<Vec<usize>>,
    /// Strahler order per channel corner (`0` elsewhere).
    order: Vec<u8>,
    /// Channel corners whose downstream is *not* a channel corner — the mouths.
    outlets: Vec<usize>,
}

impl DrainageNetwork {
    fn extract(field: &CornerField, channel_min: f32) -> Self {
        let count = field.filled.len();
        let channel: Vec<bool> = (0..count)
            .map(|idx| {
                field.is_routable(idx) && !field.sink[idx] && field.accumulation[idx] >= channel_min
            })
            .collect();

        let mut contributors: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (idx, &is_channel) in channel.iter().enumerate() {
            if !is_channel {
                continue;
            }
            let down = field.downstream[idx];
            if down != usize::MAX {
                contributors[down].push(idx);
            }
        }
        for list in contributors.iter_mut() {
            list.sort_by(|a, b| {
                field.accumulation[*b]
                    .partial_cmp(&field.accumulation[*a])
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
        }

        let mut outlets: Vec<usize> = (0..count)
            .filter(|&idx| {
                channel[idx] && {
                    let down = field.downstream[idx];
                    down == usize::MAX || !channel[down]
                }
            })
            .collect();
        outlets.sort_by(|a, b| {
            field.accumulation[*b]
                .partial_cmp(&field.accumulation[*a])
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        let order = Self::strahler(field, &channel, &contributors);
        Self {
            channel,
            contributors,
            order,
            outlets,
        }
    }

    /// Strahler order on the **real channel tree** (not on a per-tile flow field, where it was never
    /// defined). A channel corner with no channel contributors is order 1; otherwise its order is
    /// the greatest contributor order, **+1 iff at least two contributors share it**.
    fn strahler(field: &CornerField, channel: &[bool], contributors: &[Vec<usize>]) -> Vec<u8> {
        let mut order = vec![0u8; channel.len()];
        // Descending `filled` = every contributor is resolved before the corner it feeds.
        for &idx in &field.topo_order {
            if !channel[idx] {
                continue;
            }
            let mut max_order = 0u8;
            let mut at_max = 0usize;
            for &up in &contributors[idx] {
                let up_order = order[up];
                match up_order.cmp(&max_order) {
                    Ordering::Greater => {
                        max_order = up_order;
                        at_max = 1;
                    }
                    Ordering::Equal => at_max += 1,
                    Ordering::Less => {}
                }
            }
            order[idx] = if max_order == 0 {
                1
            } else if at_max >= 2 {
                max_order.saturating_add(1)
            } else {
                max_order
            };
        }
        order
    }

    /// **Main-stem decomposition.** Walk each outlet's tree *upstream*, always taking the largest
    /// unclaimed contributor: that path is the classic main stem ("the Missouri joins the
    /// Mississippi"), and each contributor it passes over becomes a tributary stem joining at
    /// exactly the corner it was passed over at. Every channel corner lands in exactly one stem.
    ///
    /// Upstream-from-the-outlet, **not** downstream-from-headwaters: every headwater's accumulation
    /// is barely above the channel threshold (nothing upstream of it is a channel), so "the biggest
    /// headwater" does not identify the main stem — but "always take the biggest contributor,
    /// walking up from the mouth" does, by definition.
    ///
    /// Returns each stem as `(path headwater→mouth, terminus corner)`, in emission order: the
    /// largest outlet's main stem first, then its tributaries, and so on.
    fn decompose(&self, field: &CornerField) -> Vec<Stem> {
        let mut claimed = vec![false; self.channel.len()];
        let mut stems: Vec<Stem> = Vec::new();

        for &outlet in &self.outlets {
            if claimed[outlet] {
                continue;
            }
            let mut queue: VecDeque<usize> = VecDeque::from([outlet]);
            while let Some(start) = queue.pop_front() {
                if claimed[start] {
                    continue;
                }
                let terminus = field.downstream[start];
                let mut path: Vec<usize> = Vec::new();
                let mut current = start;
                loop {
                    path.push(current);
                    claimed[current] = true;
                    // Already sorted (accumulation DESC, index ASC) — the head is the main stem,
                    // and everything else joins the network *at this corner*.
                    let mut unclaimed = self.contributors[current]
                        .iter()
                        .copied()
                        .filter(|&c| !claimed[c]);
                    let Some(best) = unclaimed.next() else {
                        break;
                    };
                    queue.extend(unclaimed);
                    current = best;
                }
                path.reverse(); // headwater → mouth
                stems.push(Stem { path, terminus });
            }
        }
        stems
    }
}

/// One channel path plus the corner it hands its water to: an ocean sink corner (a main stem), or a
/// corner on the stem it is a tributary of.
struct Stem {
    /// Channel corners, headwater → mouth.
    path: Vec<usize>,
    /// `downstream` of the path's last corner. `usize::MAX` only if the flow field failed to give
    /// the outlet a downstream at all (it always does — see `derive_downstream`).
    terminus: usize,
}

// ---------------------------------------------------------------------------
// Emission: a stem becomes river edges (+ a navigable hex tail)
// ---------------------------------------------------------------------------

/// One emitted river before the noise gate: its edge chain, its navigable tail, and how it ended.
struct TracedRiver {
    edges: Vec<RiverEdge>,
    navigable_hexes: Vec<UVec2>,
    /// The corner of `navigable_hexes[0]` the edge chain arrives at (see `RiverInflow`).
    navigable_inflow: Option<RiverInflow>,
    /// The stem corner this river's geometry ends on — what its Strahler order is read from.
    end_corner: usize,
}

impl TracedRiver {
    /// Length in **hexes**, so `river_min_length` stays denominated in hexes across both models: a
    /// corner step covers one hex side (~half a hex of downstream progress), while a navigable hex
    /// is a whole hex.
    fn hex_length(&self) -> usize {
        self.edges.len() / CORNER_STEPS_PER_HEX + self.navigable_hexes.len()
    }
}

/// Everything emitting a stem needs: the flow field it reads discharge from, the class thresholds,
/// and the water mask that says where the map's water already is.
struct StemEmitter<'a> {
    grid: HexGrid,
    field: &'a CornerField,
    tiles: &'a TileWorld<'a>,
    elevation_field: &'a ElevationField,
    thresholds: RiverClassThresholds,
    /// The shortest navigable hex chain that still reads as a river. A shorter chain is a puddle, so
    /// it is demoted to the river's edge (Major) form (`river_navigable_min_hexes`).
    navigable_min_hexes: usize,
}

impl StemEmitter<'_> {
    /// Emit a stem as **one river per contiguous run of steps that touch no standing water** — but a
    /// river **connects** to the water it ends at rather than stopping one step short of it.
    ///
    /// A river **ends the moment it touches standing water, and a new river begins where it leaves**:
    /// feed-in and drain-out are separate segments, not one river threaded through — or *around* — the
    /// water. The first water-touching edge is emitted as the **mouth** — the connecting edge that
    /// reaches the water — and terminates the run; the *rest* of the consecutive water-touching edges
    /// (the shore-hug + the submerged stretch) are then **skipped, not drawn**, and a new run resumes
    /// at the next dry (non-water-touching) edge. So there is exactly ONE water-touching edge per
    /// river and it is the LAST one: the river runs *into* the lake/sea/trunk and stops, rather than
    /// hugging the shore along it, and the drain-out below re-emerges as its own segment (connected on
    /// its source side, its first corner being water-adjacent). The accumulation still flows through
    /// underneath, so the outlet stays a big river below a big lake — only the rendered segmentation
    /// changes. The split is also required because a `RiverSegment`'s edge chain and navigable chain
    /// are both **paths**: a chain with a water-shaped hole in it would be neither contiguous nor
    /// drawable. Index-based so the shore-hug stretch can be skipped without re-emitting fragments.
    fn emit(&self, stem: &Stem, existing_navigable: &HashSet<usize>) -> Vec<TracedRiver> {
        let steps = self.steps(stem);
        let mut rivers = Vec::new();
        let mut run: Vec<(usize, CornerStep)> = Vec::new();
        let mut i = 0;
        while i < steps.len() {
            let (from, step) = steps[i];
            if self.edge_touches_water(&step, existing_navigable) {
                // Emit the CONNECTING edge (it reaches the water = the mouth), terminate the run here,
                // then skip the rest of the consecutive water-touching edges (shore-hug + submerged):
                // they are NOT drawn and NOT turned into fragments.
                run.push((from, step));
                if let Some(river) = self.emit_run(&run, stem.terminus, existing_navigable) {
                    rivers.push(river);
                }
                run.clear();
                i += 1;
                while i < steps.len() && self.edge_touches_water(&steps[i].1, existing_navigable) {
                    i += 1;
                }
                continue;
            }
            run.push((from, step));
            i += 1;
        }
        if let Some(river) = self.emit_run(&run, stem.terminus, existing_navigable) {
            rivers.push(river);
        }
        rivers
    }

    /// The stem's corner path as steps: each consecutive pair, plus the final hand-over to the
    /// terminus. That final step is what makes a main stem **touch the shore** (the terminus is the
    /// ocean-touching sink corner) and a tributary **land on its trunk** (the terminus is a claimed
    /// corner of the parent stem) — one uniform rule, no special case.
    fn steps(&self, stem: &Stem) -> Vec<(usize, CornerStep)> {
        let mut steps = Vec::with_capacity(stem.path.len());
        for pair in stem.path.windows(2) {
            if let Some(step) = self.grid.step_between(pair[0], pair[1]) {
                steps.push((pair[0], step));
            }
        }
        if let (Some(&last), true) = (stem.path.last(), stem.terminus != usize::MAX) {
            if let Some(step) = self.grid.step_between(last, stem.terminus) {
                steps.push((last, step));
            }
        }
        steps
    }

    /// **Either** hex flanking this edge is standing water — the river has reached the shore. Standing
    /// water is a lake / inland sea / ocean on the terrain map (`is_water_hex`) **or** a previously
    /// stamped navigable trunk (`existing_navigable`); the latter is not on the terrain map yet during
    /// extraction, so both sources must be consulted. A half-submerged edge (land on one bank, water
    /// on the other) is what makes an edge river hug a shoreline or a tributary climb the side of a
    /// trunk hex to a far corner; ending the run here terminates the river *into* the water instead.
    fn edge_touches_water(&self, step: &CornerStep, existing_navigable: &HashSet<usize>) -> bool {
        let is_standing_water = |hex: UVec2| {
            self.tiles.is_water_hex(hex) || existing_navigable.contains(&self.grid.tile_index(hex))
        };
        if is_standing_water(step.hex) {
            return true;
        }
        self.grid
            .neighbor(step.hex, step.dir)
            .is_some_and(is_standing_water)
    }

    /// Trace one run into a river, then enforce the two navigable invariants: a navigable chain must
    /// **connect to standing water** (Part B) and be no shorter than `navigable_min_hexes` (Part C).
    /// A chain that dead-ends on dry land, or is a 1–2 hex puddle, is **demoted** to the river's edge
    /// (Major) form — the river stays, it just isn't navigable water — by re-tracing the same run with
    /// the navigable model disabled.
    fn emit_run(
        &self,
        run: &[(usize, CornerStep)],
        terminus: usize,
        existing_navigable: &HashSet<usize>,
    ) -> Option<TracedRiver> {
        let traced = self.trace_run(
            run,
            terminus,
            existing_navigable,
            self.thresholds.navigable_enabled,
        )?;
        if !traced.navigable_hexes.is_empty()
            && (!self.navigable_reaches_water(&traced.navigable_hexes, existing_navigable)
                || traced.navigable_hexes.len() < self.navigable_min_hexes)
        {
            // Demote: this navigable chain is landlocked or too short. Re-emit the whole run as Major
            // edges (navigable disabled) so the river survives on the edge model.
            return self.trace_run(run, terminus, existing_navigable, false);
        }
        Some(traced)
    }

    /// The last hex of a navigable chain is standing water itself (it merged onto an existing trunk)
    /// or is hex-adjacent to it (sea / lake / a stamped navigable trunk) — i.e. the chain **reaches**
    /// the water rather than dead-ending on dry land.
    fn navigable_reaches_water(
        &self,
        chain: &[UVec2],
        existing_navigable: &HashSet<usize>,
    ) -> bool {
        let Some(&last) = chain.last() else {
            return false;
        };
        let is_standing = |hex: UVec2| {
            self.tiles.is_water_hex(hex) || existing_navigable.contains(&self.grid.tile_index(hex))
        };
        is_standing(last)
            || (0..HEX_DIRECTION_COUNT as u8)
                .filter_map(|dir| self.grid.neighbor(last, dir))
                .any(is_standing)
    }

    /// Trace one run's edges + navigable tail. `navigable_enabled` overrides the emitter's threshold
    /// so `emit_run` can re-trace a demoted run purely on the edge model.
    fn trace_run(
        &self,
        run: &[(usize, CornerStep)],
        terminus: usize,
        existing_navigable: &HashSet<usize>,
        navigable_enabled: bool,
    ) -> Option<TracedRiver> {
        if run.is_empty() {
            return None;
        }
        let thresholds = RiverClassThresholds {
            navigable_enabled,
            ..self.thresholds
        };

        let mut edges: Vec<RiverEdge> = Vec::new();
        let mut last_emitted: Option<RiverEdge> = None;
        let mut navigable_at: Option<usize> = None;
        let mut navigable_from: Option<UVec2> = None;
        let mut navigable_inflow: Option<RiverInflow> = None;

        for (index, (from, step)) in run.iter().enumerate() {
            // Discharge of the edge about to be crossed = accumulation at its upstream corner. This
            // is monotonically non-decreasing downstream, so an edge's class never shrinks.
            let discharge = self.field.accumulation[*from];
            match thresholds.classify(discharge) {
                Some(class) => {
                    let edge = RiverEdge {
                        hex: step.hex,
                        dir: step.dir,
                        class,
                        discharge,
                    };
                    edges.push(edge);
                    last_emitted = Some(edge);
                }
                None => {
                    // The river has outgrown the edge model: it becomes a body of water. The hex
                    // chain must join the edge chain across a shared EDGE, so it is anchored on the
                    // last edge actually **emitted** — not on this one, which is skipped.
                    //
                    // Both edges are incident to `from`, and *three* hexes meet at a corner: the two
                    // hexes flanking the un-emitted edge can include the third hex, the one the
                    // emitted chain never touches. Anchoring there let the two chains meet at a bare
                    // corner, so the first navigable hex carried no `river_edges` bits and the
                    // tributary visibly dead-ended at the trunk. Anchoring on the last emitted edge
                    // makes the shared edge true by construction. A river that crosses the threshold
                    // on its very first step emitted nothing to anchor to, so it falls back to the
                    // edge it stopped at.
                    let (anchor_hex, anchor_dir) = last_emitted
                        .map(|last| (last.hex, last.dir))
                        .unwrap_or((step.hex, step.dir));
                    navigable_from = self.channel_hex(anchor_hex, anchor_dir);

                    // `from` is the corner the last emitted edge *landed on* — the vertex where the
                    // water leaves the edge model and enters the navigable hex. It is an endpoint of
                    // that edge, hence a corner of both its flanking hexes, so it always resolves to
                    // a local corner of `navigable_from`. A river with no emitted edges has no
                    // tributary: it reports no inflow rather than inventing one.
                    navigable_inflow =
                        last_emitted.zip(navigable_from).and_then(|(last, first)| {
                            self.grid
                                .local_corner_index(first, *from)
                                .map(|corner| RiverInflow {
                                    hex: first,
                                    corner,
                                    class: last.class,
                                })
                        });
                    navigable_at = Some(index);
                    break;
                }
            }
        }

        let navigable_hexes = match (navigable_at, navigable_from) {
            (Some(index), Some(first)) => {
                self.navigable_chain(first, &run[index..], existing_navigable)
            }
            _ => Vec::new(),
        };

        // The inflow is anchored on `navigable_from`, so it is only meaningful while that hex is
        // still the head of the chain — if the chain came back empty (its anchor was water), there
        // is no tile to carry it.
        let mut navigable_inflow =
            navigable_inflow.filter(|_| navigable_hexes.first() == navigable_from.as_ref());

        // The *other* hand-over: this river stayed on the edge model all the way, and its last edge
        // lands on a vertex of an **already-stamped navigable trunk** — a tributary joining a great
        // river mid-chain. Without recording it, the tributary's edge band ends at a bare vertex
        // while the trunk's arms only reach its edge midpoints, and the tributary visibly dead-ends
        // short of the water it feeds. Stems are emitted main-stem-first, so the trunk is always
        // already there when its tributaries arrive.
        if navigable_inflow.is_none() {
            navigable_inflow = last_emitted.zip(run.last()).and_then(|(last, (_, step))| {
                self.trunk_handover(step.corner, existing_navigable)
                    .and_then(|hex| {
                        self.grid
                            .local_corner_index(hex, step.corner)
                            .map(|corner| RiverInflow {
                                hex,
                                corner,
                                class: last.class,
                            })
                    })
            });
        }

        if edges.is_empty() && navigable_hexes.is_empty() {
            return None;
        }

        let end_corner = run
            .last()
            .map(|(from, step)| {
                // The run's most downstream corner **that belongs to this river** — the one whose
                // Strahler order is this segment's own. The last step lands on our own corner unless
                // it is the stem's `terminus`, which is either a sink (no order) or a **foreign trunk
                // corner** carrying the *trunk's* order, not ours. Comparing against the terminus
                // directly is exact; the earlier `!sink` test missed the foreign-trunk case and made
                // a tributary report the order of the trunk it joined.
                if step.corner == terminus {
                    *from
                } else {
                    step.corner
                }
            })
            .unwrap_or(0);

        Some(TracedRiver {
            edges,
            navigable_hexes,
            navigable_inflow,
            end_corner,
        })
    }

    /// The navigable trunk hex this river's edge chain lands on, if any: of the three hexes meeting
    /// at the terminal corner, the lowest one that is already a navigable channel. (Two of the three
    /// can be channel hexes where a trunk bends around the vertex; the water arrives at the vertex,
    /// so either reads the same and the lower is the one it runs into.)
    fn trunk_handover(&self, corner: usize, existing_navigable: &HashSet<usize>) -> Option<UVec2> {
        let (pos, slot) = self.grid.corner_parts(corner);
        let hexes = self.grid.corner_hexes(pos, slot)?;
        hexes
            .into_iter()
            .filter(|hex| existing_navigable.contains(&self.grid.tile_index(*hex)))
            .min_by(|a, b| {
                self.elevation_field
                    .sample(a.x, a.y)
                    .partial_cmp(&self.elevation_field.sample(b.x, b.y))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| (a.y, a.x).cmp(&(b.y, b.x)))
            })
    }

    /// The `NavigableRiver` hex chain, read straight off the river's own corner path: the hex the
    /// channel is *inside* at each remaining step.
    ///
    /// Consecutive steps share a corner, and the three hexes meeting at a corner are pairwise
    /// adjacent — so consecutive picks are adjacent (or identical) and the chain is **contiguous by
    /// construction**. `hex_contiguous_chain` stays as the belt-and-braces check.
    ///
    /// Two rules keep it a **simple path**, which is what a channel is:
    /// - **Sticky.** While the current hex still flanks the edge being crossed, the river has not
    ///   left it — the corner path is running along that hex's own sides. Without this the chain
    ///   hops between the two banks as the path zig-zags, and can re-enter a hex it already left.
    /// - **No self-crossing.** A channel that would double back onto a hex it already occupies ends
    ///   there. (A corner path never revisits a *corner* — it is a strict descent — but a hex is
    ///   touched by many corners, so the *hex* path can.)
    fn navigable_chain(
        &self,
        first: UVec2,
        steps: &[(usize, CornerStep)],
        existing_navigable: &HashSet<usize>,
    ) -> Vec<UVec2> {
        if self.tiles.is_water_hex(first) {
            // A chain must not begin inside a lake: the river IS the water there.
            return Vec::new();
        }
        let mut raw = vec![first];
        for (_, step) in steps {
            let current = *raw.last().expect("seeded with `first`");
            let far = self.grid.neighbor(step.hex, step.dir);
            if step.hex == current || far == Some(current) {
                continue; // still inside the same hex — the channel has not moved on
            }
            let Some(hex) = self.channel_hex(step.hex, step.dir) else {
                continue;
            };
            if raw.contains(&hex) {
                break; // a channel does not cross itself
            }
            raw.push(hex);
        }
        let chain = hex_contiguous_chain(&raw, &self.grid, self.elevation_field);
        // A chain that reaches standing water has arrived: the water body is the mouth, not another
        // channel hex.
        let chain: Vec<UVec2> = chain
            .into_iter()
            .take_while(|pos| !self.tiles.is_water_hex(*pos))
            .collect();
        truncate_at_existing_channel(chain, &self.grid, existing_navigable)
    }

    /// The hex a navigable channel occupies where it crosses edge `(hex, dir)`: the **lower dry** of
    /// the two flanking hexes. Lower because water settles into the valley, not onto its shoulder;
    /// dry because the channel is the river, and where a flank is already a lake or the sea the
    /// river has *arrived* rather than continuing through it.
    fn channel_hex(&self, hex: UVec2, dir: u8) -> Option<UVec2> {
        let far = self.grid.neighbor(hex, dir)?;
        [hex, far]
            .into_iter()
            .filter(|pos| !self.tiles.is_water_hex(*pos))
            .min_by(|a, b| {
                self.elevation_field
                    .sample(a.x, a.y)
                    .partial_cmp(&self.elevation_field.sample(b.x, b.y))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| (a.y, a.x).cmp(&(b.y, b.x)))
            })
    }
}

/// Make a hex path **hex-contiguous**: bridge any gap between two consecutive hexes that are not
/// odd-r neighbours with the lowest common hex-neighbour (water settles into the valley), and
/// truncate where no bridge exists — a short contiguous waterway is correct, a broken one is not.
///
/// The corner-path construction in `StemEmitter::navigable_chain` already guarantees contiguity, so
/// this is now a defensive identity rather than a repair; it stays because a broken waterway is the
/// one failure mode a navigable river must never have.
fn hex_contiguous_chain(
    path: &[UVec2],
    grid: &HexGrid,
    elevation_field: &ElevationField,
) -> Vec<UVec2> {
    let mut chain: Vec<UVec2> = Vec::with_capacity(path.len());
    let Some(&first) = path.first() else {
        return chain;
    };
    chain.push(first);

    for window in path.windows(2) {
        let (from, to) = (window[0], window[1]);
        if hex_adjacent(from, to, grid) {
            chain.push(to);
            continue;
        }
        let bridge = (0..HEX_DIRECTION_COUNT as u8)
            .filter_map(|dir| grid.neighbor(from, dir))
            .filter(|mid| hex_adjacent(*mid, to, grid))
            .min_by(|a, b| {
                elevation_field
                    .sample(a.x, a.y)
                    .partial_cmp(&elevation_field.sample(b.x, b.y))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| (a.y, a.x).cmp(&(b.y, b.x)))
            });
        match bridge {
            Some(mid) => {
                chain.push(mid);
                chain.push(to);
            }
            None => break,
        }
    }
    chain
}

fn hex_adjacent(a: UVec2, b: UVec2, grid: &HexGrid) -> bool {
    (0..HEX_DIRECTION_COUNT as u8).any(|dir| grid.neighbor(a, dir) == Some(b))
}

/// **Merge on contact.** Cut a freshly traced navigable chain at the first hex that is *already*
/// navigable water (stamped by an earlier-emitted river), keeping that hex as the chain's last
/// element: it is the confluence, the hex where this river hands its water to the trunk.
///
/// Stems are emitted main-stem-first, so a tributary that reaches its trunk finds it already
/// stamped and joins it rather than digging a second channel alongside it.
fn truncate_at_existing_channel(
    chain: Vec<UVec2>,
    grid: &HexGrid,
    existing_navigable: &HashSet<usize>,
) -> Vec<UVec2> {
    // Contact is ADJACENCY, not identity: two water hexes that touch are one body of water. A chain
    // that merely runs *alongside* an existing channel has already joined it — testing identity
    // alone lets parallel chains slide past each other one hex apart and re-form a blob.
    for (index, pos) in chain.iter().enumerate() {
        if existing_navigable.contains(&grid.tile_index(*pos)) {
            // Stepped onto the trunk itself: this chain ends there, on the shared confluence hex.
            return chain.into_iter().take(index + 1).collect();
        }

        // Merely *beside* the trunk. The chain still ends here, but it must end **on** the trunk hex
        // it joined, not next to it: a chain that stopped alongside would be a dead end reaching
        // neither the trunk nor the sea, and the renderer would draw a river that runs into a bank.
        let trunk = (0..HEX_DIRECTION_COUNT as u8)
            .filter_map(|dir| grid.neighbor(*pos, dir))
            .find(|n| existing_navigable.contains(&grid.tile_index(*n)));
        if let Some(trunk) = trunk {
            let mut merged: Vec<UVec2> = chain.into_iter().take(index + 1).collect();
            merged.push(trunk);
            return merged;
        }
    }
    chain
}

/// The hexes of a river in **delta-search order**: like `touched_hexes`, but the two hexes flanking
/// each edge are emitted *higher bank first*, so a land→water transition in the sequence lands on
/// the **low** bank.
///
/// This matters because an edge river runs *between* two hexes: both are equally far downstream, so
/// "the land hex bordering the water" is ambiguous without a tie-break. A delta forms on the low
/// ground where the river drops its load, never on the bluff opposite it — and a delta on the bluff
/// would also be steep coast, which the coastal-shelf pass correctly refuses to put a shelf in front
/// of.
fn delta_scan_order(
    edges: &[RiverEdge],
    navigable: &[UVec2],
    grid: &HexGrid,
    elevation_field: &ElevationField,
) -> Vec<UVec2> {
    let mut hexes = Vec::with_capacity(edges.len() * 2 + navigable.len());
    for edge in edges {
        match grid.neighbor(edge.hex, edge.dir) {
            Some(other) => {
                let here = elevation_field.sample(edge.hex.x, edge.hex.y);
                let there = elevation_field.sample(other.x, other.y);
                if here > there {
                    hexes.push(edge.hex);
                    hexes.push(other);
                } else {
                    hexes.push(other);
                    hexes.push(edge.hex);
                }
            }
            None => hexes.push(edge.hex),
        }
    }
    hexes.extend(navigable.iter().copied());
    hexes
}

/// Every hex a river touches, upstream → downstream (see `RiverSegment::touched_hexes`).
fn touched_hexes(edges: &[RiverEdge], navigable: &[UVec2], grid: &HexGrid) -> Vec<UVec2> {
    let mut hexes = Vec::with_capacity(edges.len() * 2 + navigable.len());
    for edge in edges {
        hexes.push(edge.hex);
        if let Some(other) = grid.neighbor(edge.hex, edge.dir) {
            hexes.push(other);
        }
    }
    hexes.extend(navigable.iter().copied());
    hexes
}

/// Write a river class into one slot of one hex's packed mask — a *side* slot for `river_edges`, a
/// *corner* slot for `river_inflow` (both pack `RiverClass::BITS_PER_DIR` bits per slot, so one
/// routine serves both). The single place the packing layout is applied on the sim side
/// (`Tile::set_river_class_on_side` / `Tile::set_river_class_at_corner` are its component twins).
fn set_tile_river_class(mask: &mut [u16], tile_idx: usize, slot: u8, class: RiverClass) {
    let shift = u32::from(slot) * RiverClass::BITS_PER_DIR;
    mask[tile_idx] &= !(RiverClass::SLOT_MASK << shift);
    mask[tile_idx] |= class.bits() << shift;
}

/// Read a river class back out of a packed mask (the inverse of `set_tile_river_class`).
fn tile_river_class(mask: &[u16], tile_idx: usize, slot: u8) -> RiverClass {
    RiverClass::from_bits(mask[tile_idx] >> (u32::from(slot) * RiverClass::BITS_PER_DIR))
}

/// Merge a river class into a slot, keeping the **wider** of the two.
///
/// Two rivers can hand off to the same navigable hex at the same vertex — three hexes meet at a
/// corner, so two tributaries running down either bank converge there (a confluence *at a corner*,
/// seen on real maps). One slot holds one class, and the class the eye sees arriving is the wider
/// one, so `Major` beats `Minor`. Taking the max also makes the result independent of the order
/// rivers were emitted in, which last-write-wins would not be.
fn widen_tile_river_class(mask: &mut [u16], tile_idx: usize, slot: u8, class: RiverClass) {
    let widest = class.max(tile_river_class(mask, tile_idx, slot));
    set_tile_river_class(mask, tile_idx, slot, widest);
}

/// Record a **channel exit** through side `slot` in a packed channel mask. OR-ed, never
/// overwritten — a confluence hex carries the union of every chain running through it. The single
/// place the channel packing is applied on the sim side (`Tile::set_channel_exit` is its component
/// twin).
fn set_tile_channel_exit(mask: &mut [u8], tile_idx: usize, slot: u8) {
    mask[tile_idx] |= RiverChannel::SLOT_MASK << (u32::from(slot) * RiverChannel::BITS_PER_DIR);
}

/// The odd-r direction stepping from `from` to its neighbour `to`, or `None` if the two are not
/// adjacent. Wrap-aware, because it asks `grid.neighbor` rather than differencing coordinates.
fn direction_between(from: UVec2, to: UVec2, grid: &HexGrid) -> Option<u8> {
    (0..HEX_DIRECTION_COUNT as u8).find(|dir| grid.neighbor(from, *dir) == Some(to))
}

// ---------------------------------------------------------------------------
// The drainage census — the measurement instrument for the network (test-only consumer)
// ---------------------------------------------------------------------------

/// A census of the corner drainage network, restricted where noted to LAND corners (a sink corner is
/// in the ocean: its accumulation is drainage that has already left the land, and is meaningless as
/// a river signal). The three land-corner vectors are index-aligned.
#[doc(hidden)]
pub struct DrainageCensus {
    /// Flow accumulation at each land corner (precipitation-weighted hex-equivalents).
    pub land_accumulation: Vec<f32>,
    /// How many of a land corner's 3 neighbours route into it. **Structurally capped at 2** for a
    /// non-sink corner: one of the three neighbours is the corner's own downstream, and a strict
    /// descent tree can never route it back. `2` is a confluence.
    pub land_contributors: Vec<u32>,
    /// Strahler order of each land corner on the **whole drainage tree** — index-aligned with
    /// `land_accumulation`, and independent of any threshold, so it measures the *landscape's*
    /// branching rather than the extraction's.
    pub land_orders: Vec<u8>,
    /// The contributor count restricted to the **channel tree** — how many *channel* corners drain
    /// into each channel corner. `0` = headwater, `1` = pass-through, `2` = confluence.
    pub channel_contributors: Vec<u32>,
    /// Strahler order of each channel corner (index-aligned with `channel_contributors`).
    pub channel_orders: Vec<u8>,
}

/// Rebuild the flow field and the drainage network from the same inputs `generate_hydrology` feeds
/// them, and report their shape. It measures; it changes nothing.
#[doc(hidden)]
pub fn debug_drainage_census(world: &World) -> DrainageCensus {
    let cfg = world.resource::<SimulationConfig>().clone();
    let width = cfg.grid_size.x;
    let height = cfg.grid_size.y;
    let preset_opt = world
        .get_resource::<MapPresetsHandle>()
        .and_then(|handle| handle.get().get(&cfg.map_preset_id).cloned());
    let seed = world
        .get_resource::<WorldGenSeed>()
        .map(|s| s.0)
        .unwrap_or(0);
    let elevation_field = world
        .get_resource::<ElevationField>()
        .cloned()
        .unwrap_or_else(|| {
            crate::heightfield::build_elevation_field(&cfg, preset_opt.as_ref(), seed)
        });
    let sea_level = preset_opt
        .as_ref()
        .map(|p| p.sea_level)
        .unwrap_or(crate::heightfield::DEFAULT_SEA_LEVEL);
    let grid = HexGrid {
        width,
        height,
        wrap_horizontal: cfg.map_topology.wrap_horizontal,
    };
    let levers = HydrologyLevers::resolve(&cfg, preset_opt.as_ref());
    let moisture = world.get_resource::<MoistureRaster>().cloned();

    let total_tiles = (width * height) as usize;
    let mut tile_terrain: Vec<Option<(TerrainType, TerrainTags)>> = vec![None; total_tiles];
    if let Some(registry) = world.get_resource::<TileRegistry>() {
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            if idx >= total_tiles {
                break;
            }
            if let Some(tile) = world.get::<Tile>(entity) {
                tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
            }
        }
    }
    let seamask = build_seamask(&elevation_field, sea_level, &tile_terrain, grid);
    let tiles = TileWorld {
        grid,
        seamask: &seamask,
        terrain: &tile_terrain,
    };
    let field = CornerField::build(
        &tiles,
        &elevation_field,
        moisture.as_ref(),
        seed,
        &levers.flow,
    );
    let network = DrainageNetwork::extract(&field, levers.channel_min);

    // The whole drainage tree (every land corner, no threshold): its contributors and its Strahler
    // order measure the LANDSCAPE's branching, not the extraction's.
    let land = |corner: usize| field.is_routable(corner) && !field.sink[corner];
    let contributors_of = |corner: usize| -> Vec<usize> {
        grid.corner_neighbors(corner)
            .into_iter()
            .flatten()
            .filter(|step| land(step.corner) && field.downstream[step.corner] == corner)
            .map(|step| step.corner)
            .collect()
    };
    let mut land_order = vec![0u8; field.filled.len()];
    for &corner in &field.topo_order {
        if !land(corner) {
            continue;
        }
        let mut max_order = 0u8;
        let mut at_max = 0usize;
        for up in contributors_of(corner) {
            match land_order[up].cmp(&max_order) {
                Ordering::Greater => {
                    max_order = land_order[up];
                    at_max = 1;
                }
                Ordering::Equal => at_max += 1,
                Ordering::Less => {}
            }
        }
        land_order[corner] = if max_order == 0 {
            1
        } else if at_max >= 2 {
            max_order.saturating_add(1)
        } else {
            max_order
        };
    }

    let mut census = DrainageCensus {
        land_accumulation: Vec::new(),
        land_contributors: Vec::new(),
        land_orders: Vec::new(),
        channel_contributors: Vec::new(),
        channel_orders: Vec::new(),
    };
    for (corner, &order) in land_order.iter().enumerate() {
        if !land(corner) {
            continue;
        }
        census.land_accumulation.push(field.accumulation[corner]);
        census
            .land_contributors
            .push(contributors_of(corner).len() as u32);
        census.land_orders.push(order);
        if network.channel[corner] {
            census
                .channel_contributors
                .push(network.contributors[corner].len() as u32);
            census.channel_orders.push(network.order[corner]);
        }
    }
    census
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Every hydrology lever, resolved once (overrides > preset > default).
struct HydrologyLevers {
    flow: FlowConfig,
    thresholds: RiverClassThresholds,
    /// The extraction threshold, after `river_density` has scaled it.
    channel_min: f32,
    /// The only noise gate left: an emitted river shorter than this (in hexes) is dropped.
    min_length: usize,
    /// The shortest navigable hex chain that still reads as a river; a shorter one is demoted to the
    /// river's edge (Major) form (`river_navigable_min_hexes`).
    navigable_min_hexes: usize,
    /// The gentle-coast gate deltas are stamped under — the shelf's own threshold, so "gentle coast"
    /// means one thing across worldgen.
    coast_height_threshold: f32,
}

impl HydrologyLevers {
    fn resolve(cfg: &SimulationConfig, preset: Option<&crate::map_preset::MapPreset>) -> Self {
        let overrides = &cfg.hydrology;
        let lever = |over: Option<f32>, from_preset: Option<f32>, default: f32| -> f32 {
            over.or(from_preset).unwrap_or(default)
        };

        // `river_density` re-expressed as a multiplier on the channel threshold: higher density →
        // lower threshold → more channels. One knob for "how wet does this map read".
        let density = lever(
            overrides.river_density,
            preset.map(|p| p.river_density),
            crate::map_preset::default_river_density(),
        )
        .clamp(MIN_RIVER_DENSITY, MAX_RIVER_DENSITY);
        let channel_min_discharge = lever(
            overrides.channel_min_discharge,
            preset.map(|p| p.river_channel_min_discharge),
            default_river_channel_min_discharge(),
        );
        let channel_min = (channel_min_discharge / density).max(CHANNEL_MIN_DISCHARGE_FLOOR);

        Self {
            flow: FlowConfig {
                fill_epsilon: lever(
                    overrides.fill_epsilon,
                    preset.map(|p| p.river_fill_epsilon),
                    default_river_fill_epsilon(),
                ),
                flat_jitter: lever(
                    overrides.flat_jitter,
                    preset.map(|p| p.river_flat_jitter),
                    default_river_flat_jitter(),
                ),
                base_runoff: lever(
                    overrides.base_runoff,
                    preset.map(|p| p.river_base_runoff),
                    default_river_base_runoff(),
                ),
                moisture_weight: lever(
                    overrides.moisture_weight,
                    preset.map(|p| p.river_moisture_weight),
                    default_river_moisture_weight(),
                ),
            },
            thresholds: RiverClassThresholds {
                major_min: lever(
                    overrides.class_major_min_discharge,
                    preset.map(|p| p.river_class_major_min_discharge),
                    default_river_class_major_min_discharge(),
                ),
                navigable_min: lever(
                    overrides.class_navigable_min_discharge,
                    preset.map(|p| p.river_class_navigable_min_discharge),
                    default_river_class_navigable_min_discharge(),
                ),
                navigable_enabled: overrides
                    .navigable_enabled
                    .or(preset.map(|p| p.river_navigable_enabled))
                    .unwrap_or(true),
            },
            channel_min,
            min_length: overrides
                .min_length
                .or(preset.map(|p| p.river_min_length))
                .unwrap_or(crate::map_preset::default_river_min_length()),
            navigable_min_hexes: overrides
                .navigable_min_hexes
                .or(preset.map(|p| p.river_navigable_min_hexes))
                .unwrap_or(crate::map_preset::default_river_navigable_min_hexes()),
            coast_height_threshold: preset
                .map(|p| p.shelf.coast_height_threshold)
                .unwrap_or(DEFAULT_COAST_HEIGHT_THRESHOLD),
        }
    }
}

/// `river_density` bounds — a multiplier on the channel threshold, so it must stay strictly positive
/// and can't be allowed to swamp the map.
const MIN_RIVER_DENSITY: f32 = 0.1;
const MAX_RIVER_DENSITY: f32 = 5.0;

/// Fallback for a preset-less world (the shelf's own default, `ShelfConfig::coast_height_threshold`).
const DEFAULT_COAST_HEIGHT_THRESHOLD: f32 = 0.10;

/// The elevation-derived water mask: a tile below sea level is water unless its stamped terrain says
/// otherwise (the terrain, once it exists, is the authority).
fn build_seamask(
    elevation_field: &ElevationField,
    sea_level: f32,
    tile_terrain: &[Option<(TerrainType, TerrainTags)>],
    grid: HexGrid,
) -> Vec<bool> {
    let mut seamask = vec![false; (grid.width * grid.height) as usize];
    for y in 0..grid.height {
        for x in 0..grid.width {
            let idx = grid.tile_index(UVec2::new(x, y));
            let mut water = elevation_field.sample(x, y) <= sea_level;
            if let Some((terrain, _)) = tile_terrain[idx] {
                if !is_water_terrain(terrain) {
                    water = false;
                }
            }
            seamask[idx] = water;
        }
    }
    seamask
}

// ---------------------------------------------------------------------------
// The worldgen pass
// ---------------------------------------------------------------------------

pub fn generate_hydrology(world: &mut World) {
    let cfg = world.resource::<SimulationConfig>().clone();
    let width = cfg.grid_size.x;
    let height = cfg.grid_size.y;
    let preset_opt = world
        .get_resource::<MapPresetsHandle>()
        .and_then(|handle| handle.get().get(&cfg.map_preset_id).cloned());
    let world_seed = world
        .get_resource::<WorldGenSeed>()
        .map(|s| s.0)
        .unwrap_or(0);
    let elevation_field = world
        .get_resource::<ElevationField>()
        .cloned()
        .unwrap_or_else(|| {
            crate::heightfield::build_elevation_field(&cfg, preset_opt.as_ref(), world_seed)
        });
    let moisture = world.get_resource::<MoistureRaster>().cloned();
    let sea_level = preset_opt
        .as_ref()
        .map(|p| p.sea_level)
        .unwrap_or(crate::heightfield::DEFAULT_SEA_LEVEL);
    let levers = HydrologyLevers::resolve(&cfg, preset_opt.as_ref());

    let grid = HexGrid {
        width,
        height,
        wrap_horizontal: cfg.map_topology.wrap_horizontal,
    };
    let total_tiles = (width * height) as usize;

    let mut tile_terrain: Vec<Option<(TerrainType, TerrainTags)>> = vec![None; total_tiles];
    if let Some(registry) = world.get_resource::<TileRegistry>() {
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            if idx >= total_tiles {
                break;
            }
            if let Some(tile) = world.get::<Tile>(entity) {
                tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
            }
        }
    }
    let seamask = build_seamask(&elevation_field, sea_level, &tile_terrain, grid);

    let rivers = {
        let tiles = TileWorld {
            grid,
            seamask: &seamask,
            terrain: &tile_terrain,
        };
        let field = CornerField::build(
            &tiles,
            &elevation_field,
            moisture.as_ref(),
            world_seed,
            &levers.flow,
        );
        let network = DrainageNetwork::extract(&field, levers.channel_min);
        let emitter = StemEmitter {
            grid,
            field: &field,
            tiles: &tiles,
            elevation_field: &elevation_field,
            thresholds: levers.thresholds,
            navigable_min_hexes: levers.navigable_min_hexes,
        };

        // The channel this river lays down is what a later tributary merges *into* (see
        // `truncate_at_existing_channel`), so it must be visible to the emissions that follow. Stems
        // come out main-stem-first, so the trunk always exists before its tributaries reach it.
        let mut navigable_tiles: HashSet<usize> = HashSet::new();
        let mut rivers: Vec<RiverSegment> = Vec::new();
        for stem in network.decompose(&field) {
            for traced in emitter.emit(&stem, &navigable_tiles) {
                if traced.hex_length() < levers.min_length {
                    continue; // noise gate: too short to read as a river
                }
                for pos in &traced.navigable_hexes {
                    navigable_tiles.insert(grid.tile_index(*pos));
                }
                rivers.push(RiverSegment {
                    id: rivers.len() as u32 + 1,
                    order: network.order[traced.end_corner].max(1),
                    edges: traced.edges,
                    navigable_hexes: traced.navigable_hexes,
                    navigable_inflow: traced.navigable_inflow,
                });
            }
        }
        rivers
    };

    let tiles = TileWorld {
        grid,
        seamask: &seamask,
        terrain: &tile_terrain,
    };
    // Gentle coast: the same `elevation.sample - sea_level < coast_height_threshold` test
    // `classify_bands` / `reconcile_coastal_shelf` use to split gentle from cliff coasts. A river
    // that meets the water at a cliff has no delta (it is an estuary).
    let is_gentle_coast = |pos: UVec2| -> bool {
        elevation_field.sample(pos.x, pos.y) - sea_level < levers.coast_height_threshold
    };

    // Deltas are **per transition, not per terminus**: lakes now flow through, so a river both
    // *enters* a standing water body and *leaves* it, and a lacustrine delta and the ocean delta are
    // different tiles on the same river. Walk each river's ordered hex path and stamp a delta at
    // every land→standing-water transition (plus the mouth, where the river's own path ends against
    // the water it drains into) — each still gentle-coast gated, each still required to actually
    // border that water.
    let mut delta_candidates: Vec<usize> = Vec::new();
    let mut navigable_candidates: Vec<usize> = Vec::new();
    // Every navigable hex that is not the last of its own chain. A delta may never take one: the
    // channel flows *through* it, and turning it into depositional land would break the chain in
    // two. A river running along a lake shore before it goes navigable, and a tributary merging onto
    // a trunk hex, both otherwise nominate a live channel hex as a delta.
    let mut navigable_interior: HashSet<usize> = HashSet::new();
    let mut class_histogram = [0usize; RIVER_CLASS_HISTOGRAM_SLOTS];
    let mut total_length = 0usize;
    let mut max_order_seg = 0u8;
    for segment in &rivers {
        total_length += segment.edges.len() + segment.navigable_hexes.len();
        max_order_seg = max_order_seg.max(segment.order);
        for edge in &segment.edges {
            class_histogram[edge.class as usize] += 1;
        }
        for pos in &segment.navigable_hexes {
            navigable_candidates.push(grid.tile_index(*pos));
        }
        for pos in segment
            .navigable_hexes
            .iter()
            .take(segment.navigable_hexes.len().saturating_sub(1))
        {
            navigable_interior.insert(grid.tile_index(*pos));
        }

        let scan = delta_scan_order(
            &segment.edges,
            &segment.navigable_hexes,
            &grid,
            &elevation_field,
        );
        for pair in scan.windows(2) {
            let (land, water) = (pair[0], pair[1]);
            let land_idx = grid.tile_index(land);
            if tiles.is_water(land_idx) || !tiles.is_water_hex(water) {
                continue;
            }
            if is_gentle_coast(land) && hex_adjacent(land, water, &grid) {
                delta_candidates.push(land_idx);
            }
        }
        // The mouth: the river's path simply *ends* against the water it drains into, so the final
        // transition has no successor in the scan to pair with.
        if let Some(&last) = scan.last() {
            let last_idx = grid.tile_index(last);
            let borders_water = (0..HEX_DIRECTION_COUNT as u8).any(|dir| {
                grid.neighbor(last, dir)
                    .map(|n| tiles.is_water_hex(n))
                    .unwrap_or(false)
            });
            if !tiles.is_water(last_idx) && is_gentle_coast(last) && borders_water {
                delta_candidates.push(last_idx);
            }
        }
    }

    // The mouth is a delta, not open water: a navigable river ends in the wetland it deposits.
    // Excluding the delta here also stops the delta stamp below from OR-ing WETLAND onto a tile that
    // was just made WATER.
    let delta_set: HashSet<usize> = delta_candidates
        .iter()
        .copied()
        .filter(|idx| !navigable_interior.contains(idx))
        .collect();
    let navigable_set: HashSet<usize> = navigable_candidates
        .into_iter()
        .filter(|idx| !delta_set.contains(idx))
        .collect();

    // Per-tile river-edge mask: both hexes flanking an edge record it on their own side, so a hex
    // and its neighbour always agree about the river between them. This is the primitive a future
    // movement system reads.
    let mut tile_river_edges = vec![0u16; total_tiles];
    for segment in rivers.iter() {
        for edge in &segment.edges {
            let Some(neighbor) = grid.neighbor(edge.hex, edge.dir) else {
                continue;
            };
            set_tile_river_class(
                &mut tile_river_edges,
                grid.tile_index(edge.hex),
                edge.dir,
                edge.class,
            );
            set_tile_river_class(
                &mut tile_river_edges,
                grid.tile_index(neighbor),
                opposite_dir(edge.dir),
                edge.class,
            );
        }
    }

    // Per-tile river-inflow mask: **a tributary hands over to the channel at this vertex.** With a
    // real drainage network a tributary joins its trunk *mid-chain*, so this is set on whatever
    // navigable hex the handing-over tributary actually terminates at — not only on a chain head. An
    // edge river ends at a vertex, never mid-side, and the edge mask alone cannot say which vertex
    // (a trunk hex can flank several river edges), so the sim states it. Widest-wins on collision.
    let mut tile_river_inflow = vec![0u16; total_tiles];
    for segment in rivers.iter() {
        let Some(inflow) = segment.navigable_inflow.as_ref() else {
            continue;
        };
        widen_tile_river_class(
            &mut tile_river_inflow,
            grid.tile_index(inflow.hex),
            inflow.corner,
            inflow.class,
        );
    }

    // Per-tile channel-exit mask: which sides a navigable hex's channel actually flows out through.
    // The chain is a PATH — each hex links only to its upstream and downstream neighbours — and only
    // the tracer knows which those are. Without this the renderer had to guess from terrain, arming
    // every navigable/water neighbour, so adjacent chains cross-linked into a web of triangles.
    // Bits are OR-ed, never overwritten: a confluence hex carries the union of the chains through it.
    let mut tile_river_channel = vec![0u8; total_tiles];

    // Pass 1 — the chain itself. Consecutive pairs are symmetric, exactly like `river_edges`: hex A
    // exits toward B and B exits back toward A, so the two never disagree about the channel between
    // them. Every segment is laid down before any mouth is decided, so pass 2 sees the finished
    // network rather than a half-built one (order-independence).
    for segment in rivers.iter() {
        for pair in segment.navigable_hexes.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let Some(dir) = direction_between(from, to, &grid) else {
                continue;
            };
            set_tile_channel_exit(&mut tile_river_channel, grid.tile_index(from), dir);
            set_tile_channel_exit(
                &mut tile_river_channel,
                grid.tile_index(to),
                opposite_dir(dir),
            );
        }
    }

    // Pass 2 — the mouth. A chain's final hex must also exit toward the water it drains into (the
    // ocean, an inland sea, or the `RiverDelta` stamped at its own mouth), or the drawn river stops
    // one hex short of the sea. The water body carries no channel of its own, so this exit is
    // deliberately **not** mirrored back — it is the one asymmetric bit in the mask.
    //
    // Only a genuine **dead end** earns it. A tributary that merged into an existing trunk also
    // *ends* on its last hex, but that hex is a confluence in the middle of the trunk: the channel
    // already flows on through it, and handing it a second exit into whatever water it happens to sit
    // beside would draw a spurious arm off the side of the trunk. "Has no exit but the one back
    // upstream" is exactly the test for that, and it does not depend on emission order.
    for segment in rivers.iter() {
        let Some(&last) = segment.navigable_hexes.last() else {
            continue;
        };
        let last_idx = grid.tile_index(last);
        let upstream = segment
            .navigable_hexes
            .iter()
            .rev()
            .nth(1)
            .and_then(|prev| direction_between(last, *prev, &grid));

        let flows_on = (0..HEX_DIRECTION_COUNT as u8)
            .filter(|dir| Some(*dir) != upstream)
            .any(|dir| {
                tile_river_channel[last_idx]
                    & (RiverChannel::SLOT_MASK << (u32::from(dir) * RiverChannel::BITS_PER_DIR))
                    != 0
            });
        if flows_on {
            continue; // A confluence inside a trunk, not a mouth — the water already has a way out.
        }

        let mouth = (0..HEX_DIRECTION_COUNT as u8)
            .filter(|dir| Some(*dir) != upstream)
            .find(|dir| {
                grid.neighbor(last, *dir)
                    .map(|n| {
                        let idx = grid.tile_index(n);
                        // Only open water or the river's own delta — never another *navigable* hex,
                        // which would invent exactly the cross-link this mask exists to prevent.
                        delta_set.contains(&idx)
                            || (tiles.is_water(idx) && !navigable_set.contains(&idx))
                    })
                    .unwrap_or(false)
            });
        if let Some(dir) = mouth {
            set_tile_channel_exit(&mut tile_river_channel, last_idx, dir);
        }
    }

    let mut delta_tiles_applied = 0usize;
    let mut navigable_tiles_applied = 0usize;
    let updates: Vec<(usize, Entity)> = if let Some(registry) = world.get_resource::<TileRegistry>()
    {
        registry
            .tiles
            .iter()
            .enumerate()
            .map(|(idx, &entity)| (idx, entity))
            .collect()
    } else {
        Vec::new()
    };

    let navigable_tags = terrain_definition(TerrainType::NavigableRiver).tags;
    for (idx, entity) in updates {
        let Some(mut tile) = world.get_mut::<Tile>(entity) else {
            continue;
        };
        tile.river_edges = tile_river_edges[idx];
        tile.river_inflow = tile_river_inflow[idx];
        tile.river_channel = tile_river_channel[idx];

        if navigable_set.contains(&idx) && tile.terrain != TerrainType::NavigableRiver {
            // A navigable river IS the hex — take the terrain's own tags wholesale rather than
            // OR-ing water onto whatever biome was there. But PRESERVE the biome it was cut through:
            // the channel yields the valley's forage/graze, not open water (read via
            // `Tile::resource_terrain`). Captured before the overwrite, only on tiles that actually
            // become navigable.
            tile.underlying_terrain = Some(tile.terrain);
            tile.terrain = TerrainType::NavigableRiver;
            tile.terrain_tags = navigable_tags;
            navigable_tiles_applied += 1;
        } else if delta_set.contains(&idx) && tile.terrain != TerrainType::RiverDelta {
            tile.terrain = TerrainType::RiverDelta;
            tile.terrain_tags |= TerrainTags::WETLAND;
            tile.terrain_tags |= TerrainTags::FRESHWATER;
            delta_tiles_applied += 1;
        }
    }

    let river_count = rivers.len();
    let total_edges: usize = rivers.iter().map(|r| r.edges.len()).sum();
    let avg_length = if river_count == 0 {
        0.0
    } else {
        total_length as f32 / river_count as f32
    };
    let navigable_rivers = rivers
        .iter()
        .filter(|r| !r.navigable_hexes.is_empty())
        .count();
    let max_discharge = rivers
        .iter()
        .flat_map(|r| r.edges.iter().map(|e| e.discharge))
        .fold(0.0f32, f32::max);

    let mut state = world
        .remove_resource::<HydrologyState>()
        .unwrap_or_default();
    state.rivers = rivers;
    world.insert_resource(state);

    tracing::info!(
        target: "shadow_scale::mapgen",
        rivers = river_count,
        avg_length,
        max_order = max_order_seg,
        max_discharge,
        delta_tiles = delta_tiles_applied,
        total_edges,
        minor_edges = class_histogram[RiverClass::Minor as usize],
        major_edges = class_histogram[RiverClass::Major as usize],
        navigable_rivers,
        navigable_tiles = navigable_tiles_applied,
        channel_min = levers.channel_min,
        class_major_min = levers.thresholds.major_min,
        class_navigable_min = levers.thresholds.navigable_min,
        "hydrology.generated"
    );
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HydrologyState>();
};

/// A min-heap entry for the priority flood (`BinaryHeap` is a max-heap, so `Ord` is reversed).
/// **Ties break by index ascending** — the pop order is a total order, which is what makes the fill
/// deterministic.
#[derive(Copy, Clone, Debug)]
struct HeapEntry {
    key: f32,
    idx: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.idx == other.idx
    }
}

impl Eq for HeapEntry {}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed on `key` (lowest pops first) and reversed again on `idx` (lowest index pops
        // first among equal keys).
        other
            .key
            .partial_cmp(&self.key)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.idx.cmp(&self.idx))
    }
}

/// Test-only geometry cross-check: the two corners an edge connects, derived *independently* of
/// `corner_neighbors`. Used to assert that a traced edge chain is contiguous and that the adjacency
/// table is self-consistent.
#[cfg(test)]
impl HexGrid {
    fn edge_corners(&self, hex: UVec2, dir: u8) -> Option<[usize; 2]> {
        match dir {
            // The vertical edge: its endpoints are the bottom corner of the hex NE of it and the
            // top corner of the hex SE of it.
            DIR_E => Some([
                self.corner_index(self.neighbor(hex, DIR_SE)?, CORNER_TOP),
                self.corner_index(self.neighbor(hex, DIR_NE)?, CORNER_BOTTOM),
            ]),
            DIR_SE => Some([
                self.corner_index(hex, CORNER_BOTTOM),
                self.corner_index(self.neighbor(hex, DIR_SE)?, CORNER_TOP),
            ]),
            DIR_SW => Some([
                self.corner_index(hex, CORNER_BOTTOM),
                self.corner_index(self.neighbor(hex, DIR_SW)?, CORNER_TOP),
            ]),
            _ => {
                let (canon_hex, canon_dir) = self.canonical_edge(hex, dir)?;
                self.edge_corners(canon_hex, canon_dir)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ElementKind, Tile};
    use crate::grid_utils::hex_edge_corner_indices;
    use crate::resources::TileRegistry;
    use crate::scalar::scalar_zero;

    const TEST_SEA_LEVEL: f32 = 0.4;

    fn idx(width: u32, x: u32, y: u32) -> usize {
        (y * width + x) as usize
    }

    fn grid(width: u32, height: u32, wrap: bool) -> HexGrid {
        HexGrid {
            width,
            height,
            wrap_horizontal: wrap,
        }
    }

    fn test_flow() -> FlowConfig {
        FlowConfig {
            fill_epsilon: default_river_fill_epsilon(),
            flat_jitter: default_river_flat_jitter(),
            base_runoff: default_river_base_runoff(),
            moisture_weight: default_river_moisture_weight(),
        }
    }

    /// A synthetic map's tile layer plus its corner flow field.
    struct Fixture {
        grid: HexGrid,
        seamask: Vec<bool>,
        terrain: Vec<Option<(TerrainType, TerrainTags)>>,
        elevation: ElevationField,
    }

    impl Fixture {
        fn new(
            grid: HexGrid,
            elevations: Vec<f32>,
            water: &dyn Fn(u32, u32) -> bool,
            terrain_at: &dyn Fn(u32, u32) -> TerrainType,
        ) -> Self {
            let total = (grid.width * grid.height) as usize;
            let mut seamask = vec![false; total];
            let mut terrain = vec![None; total];
            for y in 0..grid.height {
                for x in 0..grid.width {
                    let i = idx(grid.width, x, y);
                    let t = terrain_at(x, y);
                    terrain[i] = Some((t, terrain_definition(t).tags));
                    seamask[i] = water(x, y);
                }
            }
            Self {
                grid,
                seamask,
                terrain,
                elevation: ElevationField::new(grid.width, grid.height, elevations),
            }
        }

        fn tiles(&self) -> TileWorld<'_> {
            TileWorld {
                grid: self.grid,
                seamask: &self.seamask,
                terrain: &self.terrain,
            }
        }

        fn field(&self, tiles: &TileWorld) -> CornerField {
            CornerField::build(tiles, &self.elevation, None, 0, &test_flow())
        }
    }

    fn edge_only_thresholds() -> RiverClassThresholds {
        RiverClassThresholds {
            major_min: f32::INFINITY,
            navigable_min: f32::INFINITY,
            navigable_enabled: false,
        }
    }

    /// Emit every river of a fixture at the given thresholds — the whole extraction pipeline, as
    /// `generate_hydrology` runs it.
    fn extract(
        fixture: &Fixture,
        thresholds: RiverClassThresholds,
        channel_min: f32,
    ) -> Vec<TracedRiver> {
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);
        let network = DrainageNetwork::extract(&field, channel_min);
        let emitter = StemEmitter {
            grid: fixture.grid,
            field: &field,
            tiles: &tiles,
            elevation_field: &fixture.elevation,
            thresholds,
            // Unit fixtures exercise the hand-off geometry on tiny maps, so the Part C puddle gate is
            // held off (1) here; the worldgen minimum is swept by the integration tests.
            navigable_min_hexes: 1,
        };
        let mut navigable: HashSet<usize> = HashSet::new();
        let mut out = Vec::new();
        for stem in network.decompose(&field) {
            for traced in emitter.emit(&stem, &navigable) {
                for pos in &traced.navigable_hexes {
                    navigable.insert(fixture.grid.tile_index(*pos));
                }
                out.push(traced);
            }
        }
        out
    }

    #[test]
    fn canonical_edge_round_trips_both_representations() {
        for wrap in [false, true] {
            let g = grid(8, 6, wrap);
            for y in 0..g.height {
                for x in 0..g.width {
                    let pos = UVec2::new(x, y);
                    for dir in 0..HEX_DIRECTION_COUNT as u8 {
                        let Some(canon) = g.canonical_edge(pos, dir) else {
                            continue;
                        };
                        // The canonical form always names the edge by one of {E, SE, SW}.
                        assert!(canon.1 < CANONICAL_DIR_COUNT, "{canon:?} is not canonical");
                        // The *other* representation of the same edge canonicalizes identically.
                        let other = g
                            .neighbor(pos, dir)
                            .expect("edge exists, so its far hex exists");
                        let mirrored = g
                            .canonical_edge(other, opposite_dir(dir))
                            .expect("the mirrored representation names the same edge");
                        assert_eq!(
                            canon, mirrored,
                            "({pos:?}, {dir}) and its mirror disagree (wrap={wrap})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn canonical_edge_round_trips_across_the_wrap_seam() {
        let g = grid(8, 6, true);
        // The seam edge between column 7 and column 0.
        let east = g.canonical_edge(UVec2::new(7, 2), DIR_E).expect("wraps");
        let west = g
            .canonical_edge(UVec2::new(0, 2), opposite_dir(DIR_E))
            .expect("wraps");
        assert_eq!(east, west);
        assert_eq!(east, (UVec2::new(7, 2), DIR_E));
    }

    #[test]
    fn corner_adjacency_is_symmetric_and_shares_the_same_edge() {
        for wrap in [false, true] {
            let g = grid(7, 7, wrap);
            for corner in 0..g.corner_count() {
                let (pos, slot) = g.corner_parts(corner);
                if g.corner_hexes(pos, slot).is_none() {
                    continue; // border corner — excluded from routing
                }
                for step in g.corner_neighbors(corner).into_iter().flatten() {
                    // Symmetry: the neighbour lists this corner back...
                    let back = g
                        .corner_neighbors(step.corner)
                        .into_iter()
                        .flatten()
                        .find(|s| s.corner == corner)
                        .unwrap_or_else(|| {
                            panic!("corner {corner} -> {} is not symmetric", step.corner)
                        });
                    // ...across the very same canonical edge.
                    assert_eq!(
                        (back.hex, back.dir),
                        (step.hex, step.dir),
                        "corner {corner} and {} disagree about the edge between them",
                        step.corner
                    );
                }
            }
        }
    }

    #[test]
    fn every_corner_step_crosses_exactly_one_hex_edge() {
        let g = grid(7, 7, true);
        for corner in 0..g.corner_count() {
            let (pos, slot) = g.corner_parts(corner);
            if g.corner_hexes(pos, slot).is_none() {
                continue;
            }
            for step in g.corner_neighbors(corner).into_iter().flatten() {
                let corners = g
                    .edge_corners(step.hex, step.dir)
                    .expect("a step's edge is on the map");
                assert!(
                    corners.contains(&corner) && corners.contains(&step.corner),
                    "step {corner} -> {} claims edge {:?}/{}, whose corners are {corners:?}",
                    step.corner,
                    step.hex,
                    step.dir
                );
            }
        }
    }

    #[test]
    fn local_corner_index_is_a_bijection_on_every_hex() {
        for wrap in [false, true] {
            let g = grid(7, 7, wrap);
            for y in 0..g.height {
                for x in 0..g.width {
                    let pos = UVec2::new(x, y);
                    let mut seen = [false; HEX_CORNER_COUNT];
                    for (index, &(dir, slot)) in HEX_CORNER_LAYOUT.iter().enumerate() {
                        let owner = match dir {
                            Some(dir) => g.neighbor(pos, dir),
                            None => Some(pos),
                        };
                        let Some(owner) = owner else {
                            continue; // off-map corner of a border hex
                        };
                        let corner = g.corner_index(owner, slot);
                        let back = g
                            .local_corner_index(pos, corner)
                            .expect("a corner of `pos` resolves back to a local index");
                        assert_eq!(
                            back as usize, index,
                            "hex {pos:?} corner {index} round-trips to {back}"
                        );
                        assert!(!seen[index], "hex {pos:?} names corner {index} twice");
                        seen[index] = true;
                    }
                }
            }
        }
    }

    /// The center of hex `pos` in world space, pointy-top odd-r, +y down (the client's layout).
    fn hex_center_world(pos: UVec2, radius: f64) -> (f64, f64) {
        let odd = (pos.y & 1) as f64;
        let width = 3f64.sqrt() * radius;
        let x = width * (pos.x as f64 + 0.5 * odd);
        let y = 1.5 * radius * pos.y as f64;
        (x, y)
    }

    /// The world position of the sim's `(hex, TOP|BOTTOM)` corner: TOP is the vertex directly above
    /// the center, BOTTOM the one directly below (+y down).
    fn sim_corner_world(pos: UVec2, slot: u8, radius: f64) -> (f64, f64) {
        let (cx, cy) = hex_center_world(pos, radius);
        if slot == CORNER_TOP {
            (cx, cy - radius)
        } else {
            (cx, cy + radius)
        }
    }

    /// The world position of the client's corner `index`: the vertex at screen angle `60*i + 30`.
    fn client_corner_world(pos: UVec2, index: usize, radius: f64) -> (f64, f64) {
        let (cx, cy) = hex_center_world(pos, radius);
        let angle = (60.0 * index as f64 + 30.0).to_radians();
        (cx + radius * angle.cos(), cy + radius * angle.sin())
    }

    fn assert_same_point(a: (f64, f64), b: (f64, f64), what: &str) {
        const TOLERANCE: f64 = 1e-9;
        assert!(
            (a.0 - b.0).abs() < TOLERANCE && (a.1 - b.1).abs() < TOLERANCE,
            "{what}: {a:?} != {b:?}"
        );
    }

    /// **The wire contract.** `HEX_CORNER_LAYOUT` is pinned to the client's *geometry*, not merely to
    /// itself: a table rotated by one position would still be a bijection, but would put every
    /// tributary on the wrong vertex.
    #[test]
    fn hex_corner_layout_matches_the_clients_corner_geometry() {
        const RADIUS: f64 = 1.0;
        let g = grid(9, 9, false);
        for y in 1..g.height - 1 {
            for x in 1..g.width - 1 {
                let pos = UVec2::new(x, y);
                for (index, &(dir, slot)) in HEX_CORNER_LAYOUT.iter().enumerate() {
                    let owner = match dir {
                        Some(dir) => g.neighbor(pos, dir).expect("interior hex"),
                        None => pos,
                    };
                    assert_same_point(
                        sim_corner_world(owner, slot, RADIUS),
                        client_corner_world(pos, index, RADIUS),
                        &format!("hex {pos:?} corner {index}"),
                    );
                }
            }
        }
    }

    /// The other half of the contract: `grid_utils::hex_edge_corner_indices(dir)` names the two
    /// corners the side in direction `dir` actually spans, in world space.
    #[test]
    fn hex_edge_corner_indices_are_the_shared_edges_endpoints() {
        const RADIUS: f64 = 1.0;
        let g = grid(9, 9, false);
        for y in 1..g.height - 1 {
            for x in 1..g.width - 1 {
                let pos = UVec2::new(x, y);
                for dir in 0..HEX_DIRECTION_COUNT {
                    let neighbor = g.neighbor(pos, dir as u8).expect("interior hex");
                    let [a, b] = hex_edge_corner_indices(dir).expect("dir in range");
                    let opposite = usize::from(opposite_dir(dir as u8));
                    let [c, d] = hex_edge_corner_indices(opposite).expect("dir in range");

                    let near: Vec<(f64, f64)> = [a, b]
                        .iter()
                        .map(|&i| client_corner_world(pos, i, RADIUS))
                        .collect();
                    let far: Vec<(f64, f64)> = [c, d]
                        .iter()
                        .map(|&i| client_corner_world(neighbor, i, RADIUS))
                        .collect();

                    // The same two world points, from either hex (order may differ).
                    for point in &near {
                        assert!(
                            far.iter().any(|other| (point.0 - other.0).abs() < 1e-9
                                && (point.1 - other.1).abs() < 1e-9),
                            "hex {pos:?} side {dir}: corner {point:?} is not shared with {neighbor:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn local_corner_index_rejects_a_corner_the_hex_does_not_touch() {
        let g = grid(7, 7, false);
        let far = g.corner_index(UVec2::new(5, 5), CORNER_TOP);
        assert_eq!(g.local_corner_index(UVec2::new(1, 1), far), None);
    }

    #[test]
    fn hex_edge_corner_indices_match_the_corner_model() {
        for wrap in [false, true] {
            let g = grid(7, 7, wrap);
            for y in 0..g.height {
                for x in 0..g.width {
                    let pos = UVec2::new(x, y);
                    for dir in 0..HEX_DIRECTION_COUNT as u8 {
                        let Some(corners) = g.edge_corners(pos, dir) else {
                            continue;
                        };
                        let Some(expected) = hex_edge_corner_indices(usize::from(dir)) else {
                            panic!("dir {dir} out of range");
                        };
                        let mut local: Vec<usize> = corners
                            .iter()
                            .filter_map(|&c| g.local_corner_index(pos, c))
                            .map(usize::from)
                            .collect();
                        local.sort_unstable();
                        let mut expected = expected.to_vec();
                        expected.sort_unstable();
                        assert_eq!(
                            local, expected,
                            "hex {pos:?} side {dir}: corner model says {local:?}, \
                             hex_edge_corner_indices says {expected:?}"
                        );
                    }
                }
            }
        }
    }

    /// A gentle valley down column 2 to an ocean row: rivers must run the valley and reach the sea.
    fn valley_map(width: u32, height: u32) -> (HexGrid, Vec<f32>) {
        let g = grid(width, height, false);
        let mut elevations = vec![0.0f32; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                elevations[idx(width, x, y)] = if y == 0 {
                    0.2
                } else {
                    0.45 + 0.06 * y as f32 + 0.03 * (x as i32 - 2).unsigned_abs() as f32
                };
            }
        }
        (g, elevations)
    }

    fn ocean_row_terrain(_x: u32, y: u32) -> TerrainType {
        if y == 0 {
            TerrainType::DeepOcean
        } else {
            TerrainType::MixedWoodland
        }
    }

    /// Sinks are the OCEAN and nothing else: a lake is an ordinary low corner the fill raises to its
    /// spill point, so the catchment drains **through** it.
    #[test]
    fn only_ocean_corners_are_sinks() {
        let g = grid(5, 5, false);
        let elevations = vec![TEST_SEA_LEVEL + 0.3; 25];
        let lake = UVec2::new(2, 2);
        let fixture = Fixture::new(
            g,
            elevations,
            &|_, _| false, // nothing below sea level
            &|x, y| {
                if UVec2::new(x, y) == lake {
                    TerrainType::InlandSea
                } else {
                    TerrainType::MixedWoodland
                }
            },
        );
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);
        for slot in [CORNER_TOP, CORNER_BOTTOM] {
            let corner = g.corner_index(lake, slot);
            assert!(
                !field.sink[corner],
                "a lake corner must NOT be a sink — lakes flow through"
            );
        }
        // ...and with no ocean anywhere, nothing on the map is reachable, so nothing routes.
        assert!(field.filled.iter().all(|f| !f.is_finite()));
    }

    #[test]
    fn an_ocean_corner_is_a_sink_and_a_lake_corner_drains_through_it() {
        let (g, elevations) = valley_map(5, 7);
        let lake = UVec2::new(2, 3);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &|x, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else if UVec2::new(x, y) == lake {
                TerrainType::InlandSea
            } else {
                TerrainType::MixedWoodland
            }
        });
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);

        let lake_corner = g.corner_index(lake, CORNER_BOTTOM);
        assert!(!field.sink[lake_corner], "a lake corner is not a sink");
        assert!(
            field.filled[lake_corner].is_finite(),
            "a lake corner is reachable from the ocean"
        );
        assert_ne!(
            field.downstream[lake_corner],
            usize::MAX,
            "a lake corner drains onward — it does not terminate the flow"
        );

        // The ocean row's corners are the sinks.
        let ocean_corner = g.corner_index(UVec2::new(2, 1), CORNER_TOP);
        assert!(
            field.sink[ocean_corner],
            "a corner touching ocean is a sink"
        );
    }

    /// Every reachable non-sink corner has a **strictly lower** filled neighbour — that is the whole
    /// point of the `+epsilon` fill, and it is what guarantees a descent to the sea from anywhere.
    #[test]
    fn the_epsilon_fill_leaves_a_strict_descent_from_every_reachable_corner() {
        let (g, elevations) = valley_map(9, 9);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);

        let mut checked = 0usize;
        for corner in 0..field.filled.len() {
            if !field.is_routable(corner) || field.sink[corner] {
                continue;
            }
            let down = field.downstream[corner];
            assert_ne!(down, usize::MAX, "corner {corner} has no descent");
            assert!(
                field.filled[down] < field.filled[corner],
                "corner {corner} does not descend ({} -> {})",
                field.filled[corner],
                field.filled[down]
            );
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// Accumulation is a **precipitation-weighted drainage area in hex-equivalents**: with no
    /// moisture raster (uniform precip = 1) every hex contributes exactly `base_runoff +
    /// moisture_weight`, so the total runoff seeded on the map is that times the corner count.
    #[test]
    fn accumulation_is_seeded_in_hex_equivalents() {
        let (g, elevations) = valley_map(7, 7);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);
        let flow = test_flow();
        let per_corner = (flow.base_runoff + flow.moisture_weight) / CORNERS_PER_HEX as f32;

        // A headwater corner (nothing drains into it) carries exactly its own seed.
        let headwater = (0..field.filled.len())
            .find(|&c| {
                field.is_routable(c)
                    && !field.sink[c]
                    && g.corner_neighbors(c)
                        .into_iter()
                        .flatten()
                        .all(|step| field.downstream[step.corner] != c)
            })
            .expect("some corner is a headwater");
        assert!((field.accumulation[headwater] - per_corner).abs() < 1e-4);

        // ...and accumulation never shrinks downstream.
        for corner in 0..field.filled.len() {
            if !field.is_routable(corner) || field.sink[corner] {
                continue;
            }
            let down = field.downstream[corner];
            assert!(
                field.accumulation[down] >= field.accumulation[corner] - 1e-4,
                "accumulation shrank downstream"
            );
        }
    }

    /// A tributary joins its trunk **at the corner it was passed over at** — that is the whole point
    /// of walking upstream from the outlet and always taking the largest contributor.
    #[test]
    fn tributaries_terminate_on_the_stem_they_join() {
        let (g, elevations) = valley_map(9, 9);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);
        let network = DrainageNetwork::extract(&field, 1.0);
        let stems = network.decompose(&field);
        assert!(stems.len() > 1, "expected a trunk plus tributaries");

        let mut claimed: HashSet<usize> = HashSet::new();
        for stem in &stems {
            // Every channel corner lands in exactly one stem.
            for &corner in &stem.path {
                assert!(claimed.insert(corner), "corner {corner} claimed twice");
            }
        }
        let channel_corners = (0..field.filled.len())
            .filter(|&c| network.channel[c])
            .count();
        assert_eq!(
            claimed.len(),
            channel_corners,
            "every channel corner is in a stem"
        );

        // The trunk (first stem) ends on a sink; each tributary ends on a corner of an earlier stem.
        let trunk = &stems[0];
        assert!(
            field.sink[trunk.terminus],
            "the main stem reaches the ocean"
        );
        for stem in &stems[1..] {
            assert!(
                network.channel[stem.terminus] || field.sink[stem.terminus],
                "a stem ends on the channel it joins, or on the sea"
            );
        }
    }

    #[test]
    fn river_reaches_the_coast_and_discharge_never_decreases() {
        let (g, elevations) = valley_map(7, 9);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        let rivers = extract(&fixture, edge_only_thresholds(), 1.0);
        assert!(!rivers.is_empty(), "expected rivers");

        let reaches_coast = rivers.iter().any(|river| {
            touched_hexes(&river.edges, &river.navigable_hexes, &g)
                .iter()
                .any(|p| p.y <= 1)
        });
        assert!(reaches_coast, "no river reached the coast");

        for river in &rivers {
            let mut previous: Option<[usize; 2]> = None;
            let mut last_discharge = 0.0f32;
            for edge in &river.edges {
                let corners = g
                    .edge_corners(edge.hex, edge.dir)
                    .expect("every emitted edge lies on the map");
                if let Some(prev) = previous {
                    assert!(
                        corners.iter().any(|c| prev.contains(c)),
                        "a river has a break between consecutive edges"
                    );
                }
                assert!(
                    edge.discharge >= last_discharge,
                    "discharge dropped downstream ({last_discharge} -> {})",
                    edge.discharge
                );
                previous = Some(corners);
                last_discharge = edge.discharge;
            }
        }
    }

    /// A river flowing through a lake emits **no edge inside the lake** — there the river IS the
    /// water — and re-emerges below it.
    #[test]
    fn no_river_edge_is_emitted_inside_a_water_body() {
        let (g, elevations) = valley_map(7, 9);
        let lake = [UVec2::new(2, 4), UVec2::new(3, 4)];
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &|x, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else if lake.contains(&UVec2::new(x, y)) {
                TerrainType::InlandSea
            } else {
                TerrainType::MixedWoodland
            }
        });
        let tiles = fixture.tiles();
        let rivers = extract(&fixture, edge_only_thresholds(), 1.0);
        for river in &rivers {
            for edge in &river.edges {
                let far = g.neighbor(edge.hex, edge.dir).expect("on map");
                assert!(
                    !(tiles.is_water_hex(edge.hex) && tiles.is_water_hex(far)),
                    "an edge was emitted inside the water body ({:?} dir {})",
                    edge.hex,
                    edge.dir
                );
            }
        }
    }

    #[test]
    fn class_thresholds_grow_with_discharge_and_hand_off_to_navigable() {
        let thresholds = RiverClassThresholds {
            major_min: 100.0,
            navigable_min: 1000.0,
            navigable_enabled: true,
        };
        assert_eq!(thresholds.classify(1.0), Some(RiverClass::Minor));
        assert_eq!(thresholds.classify(99.9), Some(RiverClass::Minor));
        assert_eq!(thresholds.classify(100.0), Some(RiverClass::Major));
        assert_eq!(thresholds.classify(999.9), Some(RiverClass::Major));
        assert_eq!(thresholds.classify(1000.0), None); // becomes a NavigableRiver hex chain

        // The kill switch keeps the biggest rivers on the edge model as Major.
        let capped = RiverClassThresholds {
            navigable_enabled: false,
            ..thresholds
        };
        assert_eq!(capped.classify(10_000.0), Some(RiverClass::Major));
    }

    /// The hand-off must anchor on the **last emitted edge**, so the hex chain and the edge chain
    /// share an edge. Three hexes meet at a corner, so anchoring on the *un-emitted* edge the
    /// emitter stopped at could pick the third hex — one the edge chain never touches, leaving the
    /// two chains joined at a bare corner and the first navigable hex with an empty river mask.
    #[test]
    fn the_navigable_handoff_anchors_on_the_last_emitted_edge() {
        let (g, elevations) = valley_map(7, 9);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        // Low thresholds so the main stem outgrows the edge model partway down.
        let thresholds = RiverClassThresholds {
            major_min: 2.0,
            navigable_min: 5.0,
            navigable_enabled: true,
        };
        let rivers = extract(&fixture, thresholds, 1.0);

        let mut checked = 0usize;
        for river in &rivers {
            let (Some(&first), Some(last)) = (river.navigable_hexes.first(), river.edges.last())
            else {
                continue;
            };
            checked += 1;
            let far = g.neighbor(last.hex, last.dir).expect("on map");
            assert!(
                first == last.hex || first == far,
                "navigable chain starts at {first:?}, which flanks neither hex of the last emitted \
                 edge ({:?} dir {})",
                last.hex,
                last.dir
            );

            // The hand-off also names the CORNER the tributary arrives at, with the class of the
            // last edge it emitted.
            let inflow = river
                .navigable_inflow
                .expect("a hand-off from an emitted edge names an inflow corner");
            assert_eq!(inflow.class, last.class);
            let endpoints = g.edge_corners(last.hex, last.dir).expect("on map");
            let local: Vec<u8> = endpoints
                .iter()
                .filter_map(|&corner| g.local_corner_index(first, corner))
                .collect();
            assert!(
                local.contains(&inflow.corner),
                "inflow corner {} is not an endpoint of the last emitted edge (endpoints {local:?})",
                inflow.corner
            );
        }
        assert!(checked > 0, "expected a navigable hand-off in the fixture");
    }

    /// A river navigable from its very first step emitted no edges, so it has no tributary: it must
    /// still produce a chain, but must not fabricate an inflow corner.
    #[test]
    fn a_river_navigable_from_its_first_step_reports_no_inflow() {
        let (g, elevations) = valley_map(7, 9);
        let fixture = Fixture::new(g, elevations, &|_, y| y == 0, &ocean_row_terrain);
        let thresholds = RiverClassThresholds {
            major_min: 0.0,
            navigable_min: 0.0,
            navigable_enabled: true,
        };
        let rivers = extract(&fixture, thresholds, 1.0);
        assert!(!rivers.is_empty());
        for river in &rivers {
            assert!(
                river.edges.is_empty(),
                "nothing should be classified onto an edge at these thresholds"
            );
            assert!(
                river.navigable_inflow.is_none(),
                "a river with no emitted edges has no tributary to join"
            );
            for pos in &river.navigable_hexes {
                assert!(pos.x < g.width && pos.y < g.height);
            }
        }
    }

    #[test]
    fn corner_elevation_is_the_mean_of_its_three_hexes() {
        // The mean (not the min) puts a corner low in the *trough* between two low hexes, which is
        // what makes rivers settle into valleys rather than hug a single low tile.
        let g = grid(5, 5, false);
        let mut elevations = vec![0.5f32; 25];
        elevations[idx(5, 2, 2)] = 0.8;
        let fixture = Fixture::new(g, elevations, &|_, _| false, &|_, _| {
            TerrainType::MixedWoodland
        });
        let tiles = fixture.tiles();
        let field = fixture.field(&tiles);
        let corner = g.corner_index(UVec2::new(2, 2), CORNER_TOP);
        // TOP(2,2) is shared by (2,2)=0.8, NW=(1,1)=0.5, NE=(2,1)=0.5 — plus the flat-tie jitter,
        // which is bounded by half the jitter amplitude.
        let mean = (0.8 + 0.5 + 0.5) / 3.0;
        assert!((field.elevation[corner] - mean).abs() <= test_flow().flat_jitter);
    }

    /// The jitter is a **pure hash** of `(world_seed, corner)`: same inputs, same value, always — and
    /// different seeds move it.
    #[test]
    fn the_flat_jitter_is_deterministic_and_seed_dependent() {
        let a: Vec<f32> = (0..64).map(|i| hash01(7, i)).collect();
        let b: Vec<f32> = (0..64).map(|i| hash01(7, i)).collect();
        assert_eq!(a, b, "the same seed must produce the same jitter");
        let c: Vec<f32> = (0..64).map(|i| hash01(8, i)).collect();
        assert_ne!(a, c, "a different seed must produce different jitter");
        assert!(
            a.iter().all(|v| (0.0..1.0).contains(v)),
            "hash01 is in [0,1)"
        );
    }

    #[test]
    fn border_corners_are_excluded_from_routing() {
        let g = grid(5, 5, false);
        // Top row has no NW/NE neighbours, so its TOP corners are off-map.
        assert!(g.corner_hexes(UVec2::new(2, 0), CORNER_TOP).is_none());
        assert!(g.corner_hexes(UVec2::new(2, 4), CORNER_BOTTOM).is_none());
        // Without wrap, the left/right columns lose corners too.
        assert!(g.corner_hexes(UVec2::new(0, 2), CORNER_TOP).is_none());
        // With wrap, the same corner is routable.
        let wrapped = grid(5, 5, true);
        assert!(wrapped.corner_hexes(UVec2::new(0, 2), CORNER_TOP).is_some());
    }

    /// Spawn a 7x7 world with an ocean row and a valley, run the real `generate_hydrology`, and
    /// return the world so the invariants can be checked against the actual ECS state.
    fn generate_small_world() -> World {
        let width = 7u32;
        let height = 7u32;

        let mut world = World::new();
        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(width, height);
        config.map_preset_id = "debug".to_string();
        config.map_topology.wrap_horizontal = false;
        config.hydrology.min_length = Some(2);
        config.hydrology.channel_min_discharge = Some(1.0);
        config.hydrology.river_density = Some(1.0);
        world.insert_resource(config);
        world.insert_resource(WorldGenSeed(0));

        let (_, elevations) = valley_map(width, height);
        world.insert_resource(ElevationField::new(width, height, elevations));

        let mut tiles = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let terrain = if y == 0 {
                    TerrainType::DeepOcean
                } else if y == 1 {
                    TerrainType::TidalFlat
                } else {
                    TerrainType::MixedWoodland
                };
                let entity = world
                    .spawn(Tile {
                        position: UVec2::new(x, y),
                        element: ElementKind::Ferrite,
                        mass: scalar_zero(),
                        temperature: scalar_zero(),
                        terrain,
                        terrain_tags: terrain_definition(terrain).tags,
                        underlying_terrain: None,
                        mountain: None,
                        river_edges: 0,
                        river_inflow: 0,
                        river_channel: 0,
                    })
                    .id();
                tiles.push(entity);
            }
        }
        world.insert_resource(TileRegistry {
            tiles,
            width,
            height,
        });

        generate_hydrology(&mut world);
        world
    }

    #[test]
    fn generates_river_reaching_the_ocean_on_a_small_grid() {
        let world = generate_small_world();
        let hydro = world.resource::<HydrologyState>();
        assert!(!hydro.rivers.is_empty(), "expected at least one river");

        let g = grid(7, 7, false);
        // An edge river ends at the shore: it terminates on the corner where the sea begins,
        // flanking the coastal row.
        let reaches_ocean = hydro.rivers.iter().any(|river| {
            river
                .touched_hexes(g.width, g.height, g.wrap_horizontal)
                .iter()
                .any(|p| p.y <= 1)
        });
        assert!(reaches_ocean, "no river reached the coast");
    }

    #[test]
    fn the_per_tile_mask_agrees_on_both_sides_of_every_river_edge() {
        let world = generate_small_world();
        let g = grid(7, 7, false);
        let rivers = world.resource::<HydrologyState>().rivers.clone();
        let registry = world.resource::<TileRegistry>().clone();

        let mut any_edge = false;
        for river in &rivers {
            for edge in &river.edges {
                any_edge = true;
                let neighbor = g
                    .neighbor(edge.hex, edge.dir)
                    .expect("an emitted edge has both hexes on the map");

                let near = world
                    .get::<Tile>(registry.tiles[g.tile_index(edge.hex)])
                    .expect("tile exists")
                    .river_class_on_side(edge.dir);
                let far = world
                    .get::<Tile>(registry.tiles[g.tile_index(neighbor)])
                    .expect("tile exists")
                    .river_class_on_side(opposite_dir(edge.dir));

                assert_eq!(near, edge.class, "near hex disagrees with the edge");
                assert_eq!(far, edge.class, "far hex disagrees with the near hex");
            }
        }
        assert!(any_edge, "expected the generated rivers to carry edges");
    }

    #[test]
    fn tile_river_mask_round_trips_every_class_on_every_side() {
        let mut tile = Tile::default();
        assert!(!tile.has_any_river_edge());
        for dir in 0..HEX_DIRECTION_COUNT as u8 {
            for class in [RiverClass::Minor, RiverClass::Major, RiverClass::None] {
                tile.set_river_class_on_side(dir, class);
                assert_eq!(tile.river_class_on_side(dir), class);
            }
        }
        // Six independent slots: setting one never disturbs another.
        tile.set_river_class_on_side(DIR_E, RiverClass::Major);
        tile.set_river_class_on_side(DIR_SW, RiverClass::Minor);
        assert_eq!(tile.river_class_on_side(DIR_E), RiverClass::Major);
        assert_eq!(tile.river_class_on_side(DIR_SW), RiverClass::Minor);
        assert_eq!(tile.river_class_on_side(DIR_SE), RiverClass::None);
        assert!(tile.has_any_river_edge());
    }
}
