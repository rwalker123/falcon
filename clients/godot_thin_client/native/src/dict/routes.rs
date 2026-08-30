//! `routes` section -- the roads in the ground (arc #532, `.claude/rules/core_sim/routes.md`).
//!
//! ONE ROW PER ROAD **TILE**. A road is a PER-TILE improvement, structurally identical to a forage
//! patch: each tile carries its own rung, its own meter, its own keeper and its own decay. It is not
//! an edge between two bands and it does not follow a camp.
//!
//! **THERE IS NO STORED PATH ON THIS ROW, and that is the model.** A path object cannot be
//! half-maintained — there was no way to say "these people look after this end and those people look
//! after that end", which is the ordinary case the moment two camps sit at either end of a long
//! road. A link already knows its two endpoints, so the tiles between them are computable.
//!
//! ⛔ **THE GDScript SIDE HAS NOT BEEN RE-WRITTEN FOR THE PER-TILE SHAPE.** No Godot script reads
//! this section yet (there is no road drawn on the map and no `roadwork` row on the Work board), so
//! the rename below breaks nothing today — but a renderer written against it must join rows on
//! `tile_x`/`tile_y` and draw a **tile**, never a polyline.
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
        // **THE TILE IS THE ROW'S IDENTITY**, and it replaced the retired `RouteId`: with one record
        // per tile there is nothing left for a separate id to name. A consumer joins and diffs rows
        // on the pair.
        let _ = dict.insert("tile_x", route.tileX() as i64);
        let _ = dict.insert("tile_y", route.tileY() as i64);
        // **THE RUNG STRING IS THE BOOL -- never infer one from the float below.** `build_fraction`
        // is the meter on the rung being RAISED, which is a DIFFERENT rung; a consumer that
        // thresholded it would call a fully-worn trail a dirt road on the turn its first traffic
        // banked. `"route:path"` | `"route:trail"` | `"route:dirt_road"` | `"route:paved_road"`.
        let _ = dict.insert("rung", route.rung().unwrap_or_default());
        // The meter on the rung being raised, 0..1 -- the route twin of `cultivation_progress`.
        // **NEVER DERIVED BY SUBTRACTION** sim-side, so a road that has just completed a rung reads
        // exactly `1.0` here, and so does one at the top of the ladder: draw a full bar, not an
        // empty one.
        let _ = dict.insert("build_fraction", f64::from(route.buildFraction()));
        // WHOSE JOB THIS TILE IS. **Read `has_keeper` first**: `keeper_band_id` 0 is a real band id,
        // so the bool is the field that answers. `false` across the whole free floor — a path
        // and a trail are formed by use and nobody keeps them, which is the commonest road in the
        // game rather than an edge case. ONE KEEPER PER TILE, never a share: that is what makes "one
        // band keeps half the tiles between two camps and another the other half" representable.
        let _ = dict.insert("has_keeper", route.hasKeeper());
        let _ = dict.insert("keeper_band_id", route.keeperBandId() as i64);
        // WHAT DISTANCE DID TO THIS ROAD'S PRICE, as a multiple of the rung's own — quoted when the
        // keeper took the tile on. `1.0` inside the base keeping range and on every road nobody
        // keeps; higher beyond it. **Distance is a COST, never a wall**: no tile is refused for being
        // far away, and this multiplier is the only way to explain a bill larger than the rung says.
        let _ = dict.insert("keeper_remoteness", f64::from(route.keeperRemoteness()));
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
        // NOTHING AT RISK here -- a road holding only the path, which declares no upkeep and
        // so has no meter to lose. **Read the bool first**; the number reuses the "biting now" 0
        // rather than inventing a sentinel.
        let _ = dict.insert("has_neglect_grace", route.hasNeglectGrace());
        let _ = dict.insert(
            "neglect_grace_remaining",
            route.neglectGraceRemaining() as i64,
        );
        // **IS THIS ROAD LIGHTING ITS OWN TILE RIGHT NOW?** The RESOLVED answer, because a client
        // cannot re-derive "is the bill met" -- that is a comparison against the stamped basis with
        // the sim's own epsilon. A road in shortfall GOES DARK BEFORE IT DECAYS, which is the honest
        // early warning that it is being lost.
        let _ = dict.insert("grants_sight", route.grantsSight());
        // WHAT THIS RUNG IS BUYING, off the road's own stamped payoff -- and the half of this row
        // that makes the branch a ladder rather than a tax.
        //
        // `friction_multiplier` = the fraction of the base pooling friction a haul over THIS TILE
        //                         pays. `1.0` = no help. A journey's saving is the MEAN of the tiles
        //                         it crosses, so a partly-roaded run pays partly.
        // `holds_link_to_tiles` = how far this tile's rung holds a pooling link open, in tiles. A
        //                         journey's reach is the WEAKEST tile it crosses — one gap breaks a
        //                         link goods must get THROUGH.
        //                         **AUTHORED AND NOT YET CONSUMED BY THE SIM** (slice 13b), so a
        //                         readout must state it as what the rung WILL hold, never as a live
        //                         effect. `0` on the path is a live reading, not a parked dial.
        let _ = dict.insert("friction_multiplier", f64::from(route.frictionMultiplier()));
        let _ = dict.insert("holds_link_to_tiles", route.holdsLinkToTiles() as i64);
        array.push(&dict.to_variant());
    }
    array
}

/// **THE ROUTE BRANCH'S RUNG CATALOG** -- one row per rung of `intensification_ladder.json`'s route
/// branch, published ONCE PER WORLD beside `ladderKnowledge` and carrying no faction and no tile.
///
/// ⛔ **THIS IS WHAT LETS THE TILE CARD'S ROAD ACTION OPEN A LADDER RATHER THAN A BUTTON PER VERB.**
/// Every rung's name, its price, what it buys and what gates it is resolved sim-side, so a rung
/// added to that config appears as a row in the client with no client edit at all -- the same
/// property `ladder_knowledge_to_array` buys the knowledge screen, for the same reason.
///
/// **THE ROW ORDER IS THE CLIMB ORDER**, bottom rung first, and `order` carries it so a consumer
/// need not trust the vector's sequence to join a tile's standing to a position on the branch.
///
/// **THE THREE `""` FIELDS ARE STATES, NOT ABSENCES**: `verb` is empty on a rung nobody declares
/// (the free floor is worn in by traffic), `unlockKnowledge` on one nothing gates, `requiresRung` at
/// the branch's floor. A reader forks on each rather than treating it as missing data.
pub(crate) fn route_rungs_to_array(
    rungs: Vector<'_, ForwardsUOffset<fb::RouteRungState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for rung in rungs {
        let mut dict = VarDictionary::new();
        // The join key with a road tile's own `rung`, spelled `"<branch>:<id>"` exactly as
        // `RouteState.rung` spells it.
        let _ = dict.insert("rung_key", rung.rungKey().unwrap_or_default());
        let _ = dict.insert("order", rung.order() as i64);
        // "Dirt Road" -- resolved SIM-SIDE, so no client authors a second spelling of it. The
        // client's own `HudRouteVocab.RUNG_LABELS` is the tile card's four-rung readout table and
        // must never be read for a ladder row: a fifth rung would render as its raw wire key.
        let _ = dict.insert("display_name", rung.displayName().unwrap_or_default());
        // The TILE COMMAND that raises this rung, and `""` where the rung declares none.
        let _ = dict.insert("verb", rung.verb().unwrap_or_default());
        // The ladder knowledge that gates it, joining to `LadderKnowledgeState.knowledgeId`.
        let _ = dict.insert(
            "unlock_knowledge",
            rung.unlockKnowledge().unwrap_or_default(),
        );
        // ⛔ **THE RUNG DIRECTLY BENEATH, AND THIS IS WHY A ROAD CANNOT BE BUILT ON BARE GROUND.**
        // `route:dirt_road` requires `route:trail`, and a trail is reached only by traffic -- so
        // roads are upgraded where people already walk. It is also the CHAIN a client renders the
        // climb from without holding a second copy of the order.
        let _ = dict.insert("requires_rung", rung.requiresRung().unwrap_or_default());
        // **THE BASE PRICE, BEFORE THE TILE'S OWN REMOTENESS QUOTE.** `RouteState.keeperRemoteness`
        // is that multiplier and a readout states it as its own clause; a client that multiplied
        // the two would hold a copy of the sim's pricing formula, where it can drift.
        let _ = dict.insert("work_cost", f64::from(rung.workCost()));
        // ...and the standing bill, likewise BEFORE the tile's own load scales it
        // (`RouteState.upkeepDemand` is the resolved per-tile reading).
        let _ = dict.insert("upkeep_work_per_turn", f64::from(rung.upkeepWorkPerTurn()));
        // WHAT THE RUNG BUYS, the same three axes a built road's row carries -- read here as what
        // the rung WILL pay once it stands, rather than as a live effect.
        let _ = dict.insert("friction_multiplier", f64::from(rung.frictionMultiplier()));
        let _ = dict.insert("holds_link_to_tiles", rung.holdsLinkToTiles() as i64);
        // **THE RUNG'S OWN ANSWER, NOT A TILE'S.** `RouteState.grantsSight` is the RESOLVED
        // per-tile reading and goes dark while the keeping is unmet; this says whether the rung
        // lights its tiles at all once it stands and its bill is paid.
        let _ = dict.insert("grants_sight", rung.grantsSight());
        array.push(&dict.to_variant());
    }
    array
}
