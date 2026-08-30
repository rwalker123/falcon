//! `routes` section -- the roads in the ground (arc #532, `.claude/rules/core_sim/routes.md`).
//!
//! ONE ROW PER ROAD. A road is a WORLD OBJECT with a fixed tile path and its own id: it is not an
//! edge between two bands and it does not follow a camp, so there is no band pair anywhere on this
//! row. The bands standing on it are who USES it, never what it IS.
//!
//! Already fog-filtered SIM-SIDE, and the gate is `Discovered` rather than the herd list's `Active`
//! -- a road does not wander off, so remembering one is remembering something true. A road on ground
//! nobody of yours has ever stood on does not reach the client at all.
//!
//! **THE BAND'S BILL IS NOT A SUM OF THESE ROWS.** `PopulationCohortState`'s `roadwork_demand` /
//! `roadwork_supplied` / `roadwork_shortfall` carry the band's own keeping, and they are the fields
//! a Work board reads: these rows are fog-filtered, so a road out of sight would silently drop out
//! of any client-side total while the band certainly still owes its keeping. The identical rule
//! `fodder_need` is minted under, for the identical reason.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

pub(crate) fn routes_to_array(list: Vector<'_, ForwardsUOffset<fb::RouteState<'_>>>) -> VarArray {
    let mut array = VarArray::new();
    for route in list {
        let mut dict = VarDictionary::new();
        // The `RouteId` -- stable for the life of the road and never reused, so a consumer may
        // cache the polyline against it and redraw only when the id is new.
        let _ = dict.insert("id", route.id() as i64);
        // THE TILES THIS ROAD RUNS OVER, IN PATH ORDER, zipped x/y (the `pendingRevealX`/`Y`
        // convention). Carried across as the two packed halves the wire states rather than zipped
        // into one array of pairs here: the consumer walks them by index, and a per-point Array
        // would allocate a Variant per tile of every road on the map, every frame.
        let _ = dict.insert(
            "path_x",
            &crate::dict::u32_vector_to_packed_int32(route.pathX()),
        );
        let _ = dict.insert(
            "path_y",
            &crate::dict::u32_vector_to_packed_int32(route.pathY()),
        );
        // **THE RUNG STRING IS THE BOOL -- never infer one from the float below.** `build_fraction`
        // is the meter on the rung being RAISED, which is a DIFFERENT rung; a consumer that
        // thresholded it would call a fully-worn trail a dirt road on the turn its first traffic
        // banked. `"route:game_trail"` | `"route:trail"` | `"route:dirt_road"` | `"route:paved_road"`.
        let _ = dict.insert("rung", route.rung().unwrap_or_default());
        // The meter on the rung being raised, 0..1 -- the route twin of `cultivation_progress`.
        // **NEVER DERIVED BY SUBTRACTION** sim-side, so a road that has just completed a rung reads
        // exactly `1.0` here, and so does one at the top of the ladder: draw a full bar, not an
        // empty one.
        let _ = dict.insert("build_fraction", f64::from(route.buildFraction()));
        // THE STANDING BILL, in work units per turn -- the road twin of the patch and herd rows'
        // identical four fields. **`demand - supplied == shortfall` HOLDS VERBATIM**, all three
        // reading the sim's stamped basis, so nothing here is re-derived by subtraction.
        // `upkeep_workers_needed` is the whole `roadwork` keepers that bill wants, and it is the
        // readout that makes a standing cost legible ("wants 2, you have 0").
        let _ = dict.insert("upkeep_demand", f64::from(route.upkeepDemand()));
        let _ = dict.insert("upkeep_supplied", f64::from(route.upkeepSupplied()));
        let _ = dict.insert("upkeep_shortfall", f64::from(route.upkeepShortfall()));
        let _ = dict.insert("upkeep_workers_needed", route.upkeepWorkersNeeded() as i64);
        // THE NEGLECT COUNTDOWN, NOT THE COUNTER: `0` means IT IS REVERTING NOW, and a road whose
        // bill is met reads its rung's full grace + 1. `has_neglect_grace == false` means there is
        // NOTHING AT RISK here -- a road holding only the game trail, which declares no upkeep and
        // so has no meter to lose. **Read the bool first**; the number reuses the "biting now" 0
        // rather than inventing a sentinel.
        let _ = dict.insert("has_neglect_grace", route.hasNeglectGrace());
        let _ = dict.insert(
            "neglect_grace_remaining",
            route.neglectGraceRemaining() as i64,
        );
        // **IS THIS ROAD LIGHTING ITS OWN TILES RIGHT NOW?** The RESOLVED answer, because a client
        // cannot re-derive "is the bill met" -- that is a comparison against the stamped basis with
        // the sim's own epsilon. A road in shortfall GOES DARK BEFORE IT DECAYS, which is the honest
        // early warning that it is being lost.
        let _ = dict.insert("grants_sight", route.grantsSight());
        // WHAT THIS RUNG IS BUYING, off the road's own stamped payoff -- and the half of this row
        // that makes the branch a ladder rather than a tax.
        //
        // `friction_multiplier` = the fraction of the base pooling friction a network bound by this
        //                         road pays. `1.0` = no help, which is the game trail.
        // `holds_link_to_tiles` = how far this rung holds a pooling link open, in tiles.
        //                         **AUTHORED AND NOT YET CONSUMED BY THE SIM** (slice 13b), so a
        //                         readout must state it as what the rung WILL hold, never as a live
        //                         effect. `0` on the game trail is a live reading, not a parked dial.
        let _ = dict.insert("friction_multiplier", f64::from(route.frictionMultiplier()));
        let _ = dict.insert("holds_link_to_tiles", route.holdsLinkToTiles() as i64);
        array.push(&dict.to_variant());
    }
    array
}
