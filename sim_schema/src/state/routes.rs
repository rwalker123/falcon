//! Route-section state: the roads in the ground, **one row per tile**.
//!
//! `docs/plan_standing_upkeep.md` §4.13b, issue #532. **A road is a per-TILE improvement**,
//! structurally identical to a forage patch — each tile carries its own rung, its own meter, its own
//! keeper and its own decay. It is not an edge between two bands, it does not follow a camp, and
//! there is deliberately **no stored path on this row**: a link knows its two endpoints, so the tiles
//! between them are computable.

use serde::{Deserialize, Serialize};

/// One road tile: where it is, where it stands on the route branch, whose job it is, what its keeping
/// costs, and what the rung it holds is buying.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RouteState {
    /// **The tile this road is on** — the row's identity, and what a client joins and diffs rows on.
    /// It replaced the retired `RouteId`: with one record per tile there is nothing left for a
    /// separate id to name.
    pub tile_x: u32,
    pub tile_y: u32,
    /// The rung this road **holds**, as `"<branch>:<id>"` (`RungKey::wire_key`). **This string is
    /// the bool** — a rung is never to be inferred from [`Self::build_fraction`], which is a
    /// different rung's meter.
    pub rung: String,
    /// The meter on the rung being **raised**, `0..=1`, off the same
    /// `intensification::rung_work_done` / `build_fraction` seam both food webs publish theirs from.
    /// Reads exactly `1.0` for a road that has just completed a rung and for one at the top of the
    /// ladder.
    pub build_fraction: f32,
    /// **Is this tile anybody's job?** `false` across the whole free floor — a game trail and a trail
    /// are formed by use and nobody keeps them, which is the commonest road in the game rather than
    /// an edge case. **Read this before [`Self::keeper_band_id`]**, whose `0` is a real band id.
    pub has_keeper: bool,
    /// The `BandId` of the band that graded or paved this tile and therefore keeps it. **One keeper
    /// per tile, never a share** — which is what makes *"one band keeps half the tiles between two
    /// camps and another the other half"* the representable state and co-payment the unrepresentable
    /// one.
    pub keeper_band_id: u64,
    /// **What distance did to this road's price**, as a multiple of the rung's own — quoted when the
    /// keeper took the tile on and held for the whole job. `1.0` inside the base range
    /// (`routes::road_keeping_range`) and on every road nobody keeps; above it beyond.
    ///
    /// Published because it is the only way a client can explain a bill that is larger than the rung
    /// says: **distance is a cost, never a wall**, and this is the cost.
    pub keeper_remoteness: f32,
    /// **The standing bill**, in work units per turn. All three read the **stamped** basis, so
    /// `demand − supplied == shortfall` holds verbatim on the wire.
    pub upkeep_demand: f32,
    pub upkeep_supplied: f32,
    pub upkeep_shortfall: f32,
    /// Whole `roadwork` keepers the bill wants — `ceil(demand / per-worker output)`.
    pub upkeep_workers_needed: u32,
    /// `false` = **nothing at risk here** (a road anywhere on the free floor, which declares no
    /// upkeep). Read this before the countdown beside it.
    pub has_neglect_grace: bool,
    /// The **countdown**, not the counter: turns of shortfall left before the rung bleeds, `0` =
    /// reverting now, and a kept road reads its rung's full grace + 1.
    pub neglect_grace_remaining: u32,
    /// **Is this road lighting its own tile right now** — the resolved answer, because a client
    /// cannot re-derive *"is the bill met"*. The fog it lifts is its **keeper's**.
    pub grants_sight: bool,
    /// The fraction of the base pooling friction a haul over **this tile** pays; `1.0` = no help. A
    /// journey's saving is the **mean** of its tiles' readings, so a partly-roaded run pays partly.
    pub friction_multiplier: f32,
    /// How far this tile's rung holds a pooling link open, in tiles. A journey's reach is the
    /// **weakest** tile it crosses — one gap breaks the run. **Authored and not yet consumed by the
    /// sim** (it is slice 13b's), and published anyway because it is half of the client's *"what this
    /// road buys"* line.
    pub holds_link_to_tiles: u32,
}
