use bevy::prelude::*;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
    f32::consts::SQRT_2,
};

use crate::{
    components::Tile,
    grid_utils::{hex_neighbor, HEX_CORNER_COUNT, HEX_DIRECTION_COUNT},
    heightfield::ElevationField,
    map_preset::{
        default_river_class_major_min_discharge, default_river_class_navigable_min_discharge,
        MapPresetsHandle,
    },
    mapgen::WorldGenSeed,
    resources::{SimulationConfig, TileRegistry},
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
/// the hex-denominated `river_min_length` config levers into corner-graph step budgets.
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

/// Baseline distance cost of one hex of downstream progress, so that on perfectly flat ground the
/// flood still prefers the shorter route to the sea. This is the hex flow field's `0.01 * step_len`
/// term.
const FLOW_STEP_BASE_COST: f32 = 0.01;

/// Baseline cost of one **corner** step. A corner step covers one hex *side* — about half a hex of
/// downstream progress — so it costs half as much as a hex step. Keeping the cost per unit of
/// progress identical to the hex field means the corner cost field is a faithful rescaling of it,
/// with the same slope-vs-distance balance (rather than double-weighting distance).
const CORNER_STEP_BASE_COST: f32 = FLOW_STEP_BASE_COST / CORNER_STEPS_PER_HEX as f32;

/// How many times the acceptance loop re-runs with a relaxed min-spacing before giving up on
/// reaching the target river count.
const MAX_SPACING_RELAXATION_PASSES: usize = 3;

/// Each relaxation pass halves the squared min-spacing (i.e. spacing shrinks by `sqrt(2)`).
const SPACING_RELAXATION_FACTOR: f32 = 0.5;

/// Histogram slots for the class telemetry — one per `RiverClass` discriminant (None/Minor/Major).
const RIVER_CLASS_HISTOGRAM_SLOTS: usize = 3;

/// The opposite odd-r direction (`E ↔ W`, `SE ↔ NW`, `SW ↔ NE`).
#[inline]
fn opposite_dir(dir: u8) -> u8 {
    (dir + CANONICAL_DIR_COUNT) % HEX_DIRECTION_COUNT as u8
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
    /// Corner flow accumulation at the **upstream** corner of this step — monotonically
    /// non-decreasing downstream, so `class` never shrinks toward the mouth.
    pub discharge: u32,
}

/// Where an edge-river chain hands its water to the navigable trunk it becomes: the **corner** of
/// the first navigable hex the edge chain terminated at, and the class of the last edge it emitted.
///
/// An edge river runs *along* a side, corner to corner, so it does not end mid-edge — it ends at a
/// vertex. The per-tile `river_edges` mask cannot express that (a trunk hex may flank three river
/// edges, leaving two candidate chain-ends), so the terminus is recorded explicitly and exported as
/// `Tile::river_inflow`.
#[derive(Debug, Clone, Copy)]
pub struct RiverInflow {
    /// Corner index `0..HEX_CORNER_COUNT` **on the first navigable hex**, in the client's
    /// screen-space corner order (see `HEX_CORNER_LAYOUT`).
    pub corner: u8,
    /// Class of the last edge the chain emitted — the tributary's own width where it arrives.
    pub class: RiverClass,
}

#[derive(Debug, Clone)]
pub struct RiverSegment {
    pub id: u32,
    /// Strahler order (unchanged: derived from the hex flow field).
    pub order: u8,
    /// The hex edges the river runs along, upstream → downstream.
    pub edges: Vec<RiverEdge>,
    /// The `NavigableRiver` hex chain the river becomes once its discharge crosses
    /// `river_class_navigable_min_discharge`. Empty unless the river went navigable.
    pub navigable_hexes: Vec<UVec2>,
    /// The corner of `navigable_hexes[0]` where the edge chain arrives. `None` when the river never
    /// went navigable, or when it was navigable from its very first step (no edges emitted, so no
    /// tributary to join).
    pub navigable_inflow: Option<RiverInflow>,
    termination: TerminationClass,
}

impl RiverSegment {
    /// Every hex the river touches, upstream → downstream: both hexes flanking each edge, then the
    /// navigable tail. Not deduplicated — callers that need the *most downstream* qualifying hex
    /// (delta placement) scan it in reverse.
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
#[derive(Debug, Clone, Copy)]
struct RiverClassThresholds {
    major_min: u32,
    navigable_min: u32,
    navigable_enabled: bool,
}

impl RiverClassThresholds {
    /// The class of an edge carrying `discharge`, or `None` when the river has outgrown the edge
    /// model entirely and must become a `NavigableRiver` hex chain.
    fn classify(&self, discharge: u32) -> Option<RiverClass> {
        if discharge < self.major_min {
            Some(RiverClass::Minor)
        } else if discharge < self.navigable_min || !self.navigable_enabled {
            Some(RiverClass::Major)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TerminationClass {
    Ocean,
    Lake,
    Wetland,
    Desert,
    Karst,
    Endorheic,
    None,
}

/// What the hex-center trace returns: the hex path, per-step debug samples, and the termination.
/// It no longer emits edges — Minor/Major rivers are traced on the corner graph; this trace now
/// serves only the **navigable tail**, which is made of whole hexes.
type RiverTraceResult = (
    Vec<UVec2>,
    Vec<(u32, u32, f32, f32)>,
    Option<TerminationClass>,
);

/// Cost tolerance when comparing two flow-field costs: below this they are the same height, so
/// the trace may step "sideways" rather than stalling on float noise.
const FLOW_COST_EPSILON: f32 = 1e-6;

type FlowCandidateMetrics = (u8, f32, f32, f32);
/// Candidate ranking key for a corner step: (downhill-ness, cost, elevation). Unlike the hex
/// trace there is no step-length term — every corner step crosses exactly one hex side.
type CornerCandidateKey = (u8, f32, f32);
type NeighborCandidateState = (usize, i32, i32, f32, f32, TerminationClass);
type CandidateEntry = (FlowCandidateMetrics, NeighborCandidateState);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SourceCategory {
    Glacier,
    LakeOutlet,
    Runoff,
    Fallback,
}

fn termination_class_for(terrain: TerrainType, tags: TerrainTags) -> TerminationClass {
    use TerrainType::*;
    match terrain {
        DeepOcean | ContinentalShelf | CoralShelf | HydrothermalVentField => {
            TerminationClass::Ocean
        }
        InlandSea => TerminationClass::Lake,
        RiverDelta | MangroveSwamp | FreshwaterMarsh | TidalFlat | PeatHeath => {
            TerminationClass::Wetland
        }
        HotDesertErg | RockyReg | SemiAridScrub | SaltFlat | OasisBasin => TerminationClass::Desert,
        KarstHighland | KarstCavernMouth | SinkholeField | AquiferCeiling => {
            TerminationClass::Karst
        }
        Glacier | SeasonalSnowfield => TerminationClass::None,
        _ => {
            if tags.contains(TerrainTags::WETLAND) {
                TerminationClass::Wetland
            } else if tags.contains(TerrainTags::FRESHWATER) {
                TerminationClass::Lake
            } else if tags.contains(TerrainTags::ARID) {
                TerminationClass::Desert
            } else if tags.contains(TerrainTags::SUBSURFACE) {
                TerminationClass::Karst
            } else {
                TerminationClass::None
            }
        }
    }
}

fn path_meets_length(
    category: SourceCategory,
    path_len: usize,
    min_length: usize,
    fallback_min_length: usize,
) -> bool {
    if path_len >= min_length {
        return true;
    }
    matches!(category, SourceCategory::Fallback) && path_len >= fallback_min_length
}

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

fn neighbor_dirs() -> &'static [(i32, i32)] {
    // 8-neighborhood: E, NE, N, NW, W, SW, S, SE
    &[
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
}

struct RiverTraceContext<'a> {
    width: u32,
    height: u32,
    elevation_field: &'a ElevationField,
    cost: &'a [f32],
    termination_classes: &'a [TerminationClass],
}

fn trace_river_path(
    start: UVec2,
    head_elevation: f32,
    max_allowed_elevation: f32,
    ctx: &RiverTraceContext,
) -> RiverTraceResult {
    let mut path: Vec<UVec2> = Vec::new();
    let mut samples: Vec<(u32, u32, f32, f32)> = Vec::new();
    let mut termination = None;
    let mut best_termination: Option<TerminationClass> = None;
    let mut visited = HashSet::new();
    let mut cx = start.x as i32;
    let mut cy = start.y as i32;
    let max_steps = (ctx.width + ctx.height) as usize;
    let mut remaining_steps = max_steps;
    let head_limit = max_allowed_elevation.max(head_elevation);

    let start_idx = (start.y * ctx.width + start.x) as usize;
    path.push(start);
    samples.push((start.x, start.y, head_elevation, ctx.cost[start_idx]));

    while remaining_steps > 0 {
        remaining_steps -= 1;
        let idx = (cy as u32 * ctx.width + cx as u32) as usize;
        let current_cost = ctx.cost[idx];

        if visited.contains(&idx) {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        }
        visited.insert(idx);

        if let Some(class) = ctx.termination_classes.get(idx).copied() {
            match class {
                TerminationClass::Ocean => {
                    termination = Some(TerminationClass::Ocean);
                    break;
                }
                TerminationClass::Lake
                | TerminationClass::Wetland
                | TerminationClass::Desert
                | TerminationClass::Karst => {
                    best_termination.get_or_insert(class);
                }
                _ => {}
            }
        }
        let mut best_candidate: Option<CandidateEntry> = None;
        for (dir_idx, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= ctx.width as i32 || ny >= ctx.height as i32 {
                continue;
            }
            let nidx = (ny as u32 * ctx.width + nx as u32) as usize;
            if visited.contains(&nidx) {
                continue;
            }
            let neighbor_cost = ctx.cost[nidx];
            if !neighbor_cost.is_finite() {
                continue;
            }
            let neighbor_elev = ctx.elevation_field.sample(nx as u32, ny as u32);
            if neighbor_elev > head_limit + f32::EPSILON {
                continue;
            }

            let downhill = neighbor_cost + FLOW_COST_EPSILON < current_cost;
            let equalish = (neighbor_cost - current_cost).abs() <= FLOW_COST_EPSILON;
            if !downhill && !equalish {
                // Allow gentle pooling only if still below the head limit.
                if neighbor_elev + f32::EPSILON < head_limit {
                    // permitted small uphill, keep
                } else {
                    continue;
                }
            }

            let class = ctx
                .termination_classes
                .get(nidx)
                .copied()
                .unwrap_or(TerminationClass::None);
            let step_len = if dx == 0 || dy == 0 { 1.0 } else { SQRT_2 };
            let ranking_key = (
                if downhill {
                    0u8
                } else if equalish {
                    1u8
                } else {
                    2u8
                },
                neighbor_cost,
                neighbor_elev,
                step_len,
            );

            if best_candidate
                .as_ref()
                .map(|(best_key, _)| ranking_key < *best_key)
                .unwrap_or(true)
            {
                best_candidate = Some((
                    ranking_key,
                    (dir_idx, nx, ny, neighbor_cost, neighbor_elev, class),
                ));
            }
        }

        let Some((_, (_dir_idx, nx, ny, neighbor_cost, neighbor_elev, class))) = best_candidate
        else {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        };

        cx = nx;
        cy = ny;
        let next_pos = UVec2::new(cx as u32, cy as u32);
        path.push(next_pos);
        samples.push((next_pos.x, next_pos.y, neighbor_elev, neighbor_cost));

        if matches!(
            class,
            TerminationClass::Lake
                | TerminationClass::Wetland
                | TerminationClass::Desert
                | TerminationClass::Karst
        ) && best_termination.is_none()
        {
            best_termination = Some(class);
        }
    }

    if termination.is_none() {
        termination = best_termination;
    }

    (path, samples, termination)
}

/// The corner-graph flow field: elevation, sinks, costs and accumulation, all indexed by corner.
struct CornerField {
    grid: HexGrid,
    /// `false` for border corners (not all 3 hexes on the map) — excluded from routing.
    valid: Vec<bool>,
    /// Mean of the corner's 3 hexes' elevation samples. **Mean, not min**: it puts a corner low
    /// exactly in the trough between two low hexes, so rivers settle into valleys instead of
    /// hugging a single low tile.
    elevation: Vec<f32>,
    /// Corner cost to the sea (priority-flood from every sea corner). `INFINITY` = unreachable.
    cost: Vec<f32>,
    /// Flow accumulation: every corner seeds 1 and sums downstream.
    accumulation: Vec<u32>,
    termination: Vec<TerminationClass>,
}

impl CornerField {
    /// A corner is a **sea corner** (a sink) if any of its 3 hexes is water.
    fn build(
        grid: HexGrid,
        elevation_field: &ElevationField,
        seamask: &[bool],
        tile_terrain: &[Option<(TerrainType, TerrainTags)>],
        termination_classes: &[TerminationClass],
    ) -> Self {
        let count = grid.corner_count();
        let mut valid = vec![false; count];
        let mut elevation = vec![f32::INFINITY; count];
        let mut termination = vec![TerminationClass::None; count];
        let mut cost = vec![f32::INFINITY; count];
        let mut heap = BinaryHeap::new();

        for y in 0..grid.height {
            for x in 0..grid.width {
                let pos = UVec2::new(x, y);
                for slot in [CORNER_TOP, CORNER_BOTTOM] {
                    let Some(hexes) = grid.corner_hexes(pos, slot) else {
                        continue;
                    };
                    let idx = grid.corner_index(pos, slot);
                    valid[idx] = true;
                    elevation[idx] = hexes
                        .iter()
                        .map(|h| elevation_field.sample(h.x, h.y))
                        .sum::<f32>()
                        / HEXES_PER_CORNER as f32;

                    let mut is_sea = false;
                    let mut class = TerminationClass::None;
                    for hex in hexes {
                        let hex_idx = grid.tile_index(hex);
                        if seamask[hex_idx]
                            || tile_terrain[hex_idx]
                                .map(|(terrain, _)| is_water_terrain(terrain))
                                .unwrap_or(false)
                        {
                            is_sea = true;
                        }
                        class = stronger_termination(class, termination_classes[hex_idx]);
                    }
                    termination[idx] = class;
                    if is_sea {
                        cost[idx] = 0.0;
                        heap.push(HeapEntry { cost: 0.0, idx });
                    }
                }
            }
        }

        let mut field = Self {
            grid,
            valid,
            elevation,
            cost,
            accumulation: vec![0; count],
            termination,
        };
        field.priority_flood(heap);
        field.accumulate();
        field
    }

    /// Dijkstra outward from every sea corner over the 3-neighbour corner graph — the corner-graph
    /// twin of the hex flow field's priority flood.
    fn priority_flood(&mut self, mut heap: BinaryHeap<HeapEntry>) {
        while let Some(HeapEntry {
            cost: current_cost,
            idx,
        }) = heap.pop()
        {
            if current_cost > self.cost[idx] {
                continue;
            }
            let elev_here = self.elevation[idx];
            for step in self.grid.corner_neighbors(idx).into_iter().flatten() {
                if !self.valid[step.corner] {
                    continue;
                }
                let slope_penalty = (self.elevation[step.corner] - elev_here).max(0.0);
                let new_cost = current_cost + slope_penalty + CORNER_STEP_BASE_COST;
                if new_cost + f32::EPSILON < self.cost[step.corner] {
                    self.cost[step.corner] = new_cost;
                    heap.push(HeapEntry {
                        cost: new_cost,
                        idx: step.corner,
                    });
                }
            }
        }
    }

    /// Seed every corner at 1 and sum downstream in descending-cost topological order (mirroring
    /// the hex accumulation). "Downstream" is the strictly-cheaper neighbour, so sea corners — at
    /// cost 0 — are the terminal sinks.
    fn accumulate(&mut self) {
        let count = self.cost.len();
        let mut downstream = vec![usize::MAX; count];
        for (idx, down) in downstream.iter_mut().enumerate() {
            if !self.valid[idx] || !self.cost[idx].is_finite() {
                continue;
            }
            let mut best = self.cost[idx];
            for step in self.grid.corner_neighbors(idx).into_iter().flatten() {
                if self.valid[step.corner] && self.cost[step.corner] < best {
                    best = self.cost[step.corner];
                    *down = step.corner;
                }
            }
            self.accumulation[idx] = 1;
        }

        let mut order: Vec<usize> = (0..count)
            .filter(|&idx| self.valid[idx] && self.cost[idx].is_finite())
            .collect();
        order.sort_by(|a, b| {
            self.cost[*b]
                .partial_cmp(&self.cost[*a])
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        for idx in order {
            let down = downstream[idx];
            if down != usize::MAX {
                self.accumulation[down] =
                    self.accumulation[down].saturating_add(self.accumulation[idx]);
            }
        }
    }

    /// The lower-cost of a hex's two corners — the one the hex drains toward. `None` if neither is
    /// a routable (non-border, sea-reachable) corner.
    fn drain_corner(&self, pos: UVec2) -> Option<usize> {
        [CORNER_TOP, CORNER_BOTTOM]
            .into_iter()
            .map(|slot| self.grid.corner_index(pos, slot))
            .filter(|&idx| self.valid[idx] && self.cost[idx].is_finite())
            .min_by(|a, b| {
                self.cost[*a]
                    .partial_cmp(&self.cost[*b])
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.cmp(b))
            })
    }
}

/// A corner inherits the "strongest" termination class among its 3 hexes: the ocean outranks every
/// inland sink, and an inland sink outranks nothing at all.
fn stronger_termination(a: TerminationClass, b: TerminationClass) -> TerminationClass {
    let rank = |class: TerminationClass| match class {
        TerminationClass::Ocean => 0u8,
        TerminationClass::Lake => 1,
        TerminationClass::Wetland => 2,
        TerminationClass::Desert => 3,
        TerminationClass::Karst => 4,
        TerminationClass::Endorheic | TerminationClass::None => 5,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// What a corner trace produced: the classified edge chain, where (if anywhere) the river outgrew
/// the edge model, and how it ended.
struct CornerTrace {
    edges: Vec<RiverEdge>,
    /// The first hex of the navigable tail — the lower of the two hexes flanking the **last emitted
    /// edge**, so the hex chain and the edge chain always share an edge (see `trace_river_edges`).
    navigable_from: Option<UVec2>,
    /// The corner of `navigable_from` the edge chain terminated at, with the class of the last edge
    /// it emitted. `None` when the river never went navigable, or emitted no edges at all.
    navigable_inflow: Option<RiverInflow>,
    termination: Option<TerminationClass>,
}

/// Walk downhill on the corner cost field from `start`, recording the hex edge crossed at each
/// step and classifying it by the discharge at its **upstream** corner. Terminates on the same
/// `TerminationClass` policy as the hex trace. If the discharge crosses `navigable_min` the river
/// has outgrown the edge model: stop emitting edges and hand off to a hex-center trace.
fn trace_river_edges(
    start: usize,
    head_elevation: f32,
    max_allowed_elevation: f32,
    field: &CornerField,
    thresholds: &RiverClassThresholds,
    elevation_field: &ElevationField,
) -> CornerTrace {
    let mut edges: Vec<RiverEdge> = Vec::new();
    let mut termination = None;
    let mut best_termination: Option<TerminationClass> = None;
    let mut navigable_from = None;
    let mut navigable_inflow = None;
    let mut visited: HashSet<usize> = HashSet::new();
    let mut current = start;
    let head_limit = max_allowed_elevation.max(head_elevation);
    let mut remaining_steps =
        (field.grid.width + field.grid.height) as usize * CORNER_STEPS_PER_HEX;

    while remaining_steps > 0 {
        remaining_steps -= 1;
        if visited.contains(&current) {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        }
        visited.insert(current);

        match field.termination[current] {
            TerminationClass::Ocean => {
                termination = Some(TerminationClass::Ocean);
                break;
            }
            class @ (TerminationClass::Lake
            | TerminationClass::Wetland
            | TerminationClass::Desert
            | TerminationClass::Karst) => {
                best_termination.get_or_insert(class);
            }
            _ => {}
        }

        let current_cost = field.cost[current];
        let mut best: Option<(CornerCandidateKey, CornerStep)> = None;
        for step in field.grid.corner_neighbors(current).into_iter().flatten() {
            if !field.valid[step.corner] || visited.contains(&step.corner) {
                continue;
            }
            let neighbor_cost = field.cost[step.corner];
            if !neighbor_cost.is_finite() {
                continue;
            }
            let neighbor_elev = field.elevation[step.corner];
            if neighbor_elev > head_limit + f32::EPSILON {
                continue;
            }

            let downhill = neighbor_cost + FLOW_COST_EPSILON < current_cost;
            let equalish = (neighbor_cost - current_cost).abs() <= FLOW_COST_EPSILON;
            if !downhill && !equalish && neighbor_elev + f32::EPSILON >= head_limit {
                // Gentle pooling is allowed only while still below the head limit.
                continue;
            }

            let rank = if downhill {
                0u8
            } else if equalish {
                1
            } else {
                2
            };
            let key: CornerCandidateKey = (rank, neighbor_cost, neighbor_elev);
            if best.as_ref().map(|(k, _)| key < *k).unwrap_or(true) {
                best = Some((key, step));
            }
        }

        let Some((_, step)) = best else {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        };

        // Discharge of the edge about to be crossed = accumulation at its upstream corner. This is
        // monotonically non-decreasing downstream, so an edge's class never shrinks toward the sea.
        let discharge = field.accumulation[current];
        match thresholds.classify(discharge) {
            Some(class) => edges.push(RiverEdge {
                hex: step.hex,
                dir: step.dir,
                class,
                discharge,
            }),
            None => {
                // The river has outgrown the edge model: it becomes a body of water. The hex chain
                // must join the edge chain across a shared EDGE, so it is anchored on the last edge
                // actually **emitted** — not on this one, which is skipped.
                //
                // Both edges are incident to `current`, and *three* hexes meet at a corner: the two
                // hexes flanking the un-emitted edge can include the third hex, the one the emitted
                // chain never touches. Anchoring there let the two chains meet at a bare corner, so
                // the first navigable hex carried no `river_edges` bits at all and the tributary
                // visibly dead-ended at the trunk. Anchoring on the last emitted edge makes the
                // shared edge true by construction.
                //
                // Of that edge's two hexes take the lower — water settles into the valley, not onto
                // its shoulder. A river that crosses the threshold on its very first step emitted
                // nothing to anchor to, so it falls back to the edge it stopped at.
                let last_emitted = edges.last().copied();
                let (hex, dir) = last_emitted
                    .map(|last| (last.hex, last.dir))
                    .unwrap_or((step.hex, step.dir));
                navigable_from = lower_flanking_hex(hex, dir, &field.grid, elevation_field);

                // `current` is the corner the last emitted edge *landed on* — the point where the
                // water leaves the edge model and enters the navigable hex. It is an endpoint of
                // that edge, hence a corner of both its flanking hexes, so it always resolves to a
                // local corner of `navigable_from`. That vertex — not a side midpoint — is where
                // the renderer must join the tributary to the trunk. A river with no emitted edges
                // has no tributary: it reports no inflow rather than inventing one.
                navigable_inflow = last_emitted.zip(navigable_from).and_then(|(last, first)| {
                    field
                        .grid
                        .local_corner_index(first, current)
                        .map(|corner| RiverInflow {
                            corner,
                            class: last.class,
                        })
                });
                break;
            }
        }

        current = step.corner;
    }

    if termination.is_none() {
        termination = best_termination;
    }

    CornerTrace {
        edges,
        navigable_from,
        navigable_inflow,
        termination,
    }
}

/// Make a hex path **hex-contiguous**.
///
/// `trace_river_path` is the legacy square-grid trace: it steps on the 8-neighbourhood, so two
/// consecutive tiles can be a *square* diagonal that is not an odd-r hex neighbour (2 hex steps
/// apart). A navigable river is a body of water you sail along, so its hexes must actually touch.
/// Bridge each such gap with the lowest common hex-neighbour — water settles into the valley.
///
/// A gap with no common neighbour cannot be bridged, so the chain is **truncated** there: a short
/// contiguous waterway is correct, a broken one is not.
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

/// The lower-elevation of the two hexes an edge separates.
fn lower_flanking_hex(
    hex: UVec2,
    dir: u8,
    grid: &HexGrid,
    elevation_field: &ElevationField,
) -> Option<UVec2> {
    let other = grid.neighbor(hex, dir)?;
    let here = elevation_field.sample(hex.x, hex.y);
    let there = elevation_field.sample(other.x, other.y);
    Some(if there < here { other } else { hex })
}

/// The hexes of a river in **delta-search order**: like `touched_hexes`, but the two hexes flanking
/// each edge are emitted *higher bank first*, so a reverse scan for the mouth naturally lands on the
/// **low** bank.
///
/// This matters because an edge river runs *between* two hexes: both are equally far downstream, so
/// "the last land hex bordering the terminal water" is ambiguous without a tie-break. A delta forms
/// on the low ground where the river drops its load, never on the bluff opposite it — and a delta on
/// the bluff would also be steep coast, which the coastal-shelf pass correctly refuses to put a
/// shelf in front of.
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
/// Two rivers can hand off to the same trunk hex at the same vertex — three hexes meet at a corner,
/// so two tributaries running down either bank converge there (a confluence *at a corner*, seen on
/// real maps). One slot holds one class, and the class the eye sees arriving is the wider one, so
/// `Major` beats `Minor`. Taking the max also makes the result independent of the order rivers were
/// traced in, which last-write-wins would not be.
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

/// Everything a single river trace needs: the corner graph it routes on, the hex flow field its
/// navigable tail falls back to, and the water mask that tells it where the map's water already is.
struct SegmentTraceContext<'a> {
    grid: HexGrid,
    corner_field: &'a CornerField,
    thresholds: RiverClassThresholds,
    elevation_field: &'a ElevationField,
    hex_ctx: &'a RiverTraceContext<'a>,
    seamask: &'a [bool],
    tile_terrain: &'a [Option<(TerrainType, TerrainTags)>],
    uphill_gain_pct: f32,
}

/// One traced river before it is accepted: its edge chain, its navigable tail, and how it ended.
struct TracedRiver {
    edges: Vec<RiverEdge>,
    navigable_hexes: Vec<UVec2>,
    /// The corner of `navigable_hexes[0]` the edge chain arrives at (see `RiverInflow`).
    navigable_inflow: Option<RiverInflow>,
    termination: Option<TerminationClass>,
}

impl TracedRiver {
    /// Length in **hexes**, so the `river_min_length` config levers stay denominated in hexes
    /// across both models: a corner step covers one hex side (~half a hex of downstream progress),
    /// while a navigable hex is a whole hex.
    fn hex_length(&self) -> usize {
        self.edges.len() / CORNER_STEPS_PER_HEX + self.navigable_hexes.len()
    }
}

impl SegmentTraceContext<'_> {
    fn is_water(&self, pos: UVec2) -> bool {
        let idx = self.grid.tile_index(pos);
        self.seamask[idx]
            || self.tile_terrain[idx]
                .map(|(terrain, _)| is_water_terrain(terrain))
                .unwrap_or(false)
    }

    /// Trace one river from a head hex: an edge chain on the corner graph, plus — if its discharge
    /// outgrew the edge model — a `NavigableRiver` hex chain traced on the hex centers.
    ///
    /// `existing_navigable` holds the tile indices of every **already-accepted** segment's navigable
    /// chain, so this trace can **merge on contact** (see `truncate_at_existing_channel`).
    fn trace(&self, head: UVec2, existing_navigable: &HashSet<usize>) -> Option<TracedRiver> {
        let start = self.corner_field.drain_corner(head)?;
        let head_elev = self.elevation_field.sample(head.x, head.y);
        let corner = trace_river_edges(
            start,
            head_elev,
            head_elev * (1.0 + self.uphill_gain_pct),
            self.corner_field,
            &self.thresholds,
            self.elevation_field,
        );

        let mut navigable_hexes = Vec::new();
        let mut termination = corner.termination;
        if let Some(first) = corner.navigable_from {
            let nav_head = self.elevation_field.sample(first.x, first.y);
            let (path, _, nav_termination) = trace_river_path(
                first,
                nav_head,
                nav_head * (1.0 + self.uphill_gain_pct),
                self.hex_ctx,
            );
            // The navigable river runs until it meets a body of water that already exists — that
            // meeting point is its mouth — so it is the leading run of non-water hexes. The legacy
            // trace is square-8-connected, so bridge it into a hex-contiguous chain first: a
            // waterway whose hexes don't touch is not a waterway.
            let chain = hex_contiguous_chain(&path, &self.grid, self.elevation_field);
            let chain: Vec<UVec2> = chain
                .into_iter()
                .take_while(|pos| !self.is_water(*pos))
                .collect();
            // A river that reaches water already flowing to the sea has *joined* it — it does not
            // dig a second channel alongside. Without this, every river that independently crossed
            // the navigable threshold in the same coastal lowland traced its own parallel chain to
            // the coast and the chains packed together into a 2D blob of water hexes.
            navigable_hexes = truncate_at_existing_channel(chain, &self.grid, existing_navigable);
            termination = nav_termination.or(termination);
        }

        // The inflow is anchored on `navigable_from`, so it is only meaningful while that hex is
        // still the head of the chain — if the chain came back empty (its first hex was already
        // water), there is no tile to carry it.
        let navigable_inflow = corner
            .navigable_inflow
            .filter(|_| navigable_hexes.first() == corner.navigable_from.as_ref());

        Some(TracedRiver {
            edges: corner.edges,
            navigable_hexes,
            navigable_inflow,
            termination,
        })
    }
}

/// **Merge on contact.** Cut a freshly traced navigable chain at the first hex that is *already*
/// navigable water (stamped by an earlier-accepted segment), keeping that hex as the chain's last
/// element: it is the confluence, the hex where this river hands its water to the trunk.
///
/// Why: the flow accumulation barely concentrates (see the "Known limitation" note on the class
/// thresholds), so a drainage's branches do not merge into one trunk upstream — several of them
/// independently cross `river_class_navigable_min_discharge` in the same flat coastal basin. Each
/// then traced its own hex chain to the *same* sink, the chains ran side by side, and the result was
/// a 2–4 hex wide **blob** of `NavigableRiver` water rather than a river (the largest measured was
/// 21 hexes across 6 chains). Rivers in the world merge; so do their channels here. The contact hex
/// belongs to both chains, so the channel-exit masks of both OR together there and the confluence is
/// connected — see `river_channel`.
fn truncate_at_existing_channel(
    chain: Vec<UVec2>,
    grid: &HexGrid,
    existing_navigable: &HashSet<usize>,
) -> Vec<UVec2> {
    // Contact is ADJACENCY, not identity: two water hexes that touch are one body of water. A chain
    // that merely runs *alongside* an existing channel has already joined it — testing identity
    // alone let parallel chains slide past each other one hex apart and re-form the blob (measured
    // on the reported map: 13 blobby hexes → 7 with identity, → 0 with adjacency).
    for (index, pos) in chain.iter().enumerate() {
        if existing_navigable.contains(&grid.tile_index(*pos)) {
            // Stepped onto the trunk itself: this chain ends there, on the shared confluence hex.
            return chain.into_iter().take(index + 1).collect();
        }

        // Merely *beside* the trunk. The chain still ends here, but it must end **on** the trunk hex
        // it joined, not next to it: a chain that stopped alongside would be a dead end reaching
        // neither the trunk nor the sea, and the renderer would draw a river that runs into a bank.
        // Ending on the trunk makes the confluence a genuine shared chain hex, so both chains' exit
        // bits meet there and the water flows on down the trunk.
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

/// Reject a source whose head sits within the current min-spacing of an already-accepted head.
fn too_close_to_existing_head(head: UVec2, heads: &[UVec2], spacing_sq: f32) -> bool {
    if spacing_sq <= 0.0 {
        return false;
    }
    heads.iter().any(|p| {
        let dx = p.x as i32 - head.x as i32;
        let dy = p.y as i32 - head.y as i32;
        (dx * dx + dy * dy) as f32 <= spacing_sq
    })
}

#[allow(clippy::too_many_arguments)] // Accepting a river threads the whole acceptance bookkeeping.
fn accept_river(
    head_idx: usize,
    head: UVec2,
    traced: TracedRiver,
    grid: &HexGrid,
    rivers: &mut Vec<RiverSegment>,
    river_tiles: &mut HashSet<usize>,
    navigable_tiles: &mut HashSet<usize>,
    accepted_heads: &mut HashSet<usize>,
    head_positions: &mut Vec<UVec2>,
) {
    accepted_heads.insert(head_idx);
    head_positions.push(head);
    let segment = RiverSegment {
        id: rivers.len() as u32 + 1,
        order: 1,
        edges: traced.edges,
        navigable_hexes: traced.navigable_hexes,
        navigable_inflow: traced.navigable_inflow,
        termination: traced.termination.unwrap_or(TerminationClass::None),
    };
    for pos in touched_hexes(&segment.edges, &segment.navigable_hexes, grid) {
        river_tiles.insert(grid.tile_index(pos));
    }
    // The channel this segment just laid down is what a later segment merges *into* (see
    // `truncate_at_existing_channel`), so it must be visible to the traces that follow.
    for pos in &segment.navigable_hexes {
        navigable_tiles.insert(grid.tile_index(*pos));
    }
    rivers.push(segment);
}

pub fn generate_hydrology(world: &mut World) {
    let cfg = world.resource::<SimulationConfig>().clone();
    let (width, height, preset_opt, elevation_field) = {
        let width = cfg.grid_size.x;
        let height = cfg.grid_size.y;
        let preset = if let Some(handle) = world.get_resource::<MapPresetsHandle>() {
            handle.get().get(&cfg.map_preset_id).cloned()
        } else {
            None
        };
        let seed = world
            .get_resource::<WorldGenSeed>()
            .map(|s| s.0)
            .unwrap_or(0);
        let elevation = world
            .get_resource::<ElevationField>()
            .cloned()
            .unwrap_or_else(|| {
                crate::heightfield::build_elevation_field(&cfg, preset.as_ref(), seed)
            });
        (width, height, preset, elevation)
    };

    let sea_level = preset_opt.as_ref().map(|p| p.sea_level).unwrap_or(0.6);
    let overrides = cfg.hydrology.clone();
    let base_river_density = preset_opt.as_ref().map(|p| p.river_density).unwrap_or(0.6);
    let river_density = overrides
        .river_density
        .unwrap_or(base_river_density)
        .clamp(0.1, 5.0);
    let base_accum_factor = preset_opt
        .as_ref()
        .map(|p| p.river_accum_threshold_factor)
        .unwrap_or(0.35);
    let accum_factor = overrides
        .accumulation_threshold_factor
        .unwrap_or(base_accum_factor)
        .clamp(0.05, 2.0);
    let base_min_accum = preset_opt
        .as_ref()
        .map(|p| p.river_min_accum)
        .unwrap_or(6)
        .max(1);
    let min_accum = base_min_accum;
    let base_min_length = preset_opt
        .as_ref()
        .map(|p| p.river_min_length)
        .unwrap_or(8)
        .max(2);
    let min_length = overrides.min_length.unwrap_or(base_min_length).max(2);
    let base_fallback_min_length = preset_opt
        .as_ref()
        .map(|p| p.river_fallback_min_length)
        .unwrap_or(4)
        .max(2);
    let fallback_min_length = overrides
        .fallback_min_length
        .unwrap_or(base_fallback_min_length)
        .max(2);
    // Per-edge class thresholds (overrides > preset > preset default). A river's class grows with
    // its corner discharge, so a headwater is Minor and the same river is Major downstream.
    let thresholds = RiverClassThresholds {
        major_min: overrides
            .class_major_min_discharge
            .or_else(|| {
                preset_opt
                    .as_ref()
                    .map(|p| p.river_class_major_min_discharge)
            })
            .unwrap_or(default_river_class_major_min_discharge())
            .max(1),
        navigable_min: overrides
            .class_navigable_min_discharge
            .or_else(|| {
                preset_opt
                    .as_ref()
                    .map(|p| p.river_class_navigable_min_discharge)
            })
            .unwrap_or(default_river_class_navigable_min_discharge())
            .max(1),
        navigable_enabled: overrides
            .navigable_enabled
            .or_else(|| preset_opt.as_ref().map(|p| p.river_navigable_enabled))
            .unwrap_or(true),
    };
    let grid = HexGrid {
        width,
        height,
        wrap_horizontal: cfg.map_topology.wrap_horizontal,
    };
    // A delta is a depositional fan: it forms only where the river meets the water across LOW,
    // gentle ground. Reuse the shelf's own gentle-vs-steep coast gate (`ShelfConfig`) rather than
    // inventing a second threshold — so "gentle coast" means one thing across worldgen. A river
    // that meets the sea at a cliff simply has no delta (it is an estuary), which is also what
    // keeps `reconcile_coastal_shelf`'s "no DeepOcean touches gentle land" invariant coherent:
    // every delta is gentle land, so every delta gets a shelf seaward of it.
    let coast_height_threshold = preset_opt
        .as_ref()
        .map(|p| p.shelf.coast_height_threshold)
        .unwrap_or(0.10);

    let total_tiles_usize = (width * height) as usize;
    let mut flow_dir = vec![255u8; total_tiles_usize];
    let mut flow_accum = vec![0u16; total_tiles_usize];

    let mut termination_classes = vec![TerminationClass::None; total_tiles_usize];
    let mut tile_terrain: Vec<Option<(TerrainType, TerrainTags)>> = vec![None; total_tiles_usize];
    if let Some(registry) = world.get_resource::<TileRegistry>() {
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            if idx >= termination_classes.len() {
                break;
            }
            if let Some(tile) = world.get::<Tile>(entity) {
                termination_classes[idx] = termination_class_for(tile.terrain, tile.terrain_tags);
                tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
            }
        }
    }

    let mut min_elev = 1.0f32;
    let mut max_elev = 0.0f32;
    let mut sum_elev = 0.0f32;
    let mut elev_samples: Vec<f32> = Vec::with_capacity(total_tiles_usize);
    let mut land_tiles = 0u32;
    let mut water_tiles = 0u32;

    let mut seamask = vec![false; total_tiles_usize];
    let mut cost = vec![f32::INFINITY; total_tiles_usize];
    let mut heap = BinaryHeap::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let elev = elevation_field.sample(x, y);
            min_elev = min_elev.min(elev);
            max_elev = max_elev.max(elev);
            sum_elev += elev;
            elev_samples.push(elev);
            let mut treat_as_water = elev <= sea_level;
            if let Some((terrain, _)) = tile_terrain[idx] {
                if !is_water_terrain(terrain) {
                    treat_as_water = false;
                }
            }

            if treat_as_water {
                seamask[idx] = true;
                water_tiles += 1;
                termination_classes[idx] = TerminationClass::Ocean;
                cost[idx] = 0.0;
                heap.push(HeapEntry { cost: 0.0, idx });
            } else {
                land_tiles += 1;
                if termination_classes[idx] == TerminationClass::Ocean {
                    termination_classes[idx] = tile_terrain[idx]
                        .map(|(terrain, tags)| termination_class_for(terrain, tags))
                        .unwrap_or(TerminationClass::None);
                }
            }
        }
    }

    while let Some(HeapEntry {
        cost: current_cost,
        idx,
    }) = heap.pop()
    {
        if current_cost > cost[idx] {
            continue;
        }
        let cx = (idx as u32) % width;
        let cy = (idx as u32) / width;
        let elev_here = elevation_field.sample(cx, cy);
        for &(dx, dy) in neighbor_dirs() {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            let elev_next = elevation_field.sample(nx as u32, ny as u32);
            let slope_penalty = (elev_next - elev_here).max(0.0);
            let step_len = if dx == 0 || dy == 0 { 1.0 } else { SQRT_2 };
            let step_cost = slope_penalty + FLOW_STEP_BASE_COST * step_len;
            let new_cost = current_cost + step_cost;
            if new_cost + f32::EPSILON < cost[nidx] {
                cost[nidx] = new_cost;
                heap.push(HeapEntry {
                    cost: new_cost,
                    idx: nidx,
                });
            }
        }
    }

    let land_unreachable = cost
        .iter()
        .enumerate()
        .filter(|(idx, c)| !seamask[*idx] && !c.is_finite())
        .count();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if seamask[idx] {
                flow_dir[idx] = 255;
                continue;
            }
            let mut best_dir: u8 = 255;
            let mut best_cost = cost[idx];
            for (d, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                if cost[nidx] < best_cost {
                    best_cost = cost[nidx];
                    best_dir = d as u8;
                }
            }
            if best_dir != 255 {
                flow_dir[idx] = best_dir;
                continue;
            }

            // Fallback to local downhill heuristic if cost map failed to provide direction.
            let elev = elevation_field.sample(x, y);
            let mut downhill_land_dir: u8 = 255;
            let mut downhill_land_elev = elev;
            let mut downhill_any_dir: u8 = 255;
            let mut downhill_any_elev = elev;
            let mut fallback_dir: u8 = 255;
            let mut fallback_elev = elev;

            for (d, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nelev = elevation_field.sample(nx as u32, ny as u32);

                if nelev < fallback_elev || fallback_dir == 255 {
                    fallback_elev = nelev;
                    fallback_dir = d as u8;
                }

                if nelev < downhill_any_elev || downhill_any_dir == 255 {
                    downhill_any_elev = nelev;
                    downhill_any_dir = d as u8;
                }

                if nelev > sea_level && (nelev < downhill_land_elev || downhill_land_dir == 255) {
                    downhill_land_elev = nelev;
                    downhill_land_dir = d as u8;
                }
            }

            let chosen_dir = if downhill_land_dir != 255 && downhill_land_elev < elev {
                downhill_land_dir
            } else if downhill_any_dir != 255 && downhill_any_elev < elev {
                downhill_any_dir
            } else if downhill_land_dir != 255 {
                downhill_land_dir
            } else if downhill_any_dir != 255 {
                downhill_any_dir
            } else {
                fallback_dir
            };

            flow_dir[idx] = chosen_dir;
        }
    }

    // Compute downstream mapping and upstream adjacency.
    let mut downstream: Vec<usize> = vec![usize::MAX; total_tiles_usize];
    let mut upstream: Vec<Vec<usize>> = vec![Vec::new(); total_tiles_usize];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let dir = flow_dir[idx];
            if dir == 255 {
                continue;
            }
            let (dx, dy) = neighbor_dirs()[dir as usize];
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            downstream[idx] = nidx;
            upstream[nidx].push(idx);
        }
    }

    let invalid_downstream = downstream
        .iter()
        .enumerate()
        .filter(|(idx, &d)| d == usize::MAX && !seamask[*idx])
        .count();

    let mut order: Vec<usize> = (0..total_tiles_usize).collect();
    order.sort_by(|a, b| cost[*b].partial_cmp(&cost[*a]).unwrap_or(Ordering::Equal));

    for accum in flow_accum.iter_mut().take(total_tiles_usize) {
        *accum = 1;
    }

    let mut orphan_tiles = 0usize;
    for idx in order {
        let downstream_idx = downstream[idx];
        if downstream_idx != usize::MAX {
            flow_accum[downstream_idx] = flow_accum[downstream_idx].saturating_add(flow_accum[idx]);
        } else if !seamask[idx] {
            orphan_tiles += 1;
        }
    }

    // Trace a handful of rivers from high-accum/high-elevation sources.
    let mut rivers: Vec<RiverSegment> = Vec::new();
    let mut river_tiles: HashSet<usize> = HashSet::new();
    let trace_ctx = RiverTraceContext {
        width,
        height,
        elevation_field: &elevation_field,
        cost: &cost,
        termination_classes: &termination_classes,
    };
    // Minor/Major rivers route on the corner graph (hex edges); only the navigable tail falls back
    // to the hex-center trace above.
    let corner_field = CornerField::build(
        grid,
        &elevation_field,
        &seamask,
        &tile_terrain,
        &termination_classes,
    );
    let river_land_ratio = preset_opt
        .as_ref()
        .map(|p| p.river_land_ratio)
        .unwrap_or(300.0)
        .clamp(1.0, 10_000.0);
    let base_river_min_count = preset_opt
        .as_ref()
        .map(|p| p.river_min_count)
        .unwrap_or(2)
        .max(1);
    let base_river_max_count = preset_opt
        .as_ref()
        .map(|p| p.river_max_count)
        .unwrap_or(128)
        .max(base_river_min_count);
    let river_min_count = overrides.river_min_count.unwrap_or(base_river_min_count);
    let river_max_count = overrides
        .river_max_count
        .unwrap_or(base_river_max_count)
        .max(river_min_count);
    let land_tile_count = land_tiles.max(1) as f32;
    let base_target = (land_tile_count / river_land_ratio).max(river_min_count as f32);
    let mut target_rivers = ((base_target * river_density).round() as usize)
        .max(river_min_count)
        .min(river_max_count);
    if target_rivers == 0 {
        target_rivers = river_min_count;
    }
    elev_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let base_source_percentile = preset_opt
        .as_ref()
        .map(|p| p.river_source_percentile)
        .unwrap_or(0.7);
    let source_percentile = overrides
        .source_percentile
        .unwrap_or(base_source_percentile)
        .clamp(0.0, 1.0);
    let base_sea_buffer = preset_opt
        .as_ref()
        .map(|p| p.river_source_sea_buffer)
        .unwrap_or(0.08)
        .max(0.0);
    let sea_buffer = overrides
        .source_sea_buffer
        .unwrap_or(base_sea_buffer)
        .max(0.0);
    let mut accum_sorted = flow_accum.clone();
    accum_sorted.sort_unstable();
    let accum_percentile = preset_opt
        .as_ref()
        .map(|p| p.river_accum_percentile)
        .unwrap_or(0.0)
        .clamp(0.0, 0.999);
    let percentile_threshold = if accum_percentile > 0.0 {
        quantile_u16(&accum_sorted, accum_percentile).round() as u16
    } else {
        0
    };
    let overall_max_accum = flow_accum.iter().copied().max().unwrap_or(0);
    let mut accumulation_threshold = if percentile_threshold > 0 {
        percentile_threshold
    } else {
        ((overall_max_accum as f32) * accum_factor).round() as u16
    };
    accumulation_threshold = accumulation_threshold
        .max(min_accum)
        .min(overall_max_accum.max(1))
        .max(1);

    let percentile_elev = quantile(&elev_samples, source_percentile);
    let headwater_threshold = percentile_elev.max(sea_level + sea_buffer);
    let fallback_threshold = sea_level + 0.05;

    let climb_headwater = |start_idx: usize, threshold: f32| -> usize {
        let mut stack = vec![start_idx];
        let mut visited = vec![false; total_tiles_usize];
        let mut best_idx = start_idx;
        let mut best_elev = {
            let x = start_idx as u32 % width;
            let y = start_idx as u32 / width;
            elevation_field.sample(x, y)
        };
        while let Some(idx) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            let x = idx as u32 % width;
            let y = idx as u32 / width;
            let elev = elevation_field.sample(x, y);
            if elev >= threshold {
                return idx;
            }
            if elev > best_elev {
                best_idx = idx;
                best_elev = elev;
            }
            for &u in &upstream[idx] {
                stack.push(u);
            }
        }
        if best_elev >= fallback_threshold {
            best_idx
        } else {
            start_idx
        }
    };

    let mut glacier_heads: Vec<usize> = Vec::new();
    let mut lake_heads: Vec<usize> = Vec::new();
    let mut runoff_heads: Vec<usize> = Vec::new();
    let mut seen_heads: HashSet<usize> = HashSet::new();

    // Classify land tiles by terrain/tags for headwater prioritisation
    for idx in 0..total_tiles_usize {
        if seamask[idx] {
            continue;
        }
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let elev = elevation_field.sample(x, y);
        if let Some((terrain, tags)) = tile_terrain[idx] {
            let is_highland = tags.contains(TerrainTags::HIGHLAND);
            let is_glacial = matches!(
                terrain,
                TerrainType::Glacier
                    | TerrainType::SeasonalSnowfield
                    | TerrainType::AlpineMountain
                    | TerrainType::HighPlateau
                    | TerrainType::KarstHighland
            );
            if is_glacial || (is_highland && elev >= headwater_threshold) {
                if seen_heads.insert(idx) {
                    glacier_heads.push(idx);
                }
                continue;
            }
        }
    }

    // Lake outlets: land tiles adjacent to lake water tiles
    let mut lake_border: HashSet<usize> = HashSet::new();
    for (idx, term_class) in termination_classes
        .iter()
        .enumerate()
        .take(total_tiles_usize)
    {
        if *term_class != TerminationClass::Lake {
            continue;
        }
        let cx = (idx as u32) % width;
        let cy = (idx as u32) / width;
        for &(dx, dy) in neighbor_dirs() {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            if seamask[nidx] {
                continue;
            }
            if seen_heads.contains(&nidx) || lake_border.contains(&nidx) {
                continue;
            }
            lake_border.insert(nidx);
            seen_heads.insert(nidx);
            lake_heads.push(nidx);
        }
    }

    // High-slope runoff tiles
    let slope_threshold = 0.04f32;
    for (idx, &is_sea) in seamask.iter().enumerate().take(total_tiles_usize) {
        if is_sea {
            continue;
        }
        if seen_heads.contains(&idx) {
            continue;
        }
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let elev = elevation_field.sample(x, y);
        if elev < headwater_threshold {
            continue;
        }
        let mut max_drop = 0.0f32;
        for &(dx, dy) in neighbor_dirs() {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let neighbor_elev = elevation_field.sample(nx as u32, ny as u32);
            max_drop = max_drop.max(elev - neighbor_elev);
        }
        if max_drop >= slope_threshold && seen_heads.insert(idx) {
            runoff_heads.push(idx);
        }
    }

    // Fallback candidates ordered by accumulation
    let mut fallback_heads: Vec<usize> = (0..total_tiles_usize)
        .filter(|idx| !seamask[*idx] && !seen_heads.contains(idx))
        .collect();
    fallback_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));

    glacier_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));
    lake_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));
    runoff_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));

    let glacier_sources: Vec<(usize, usize)> = glacier_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let lake_sources: Vec<(usize, usize)> = lake_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let runoff_sources: Vec<(usize, usize)> = runoff_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let fallback_sources: Vec<(usize, usize)> = fallback_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, fallback_threshold);
            (idx, head_idx)
        })
        .collect();

    let candidate_total =
        glacier_sources.len() + lake_sources.len() + runoff_sources.len() + fallback_sources.len();

    let max_accum = glacier_sources
        .iter()
        .chain(lake_sources.iter())
        .chain(runoff_sources.iter())
        .chain(fallback_sources.iter())
        .map(|(idx, _)| flow_accum[*idx])
        .max()
        .unwrap_or(0);

    let fallback_sources_clone = fallback_sources.clone();
    let source_groups = vec![
        (SourceCategory::Glacier, glacier_sources),
        (SourceCategory::LakeOutlet, lake_sources),
        (SourceCategory::Runoff, runoff_sources),
        (SourceCategory::Fallback, fallback_sources),
    ];
    let sink_tiles = flow_dir
        .iter()
        .enumerate()
        .filter(|(idx, &dir)| dir == 255 && !seamask[*idx])
        .count();
    let total_tiles = (width * height) as f32;
    let mean_elev = if total_tiles == 0.0 {
        0.0
    } else {
        sum_elev / total_tiles
    };
    let median_elev = quantile(&elev_samples, 0.5);

    let max_flow_total = flow_accum.iter().copied().max().unwrap_or(0);
    let tiles_over_one = flow_accum.iter().filter(|&&v| v > 1).count();
    tracing::debug!(
        target: "shadow_scale::mapgen",
        candidates = candidate_total,
        accumulation_threshold,
        max_accum,
        max_flow_total,
        tiles_over_one,
        target_rivers,
        headwater_threshold,
        source_percentile = source_percentile,
        sea_buffer,
        sink_tiles,
        invalid_downstream,
        orphan_tiles,
        land_unreachable,
        land_tiles,
        water_tiles,
        elev_mean = mean_elev,
        elev_median = median_elev,
        accum_p25 = quantile_u16(&accum_sorted, 0.25),
        accum_p50 = quantile_u16(&accum_sorted, 0.5),
        accum_p75 = quantile_u16(&accum_sorted, 0.75),
        accum_p90 = quantile_u16(&accum_sorted, 0.9),
        percentile_threshold,
        accum_percentile
    );

    let mut accepted_heads: HashSet<usize> = HashSet::new();
    // Tile indices of every accepted segment's navigable chain: what a later trace merges into
    // rather than running a parallel channel alongside (`truncate_at_existing_channel`). Segments
    // are accepted in a stable source order, so the merge is deterministic.
    let mut navigable_tiles: HashSet<usize> = HashSet::new();
    let mut head_positions: Vec<UVec2> = Vec::new();
    let base_spacing = preset_opt
        .as_ref()
        .map(|p| p.river_min_spacing)
        .unwrap_or(12.0)
        .max(0.0);
    let mut spacing_sq = overrides.spacing.unwrap_or(base_spacing).max(0.0);
    spacing_sq *= spacing_sq;
    let mut pass = 0;
    let uphill_gain_pct = overrides
        .uphill_gain_pct
        .or_else(|| preset_opt.as_ref().map(|p| p.river_uphill_gain_pct))
        .unwrap_or(0.05)
        .max(0.0);

    let segment_ctx = SegmentTraceContext {
        grid,
        corner_field: &corner_field,
        thresholds,
        elevation_field: &elevation_field,
        hex_ctx: &trace_ctx,
        seamask: &seamask,
        tile_terrain: &tile_terrain,
        uphill_gain_pct,
    };

    while pass < MAX_SPACING_RELAXATION_PASSES && rivers.len() < target_rivers {
        for (category, sources) in &source_groups {
            for &(base_idx, head_idx) in sources {
                if rivers.len() >= target_rivers {
                    break;
                }
                if accepted_heads.contains(&head_idx) {
                    continue;
                }
                let acc = flow_accum[base_idx];
                if *category == SourceCategory::Fallback && acc < accumulation_threshold {
                    continue;
                }
                let head = UVec2::new(head_idx as u32 % width, head_idx as u32 / width);
                if too_close_to_existing_head(head, &head_positions, spacing_sq) {
                    continue;
                }

                let Some(traced) = segment_ctx.trace(head, &navigable_tiles) else {
                    continue;
                };
                let hex_len = traced.hex_length();
                if hex_len == 0 {
                    continue;
                }
                tracing::debug!(
                    target: "shadow_scale::mapgen",
                    category = ?category,
                    acc,
                    sx = head.x,
                    sy = head.y,
                    hex_len,
                    edges = traced.edges.len(),
                    navigable = traced.navigable_hexes.len(),
                    termination = ?traced.termination,
                    threshold = accumulation_threshold,
                    "hydrology.candidate_trace"
                );

                let touched = touched_hexes(&traced.edges, &traced.navigable_hexes, &grid);
                let connects_existing = touched
                    .iter()
                    .any(|pos| *pos != head && river_tiles.contains(&grid.tile_index(*pos)));

                let allow_short = matches!(category, SourceCategory::Fallback)
                    || connects_existing
                    || rivers.is_empty();
                let acceptable =
                    path_meets_length(*category, hex_len, min_length, fallback_min_length)
                        || (allow_short && hex_len >= fallback_min_length);

                if acceptable {
                    accept_river(
                        head_idx,
                        head,
                        traced,
                        &grid,
                        &mut rivers,
                        &mut river_tiles,
                        &mut navigable_tiles,
                        &mut accepted_heads,
                        &mut head_positions,
                    );
                }
            }
        }
        if spacing_sq == 0.0 {
            break;
        }
        spacing_sq *= SPACING_RELAXATION_FACTOR;
        if spacing_sq < 1.0 {
            spacing_sq = 0.0;
        }
        pass += 1;
    }
    if rivers.is_empty() {
        // Last resort: seed the single best fallback source, ignoring the spacing/length gates the
        // multi-pass loop applies, so a map always has at least one river.
        if let Some(&(base_idx, head_idx)) = fallback_sources_clone.first() {
            if flow_accum[base_idx] >= 1 {
                let head = UVec2::new(head_idx as u32 % width, head_idx as u32 / width);
                if let Some(traced) = segment_ctx.trace(head, &navigable_tiles) {
                    let hex_len = traced.hex_length();
                    tracing::debug!(
                        target: "shadow_scale::mapgen",
                        category = ?SourceCategory::Fallback,
                        acc = flow_accum[base_idx],
                        sx = head.x,
                        sy = head.y,
                        hex_len,
                        edges = traced.edges.len(),
                        navigable = traced.navigable_hexes.len(),
                        termination = ?traced.termination,
                        fallback_min_length,
                        "hydrology.fallback_trace"
                    );
                    if hex_len >= fallback_min_length {
                        accept_river(
                            head_idx,
                            head,
                            traced,
                            &grid,
                            &mut rivers,
                            &mut river_tiles,
                            &mut navigable_tiles,
                            &mut accepted_heads,
                            &mut head_positions,
                        );
                    }
                }
            }
        }
    }

    // Compute per-tile Strahler orders to classify tributary strength.
    let mut topo: Vec<usize> = (0..total_tiles_usize).collect();
    topo.sort_unstable_by(|a, b| cost[*b].partial_cmp(&cost[*a]).unwrap_or(Ordering::Equal));
    let mut tile_orders: Vec<u8> = vec![0; total_tiles_usize];
    for idx in topo {
        let parents = &upstream[idx];
        if parents.is_empty() {
            tile_orders[idx] = 1;
            continue;
        }
        let mut max_order = 0u8;
        let mut duplicate_max = 0u8;
        for &p in parents {
            let order = tile_orders[p].max(1);
            if order > max_order {
                max_order = order;
                duplicate_max = 1;
            } else if order == max_order {
                duplicate_max = duplicate_max.saturating_add(1);
            }
        }
        let mut order_here = if duplicate_max >= 2 {
            max_order.saturating_add(1)
        } else {
            max_order
        };
        if order_here == 0 {
            order_here = 1;
        }
        tile_orders[idx] = order_here;
    }

    let is_water_idx = |idx: usize, terrain: &[Option<(TerrainType, TerrainTags)>]| -> bool {
        seamask[idx]
            || terrain[idx]
                .map(|(t, _)| is_water_terrain(t))
                .unwrap_or(false)
    };
    // Gentle coast: the same `elevation.sample - sea_level < coast_height_threshold` test
    // `classify_bands` / `reconcile_coastal_shelf` use to split gentle from cliff coasts.
    let is_gentle_coast = |pos: UVec2| -> bool {
        elevation_field.sample(pos.x, pos.y) - sea_level < coast_height_threshold
    };

    let mut total_length = 0usize;
    let mut max_order_seg = 0u8;
    let mut tributary_segments = 0usize;
    let mut delta_segment_count = 0usize;
    let mut delta_candidates: Vec<usize> = Vec::new();
    let mut navigable_candidates: Vec<usize> = Vec::new();
    let mut class_histogram = [0usize; RIVER_CLASS_HISTOGRAM_SLOTS];
    for segment in rivers.iter_mut() {
        let touched = segment.touched_hexes(width, height, grid.wrap_horizontal);
        total_length += segment.edges.len() + segment.navigable_hexes.len();
        for edge in &segment.edges {
            class_histogram[edge.class as usize] += 1;
        }

        // Strahler order still rides the hex flow field: it measures the tributary tree, which is a
        // property of the drainage basin, not of which side of a hex the water runs along.
        let mut seg_order = 1u8;
        for pos in &touched {
            let idx = grid.tile_index(*pos);
            seg_order = seg_order.max(tile_orders.get(idx).copied().unwrap_or(1));
        }
        segment.order = seg_order.max(1);
        if segment.order > 1 {
            tributary_segments += 1;
        }
        max_order_seg = max_order_seg.max(segment.order);

        for pos in &segment.navigable_hexes {
            navigable_candidates.push(grid.tile_index(*pos));
        }

        // Deltas form where a river meets a standing water body: the open ocean *or* an inland sea
        // / lake (lacustrine deltas, e.g. Volga→Caspian). The mouth is the most downstream land hex
        // the river touches that borders that water — for a navigable river that is the end of its
        // hex chain, for an edge river the last hex flanking its final edge. Ocean tiles are
        // sea-masked; inland seas are water *terrain* but may sit above sea level, so check both.
        if matches!(
            segment.termination,
            TerminationClass::Ocean | TerminationClass::Lake
        ) {
            delta_segment_count += 1;
            let mouth_scan = delta_scan_order(
                &segment.edges,
                &segment.navigable_hexes,
                &grid,
                &elevation_field,
            );
            if let Some(delta_idx) = mouth_scan.iter().rev().find_map(|pos| {
                let idx = grid.tile_index(*pos);
                if is_water_idx(idx, &tile_terrain) {
                    return None;
                }
                // Only a genuine shore tile (adjacent to the terminal water body) becomes a delta,
                // never a spot where the river merely petered out — and only where the coast is
                // gentle enough to deposit on.
                if !is_gentle_coast(*pos) {
                    return None;
                }
                let borders_water = (0..HEX_DIRECTION_COUNT as u8).any(|dir| {
                    grid.neighbor(*pos, dir)
                        .map(|n| is_water_idx(grid.tile_index(n), &tile_terrain))
                        .unwrap_or(false)
                });
                if borders_water {
                    Some(idx)
                } else {
                    None
                }
            }) {
                delta_candidates.push(delta_idx);
            }
        }
    }

    // The mouth is a delta, not open water: a navigable river ends in the wetland it deposits.
    // Excluding the delta here also stops the delta stamp below from OR-ing WETLAND onto a tile
    // that was just made WATER.
    let delta_set: HashSet<usize> = delta_candidates.iter().copied().collect();
    let navigable_set: HashSet<usize> = navigable_candidates
        .into_iter()
        .filter(|idx| !delta_set.contains(idx))
        .collect();

    // Per-tile river-edge mask: both hexes flanking an edge record it on their own side, so a hex
    // and its neighbour always agree about the river between them. This is the primitive a future
    // movement system reads.
    let mut tile_river_edges = vec![0u16; total_tiles_usize];
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

    // Per-tile river-inflow mask: where an edge chain hands off to a navigable trunk, the first
    // navigable hex records the **corner** the tributary arrives at (an edge river ends at a
    // vertex, never mid-side) and the class it arrives with. Only that one hex, only that one
    // corner — a trunk hex can flank several river edges, and the edge mask alone cannot say which
    // of their endpoints the chain actually ended on.
    let mut tile_river_inflow = vec![0u16; total_tiles_usize];
    for segment in rivers.iter() {
        let (Some(first), Some(inflow)) = (
            segment.navigable_hexes.first(),
            segment.navigable_inflow.as_ref(),
        ) else {
            continue;
        };
        widen_tile_river_class(
            &mut tile_river_inflow,
            grid.tile_index(*first),
            inflow.corner,
            inflow.class,
        );
    }

    // Per-tile channel-exit mask: which sides a navigable hex's channel actually flows out through.
    // The chain is a PATH — each hex links only to its upstream and downstream neighbours — and only
    // the tracer knows which those are. Without this the renderer had to guess from terrain, arming
    // every navigable/water neighbour, so adjacent chains cross-linked into a web of triangles.
    // Bits are OR-ed, never overwritten: a confluence hex carries the union of the chains through it.
    let mut tile_river_channel = vec![0u8; total_tiles_usize];

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
    // Only a genuine **dead end** earns it. A tributary that merged into an existing trunk
    // (`truncate_at_existing_channel`) also *ends* on its last hex, but that hex is a confluence in
    // the middle of the trunk: the channel already flows on through it, and handing it a second exit
    // into whatever water it happens to sit beside would draw a spurious arm off the side of the
    // trunk. "Has no exit but the one back upstream" is exactly the test for that, and it does not
    // depend on the order segments were traced in.
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
                            || (is_water_idx(idx, &tile_terrain) && !navigable_set.contains(&idx))
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
            // OR-ing water onto whatever biome was there.
            tile.terrain = TerrainType::NavigableRiver;
            tile.terrain_tags = navigable_tags;
            navigable_tiles_applied += 1;
            tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
        } else if delta_set.contains(&idx) && tile.terrain != TerrainType::RiverDelta {
            tile.terrain = TerrainType::RiverDelta;
            tile.terrain_tags |= TerrainTags::WETLAND;
            tile.terrain_tags |= TerrainTags::FRESHWATER;
            delta_tiles_applied += 1;
            tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
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

    let mut state = world
        .remove_resource::<HydrologyState>()
        .unwrap_or_default();
    state.rivers = rivers;
    world.insert_resource(state);

    tracing::info!(
        target: "shadow_scale::mapgen",
        rivers = river_count,
        candidates = candidate_total,
        max_accum,
        avg_length,
        max_order = max_order_seg,
        tributaries = tributary_segments,
        delta_segments = delta_segment_count,
        delta_tiles = delta_tiles_applied,
        accumulation_threshold,
        total_edges,
        minor_edges = class_histogram[RiverClass::Minor as usize],
        major_edges = class_histogram[RiverClass::Major as usize],
        navigable_rivers,
        navigable_tiles = navigable_tiles_applied,
        class_major_min = thresholds.major_min,
        class_navigable_min = thresholds.navigable_min,
        "hydrology.generated"
    );
}

fn quantile(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    values[idx]
}

fn quantile_u16(values: &[u16], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    values[idx] as f32
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HydrologyState>();
};

#[derive(Copy, Clone, Debug)]
struct HeapEntry {
    cost: f32,
    idx: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.idx == other.idx
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
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.idx.cmp(&other.idx))
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

    /// Build the corner field for a synthetic map: `ocean_rows` are water, everything else is land.
    fn corner_field(
        grid: HexGrid,
        elevations: Vec<f32>,
        water: &dyn Fn(u32, u32) -> bool,
        terrain_at: &dyn Fn(u32, u32) -> TerrainType,
    ) -> (CornerField, ElevationField) {
        let elevation_field = ElevationField::new(grid.width, grid.height, elevations);
        let total = (grid.width * grid.height) as usize;
        let mut seamask = vec![false; total];
        let mut tile_terrain = vec![None; total];
        let mut termination_classes = vec![TerminationClass::None; total];
        for y in 0..grid.height {
            for x in 0..grid.width {
                let i = idx(grid.width, x, y);
                let terrain = terrain_at(x, y);
                let tags = terrain_definition(terrain).tags;
                tile_terrain[i] = Some((terrain, tags));
                seamask[i] = water(x, y);
                termination_classes[i] = termination_class_for(terrain, tags);
            }
        }
        let field = CornerField::build(
            grid,
            &elevation_field,
            &seamask,
            &tile_terrain,
            &termination_classes,
        );
        (field, elevation_field)
    }

    fn edge_only_thresholds() -> RiverClassThresholds {
        RiverClassThresholds {
            major_min: u32::MAX,
            navigable_min: u32::MAX,
            navigable_enabled: false,
        }
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
                    // ...across the *same* canonical edge.
                    assert_eq!(
                        (step.hex, step.dir),
                        (back.hex, back.dir),
                        "corner {corner} <-> {} traverse different edges (wrap={wrap})",
                        step.corner
                    );
                    // And that edge's two endpoints are exactly this pair of corners.
                    let mut endpoints = g
                        .edge_corners(step.hex, step.dir)
                        .expect("a traversed edge has both endpoints on the map");
                    endpoints.sort_unstable();
                    let mut pair = [corner, step.corner];
                    pair.sort_unstable();
                    assert_eq!(endpoints, pair, "edge endpoints disagree with the step");
                }
            }
        }
    }

    #[test]
    fn every_corner_step_crosses_exactly_one_hex_edge() {
        // A corner's 3 neighbours cross 3 distinct edges — no step is a no-op or a repeat.
        let g = grid(6, 6, true);
        for corner in 0..g.corner_count() {
            let steps: Vec<_> = g.corner_neighbors(corner).into_iter().flatten().collect();
            let mut edges: Vec<(UVec2, u8)> = steps.iter().map(|s| (s.hex, s.dir)).collect();
            edges.sort_by_key(|(hex, dir)| (hex.y, hex.x, *dir));
            let before = edges.len();
            edges.dedup();
            assert_eq!(before, edges.len(), "corner {corner} repeats an edge");
        }
    }

    /// `local_corner_index` is the wire contract behind `Tile::river_inflow`: it turns the sim's
    /// `(hex, TOP|BOTTOM)` corner into the client's `0..6` vertex index. Every hex must see its six
    /// corners as six *distinct* indices covering `0..6` exactly, and the mapping must round-trip
    /// through `HEX_CORNER_LAYOUT`. If this table were wrong, every tributary would join its trunk
    /// at the wrong vertex — so it is tested exhaustively, on a wrapped grid (where every corner is
    /// routable) and an unwrapped one (where border hexes lose some).
    #[test]
    fn local_corner_index_is_a_bijection_on_every_hex() {
        for wrap in [false, true] {
            let g = grid(6, 6, wrap);
            for y in 0..g.height {
                for x in 0..g.width {
                    let hex = UVec2::new(x, y);
                    let mut seen: Vec<u8> = Vec::with_capacity(HEX_CORNER_COUNT);
                    for (slot, &(dir, corner_slot)) in HEX_CORNER_LAYOUT.iter().enumerate() {
                        let owner = match dir {
                            Some(dir) => g.neighbor(hex, dir),
                            None => Some(hex),
                        };
                        // Off-map owner: that corner simply does not exist for this hex.
                        let Some(owner) = owner else {
                            continue;
                        };
                        let corner = g.corner_index(owner, corner_slot);
                        let index = g
                            .local_corner_index(hex, corner)
                            .expect("a corner of the hex resolves to a local index");
                        assert_eq!(
                            usize::from(index),
                            slot,
                            "corner {corner} round-trips to the wrong index on {hex:?} \
                             (wrap={wrap})"
                        );
                        seen.push(index);
                    }
                    seen.sort_unstable();
                    let before = seen.len();
                    seen.dedup();
                    assert_eq!(
                        before,
                        seen.len(),
                        "{hex:?} sees a corner twice (wrap={wrap})"
                    );
                    if wrap && y > 0 && y + 1 < g.height {
                        // An interior row of a wrapped grid has all six corners on the map.
                        assert_eq!(
                            seen,
                            (0..HEX_CORNER_COUNT as u8).collect::<Vec<_>>(),
                            "{hex:?} does not cover all six corner indices"
                        );
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // The corner tables, pinned ABSOLUTELY to the client's geometry.
    //
    // `local_corner_index_is_a_bijection_on_every_hex` proves the table is *internally consistent*
    // (six distinct corners that round-trip) — and a table rotated by one position passes that
    // happily while putting every tributary on the wrong vertex. So the two tests below never ask
    // the table about itself: they compute each corner's WORLD POSITION twice, once through the
    // sim's `(hex, TOP|BOTTOM)` corner model and once through the client's `corner i at angle
    // 60*i + 30` circle, and assert the two constructions land on the same point.
    // -----------------------------------------------------------------------

    /// Hex circumradius for the geometry proofs. Any positive value works (every assertion compares
    /// two constructions at the *same* radius); `1.0` keeps the arithmetic exact and readable.
    const GEOMETRY_HEX_RADIUS: f64 = 1.0;

    /// Slack when comparing two floating-point constructions of the same vertex. The coordinates are
    /// O(radius), so this is ~9 orders of magnitude tighter than the smallest real disagreement
    /// (adjacent vertices of a hex are `radius` apart).
    const GEOMETRY_EPSILON: f64 = 1e-9;

    /// The centre of hex `(col, row)` in the pointy-top odd-r layout the client renders (+y **down**,
    /// odd rows shifted half a column right) — `MapView._offset_to_axial` + `_axial_center`.
    fn hex_center_world(pos: UVec2, radius: f64) -> (f64, f64) {
        let x = f64::sqrt(3.0) * radius * (f64::from(pos.x) + 0.5 * f64::from(pos.y & 1));
        let y = 1.5 * radius * f64::from(pos.y);
        (x, y)
    }

    /// The world position of the **sim's** corner `(hex, slot)`: the top vertex of a pointy-top hex
    /// sits one circumradius above its centre, the bottom vertex one below (+y down).
    fn sim_corner_world(pos: UVec2, slot: u8, radius: f64) -> (f64, f64) {
        let (cx, cy) = hex_center_world(pos, radius);
        if slot == CORNER_TOP {
            (cx, cy - radius)
        } else {
            (cx, cy + radius)
        }
    }

    /// The world position of the **client's** vertex `index` of `hex`: corner `i` at screen angle
    /// `60 * i + 30` degrees on the circumradius, +y down (`MapView._hex_points`, and the corner
    /// geometry in `terrain_blend.gdshader`).
    fn client_corner_world(pos: UVec2, index: usize, radius: f64) -> (f64, f64) {
        let (cx, cy) = hex_center_world(pos, radius);
        let angle = (60.0 * index as f64 + 30.0).to_radians();
        (cx + radius * angle.cos(), cy + radius * angle.sin())
    }

    fn assert_same_point(a: (f64, f64), b: (f64, f64), what: &str) {
        assert!(
            (a.0 - b.0).abs() < GEOMETRY_EPSILON && (a.1 - b.1).abs() < GEOMETRY_EPSILON,
            "{what}: sim puts it at {a:?}, the client draws it at {b:?}"
        );
    }

    /// **The absolute proof of `HEX_CORNER_LAYOUT`.** For every hex and every one of its six corners,
    /// the vertex the table names — resolved through the sim's `(hex, TOP|BOTTOM)` corner model —
    /// must be *the same point in the world* as the client's corner `i`. A table rotated by one
    /// position fails here even though it is internally consistent.
    #[test]
    fn hex_corner_layout_matches_the_clients_corner_geometry() {
        let g = grid(6, 6, false);
        // Interior hexes only: with wrap off, a border hex's off-map corner owners have no world
        // position to compare against (and a wrapped grid's seam would alias two distinct points).
        for y in 1..g.height - 1 {
            for x in 1..g.width - 1 {
                let hex = UVec2::new(x, y);
                for (index, &(dir, slot)) in HEX_CORNER_LAYOUT.iter().enumerate() {
                    let owner = match dir {
                        Some(dir) => g
                            .neighbor(hex, dir)
                            .expect("interior hex has all neighbours"),
                        None => hex,
                    };
                    assert_same_point(
                        sim_corner_world(owner, slot, GEOMETRY_HEX_RADIUS),
                        client_corner_world(hex, index, GEOMETRY_HEX_RADIUS),
                        &format!("corner {index} of {hex:?}"),
                    );
                }
            }
        }
    }

    /// **The absolute proof of `grid_utils::hex_edge_corner_indices`.** The two corners it names for
    /// side `dir` must be exactly the two endpoints of the edge `H` genuinely *shares* with its
    /// neighbour in that direction — computed as the geometric intersection of the two hexes' vertex
    /// sets, never by consulting the table.
    #[test]
    fn hex_edge_corner_indices_are_the_shared_edges_endpoints() {
        let g = grid(6, 6, false);
        for y in 1..g.height - 1 {
            for x in 1..g.width - 1 {
                let hex = UVec2::new(x, y);
                for dir in 0..HEX_DIRECTION_COUNT {
                    let neighbor = g
                        .neighbor(hex, dir as u8)
                        .expect("interior hex has all neighbours");

                    // The endpoints of the shared side = the vertices the two hexes have in common.
                    let shared: Vec<usize> = (0..HEX_CORNER_COUNT)
                        .filter(|&i| {
                            let p = client_corner_world(hex, i, GEOMETRY_HEX_RADIUS);
                            (0..HEX_CORNER_COUNT).any(|j| {
                                let q = client_corner_world(neighbor, j, GEOMETRY_HEX_RADIUS);
                                (p.0 - q.0).abs() < GEOMETRY_EPSILON
                                    && (p.1 - q.1).abs() < GEOMETRY_EPSILON
                            })
                        })
                        .collect();
                    assert_eq!(
                        shared.len(),
                        2,
                        "{hex:?} and its neighbour in direction {dir} must share exactly one side \
                         (two vertices), found {shared:?}"
                    );

                    let mut named = hex_edge_corner_indices(dir).expect("dir is in range");
                    named.sort_unstable();
                    assert_eq!(
                        named.to_vec(),
                        shared,
                        "side {dir} of {hex:?}: the table names corners {named:?}, but the side it \
                         actually shares with {neighbor:?} runs between corners {shared:?}"
                    );
                }
            }
        }
    }

    /// A corner that is not one of the hex's six resolves to `None` — the mapping is a lookup, not
    /// an assertion site, and must never silently alias onto a wrong vertex.
    #[test]
    fn local_corner_index_rejects_a_corner_the_hex_does_not_touch() {
        let g = grid(6, 6, true);
        let hex = UVec2::new(2, 2);
        let far = g.corner_index(UVec2::new(5, 5), CORNER_TOP);
        assert_eq!(g.local_corner_index(hex, far), None);
    }

    /// `grid_utils::hex_edge_corner_indices` claims side `dir` runs between local corners
    /// `{dir - 1, dir}`. Cross-check that against the corner model itself (`edge_corners`, derived
    /// independently of `HEX_CORNER_LAYOUT`): the endpoints of edge `(H, dir)` must map to exactly
    /// that pair of local indices on `H`. This is what lets the renderer put an inflow corner on
    /// the right end of the right side.
    #[test]
    fn hex_edge_corner_indices_match_the_corner_model() {
        let g = grid(6, 6, true);
        for y in 1..g.height - 1 {
            for x in 0..g.width {
                let hex = UVec2::new(x, y);
                for dir in 0..HEX_DIRECTION_COUNT as u8 {
                    let endpoints = g
                        .edge_corners(hex, dir)
                        .expect("an interior edge has both endpoints on the map");
                    let mut mapped: Vec<usize> = endpoints
                        .iter()
                        .map(|&corner| {
                            usize::from(
                                g.local_corner_index(hex, corner)
                                    .expect("an endpoint of the hex's own edge is its corner"),
                            )
                        })
                        .collect();
                    mapped.sort_unstable();

                    let mut expected = crate::grid_utils::hex_edge_corner_indices(usize::from(dir))
                        .expect("dir is in range");
                    expected.sort_unstable();
                    assert_eq!(
                        mapped,
                        expected.to_vec(),
                        "side {dir} of {hex:?} spans the wrong corners"
                    );
                }
            }
        }
    }

    /// A gentle valley down column 2 to an ocean row, with a wetland partway. Rivers must run the
    /// valley and terminate at the ocean, touching the wetland on the way.
    fn valley_map(width: u32, height: u32, wetland: Option<UVec2>) -> (HexGrid, Vec<f32>) {
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
        let _ = wetland;
        (g, elevations)
    }

    #[test]
    fn river_traces_through_wetland_until_ocean() {
        let (g, elevations) = valley_map(5, 7, None);
        let wetland = UVec2::new(2, 3);
        let (field, elevation_field) = corner_field(g, elevations, &|_, y| y == 0, &|x, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else if UVec2::new(x, y) == wetland {
                TerrainType::FreshwaterMarsh
            } else {
                TerrainType::MixedWoodland
            }
        });

        let head = UVec2::new(2, 6);
        let start = field.drain_corner(head).expect("head hex drains somewhere");
        let head_elev = elevation_field.sample(head.x, head.y);
        let trace = trace_river_edges(
            start,
            head_elev,
            head_elev * 1.05,
            &field,
            &edge_only_thresholds(),
            &elevation_field,
        );

        assert_eq!(trace.termination, Some(TerminationClass::Ocean));
        let touched = touched_hexes(&trace.edges, &[], &g);
        assert!(
            touched.contains(&wetland),
            "river never flanked the wetland tile: {touched:?}"
        );
        // An edge river runs *between* hexes, so it never occupies the ocean — it ends at the
        // shore, on the corner where the sea begins. Reaching the coastal row IS reaching the sea.
        assert!(
            touched.iter().any(|p| p.y <= 1),
            "river never reached the coast: {touched:?}"
        );
    }

    #[test]
    fn river_crosses_inland_lake_before_ocean() {
        let (g, elevations) = valley_map(5, 7, None);
        let lake = UVec2::new(2, 3);
        let (field, elevation_field) = corner_field(g, elevations, &|_, y| y == 0, &|x, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else if UVec2::new(x, y) == lake {
                TerrainType::InlandSea
            } else {
                TerrainType::MixedWoodland
            }
        });

        let head = UVec2::new(2, 6);
        let start = field.drain_corner(head).expect("head hex drains somewhere");
        let head_elev = elevation_field.sample(head.x, head.y);
        let trace = trace_river_edges(
            start,
            head_elev,
            head_elev * 1.05,
            &field,
            &edge_only_thresholds(),
            &elevation_field,
        );

        let touched = touched_hexes(&trace.edges, &[], &g);
        assert!(
            touched.contains(&lake),
            "river never reached the inland lake: {touched:?}"
        );
        // A lake is a sink, so the river ends there or carries on to the sea — never in a desert or
        // a dead end.
        assert!(
            matches!(
                trace.termination,
                Some(TerminationClass::Ocean | TerminationClass::Lake)
            ),
            "unexpected termination {:?}",
            trace.termination
        );
    }

    #[test]
    fn tributary_traces_merge_downstream() {
        // Two heads on either flank of the same valley must converge onto shared edges downstream.
        let (g, elevations) = valley_map(7, 7, None);
        let (field, elevation_field) = corner_field(g, elevations, &|_, y| y == 0, &|_, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else {
                TerrainType::MixedWoodland
            }
        });

        let trace_from = |head: UVec2| {
            let start = field.drain_corner(head).expect("head drains");
            let head_elev = elevation_field.sample(head.x, head.y);
            trace_river_edges(
                start,
                head_elev,
                head_elev * 1.05,
                &field,
                &edge_only_thresholds(),
                &elevation_field,
            )
        };

        let west: HashSet<(u32, u32, u8)> = trace_from(UVec2::new(1, 6))
            .edges
            .iter()
            .map(|e| (e.hex.x, e.hex.y, e.dir))
            .collect();
        let east: HashSet<(u32, u32, u8)> = trace_from(UVec2::new(4, 6))
            .edges
            .iter()
            .map(|e| (e.hex.x, e.hex.y, e.dir))
            .collect();

        assert!(
            west.intersection(&east).any(|(_, y, _)| *y <= 2),
            "tributaries never shared a downstream edge"
        );
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
        config.hydrology.min_length = Some(3);
        config.hydrology.fallback_min_length = Some(2);
        config.hydrology.river_density = Some(1.0);
        world.insert_resource(config);
        world.insert_resource(WorldGenSeed(0));

        let (_, elevations) = valley_map(width, height, None);
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
        // An edge river ends at the shore (see `river_traces_through_wetland_until_ocean`): it
        // terminates on the corner where the sea begins, flanking the coastal row.
        let reaches_ocean = hydro.rivers.iter().any(|river| {
            river.termination == TerminationClass::Ocean
                && river
                    .touched_hexes(g.width, g.height, g.wrap_horizontal)
                    .iter()
                    .any(|p| p.y <= 1)
        });
        assert!(reaches_ocean, "no river reached the coast");
    }

    #[test]
    fn traced_edge_chains_are_contiguous_and_discharge_never_decreases() {
        let world = generate_small_world();
        let hydro = world.resource::<HydrologyState>();
        let g = grid(7, 7, false);

        for river in &hydro.rivers {
            let mut previous: Option<[usize; 2]> = None;
            let mut last_discharge = 0u32;
            let mut last_class = RiverClass::None;
            for edge in &river.edges {
                let corners = g
                    .edge_corners(edge.hex, edge.dir)
                    .expect("every traced edge lies on the map");
                if let Some(prev) = previous {
                    assert!(
                        corners.iter().any(|c| prev.contains(c)),
                        "river {} has a break between consecutive edges",
                        river.id
                    );
                }
                assert!(
                    edge.discharge >= last_discharge,
                    "river {} discharge dropped downstream ({last_discharge} -> {})",
                    river.id,
                    edge.discharge
                );
                assert!(
                    edge.class >= last_class,
                    "river {} class shrank downstream",
                    river.id
                );
                previous = Some(corners);
                last_discharge = edge.discharge;
                last_class = edge.class;
            }
        }
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
                    .expect("a traced edge has both hexes on the map");

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

        // And a tile with no river reads None on all six sides.
        let mut has_river_free_tile = false;
        for &entity in &registry.tiles {
            let tile = world.get::<Tile>(entity).expect("tile exists");
            if !tile.has_any_river_edge() {
                has_river_free_tile = true;
                for dir in 0..HEX_DIRECTION_COUNT as u8 {
                    assert_eq!(tile.river_class_on_side(dir), RiverClass::None);
                }
            }
        }
        assert!(has_river_free_tile, "expected some tiles to be river-free");
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

    #[test]
    fn class_thresholds_grow_with_discharge_and_hand_off_to_navigable() {
        let thresholds = RiverClassThresholds {
            major_min: 100,
            navigable_min: 1000,
            navigable_enabled: true,
        };
        assert_eq!(thresholds.classify(1), Some(RiverClass::Minor));
        assert_eq!(thresholds.classify(99), Some(RiverClass::Minor));
        assert_eq!(thresholds.classify(100), Some(RiverClass::Major));
        assert_eq!(thresholds.classify(999), Some(RiverClass::Major));
        assert_eq!(thresholds.classify(1000), None); // becomes a NavigableRiver hex chain

        // The kill switch keeps the biggest rivers on the edge model as Major.
        let capped = RiverClassThresholds {
            navigable_enabled: false,
            ..thresholds
        };
        assert_eq!(capped.classify(10_000), Some(RiverClass::Major));
    }

    /// The hand-off must anchor on the **last emitted edge**, so the hex chain and the edge chain
    /// share an edge. Three hexes meet at a corner, so anchoring on the *un-emitted* edge the
    /// tracer stopped at could pick the third hex — one the edge chain never touches, leaving the
    /// two chains joined at a bare corner and the first navigable hex with an empty river mask.
    #[test]
    fn the_navigable_handoff_anchors_on_the_last_emitted_edge() {
        let (g, elevations) = valley_map(5, 7, None);
        let (field, elevation_field) = corner_field(g, elevations, &|_, y| y == 0, &|_, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else {
                TerrainType::MixedWoodland
            }
        });

        // Chosen against this fixture's measured discharge profile ([3, 4, 8, 9, 15, 16, 21, 22,
        // 31]): the river emits five edges — Minor, then Major — and outgrows the edge model partway
        // down, which is exactly the hand-off this test is about.
        let thresholds = RiverClassThresholds {
            major_min: 8,
            navigable_min: 16,
            navigable_enabled: true,
        };
        let head = UVec2::new(2, 6);
        let start = field.drain_corner(head).expect("head hex drains somewhere");
        let head_elev = elevation_field.sample(head.x, head.y);
        let trace = trace_river_edges(
            start,
            head_elev,
            head_elev * 1.05,
            &field,
            &thresholds,
            &elevation_field,
        );

        let last = *trace
            .edges
            .last()
            .expect("the river emits edges before going navigable");
        let first = trace
            .navigable_from
            .expect("the river crossed the navigable threshold");
        let far = g
            .neighbor(last.hex, last.dir)
            .expect("a traced edge has both hexes on the map");

        assert!(
            first == last.hex || first == far,
            "navigable chain starts at {first:?}, which flanks neither hex of the last emitted \
             edge ({:?} dir {})",
            last.hex,
            last.dir
        );
        // ...and of the two, it is the lower: water settles into the valley, not onto its shoulder.
        let lower = lower_flanking_hex(last.hex, last.dir, &g, &elevation_field).expect("on map");
        assert_eq!(first, lower, "the hand-off took the higher bank");

        // The hand-off also names the CORNER the tributary arrives at — an edge river ends at a
        // vertex, not mid-side — with the class of the last edge it emitted.
        let inflow = trace
            .navigable_inflow
            .expect("a hand-off from an emitted edge names an inflow corner");
        assert_eq!(inflow.class, last.class);
        let endpoints = g
            .edge_corners(last.hex, last.dir)
            .expect("a traced edge has both endpoints on the map");
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

    /// A river can cross the navigable threshold on its very first step, emitting no edges at all.
    /// There is no edge to anchor to, so the hand-off falls back to the edge it stopped at — it
    /// must still produce a starting hex rather than dropping the river.
    #[test]
    fn a_river_navigable_from_its_first_step_still_starts_somewhere() {
        let (g, elevations) = valley_map(5, 7, None);
        let (field, elevation_field) = corner_field(g, elevations, &|_, y| y == 0, &|_, y| {
            if y == 0 {
                TerrainType::DeepOcean
            } else {
                TerrainType::MixedWoodland
            }
        });

        // Everything is navigable, from the headwater on.
        let thresholds = RiverClassThresholds {
            major_min: 0,
            navigable_min: 0,
            navigable_enabled: true,
        };
        let head = UVec2::new(2, 6);
        let start = field.drain_corner(head).expect("head hex drains somewhere");
        let head_elev = elevation_field.sample(head.x, head.y);
        let trace = trace_river_edges(
            start,
            head_elev,
            head_elev * 1.05,
            &field,
            &thresholds,
            &elevation_field,
        );

        assert!(
            trace.edges.is_empty(),
            "nothing should be classified onto an edge at these thresholds"
        );
        let first = trace
            .navigable_from
            .expect("a river navigable from its first step must still name a starting hex");
        assert!(first.x < g.width && first.y < g.height);
        // No edge chain means no tributary: the hand-off must not fabricate an inflow corner.
        assert!(
            trace.navigable_inflow.is_none(),
            "a river with no emitted edges has no tributary to join"
        );
    }

    #[test]
    fn corner_elevation_is_the_mean_of_its_three_hexes() {
        // The mean (not the min) puts a corner low in the *trough* between two low hexes, which is
        // what makes rivers settle into valleys rather than hug a single low tile.
        let g = grid(5, 5, false);
        let mut elevations = vec![0.5f32; 25];
        elevations[idx(5, 2, 2)] = 0.8;
        let (field, _) = corner_field(g, elevations, &|_, _| false, &|_, _| {
            TerrainType::MixedWoodland
        });
        let corner = g.corner_index(UVec2::new(2, 2), CORNER_TOP);
        // TOP(2,2) is shared by (2,2)=0.8, NW=(1,1)=0.5, NE=(2,1)=0.5.
        assert!((field.elevation[corner] - (0.8 + 0.5 + 0.5) / 3.0).abs() < 1e-5);
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

    #[test]
    fn sea_level_constant_is_unused_by_the_corner_sea_check() {
        // Sea corners are defined by the *water mask*, not by an elevation cutoff — an inland sea
        // sitting above sea level is still a sink.
        let g = grid(5, 5, false);
        let elevations = vec![TEST_SEA_LEVEL + 0.3; 25];
        let lake = UVec2::new(2, 2);
        let (field, _) = corner_field(
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
        let corner = g.corner_index(lake, CORNER_TOP);
        assert_eq!(
            field.cost[corner], 0.0,
            "an inland sea corner must be a sink"
        );
    }

    #[test]
    fn non_fallback_requires_min_length() {
        assert!(!path_meets_length(SourceCategory::Glacier, 5, 8, 4));
        assert!(path_meets_length(SourceCategory::Glacier, 8, 8, 4));
    }

    #[test]
    fn fallback_allows_shorter_length() {
        assert!(path_meets_length(SourceCategory::Fallback, 5, 8, 4));
        assert!(!path_meets_length(SourceCategory::Fallback, 3, 8, 4));
    }
}
