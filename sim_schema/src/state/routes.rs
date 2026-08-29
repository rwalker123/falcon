//! Route-section state: the roads in the ground, one row per road.
//!
//! `docs/plan_standing_upkeep.md` §4.13, issue #532. **A road is a world object with a fixed tile
//! path and its own id** — it is not an edge between two bands and it does not follow a camp, so
//! there is deliberately **no band pair on this row**. The bands standing on it are who *uses* it,
//! never what it *is*.

use serde::{Deserialize, Serialize};

/// One road: where it runs, where it stands on the route branch, what its keeping costs, and what
/// the rung it holds is buying.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RouteState {
    /// The `RouteId`. Stable for the life of the road and never reused, so a client joins and diffs
    /// rows on it.
    pub id: u64,
    /// **The tiles this road runs over, in path order**, zipped `x`/`y` — the
    /// `pending_reveal_x`/`_y` convention one section over. Stamped once from the path the first
    /// traffic walked and never re-derived.
    pub path_x: Vec<u32>,
    pub path_y: Vec<u32>,
    /// The rung this road **holds**, as `"<branch>:<id>"` (`RungKey::wire_key`). **This string is
    /// the bool** — a rung is never to be inferred from [`Self::build_fraction`], which is a
    /// different rung's meter.
    pub rung: String,
    /// The meter on the rung being **raised**, `0..=1`, off the same
    /// `intensification::rung_work_done` / `build_fraction` seam both food webs publish theirs from.
    /// Reads exactly `1.0` for a road that has just completed a rung and for one at the top of the
    /// ladder.
    pub build_fraction: f32,
    /// **The standing bill**, in work units per turn. All three read the **stamped** basis, so
    /// `demand − supplied == shortfall` holds verbatim on the wire.
    pub upkeep_demand: f32,
    pub upkeep_supplied: f32,
    pub upkeep_shortfall: f32,
    /// Whole `roadwork` keepers the bill wants — `ceil(demand / per-worker output)`.
    pub upkeep_workers_needed: u32,
    /// `false` = **nothing at risk here** (a road holding only the game trail, which declares no
    /// upkeep). Read this before the countdown beside it.
    pub has_neglect_grace: bool,
    /// The **countdown**, not the counter: turns of shortfall left before the rung bleeds, `0` =
    /// reverting now, and a kept road reads its rung's full grace + 1.
    pub neglect_grace_remaining: u32,
    /// **Is this road lighting its own tiles right now** — the resolved answer, because a client
    /// cannot re-derive *"is the bill met"*.
    pub grants_sight: bool,
    /// The fraction of the base pooling friction a network bound by this road pays; `1.0` = no help.
    pub friction_multiplier: f32,
    /// How far this rung holds a pooling link open, in tiles. **Authored and not yet consumed by the
    /// sim** — it is slice 13b's — and published anyway because it is half of the client's *"what
    /// this road buys"* line.
    pub holds_link_to_tiles: u32,
}
