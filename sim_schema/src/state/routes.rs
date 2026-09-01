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
    /// **Is this tile anybody's job?** `false` across the whole free floor — a path and a trail
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
    /// **weakest** tile it crosses — one gap breaks the run. **Live**: `supply::balance_supply_networks`
    /// pools a pair within `max(reach_tiles, path_reach_tiles(..))`, so a road is what lets two camps
    /// pool at a distance where they simply cannot without one.
    pub holds_link_to_tiles: u32,
    /// **WHY THE POOL IS STUCK ON THIS TILE**, `""` when it is not — the same `BuildGate` vocabulary
    /// `ForagePatchState::build_blocked_reason` uses, one branch over.
    ///
    /// ⛔ **A ROAD IS A SOURCE ROW, AND THIS TABLE IS IT.** It was stated for years that a road *has
    /// no source row for an estimate to be stamped on*, and that claim outlived its truth: this row
    /// is keyed by tile exactly as a patch row is, so anything a patch publishes about the build in
    /// front of it a road can publish here. The claim is what made the material half of the route
    /// branch look impossible.
    ///
    /// The causes a road can carry are `"knowledge"` (the faction has not learned
    /// `roadbuilding`/`paving`), `"owned_by_other"` (another band keeps this tile, so this band's
    /// entry banks nothing — the remedy is a negotiation or another tile), `"no_keeper"` (**nobody**
    /// keeps it, so the tile is a job going begging — the remedy is to take it on, and re-issuing
    /// `grade`/`pave` adopts an unkept road) and `"materials"` (the rung's own gate **holds** and the
    /// store is what stopped it). **Empty is not "fine"** — a road nobody has queued is not blocked,
    /// it is simply not being built.
    ///
    /// ⛔ **`"owned_by_other"` AND `"no_keeper"` ARE NOT ONE CAUSE.** *"Another band owns this
    /// road"* said of a road nobody keeps is a **false sentence**: it sends the player looking for a
    /// rival that does not exist, past a road they could simply have claimed. A road loses its keeper
    /// without anybody deciding to drop it — decay or disuse takes the tile back below the free
    /// floor's top and the keeper is released — so the unkept state is the ordinary end of a road
    /// nobody walked rather than an edge case.
    #[serde(default)]
    pub build_blocked_reason: String,
    /// **THIS TURN'S SHARE OF THE RUNG'S BUILD PILE**, and what the band's stores actually paid of
    /// it — the material twin of [`Self::upkeep_demand`] / [`Self::upkeep_supplied`], obeying the
    /// same rule: **`demand − supplied` is the shortfall, verbatim on the wire.**
    ///
    /// The demand is what the accrual would draw *at full coverage*, spread over the rung's whole
    /// span, so a turn's share of `route:paved_road`'s twenty stone is small.
    /// [`crate::state::RouteRungState::build_material_cost`] is the whole pile.
    ///
    /// ⛔ **A SHORT STORE STALLS THE BUILD PROPORTIONALLY AND NEVER REFUSES IT.** The covered
    /// fraction scales **both** the work banked and the stone drawn, and the uncovered remainder is
    /// wasted rather than carried. A store with nothing in it blocks the head, which is the
    /// `"materials"` cause above. `0` on every rung that eats nothing.
    #[serde(default)]
    pub build_material_demand: f32,
    /// See [`Self::build_material_demand`].
    #[serde(default)]
    pub build_material_supplied: f32,
    /// **HOW LONG UNTIL THIS ROAD ARRIVES** — the **chained** countdown: everything above this entry
    /// in its band's queue, plus this entry's own span.
    ///
    /// ⛔ **THE SAME QUANTITY WITH THE SAME SENTINELS a patch and a herd publish**, through the same
    /// seam — there is deliberately no route dialect, so a client renders a road through the
    /// identical fork. [`crate::NO_BUILD_TURNS_ESTIMATE`] (`-1`), [`crate::BUILD_METER_HOLDS`]
    /// (`-2`), [`crate::BUILD_METER_ROTS`] (`-3`), [`crate::BUILD_QUEUE_BLOCKED`] (`-4`) and
    /// [`crate::BUILD_NOT_YET_ESTIMATED`] (`-5`); `>= 0` is a real count.
    ///
    /// ⛔ **ONLY A QUEUED ROAD HAS A REAL NUMBER.** A rung nobody has ordered has no quote, so it
    /// reads `-1` — never `0`, which would render as a finished build.
    ///
    /// **It is the fix for a road reading `Queued 97%` for 147 turns**: the sim published nothing
    /// here, so the client hardcoded the `-5` sentinel on every road queue model. Appended
    /// (append-only).
    #[serde(default = "no_build_turns_estimate")]
    pub build_turns_remaining: i32,
    /// **WHAT HOLDING THIS ROAD COSTS IN GOODS THIS TURN, AND WHAT THE STORES PAID OF IT** — the
    /// material twin of [`Self::upkeep_demand`] / [`Self::upkeep_supplied`], on the same rule:
    /// **`demand − supplied` is the shortfall, verbatim.** `0` on every rung that owes no material.
    ///
    /// ⛔ **THIS IS THE PAIR THAT SEPARATES *SHORT OF STONE* FROM *SHORT OF KEEPERS*.**
    /// `docs/plan_standing_upkeep.md` §2.7: *"you cannot mend a road with no stone, so a shortfall
    /// message that names the **pool** is wrong advice."* A reader must check both pairs and name
    /// whichever is short — the work pair points at the `roadwork` role, this one points at the
    /// stores, and only one of those two sentences helps at a time.
    ///
    /// **They do not double-count in the decay**: the rot rides the *worst* of the two fractions,
    /// never their sum, so a road short of both rots once at the worse rate. Appended
    /// (append-only).
    #[serde(default)]
    pub upkeep_material_demand: f32,
    /// See [`Self::upkeep_material_demand`].
    #[serde(default)]
    pub upkeep_material_supplied: f32,
}

/// The serde default of [`RouteState::build_turns_remaining`] — the *no estimate* sentinel, so a
/// road decoded from a document that predates the field reads *"the sim has no number"* rather than
/// the `0` a derived default would give, which renders as a finished build.
fn no_build_turns_estimate() -> i32 {
    crate::NO_BUILD_TURNS_ESTIMATE
}
