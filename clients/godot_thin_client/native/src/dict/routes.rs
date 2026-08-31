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
//! ⛔ **THIS SECTION HAS CONSUMERS NOW, AND A CHANGE TO THE ROW SHAPE HAS TO VISIT THEM.** It read
//! *"no Godot script reads this section yet"* for one slice, which was true when the per-tile rebuild
//! landed and is a licence to change the row freely — exactly the wrong thing to leave behind. Four
//! readers, all joining on `tile_x`/`tile_y`:
//!
//!   * `MapView._ingest_road_network` -> `MapView.road_network` / `road_tile_lookup`, the world-state
//!     cache the other three read through;
//!   * `AnnotationRenderer.draw_road_network`, which stamps ONE HEX per row -- **never a polyline**,
//!     there being no stored path to draw;
//!   * `SubjectDrawerController._tile_terrain_lines`, the tile card's road block, via
//!     `MapView._tile_info_at`'s `roads` key;
//!   * `DrawerComposeController`'s road ladder (`RungLadder.route_track` / `RungGates.route_gates`),
//!     which reads `rung`, `build_fraction`, `keeper_remoteness` and the `has_keeper` /
//!     `keeper_band_id` pair to decide what a player may order on the tile.
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
        //                         **LIVE SINCE SLICE 13b**: `balance_supply_networks` forms a
        //                         pooling link at `distance <= max(reach_tiles, this reading)`, so a
        //                         readout states a live effect. `0` on the path is a live reading
        //                         too, not a parked dial.
        let _ = dict.insert("friction_multiplier", f64::from(route.frictionMultiplier()));
        let _ = dict.insert("holds_link_to_tiles", route.holdsLinkToTiles() as i64);
        // ⛔ **WHY THE POOL IS STUCK ON THIS TILE — a free-form CAUSE STRING, never an enum.** The
        // same `BuildGate` vocabulary a patch publishes, so one reader answers for both branches:
        // `"knowledge"`, `"owned_by_other"`, `"no_keeper"` and `"materials"`. **`""` is not *fine***
        // — it is *nothing is being built here*, which is a different sentence from *nothing is
        // wrong*.
        //
        // **A ROAD IS A SOURCE ROW.** This table is keyed by tile exactly as a patch row is, so the
        // claim that a road "has no source row for an estimate to be stamped on" is retired; it is
        // what made the material half of this branch look impossible.
        let _ = dict.insert(
            "build_blocked_reason",
            route.buildBlockedReason().unwrap_or_default(),
        );
        // …and THIS TURN'S DRAW against the band's stores — what full coverage would take, and what
        // the shelf actually paid of it. **`demand - supplied` IS the shortfall, verbatim**, the
        // same identity `upkeep_demand`/`upkeep_supplied` hold one block up, so nothing here
        // recomputes a third number.
        //
        // ⛔ **A SHORT STORE STALLS THE BUILD IN PROPORTION AND NEVER REFUSES IT.** The covered
        // fraction scales the work banked AND the stone drawn together, and the uncovered remainder
        // is WASTED rather than carried — so a stalled road banks *less*, never zero, and a readout
        // that drew it as a refusal would be describing a state the sim cannot produce. Only a shelf
        // with nothing on it at all blocks the head, and that arrives as `"materials"` above.
        //
        // **TWO FLOATS AND NO MATERIAL ID HERE** — the branch eats exactly one material per rung and
        // the RUNG row names it (`build_material_id`), so a per-tile copy would be a second place
        // for the same noun to be wrong in.
        let _ = dict.insert(
            "build_material_demand",
            f64::from(route.buildMaterialDemand()),
        );
        let _ = dict.insert(
            "build_material_supplied",
            f64::from(route.buildMaterialSupplied()),
        );
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
        // ⛔ **WHAT STANDING HERE TEACHES, AND IT IS THE GATE'S REMEDY.** `unlockKnowledge` above
        // says what a rung WAITS ON; this says what a rung EARNS, and the two are different rungs.
        // A gate reason has to name the rung that TEACHES the missing craft, which is emphatically
        // not the rung directly beneath the gated one -- the two coincide on the shipped four and
        // the config is free to break that pairing, at which point an inference would send the
        // player to stand on the wrong ground. `""` where the rung teaches nothing.
        let _ = dict.insert("earns_knowledge", rung.earnsKnowledge().unwrap_or_default());
        // ⛔ **WHAT ONE BARE-HANDED WORKER BANKS IN A TURN — the SIM'S rate, not the rung's.**
        // `intensification::PER_WORKER_OUTPUT`, unscaled: before gear and before any multiplier. **The
        // same figure for every rung**, which is exactly why it rides the CATALOG — the catalog is the
        // set of numbers that are identical for every road in the world, and a per-tile copy would
        // repeat it once per road on the map.
        //
        // ⛔ **IT IS DECODED BECAUSE A ROAD HAS NO SOURCE ROW TO CARRY ONE.** Every patch and herd
        // publishes its own `buildWorkPerWorkerTurn`; roads have no such row, so the client
        // TRANSCRIBED the sim's constant for a slice — which goes stale in silence the day the sim
        // writes worker output as a sum of more terms. A reader that finds this missing or `0` states
        // NO ESTIMATE rather than substituting a rate of its own; there is no fallback anywhere in
        // the client, and putting one back is the transcription returning through the side door.
        let _ = dict.insert(
            "build_work_per_worker_turn",
            f64::from(rung.buildWorkPerWorkerTurn()),
        );
        // ⛔ **THE RUNG'S DECLARED PILE — 20 stone on `route:paved_road`, `0` on every other rung of
        // the branch — AND IT IS FLAT WHERE `work_cost` ABOVE IS NOT.**
        //
        // ⛔⛔ **DO NOT PASS IT THROUGH `keeper_remoteness`.** The tile's own multiplier scales the
        // WORK span and does not touch this: a tile of road needs the same twenty stone wherever it
        // lies, and remoteness already taxes the getting there. The sim proves it by cancellation —
        // it quotes the leg at the scaled width and draws `pile × (accrual / width)`, so a whole
        // climb banks exactly `width` and swallows exactly `pile` at any distance. A remote road
        // draws its stone MORE SLOWLY, over more turns, never more of it. Scaling it here
        // over-quotes every remote road on the map and looks perfectly plausible while doing so.
        //
        // ⛔ **THE AMOUNT AND ITS NOUN ARE ONE READING — decoded together, read together.** `20` with
        // no word for it cannot be rendered into a sentence (the row said `+ 20 to raise it`, and
        // *twenty of what?* is the whole reason the id was appended), and a noun with no amount says
        // nothing. The sim resolves the pair from ONE lookup at capture so they cannot disagree.
        //
        // ⛔ **AND THE CLIENT MUST NOT SUPPLY THE NOUN ITSELF.** *The route branch eats stone* is a
        // fact about the CONFIG; a client holding it is a second authority that goes stale the day a
        // rung is retuned to eat something else — the transcription mistake
        // `build_work_per_worker_turn` above exists to have prevented.
        //
        // **ONE ID RATHER THAN A `MaterialPayoff` LIST**, unlike the plant and animal piles: one
        // material per rung IS the model here, `build_material_cost` being a single float, so a
        // second material would make the AMOUNT meaningless before the name mattered.
        //
        // **`""` WITH `0` BESIDE IT IS *THIS RUNG EATS NOTHING*** — every route rung but
        // `route:paved_road` — and never *a pile we cannot name*.
        let _ = dict.insert("build_material_cost", f64::from(rung.buildMaterialCost()));
        let _ = dict.insert(
            "build_material_id",
            rung.buildMaterialId().unwrap_or_default(),
        );
        array.push(&dict.to_variant());
    }
    array
}
