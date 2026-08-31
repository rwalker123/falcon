//! **Roads** — the intensification ladder's third branch, and the first reader
//! `TerrainDefinition::infrastructure_cost` has ever had (`docs/plan_standing_upkeep.md` §4.13,
//! issue #532).
//!
//! Roads climb **path → trail → dirt road → paved road**. Each rung is *cheaper to travel and
//! dearer to keep*, which is deliberately **not** a straight upgrade path: you pave where the traffic
//! pays for the upkeep, and everywhere else a trail is the right answer for ever.
//!
//! # ⛔ A ROAD IS A **TILE** IMPROVEMENT, STRUCTURALLY IDENTICAL TO A FORAGE PATCH
//!
//! Ray: *"a road is a single tile improvement, not the entire path, so one band could maintain 1/2
//! the tile roads for the distance of a connection between two bands and another the other 1/2."*
//!
//! **A path object cannot be half-maintained.** The model this replaced stored a road as one object
//! holding a `Vec<UVec2>`, one ladder position and one keeping bill for the whole run, and there was
//! no way to write down *these people look after this end and those people look after that end* —
//! which is the ordinary case the moment two camps sit at either end of a long road.
//!
//! So each tile carries its own rung, its own meter, its own keeper and its own decay, held in a
//! registry keyed by tile ([`RoadRegistry`]) exactly as `forage::ForageRegistry` holds patches.
//! [`trace_path`] survives as a **function** — it is how a journey's tiles are worked out — and it is
//! no longer state.
//!
//! **Three objects were running together and only the third was removed:** a *connection* (two bands
//! know each other, `connections.rs`), a *logistics link* (goods move between them, `supply.rs`) and
//! a *road* (a built, maintained improvement on one tile). The stored path was a fourth thing that
//! nothing needs — a link already knows its two endpoints, so the tiles between them are computable.
//!
//! # ⛔ A ROAD IS IN THE GROUND. IT DOES NOT FOLLOW THE CAMP.
//!
//! Ray: *"How would a road follow a camp? That makes no sense and is in fact a factor in moving vs
//! staying. Roads can't follow camps."* **A road that follows its camp deletes a decision.** A road
//! you paid to build and pay to hold is one of the strongest reasons the game can give you to *stay*;
//! one that packs up and comes with you costs nothing to leave, so it can never weigh on
//! move/stay/fork — the pillar the whole project is built around.
//!
//! # WHO KEEPS A ROAD: THE BAND THAT BUILT IT, AND NOBODY ELSE
//!
//! 1. **A trail has no keeper**, and that is fine because there is nothing to keep. The free floor
//!    costs nothing, is formed by use and is lost to disuse.
//! 2. **`grade` / `pave` make the road that band's job** — the same act `cultivate` performs on a
//!    patch ([`Road::keeper`]). **One keeper, no shares**: the *"several bands each pay a part"*
//!    model is unrepresentable here rather than merely discouraged, which is what finally disposes
//!    of it.
//! 3. **Distance raises the cost; it never forbids the road.** There is no work-range rule — Ray:
//!    *"already forage and hunting have different work ranges, expeditions are even farther. I don't
//!    think it makes sense to restrict it."* What bounds a distant road is [`remoteness_multiplier`].
//!
//! **When the keeping band is gone** the road has no keeper, owes its bill to nobody and decays like
//! any unkept improvement. Re-issuing `grade` / `pave` is how another band picks it up — **no new
//! verb**, because adoption is the same act as building.
//!
//! # TRAFFIC PAYS FOR THE FLOOR, AND IT STOPS AT THE TOP OF IT
//!
//! *"A crew clears ground | **traffic wears the route in**."* That is true of the **free floor** and
//! of nothing else. A path and a trail declare no `verb`, append no `BuildQueueEntry` and draw
//! nothing from the builders' pool — traffic is the crew, and it banks work up to the top of
//! [`FREE_FLOOR_TOP_RUNG`] **and no further** ([`traffic_ceiling`]).
//!
//! **The cap is the load-bearing half, not where the line sits.** 13a billed `route:trail`, so two
//! camps sharing a larder wore a trail in by themselves and the band acquired a standing labour bill
//! it never opted into. Making the trail free without capping the climb only relocates that fault:
//! traffic would go on wearing a **dirt road** in for free and hand the player its bill anyway, one
//! rung later and dearer (`docs/plan_standing_upkeep.md` §4.13a).
//!
//! Above the cap a road is raised by `grade` and `pave` on the band's **builders** pool, exactly as a
//! Field or a pen is, and held out of its **`Roadwork`** pool, exactly as a Field is held out of
//! `Agriculture`.
//!
//! **Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in, so
//! *"what does it cost to raise this"* has one answer in one unit whichever branch is asked.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use sim_runtime::TerrainType;

use crate::{
    components::{BandId, Tile},
    grid_utils::{hex_distance_wrapped, hex_neighbor, HEX_DIRECTION_COUNT},
    intensification::{
        build_fraction, interpolate, neglect_grace_remaining, rung_work_done, upkeep_shortfall,
        upkeep_shortfall_fraction, LadderConfig, RungBranch, RungDef, RungKey, RungRoutePayoff,
        RungStanding, FRICTION_UNCHANGED, FULLY_SUPPLIED, NEGLECT_NONE, NO_CREW_ON_THIS_ACTIVITY,
        NO_RUNG_WORK_BANKED, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, RUNG_UNSTARTED,
    },
    orders::FactionId,
    resources::TileRegistry,
    terrain::terrain_definition,
};

// **RETIRED: `TRAILCRAFT_DISCOVERY_ID` (2011).** *Trailcraft* was taught by a path and gated
// `route:trail` — **a lesson for something you cannot fail to do.** You wear a path in by walking it;
// there is no knowing-how involved and no way to be refused, so the gate was open by the time
// anything could ask it (`docs/plan_standing_upkeep.md` §4.13a). **The id 2011 is retired, not
// reused**, and the two ids below are deliberately *not* renumbered down onto it: a gap is safer than
// a renumber, because a renumber silently re-points every start profile that already names one.
/// **NO MATERIAL ASKED FOR OR PAID THIS TURN** — the neutral of `Road::build_material_demanded` and
/// its supplied twin, named rather than a bare `0.0` because the pair is an identity on the wire
/// (`demand - supplied == shortfall`) and a zero there means *nothing was drawn*, never *nothing is
/// owed*. Every route rung but `route:paved_road` sits here permanently: they eat nothing.
pub const NO_MATERIAL_DRAWN: f32 = 0.0;

/// **Roadbuilding** — taught by a trail carrying traffic, gates `route:dirt_road` and its `grade`
/// verb. The first lesson on the branch that gates something a player actually decides.
pub const ROADBUILDING_DISCOVERY_ID: u32 = 2012;
/// **Paving** — taught by keeping a dirt road, gates `route:paved_road` and its `pave` verb.
pub const PAVING_DISCOVERY_ID: u32 = 2013;

/// ⛔ **THE TOP OF THE ROUTE BRANCH'S FREE FLOOR** — the highest rung traffic wears in by itself, and
/// the highest one that costs nothing to hold (`docs/plan_standing_upkeep.md` §4.13a).
///
/// The branch has the same two-part shape the other two webs have — `plant` is *wild* then
/// *tended · field*, `animal` is *wild* then *pastoral · pen*, and `route` is
/// **path · trail** then **dirt road · paved road**. Everything at or below this rung forms
/// from use, is lost to disuse and has **no keeper**; everything above it is ordered with a verb,
/// raised by the builders' pool, kept by one band and lost to unpaid keeping.
pub const FREE_FLOOR_TOP_RUNG: RungKey = RungKey::RouteTrail;

/// **THE FIRST RUNG SOMEBODY BUILDS AND SOMEBODY PAYS FOR** — the rung directly above
/// [`FREE_FLOOR_TOP_RUNG`], stated as its own constant because [`RungKey::above`] is not `const`.
/// `the_free_floor_and_the_first_built_rung_are_adjacent` pins the pair, so the two cannot drift.
pub const FIRST_BUILT_RUNG: RungKey = RungKey::RouteDirtRoad;

/// **No traffic this turn** — the neutral [`Road::traffic_work`] accumulates from.
pub const NO_TRAFFIC: f32 = 0.0;

/// **A ROAD TILE COSTS WHAT THE RUNG SAYS** — the remoteness multiplier inside
/// [`road_keeping_range`], and the value a road with no keeper carries.
pub const NEAR_ENOUGH_TO_KEEP: f32 = 1.0;

/// **THE BAND WHOSE JOB THIS ROAD IS.**
///
/// ⛔ **THERE IS NO OWNERSHIP HERE, AND THE WORD IS RETIRED FROM THIS ARC.** Ray: *"A road could have
/// no owner, but that is an abstract term in this game so it really has no meaning."* What this
/// records is a **job** — *these people look after this tile's road* — which is why `grade` and
/// `pave` write it and `abandon` clears it.
///
/// The faction rides beside the band because the two consumers ask different questions: the sight
/// grant lights the **faction's** fog, and the keeping payment is the **band's** pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadKeeper {
    pub faction: FactionId,
    pub band: BandId,
}

/// **A ROAD ON ONE TILE, and where it stands on the route branch.**
///
/// One position in cumulative work units, exactly as a patch and a herd carry one
/// (`docs/plan_standing_upkeep.md` §2.8) — the ladder position **is** the accumulator, so a
/// fractional turn's traffic banks as a fraction of a work unit and crosses a rung boundary when the
/// sum crosses it. **No separate traffic accumulator may be added**; that would be a second producer
/// of the same number.
#[derive(Debug, Clone)]
pub struct Road {
    /// The tile this road is on — its registry key, the plant web's `ForagePatch::tile` exactly.
    pub tile: UVec2,
    /// How far up the route branch this tile has been worked, in cumulative work units.
    position: f32,
    /// The derived standing, re-stamped on every write to [`Self::position`] so the two cannot drift
    /// — the `ForagePatch::standing` / `Herd::standing` convention.
    standing: RungStanding,
    /// **WHAT THIS ROAD BUYS, stamped beside the standing it is derived from.**
    ///
    /// Derived and re-stamped on every write to [`Self::position`], exactly as [`Self::standing`] is
    /// — so a reader asking *what is this tile worth* needs the tile and nothing else. That is what
    /// keeps the ladder out of `balance_supply_networks`: a supply pass that resolved the payoff
    /// would take a `LadderConfigHandle` to re-derive a number the road already knows, and every
    /// harness that stands the pooling up would have to hand it one.
    payoff: RungRoutePayoff,
    /// **THE BAND THIS ROAD IS THE JOB OF** — written by `grade` / `pave`, cleared by `abandon` and
    /// by a decay that takes the road back into the free floor. `None` across the whole free floor:
    /// **nobody keeps a trail**, which is the whole of what makes it free.
    pub keeper: Option<RoadKeeper>,
    /// **WHAT DISTANCE DID TO THIS ROAD'S PRICE**, as a multiple — [`remoteness_multiplier`] at the
    /// keeper band's distance, stamped **once, at the moment the keeper took the road on**.
    ///
    /// ⛔ **A QUOTE, NOT A LIVE READING**, and that is `ForagePatch::field_cost_multiplier`'s own
    /// discipline. It prices both the **build** (through [`road_rung_cost`], so a remote road is a
    /// bigger pile) and the **upkeep** (through [`road_upkeep_measure`], so it is dearer to hold);
    /// re-read live it would move the rung boundaries under a half-built road every time the band
    /// took a step, which is a second producer of a standing. **Re-issuing the verb re-prices it**,
    /// which is also how adoption works.
    pub keeper_remoteness: f32,
    /// **What this road's keeping was billed at, stamped once per turn, first-write-wins.** The wire
    /// states `demand − supplied == shortfall` verbatim, and an interpolated demand moves *within* a
    /// turn, so every reader must take the stamp rather than re-reading the live cost (§2.5).
    pub upkeep_demanded: Option<f32>,
    /// What this road's keeper supplied this turn. Cleared once per turn by the decay pass.
    ///
    /// **It accumulates (`+=`) even though there is exactly one keeper**, because the fund split
    /// hands a keeper's pool out claim by claim and the shape is the two food webs'. One keeper per
    /// tile is enforced by the verb, not by an assignment here.
    pub upkeep_supplied: f32,
    /// Consecutive turns of shortfall. Any turn the bill is met wipes it; it is not a lifetime budget.
    ///
    /// **`u16`, and the exact twin of `ForagePatch::neglect_turns` / `Herd::neglect_turns`** — it is
    /// handed straight to [`crate::intensification::RungDef::upkeep_decay`], which owns both the rate
    /// and the *strictly greater than the grace* comparison, so all three webs count in one unit.
    pub neglect_turns: u16,
    /// **Work units earned from traffic this turn**, banked into [`Self::position`] by the accrual
    /// pass and then cleared. A within-turn accumulator across the several journeys that may cross
    /// one tile, not persisted state in its own right.
    pub traffic_work: f32,
    /// **Consecutive turns this tile has carried no traffic** — the free floor's own neglect
    /// counter, and the exact twin of [`Self::neglect_turns`] one trigger over.
    ///
    /// ⛔ **THE FREE FLOOR NEEDS ITS OWN COUNTER BECAUSE IT CANNOT BE SHORT.** `route:path`
    /// and `route:trail` declare no `upkeep`, so their demand is [`NO_UPKEEP_DEMAND`], their
    /// shortfall is always zero and [`Self::neglect_turns`] can never arm on them. What takes a free
    /// road back is **disuse**, and this is what counts it.
    ///
    /// It is counted on **every** road, including the built ones, so a road that decays back down
    /// into the free floor arrives there with an honest reading rather than a zero that would buy it
    /// a second grace it has not earned.
    pub idle_turns: u16,
    /// **WHY THE POOL IS STUCK ON THIS TILE** — [`crate::intensification::BuildGate::Open`] when it
    /// is not stuck, which is also what a tile nobody has queued reads.
    ///
    /// ⛔ **A ROAD IS A SOURCE ROW AND THIS IS ITS BUILD SCRATCH**, the exact twin of
    /// `ForagePatch::build_blocked_reason`. It was believed for a long time that a road *had no
    /// source row for an estimate to be stamped on*; `RouteState` is keyed by tile precisely as a
    /// patch row is, and that mistaken claim is what made the material half of this branch look
    /// impossible to build.
    ///
    /// **Cleared at the top of every turn** by the decay pass, alongside `upkeep_demanded`, so a
    /// cause is a statement about *this* turn and a road that stops being blocked stops saying so.
    pub build_blocked_reason: crate::intensification::BuildGate,
    /// **THIS TURN'S SHARE OF THE BUILD PILE, AND WHAT THE STORE PAID OF IT** — the material twin of
    /// [`Self::upkeep_demanded`] / [`Self::upkeep_supplied`], and it holds the same identity on the
    /// wire: `demand − supplied` is the shortfall.
    ///
    /// **A total across materials rather than a map**, because the route branch eats exactly one
    /// (`route:paved_road`'s stone) and a per-material map would be a second vocabulary for a single
    /// row. Both are cleared with the keeping pair on the one-turn cycle.
    pub build_material_demanded: f32,
    /// See [`Self::build_material_demanded`].
    pub build_material_supplied: f32,
}

impl Road {
    /// A brand-new road tile at the branch's floor: a path, with work banked on nothing yet and
    /// nobody keeping it.
    pub fn worn_in(tile: UVec2, ladder: &LadderConfig) -> Self {
        Self {
            tile,
            position: RUNG_UNSTARTED,
            standing: road_standing_at(ladder, RUNG_UNSTARTED, NEAR_ENOUGH_TO_KEEP),
            payoff: road_payoff_at(ladder, RUNG_UNSTARTED, NEAR_ENOUGH_TO_KEEP),
            keeper: None,
            keeper_remoteness: NEAR_ENOUGH_TO_KEEP,
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            neglect_turns: NEGLECT_NONE,
            traffic_work: NO_TRAFFIC,
            idle_turns: NEGLECT_NONE,
            build_blocked_reason: crate::intensification::BuildGate::Open,
            build_material_demanded: NO_MATERIAL_DRAWN,
            build_material_supplied: NO_MATERIAL_DRAWN,
        }
    }

    /// Where this road stands on its branch. Read-only — [`Self::set_position`] is the one mutator.
    pub fn standing(&self) -> RungStanding {
        self.standing
    }

    /// The raw position, in cumulative work units.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// **THE ONE MUTATOR**, writing the position and its derived standing together so a caller cannot
    /// leave the pair disagreeing — `ForagePatch::set_ladder_position`'s rule.
    ///
    /// ⛔ **AND A ROAD THAT FALLS BACK INTO THE FREE FLOOR LOSES ITS KEEPER**, because there is
    /// nothing left to keep: the floor declares no `upkeep`, so a keeper held there would be a job
    /// with no work in it and a `Roadwork` row would go on naming a road that owes nothing. The test
    /// is **strictly below** the ceiling: a road sitting exactly on the top of the trail is the state
    /// a fresh `grade` leaves — keeper set, first work not yet banked — and clearing it there would
    /// undo the command on the turn it was typed.
    pub fn set_position(&mut self, position: f32, ladder: &LadderConfig) {
        self.position = position.max(RUNG_UNSTARTED);
        self.standing = road_standing_at(ladder, self.position, self.keeper_remoteness);
        self.payoff = road_payoff_at(ladder, self.position, self.keeper_remoteness);
        if self.position < traffic_ceiling(ladder) {
            self.release_keeper();
        }
    }

    /// **TAKE THIS ROAD ON** — what `grade` and `pave` do, and what adoption of a keeperless road is
    /// (there is deliberately no separate verb for the second: adoption is the same act as building).
    ///
    /// The remoteness is quoted here and held for the whole of the job — see
    /// [`Self::keeper_remoteness`]. Writing it re-resolves the standing, because the price of the
    /// rung being raised has just moved.
    pub fn take_keeper(&mut self, keeper: RoadKeeper, remoteness: f32, ladder: &LadderConfig) {
        self.keeper = Some(keeper);
        self.keeper_remoteness = remoteness;
        self.standing = road_standing_at(ladder, self.position, self.keeper_remoteness);
        self.payoff = road_payoff_at(ladder, self.position, self.keeper_remoteness);
    }

    /// **PUT THIS ROAD DOWN** — `abandon`'s whole effect on the route branch. The meter is untouched:
    /// the ground keeps whatever is on it and, with nobody keeping it, rots back down at the rung's
    /// own rate over the following turns exactly as an unkept improvement does.
    pub fn release_keeper(&mut self) {
        self.keeper = None;
        self.keeper_remoteness = NEAR_ENOUGH_TO_KEEP;
    }

    /// **What this road buys**, at the rung it holds — the stamped reading, never a re-derivation.
    pub fn payoff(&self) -> RungRoutePayoff {
        self.payoff
    }

    /// The rung this road **holds** — what it is entitled to in full.
    pub fn held_rung(&self) -> RungKey {
        self.standing.held
    }

    /// **Is this road one somebody BUILT and somebody keeps?** — `true` at [`FIRST_BUILT_RUNG`] and
    /// above, `false` across the whole free floor.
    ///
    /// **It is a rung test and not a `!= path` test.** A trail is worn in by traffic and costs
    /// nothing to hold, so it is exactly as free as the path beneath it — and the one thing
    /// hanging off this predicate, [`Self::grants_sight`], reasons from *"paying the upkeep IS the
    /// presence"*. A road nobody pays for has nobody on it, so a free trail must light nothing
    /// however worn it is.
    pub fn is_built(&self) -> bool {
        self.held_rung().is_at_or_above(FIRST_BUILT_RUNG)
    }

    /// ⛔ **DOES THIS ROAD LIGHT ITS OWN TILE?**
    ///
    /// **Yes, while it stands at a built rung and its keeping is met.** Ray: *"If a road exists and is
    /// maintained, the assumption is that there is traffic on it and it is seen."*
    ///
    /// The design pass first answered *no*, objecting that the commonest routed link is a pooling link
    /// where nobody physically walks. **That inference ran backwards.** Maintenance is not free — a
    /// kept road bills its keeper every turn out of the `Roadwork` pool, and what those hands are
    /// doing is being on the road. **Paying the upkeep IS the presence.**
    ///
    /// ⛔ **AND THIS IS WHY THE CONNECTION KEYSTONE DOES NOT BEND.** `connections.rs` states it as
    /// inviolable — *"Only presence makes a tile `Seen`. A connection can only ever grant
    /// `Discovered`."* — and names **logistics** as the first rider that will be tempted to break it.
    /// **This is not that temptation.** The sight is granted by the *road*, which is maintained
    /// presence on specific ground, and **never by the connection**. `core_sim/tests/connections.rs`
    /// passes unchanged.
    ///
    /// **The condition is the PAID BILL, not the held rung**, so a road in shortfall **goes dark
    /// before it decays** — the honest early warning that the road is being lost.
    pub fn grants_sight(&self) -> bool {
        self.is_built() && self.keeping_is_met()
    }

    /// Was this turn's keeping bill met? A road with no stamped bill has not been judged this turn and
    /// is treated as met — the same reading `Some(0.0)` gets, which is an honest *"owes nothing"*.
    pub fn keeping_is_met(&self) -> bool {
        match self.upkeep_demanded {
            Some(demand) => self.upkeep_supplied + KEEPING_EPSILON >= demand,
            None => true,
        }
    }

    /// **THE BILL THIS ROAD WAS HANDED THIS TURN** — the stamped demand, and [`NO_UPKEEP_DEMAND`]
    /// where nobody stamped one (§2.5). The plant web's `patch_keeping_basis` one branch over: every
    /// reader of the keeping takes the stamp, because an interpolated demand moves *within* a turn
    /// and the wire states `demand − supplied == shortfall` verbatim.
    pub fn upkeep_basis(&self) -> f32 {
        self.upkeep_demanded.unwrap_or(NO_UPKEEP_DEMAND)
    }

    /// What this turn's bill left unpaid, off the **stamped** basis (§2.5), never the live demand.
    pub fn upkeep_shortfall(&self) -> f32 {
        upkeep_shortfall(self.upkeep_basis(), self.upkeep_supplied)
    }
}

/// Slack on the keeping comparison, so a road funded to the last representable fraction of its bill is
/// not judged short by a rounding. The `f32` reason `RungStanding` asks `held` rather than comparing a
/// subtraction to a width.
const KEEPING_EPSILON: f32 = 1.0e-4;

/// **EVERY ROAD IN THE WORLD, ONE RECORD PER TILE** — `forage::ForageRegistry`'s exact shape one
/// branch over, and what replaced the stored-path ledger.
///
/// **`BTreeMap`, not `HashMap`** — the iteration order is observed by the snapshot and by the
/// checkpoint, so it has to be an order and not an accident. Keyed `(y, x)` so that order is
/// row-major, like every other tile sweep in the engine.
#[derive(Resource, Default, Debug, Clone)]
pub struct RoadRegistry {
    roads: BTreeMap<(u32, u32), Road>,
}

impl RoadRegistry {
    /// The road on this tile, or `None` where there has never been one.
    pub fn road(&self, tile: UVec2) -> Option<&Road> {
        self.roads.get(&(tile.y, tile.x))
    }

    pub fn road_mut(&mut self, tile: UVec2) -> Option<&mut Road> {
        self.roads.get_mut(&(tile.y, tile.x))
    }

    /// **The road on this tile, laying a path where there was none** — what traffic does the
    /// first time anything walks a tile.
    pub fn road_or_trail(&mut self, tile: UVec2, ladder: &LadderConfig) -> &mut Road {
        self.roads
            .entry((tile.y, tile.x))
            .or_insert_with(|| Road::worn_in(tile, ladder))
    }

    /// **Forget a road entirely** — what the prune does once a reverted road has nothing left on it.
    pub fn remove(&mut self, tile: UVec2) -> Option<Road> {
        self.roads.remove(&(tile.y, tile.x))
    }

    pub fn iter(&self) -> impl Iterator<Item = (UVec2, &Road)> {
        self.roads.values().map(|road| (road.tile, road))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Road> {
        self.roads.values_mut()
    }

    pub fn len(&self) -> usize {
        self.roads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.roads.is_empty()
    }

    /// **THE ROADS THIS BAND IS THE KEEPER OF** — the catchment the `Roadwork` pool pays for, in the
    /// registry's own row-major order.
    ///
    /// ⛔ **IT IS THE KEEPER, NOT WHO IS STANDING THERE.** The model this replaced billed every band
    /// camped on a road's path, which is how *"several bands each pay a share"* got in — a rule Ray
    /// rejected outright. One keeper per tile makes co-payment unrepresentable, and a band that walks
    /// away goes on paying for the road it took on, which is the whole point of a road being a thing
    /// you commit to rather than a thing you happen to be near.
    pub fn kept_by(&self, band: BandId) -> impl Iterator<Item = (UVec2, &Road)> {
        self.iter()
            .filter(move |(_, road)| road.keeper.is_some_and(|keeper| keeper.band == band))
    }
}

/// **THIS TURN'S TRAFFIC, recorded where it happens and spent where roads are worn.**
///
/// `balance_supply_networks` knows which pairs pooled and `crate::systems::advance_band_movement`
/// knows who marched; neither may also be the thing that lays roads. The pooling pass runs **before**
/// the accrual, and laying a road mid-pass would let this turn's pooling read a road this turn's
/// pooling created. So both write here and [`advance_roads`] spends it — the same producer/consumer
/// split `upkeep_supplied` uses across the Population→Logistics carry.
///
/// **Cleared by the accrual, every turn**, so a turn with no traffic wears nothing rather than
/// re-wearing last turn's journeys. A **march** therefore lands in the *next* turn's Logistics, being
/// recorded a stage later than a pooling link — banked exactly once either way, since this log has
/// one drain.
#[derive(Resource, Default, Debug, Clone)]
pub struct RouteTrafficLog {
    /// The journeys that carried traffic this turn. Unordered within a pair — a road has no
    /// direction — and duplicates are meaningful: two journeys over one tile are twice the traffic.
    pub journeys: Vec<RouteJourney>,
}

/// **ONE JOURNEY, AND WHAT EACH TILE OF IT EARNS.**
///
/// ⛔ **THE WEIGHT RIDES THE ENTRY SO THERE IS STILL EXACTLY ONE DRAIN AND ONE ACCRUAL LOOP.** Two
/// kinds of traffic bank at two rates — a standing pooling link per turn, a march per worker — and
/// the alternative to a weight here is two logs, two drains and two chances for one of them to be
/// forgotten by a third kind of traffic.
#[derive(Debug, Clone, Copy)]
pub struct RouteJourney {
    pub from: UVec2,
    pub to: UVec2,
    /// **What EACH TILE of this journey banks**, in the work units `RungBuild::work_cost` is quoted
    /// in — already resolved against the ladder at the moment the journey was recorded.
    pub work_per_tile: f32,
}

impl RouteTrafficLog {
    /// **Record one turn of a live pooling LINK between two camps** — `work_per_link_tile_per_turn`
    /// on each tile between them.
    ///
    /// ⛔ **A LINK IS NOT A HEADCOUNT**, which is why this rate is per link per turn where
    /// [`Self::marched`]'s is per worker: two camps pooling a larder are a **standing fact**, not a
    /// party of a countable size, and `balance_supply_networks` drops sub-`min_transfer` moves so a
    /// balanced network ships nothing at all. A mass term here is the error §4.13a ① already
    /// corrected.
    pub fn walked(&mut self, from: UVec2, to: UVec2, ladder: &LadderConfig) {
        self.record(from, to, ladder.route_traffic.work_per_link_tile_per_turn);
    }

    /// **Record a party MARCHING between two tiles** — `work_per_worker_tile × workers` on each tile
    /// it crossed, because a march **is** people and the boots are the whole of what wears the
    /// ground.
    ///
    /// Every travelling thing in the game reaches this through one hook
    /// (`systems::advance_band_movement`): a band moving camp, a scout, a hunt party and a **trade
    /// shipment** are all a `PopulationCohort` carrying a `BandTravel`, so *a shipment walking a
    /// connection* and *ordinary band movement* are one mechanism rather than two paths.
    pub fn marched(&mut self, from: UVec2, to: UVec2, workers: u32, ladder: &LadderConfig) {
        self.record(
            from,
            to,
            ladder.route_traffic.work_per_worker_tile * workers as f32,
        );
    }

    /// The one place a journey is appended — the `from != to` guard both recorders share, because a
    /// party that stood still walked nothing.
    fn record(&mut self, from: UVec2, to: UVec2, work_per_tile: f32) {
        if from != to {
            self.journeys.push(RouteJourney {
                from,
                to,
                work_per_tile,
            });
        }
    }
}

/// **WHAT A JOURNEY OVER THESE TILES LOSES IN TRANSIT, as a multiple of the base friction** — the
/// payoff `balance_supply_networks` reads, and the first thing a route rung has ever bought.
///
/// ⛔ **IT AVERAGES ALONG THE TILES, AND THAT IS THE PER-TILE MODEL'S OWN ANSWER.** You genuinely
/// lose less over the roaded stretch of a haul, so **a partly-built road pays partly**: half a run
/// roaded is about half the saving. A bare tile contributes [`FRICTION_UNCHANGED`], so the average can
/// never exceed it and improving any one tile can only lower it — **monotone-improving, which is the
/// additive guarantee the whole branch rests on** (*"a rung can only widen the set of links and lower
/// a loss, never the reverse"*).
///
/// **The earlier *"best road binding the network"* rule is dead.** It was an artifact of choosing
/// among independent path objects; reading a path of tiles, *best* would call a thirty-tile dirt road
/// with one paved tile a paved road.
///
/// ⛔ **THE FILTER IS [`Road::keeping_is_met`], AND IT IS NOT [`Road::grants_sight`].** The two
/// predicates answer different questions and only one of them is this payoff's.
///
/// `keeping_is_met` asks *"is this road being held up?"* — *"an unmaintained road is not carrying
/// anything"*, which is exactly what both payoffs want to exclude. **For the free floor the answer is
/// yes by arithmetic**: a path and a trail declare no `upkeep`, so nothing stamps a demand and the
/// shortfall is permanently zero. It is false only for a **built** road in shortfall.
///
/// `grants_sight` additionally asks *"is this a rung that lights ground at all?"*, which is a
/// **visibility** question resting on *paying the upkeep IS the presence*. Reading it here made the
/// whole free floor read as bare ground — a fully worn trail saved `0%` of the friction and held `0`
/// tiles open — against a config that declares `route:trail` worth `holds_link_to_tiles: 6` and
/// `friction_multiplier: 0.85`. **The free floor still BUYS something**, and it has to: `grade` is
/// gated on `roadbuilding`, which is learned from a connection only a road can hold open, so a trail
/// that extends nothing makes the whole branch unreachable.
pub fn path_friction_multiplier<'a>(
    registry: &RoadRegistry,
    path: impl IntoIterator<Item = &'a UVec2>,
) -> f32 {
    let mut tiles = 0u32;
    let mut total = 0.0f32;
    for tile in path {
        tiles += 1;
        total += registry
            .road(*tile)
            .filter(|road| road.keeping_is_met())
            .map_or(FRICTION_UNCHANGED, |road| road.payoff().friction_multiplier);
    }
    if tiles == 0 {
        return FRICTION_UNCHANGED;
    }
    total / tiles as f32
}

/// **HOW FAR THE ROADS ALONG THIS PATH HOLD A LINK OPEN, in tiles — THE WEAKEST TILE.**
///
/// ⛔ **A GAP BREAKS A LINK GOODS MUST GET THROUGH**, which is why reach takes the minimum where the
/// friction beside it takes the mean: losing a *fraction* of a haul over one rough tile is a real
/// thing, and *"the goods get most of the way there"* is not. One bare tile in an otherwise paved run
/// therefore holds nothing open, and that is the honest reading rather than a harsh one.
///
/// Monotone-improving for the same reason the average is: raising any tile can only raise the
/// minimum, so a road can never make a link worse.
///
/// **Read by `supply::balance_supply_networks`**, which pools a pair within
/// `max(reach_tiles, path_reach_tiles(..))` — so a road is what lets two camps pool at a distance
/// where they simply cannot without one.
///
/// ⛔ **THE FILTER IS [`Road::keeping_is_met`] AND NOT [`Road::grants_sight`]** — see
/// [`path_friction_multiplier`] for why the free floor has to pass it.
pub fn path_reach_tiles<'a>(
    registry: &RoadRegistry,
    path: impl IntoIterator<Item = &'a UVec2>,
) -> u32 {
    let mut reach: Option<u32> = None;
    for tile in path {
        let tile_reach = registry
            .road(*tile)
            .filter(|road| road.keeping_is_met())
            .map_or(NO_REACH_HELD_OPEN, |road| road.payoff().holds_link_to_tiles);
        reach = Some(reach.map_or(tile_reach, |held| held.min(tile_reach)));
    }
    reach.unwrap_or(NO_REACH_HELD_OPEN)
}

/// **A TILE THAT HOLDS NO LINK OPEN** — what bare ground and a path are both worth to
/// [`path_reach_tiles`], and the value an empty path answers.
pub const NO_REACH_HELD_OPEN: u32 = 0;

/// ⛔ **THE RUNG A CONNECTION OVER THESE TILES TEACHES — THE WEAKEST TILE, and `None` where the run
/// is broken.**
///
/// A tile carrying **no road at all**, or a road whose [`Road::keeping_is_met`] is false, breaks the
/// run: what you travel is the gap, so one path hex in the middle of a paved road means you do not
/// have a paved route. **This is the shipped asymmetry, one term over**: friction takes the *mean*
/// because loss accumulates per tile, reach takes the *minimum* because it is a claim about the whole
/// run holding — and **learning is the second kind**.
///
/// ⛔ ***"A path does not count"* IS NOT A SPECIAL CASE HERE.** `route:path` declares
/// `earns_knowledge: null`, so [`RungDef::route_knowledge_accrual`] answers `None` on its own —
/// exactly as the free floor owing no upkeep falls out of the arithmetic rather than an `is_built()`
/// guard. **Do not add a `path` branch.**
///
/// An empty run answers `None`: there is no connection to learn from.
pub fn path_lesson_rung<'a>(
    registry: &RoadRegistry,
    path: impl IntoIterator<Item = &'a UVec2>,
) -> Option<RungKey> {
    let mut weakest: Option<RungKey> = None;
    for tile in path {
        let held = registry
            .road(*tile)
            .filter(|road| road.keeping_is_met())
            .map(|road| road.held_rung())?;
        weakest = Some(match weakest {
            Some(held_so_far) if rung_rank(held_so_far) <= rung_rank(held) => held_so_far,
            _ => held,
        });
    }
    // `None` here is an EMPTY run and nothing else: a tile that failed the filter has already
    // returned above.
    weakest
}

/// **THE SCALE MEASURE — THIS TILE'S OWN GROUND, PRICED BY HOW FAR IT IS FROM ITS KEEPER.**
///
/// ```text
/// measure = infrastructure_cost(this tile's terrain) × keeper_remoteness
/// ```
///
/// **The length term is gone with the path object**, and what is left is the same *shape* the plant
/// web already reads: a patch scales its keeping on its tile's own `K`, and a road tile scales its
/// keeping on its tile's own `infrastructure_cost`. That is why `UpkeepScale::RouteSpan` collapsed
/// back into [`crate::intensification::UpkeepScale::SourceLoad`] with a per-branch reading — §4.11's
/// stated preference, *"one primitive with a per-branch reading beat a second variant"*.
///
/// **This is the one place `infrastructure_cost` is read**, so the bill, the quote, the decay and the
/// wire cannot answer three different geometries — the `forage::patch_land_capacity` rule.
///
/// **§2.7's line holds: the land is a SCALE term, not an offset.** It *multiplies* the demand; it
/// never subtracts from it.
pub fn road_upkeep_measure(terrain: TerrainType, remoteness: f32) -> f32 {
    terrain_definition(terrain).infrastructure_cost * remoteness
}

/// The ECS wrapper over [`road_upkeep_measure`], adding nothing but the tile lookup. A tile absent
/// from the registry measures nothing rather than defaulting to some terrain nobody chose.
pub fn road_measure(road: &Road, registry: &TileRegistry, tiles: &Query<&Tile>) -> f32 {
    registry
        .index(road.tile.x, road.tile.y)
        .and_then(|entity| tiles.get(entity).ok())
        .map_or(NO_UPKEEP_DEMAND, |tile| {
            road_upkeep_measure(tile.terrain, road.keeper_remoteness)
        })
}

/// ⛔ **HOW FAR A BAND KEEPS A ROAD AT THE RUNG'S OWN PRICE — READ THROUGH THIS FUNCTION, NEVER AS A
/// BARE CONFIG FIELD.**
///
/// Ray: *"Be flexible on the threshold… make it a function that can expand over time, don't just
/// create a hardcoded constant. You can have a configuration item for the 'base' range, but still
/// make a function accessor for it so we can calculate it later."*
///
/// So the config holds a **base** (`route_range.base_tiles`) and **every caller asks here** — the
/// build cost, the upkeep, the command's own quote and the wire. The day the range grows with
/// knowledge, faction size or a central authority (**issue #598**), that is *this function body*
/// changing and **no call site moving**. A `cfg.base_tiles` read scattered across four sites is four
/// places to find and three to miss, which is exactly why `fauna::herd_ecology` and
/// `forage::patch_land_capacity` are seams rather than field reads.
pub fn road_keeping_range(ladder: &LadderConfig) -> u32 {
    ladder.route_range.base_tiles
}

/// ⛔ **DISTANCE IS A COST, NEVER A WALL** — what a road costs its keeper as a multiple of the rung's
/// own price, at `distance` tiles from the band that took it on.
///
/// **There is no work-range rule and the reason is not complexity.** Ray: *"already forage and
/// hunting have different work ranges, expeditions are even farther. I don't think it makes sense to
/// restrict it."* A fourth arbitrary radius would say nothing; what bounds a distant road is that it
/// is dearer to hold and slower to build — the argument `TradeExpeditionConfig` already makes about
/// friction, *"what a long haul costs is already paid, and paid in the right currency."*
///
/// **A THRESHOLD, NOT A CURVE**, which is what Ray asked for and is simpler to tune: inside
/// [`road_keeping_range`] a road costs what the rung says; outside it both the build and the upkeep
/// rise by `route_range.remote_cost_multiplier`.
pub fn remoteness_multiplier(distance: u32, ladder: &LadderConfig) -> f32 {
    if distance <= road_keeping_range(ladder) {
        NEAR_ENOUGH_TO_KEEP
    } else {
        ladder.route_range.remote_cost_multiplier
    }
}

/// **WHAT THIS ROAD OWES EVERY TURN TO STAY WHERE IT IS**, in work units — the rung's own
/// `upkeep_demand`, *interpolated* on its standing and scaled by its [`road_upkeep_measure`].
///
/// It is `forage::patch_upkeep_demand`'s exact shape, with the tile's ground where the tender-loads
/// are: the rung owns the rate, the branch owns the scale measure. **Interpolated rather than read
/// off the held rung**, so a road part-way into a dirt road owes part of a dirt road.
///
/// ⛔ **THE FREE FLOOR FALLS OUT OF THE ARITHMETIC RATHER THAN BEING BRANCHED AROUND.** Neither free
/// rung declares an `upkeep`, so [`crate::intensification::RungDef::upkeep_demand`] answers
/// [`NO_UPKEEP_DEMAND`] for both and a road holding a trail owes nothing. An `is_built()` guard here
/// would be a second statement of *"nobody maintains a trail"*, free to disagree with the ladder that
/// already says it.
pub fn road_upkeep_demand(road: &Road, measure: f32, ladder: &LadderConfig) -> f32 {
    interpolate(&road.standing(), |rung| {
        ladder.rung(rung).upkeep_demand(measure)
    })
}

/// **THE BILL A CLAIM IS PRICED AT** — the **stamped** demand where this turn's keeping pass has
/// already struck one, and the live [`road_upkeep_demand`] where it has not.
///
/// `forage::patch_keeping_basis`' rule exactly, and for its reason: a claim and the bill it is
/// judged against must be one number, and an interpolated demand moves *within* a turn.
///
/// **The fallback is the only reading available to the SHED**, which counts a band's spare road
/// keepers against this bill inside `advance_labor_allocation` — a whole system before
/// [`crate::systems::settle_route_keeping`] stamps anything.
pub fn road_keeping_basis(road: &Road, measure: f32, ladder: &LadderConfig) -> f32 {
    road.upkeep_demanded
        .unwrap_or_else(|| road_upkeep_demand(road, measure, ladder))
}

/// **WHAT THIS ROAD'S METER LOSES THIS TURN** — the plant web's [`crate::forage::patch_meter_rot`],
/// one branch over, and the term a build countdown nets its supply against.
///
/// **It is the rot the DECAY PASS will actually apply**, resolved through the same three seams
/// `advance_roads` bleeds through — [`road_at_risk_rung`], the stamped bill
/// ([`road_keeping_basis`]) and the road's own `neglect_turns` — so a quote cannot promise a road
/// will finish while the pass takes more off it than the crew puts on.
///
/// [`crate::intensification::NO_UPKEEP_DECAY`] on the free floor, whose rungs declare no `upkeep` and
/// therefore can never be short: what takes a trail back is **disuse**, which is not a meter rot and
/// is not netted off anybody's build.
pub fn road_meter_rot(road: &Road, measure: f32, ladder: &LadderConfig) -> f32 {
    let rung = ladder.rung(road_at_risk_rung(&road.standing()));
    if rung.upkeep.is_none() {
        return crate::intensification::NO_UPKEEP_DECAY;
    }
    rung.meter_rot_against(
        road_keeping_basis(road, measure, ladder),
        road.upkeep_supplied,
        road.neglect_turns,
    )
}

/// **THE RUNG AT RISK ON THIS ROAD** — the newest rung carrying work, which is the rung a decay eats
/// and the rung whose grace and rot rate govern.
///
/// **One helper because three readers must agree**: the bill interpolates *through* it, the grace
/// lookup asks it how long neglect is forgiven, and [`advance_roads`] bleeds it. A road that billed
/// one rung and decayed another is exactly the drift `forage::patch_unwinding_key` exists to prevent
/// one branch over.
pub fn road_at_risk_rung(standing: &RungStanding) -> RungKey {
    standing
        .raising
        .filter(|_| standing.banked > NO_RUNG_WORK_BANKED)
        .unwrap_or(standing.held)
}

/// **THE METER ON THE RUNG THIS ROAD IS ACTUALLY RAISING**, `0..=1` — the route branch's twin of
/// `cultivationProgress` / `corralProgress`, and what `RoadState::buildFraction` publishes.
///
/// ⛔ **IT GOES THROUGH [`rung_work_done`], NEVER THROUGH A SUBTRACTION.** That seam answers a rung
/// the standing already holds with the rung's full `width` by construction rather than with
/// `fl(base + width) − base`, which is the rounding that published a completed Field at *"99%"*.
pub fn road_build_fraction(road: &Road, ladder: &LadderConfig) -> f32 {
    let standing = road.standing();
    let at_risk = road_at_risk_rung(&standing);
    let span = road_rung_span(at_risk, ladder, road.keeper_remoteness);
    build_fraction(
        rung_work_done(standing, at_risk, road.position(), span),
        span.1,
    )
}

/// **A meter with nothing left to raise** — what [`road_build_fraction`] answers at the top of the
/// branch, and the value [`build_fraction`] returns for a rung the standing already holds.
pub const METER_FULL: f32 = 1.0;

/// **HOW MANY WHOLE ROAD KEEPERS THIS ROAD'S BILL WANTS** — `ceil(basis / PER_WORKER_OUTPUT)`, the
/// route twin of `forage::patch_upkeep_workers_needed`.
pub fn road_upkeep_workers_needed(road: &Road, measure: f32, ladder: &LadderConfig) -> u32 {
    let demand = road_keeping_basis(road, measure, ladder);
    if demand <= NO_UPKEEP_DEMAND {
        return NO_CREW_ON_THIS_ACTIVITY;
    }
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

/// **HOW MANY MORE TURNS OF SHORTFALL THIS ROAD CAN ABSORB BEFORE IT BLEEDS** — the countdown, not
/// the counter, through [`crate::intensification::neglect_grace_remaining`] so all three webs and
/// the wire mean one thing by a grace.
///
/// **`None` = THERE IS NOTHING AT RISK HERE**, which is a road anywhere on the free floor: those
/// rungs declare no `upkeep`, so there is no grace to count and no meter to lose.
pub fn road_neglect_grace_remaining(road: &Road, ladder: &LadderConfig) -> Option<u32> {
    let rung = ladder.rung(road_at_risk_rung(&road.standing()));
    rung.upkeep.as_ref()?;
    Some(neglect_grace_remaining(
        road.neglect_turns,
        rung.upkeep_grace_turns(),
    ))
}

/// **What a route rung costs to raise on this road**, in work units.
///
/// **The free floor is never re-priced by distance**, and that is deliberate: traffic wears a trail
/// in, and traffic does not care how far anybody's camp is. Only the two rungs a band **orders** —
/// the ones [`FIRST_BUILT_RUNG`] and above — carry the remoteness quote, which is also what keeps
/// [`traffic_ceiling`] a fixed number the accrual can cap against.
pub fn road_rung_cost(rung: RungKey, ladder: &LadderConfig, remoteness: f32) -> Option<f32> {
    let multiplier = if rung.is_at_or_above(FIRST_BUILT_RUNG) {
        remoteness
    } else {
        NEAR_ENOUGH_TO_KEEP
    };
    ladder.rung(rung).build_cost(multiplier)
}

/// **What a road standing at `position` buys.** Read off the rung it *holds*, so a half-worn dirt
/// road buys exactly what the trail beneath it buys until the rung fills — the payoff is a property
/// of the road you have, not of the one you are wearing in.
pub fn road_payoff_at(ladder: &LadderConfig, position: f32, remoteness: f32) -> RungRoutePayoff {
    let held = road_standing_at(ladder, position, remoteness).held;
    *ladder
        .rung(held)
        .route_payoff
        .as_ref()
        .expect("validate requires a route_payoff on every route rung")
}

/// Resolve a road position through the ladder — the one seam that answers *where does this road
/// stand*, so no call site re-derives a standing from a meter.
pub fn road_standing_at(ladder: &LadderConfig, position: f32, remoteness: f32) -> RungStanding {
    RungStanding::at(ladder, RungBranch::Route, position, |rung| {
        road_rung_cost(rung, ladder, remoteness)
    })
}

/// **Where a route rung starts and how wide it is**, in cumulative work units — `route:trail` is
/// `(0, 40)`, `route:dirt_road` `(40, 110)` and `route:paved_road` `(150, 260)` on the shipped
/// ladder at [`NEAR_ENOUGH_TO_KEEP`].
pub fn road_rung_span(rung: RungKey, ladder: &LadderConfig, remoteness: f32) -> (f32, f32) {
    crate::intensification::rung_span(rung, &|key| road_rung_cost(key, ladder, remoteness))
}

/// ⛔ **THE POSITION TRAFFIC MAY BANK UP TO AND NO FURTHER** — the top of [`FREE_FLOOR_TOP_RUNG`]'s
/// span, `40` on the shipped ladder.
///
/// **This cap is what makes the free/paid line mean anything** (`docs/plan_standing_upkeep.md`
/// §4.13a rule 1). Moving `route:trail`'s bill off it without capping the climb only relocates the
/// fault: traffic would go on wearing a **dirt road** in for free, and the player would be handed a
/// standing labour bill for a road they never ordered.
///
/// It is read off the **ladder**, never written as a number, and the free floor takes no remoteness
/// quote — so this is one number for every road on the map.
pub fn traffic_ceiling(ladder: &LadderConfig) -> f32 {
    let (base, width) = road_rung_span(FREE_FLOOR_TOP_RUNG, ladder, NEAR_ENOUGH_TO_KEEP);
    base + width
}

/// **EVERY RUNG THE ROUTE BRANCH DECLARES, IN CLIMB ORDER** — the branch's catalog, read straight
/// off the config records.
///
/// ⛔ **IT WALKS `ladder.rungs` AND NOT [`RungKey::ALL`], DELIBERATELY.** The key enum names the
/// rungs a *system* reasons about; this answers *what does the config hold*, so a rung added to
/// `intensification_ladder.json` is in the catalog — and therefore on the wire and in the client's
/// ladder — with no code edit. Sorted by the record's own `order`, which the ladder validates as a
/// dense climb from `1`.
pub fn route_rungs_in_climb_order(ladder: &LadderConfig) -> Vec<&RungDef> {
    let mut rungs: Vec<&RungDef> = ladder
        .rungs
        .iter()
        .filter(|rung| rung.branch == RungBranch::Route)
        .collect();
    rungs.sort_by_key(|rung| rung.order);
    rungs
}

/// ⛔ **THE FURTHEST ANY ROUTE RUNG HOLDS A LINK OPEN — the widest distance the pooling test can
/// accept, AND NOBODY READS THE RUNG RECORDS FOR IT DIRECTLY.**
///
/// `supply::balance_supply_networks` bins its nodes into cells and scans a fixed neighbourhood of
/// them, under the invariant that pass states itself: *"the neighbourhood must be a superset of what
/// the distance test accepts, or pairs are dropped silently"*. Once a road can hold a link open at
/// `16` tiles, a cell size of `reach_tiles` breaks it — and it breaks as *some long roads just don't
/// work*, with nothing erroring anywhere.
///
/// So the binning asks **here**, exactly as the build cost, the upkeep and the wire ask
/// [`road_keeping_range`] rather than reading `route_range.base_tiles`: the day a rung is added or
/// its reach retuned, the neighbourhood follows it with **no call site moving**.
pub fn max_route_reach_tiles(ladder: &LadderConfig) -> u32 {
    route_rungs_in_climb_order(ladder)
        .iter()
        .filter_map(|rung| rung.route_payoff)
        .map(|payoff| payoff.holds_link_to_tiles)
        .max()
        .unwrap_or(NO_REACH_HELD_OPEN)
}

/// ⛔ **DOES A ROAD AT THIS RUNG LIGHT ITS OWN TILE?** — the *rung's* half of
/// [`Road::grants_sight`], which is that answer **and** this turn's paid bill.
///
/// **It asks whether the rung declares an `upkeep`, because paying the bill IS the presence** —
/// a road nobody pays for has nobody on it, so the free floor lights nothing however worn it is.
/// That is the same reading [`Road::is_built`] makes one step earlier: the free floor is exactly the
/// rungs that cost nothing to hold, pinned by
/// `the_free_floor_and_the_first_built_rung_are_adjacent`, so asking the **record** rather than
/// [`FIRST_BUILT_RUNG`] answers identically for the shipped ladder and keeps answering for a rung
/// the coded climb has never heard of.
pub fn rung_grants_sight(rung: &RungDef) -> bool {
    rung.upkeep.is_some()
}

/// **HOW GOOD THE GOING IS ON A TILE, as a rank the walk can compare** — [`NO_ROAD_HERE`] on bare
/// ground, and one more for each rung climbed from `route:path` up.
///
/// It walks the **coded** climb ([`RungKey::above`]) rather than the config's `order`, for the reason
/// [`RungKey::is_at_or_above`] does one function over: the caller is [`trace_path`], which takes no
/// ladder and must not start — a walk that needed a config handle would have to be threaded through
/// every harness that traces a journey.
fn rung_rank(rung: RungKey) -> u32 {
    let mut rank = NO_ROAD_HERE + 1;
    let mut cursor = Some(RungKey::RoutePath);
    while let Some(key) = cursor {
        if key == rung {
            return rank;
        }
        rank += 1;
        cursor = key.above();
    }
    // Unreachable for a road: `Road::held_rung` is always a `Route` rung, and every one of them is
    // on the climb walked above. A rung off this branch ranks as no road rather than as a panic.
    NO_ROAD_HERE
}

/// **A TILE WITH NOTHING ON IT** — the rank bare ground carries, and the floor [`rung_rank`] counts
/// up from.
const NO_ROAD_HERE: u32 = 0;

/// The rank of whatever this tile is **holding** — never the rung being raised on it, which is work
/// in progress and not something a traveller can walk on.
fn tile_rank(registry: &RoadRegistry, tile: UVec2) -> u32 {
    registry
        .road(tile)
        .map_or(NO_ROAD_HERE, |road| rung_rank(road.held_rung()))
}

/// **The tiles a journey between two tiles crosses** — a hex walk that greedily closes the distance
/// and, among the steps that close it equally, **takes the road**.
///
/// ⛔ **IT IS A FUNCTION AND NOT STATE**, which is the whole of what the per-tile model removed. A
/// link already knows its two endpoints, so the tiles between them are computable; storing them was
/// the fourth object nothing needed. Ray's phrasing is the right one — *"a route projected onto the
/// tiles"*: the **link** is the object, the **roads** are the ground it runs over.
///
/// # ⛔ NEW TRAFFIC PREFERS AN EXISTING ROAD, AND IT NEVER DETOURS TO REACH ONE
///
/// Only neighbours that **equally minimise** the remaining hex distance are compared on their roads,
/// so the hex distance still bounds the walk exactly: it cannot get longer, and it cannot loop. What
/// the preference buys is that a second journey between two camps runs over the road the first one
/// wore rather than beside it — the difference between a road forming and a smear of half-worn
/// tiles.
///
/// **The named limitation**: a road only helps a link where it lies along a *shortest* hex path
/// between the two camps. That is self-consistent rather than a gap — roads are worn in by traced
/// journeys in the first place, so they form on shortest paths.
///
/// ⛔ **IT READS NO TERRAIN, AND MUST NOT START.** Roads forming across a lake (#601) is blocked on
/// #282: two answers to *"can you cross this hex"* is worse than the bug.
///
/// Deterministic — the lowest-numbered direction among steps that tie on **both** distance and road
/// — wrap-aware through [`hex_neighbor`], and inclusive of both ends: a journey runs from the camp
/// to the camp.
pub fn trace_path(
    from: UVec2,
    to: UVec2,
    width: u32,
    height: u32,
    wrap: bool,
    roads: &RoadRegistry,
) -> Vec<UVec2> {
    let mut path = vec![from];
    let mut cursor = from;
    // The walk closes the distance by at least one step each iteration, so the hex distance bounds it
    // — no unbounded loop is reachable, and the guard is a belt for an unreachable tie.
    let mut budget = hex_distance_wrapped(from, to, width, wrap);
    while cursor != to && budget > 0 {
        let mut best: Option<(u32, u32, UVec2)> = None;
        for dir in 0..HEX_DIRECTION_COUNT {
            let Some((nx, ny)) = hex_neighbor(cursor.x, cursor.y, dir, width, height, wrap) else {
                continue;
            };
            let candidate = UVec2::new(nx, ny);
            let distance = hex_distance_wrapped(candidate, to, width, wrap);
            let rank = tile_rank(roads, candidate);
            // **Distance first, road second, direction last.** Replacing only on a strictly better
            // step is what leaves the lowest direction index holding a full tie.
            let better = best.is_none_or(|(held_distance, held_rank, _)| {
                distance < held_distance || (distance == held_distance && rank > held_rank)
            });
            if better {
                best = Some((distance, rank, candidate));
            }
        }
        let Some((_, _, step)) = best else { break };
        cursor = step;
        path.push(cursor);
        budget -= 1;
    }
    path
}

/// **TRAFFIC WEARS THE ROADS IN, AND NEGLECT WEARS THEM OUT** — the route branch's traffic accrual
/// and its decay pass in one system, the counterpart of `forage::advance_cultivation` and
/// `fauna::advance_husbandry` on the two food webs.
///
/// # ⛔ TWO DECAY TRIGGERS, BECAUSE A FREE RUNG CANNOT BE SHORT
///
/// - **The built rungs** (`dirt_road`, `paved_road`) revert on **unpaid keeping** — phases 1 and 2.
/// - **The free floor** (`path`, `trail`) costs nothing, so it can never be short: it reverts
///   on **DISUSE** — phase 5. That is `plan_contact_and_logistics.md` §Q4's own *"an unused road
///   reverts"*, which 13a collapsed into the shortfall path and left a free trail immortal.
///
/// **Each owns its own region of the position** and they do not overlap: disuse applies only inside
/// [`traffic_ceiling`], the shortfall bleed only above it.
///
/// # THE PHASES, IN THIS ORDER
///
/// 1. **Judge last turn's keeping**, off the **stamped** bill ([`Road::upkeep_basis`]) — consecutive
///    turns short, never a lifetime budget.
/// 2. **Bleed the rung at risk** ([`road_at_risk_rung`]) at `shortfall_fraction × meter_decay`, once
///    the neglect has outlasted that rung's own `grace_turns`.
/// 3. **Clear the bill** for the coming turn's stamp.
/// 4. **Bank this turn's traffic** on **every tile each journey crossed**, capped at
///    [`traffic_ceiling`] — the top of the free floor, and no further.
/// 5. **Bleed a free road nobody walked**, past `route_traffic.disuse_grace_turns`.
///
/// Then the registry is **pruned** of every road back at [`RUNG_UNSTARTED`] — after the banking,
/// because a tile first walked this turn is at the floor until its first traffic lands.
///
/// # ⛔ THE ONE-TURN CARRY IS THE ARRANGEMENT, NOT A DEFECT TO FIX
///
/// Logistics runs **before** Population, so the [`Road::upkeep_supplied`] phase 1 judges was stamped
/// by *last* turn's [`crate::systems::settle_route_keeping`] — the same lag
/// `forage::advance_cultivation` and `fauna::advance_husbandry` already run on. It runs in
/// `TurnStage::Logistics` **after `balance_supply_networks`**, which is what lets it see this turn's
/// links, and the *payoff* is therefore read at the standing as of the **previous** turn. **Do not
/// reorder a stage for it**, and do not let the supply pass raise a road: that would be a second
/// producer of a rung's position.
pub fn advance_roads(
    mut registry: ResMut<RoadRegistry>,
    mut traffic: ResMut<RouteTrafficLog>,
    ladder: Res<crate::intensification::LadderConfigHandle>,
    sim_config: Res<crate::resources::SimulationConfig>,
    tile_registry: Res<TileRegistry>,
) {
    let ladder = ladder.get();
    let (width, height) = (tile_registry.width, tile_registry.height);
    let wrap = sim_config.map_topology.wrap_horizontal;

    // ## Phases 1-3, over every road that already existed when this turn began.
    for road in registry.iter_mut() {
        // **1 — HOW SHORT, as a fraction of what was asked**, off the stamped basis and through the
        // ladder's own seam, so the three branches share one reading of *"how short"*. A road nobody
        // billed reads [`FULLY_SUPPLIED`] and is forgiven.
        let shortfall_fraction =
            upkeep_shortfall_fraction(road.upkeep_basis(), road.upkeep_supplied);
        if shortfall_fraction > FULLY_SUPPLIED {
            road.neglect_turns = road.neglect_turns.saturating_add(1);
        } else {
            road.neglect_turns = NEGLECT_NONE;
        }
        // **2 — THE BLEED, at the at-risk rung's own rate, past that rung's own grace.**
        // `upkeep_decay` owns the `>` that decides whether the penalty is biting, so nothing here
        // restates the grace comparison.
        let at_risk = road_at_risk_rung(&road.standing());
        let decay = ladder
            .rung(at_risk)
            .upkeep_decay(shortfall_fraction, road.neglect_turns);
        if decay > NO_UPKEEP_DECAY {
            let bled = road.position() - decay;
            road.set_position(bled, &ladder);
        }
        // **3 — the bill and this turn's payment, cleared on the one-turn cycle.** The BUILD's
        // material pair and its blocked cause clear with them and for the same reason: each is a
        // statement about the turn just resolved, and a stale one would leave a road that is
        // building perfectly well still saying it was short of stone.
        road.upkeep_demanded = None;
        road.upkeep_supplied = NO_UPKEEP_DEMAND;
        road.build_blocked_reason = crate::intensification::BuildGate::Open;
        road.build_material_demanded = NO_MATERIAL_DRAWN;
        road.build_material_supplied = NO_MATERIAL_DRAWN;
    }

    // ## Phase 4 — bank this turn's traffic, ON EVERY TILE THE JOURNEY CROSSED.
    //
    // Drained rather than read: this turn's traffic is spent once, and a turn with no pooling must
    // wear nothing rather than re-wearing last turn's links.
    //
    // ⛔ **THE RATE IS PER TILE, and under the per-tile model that is literal.** The stored-path
    // model banked `rate × path length` onto one object; here each tile a journey crosses banks the
    // journey's own weight, so a long haul wears many tiles a little rather than one object a lot —
    // which is what makes *"one band keeps half the tiles and another the other half"* a state the
    // traffic can actually produce. **The weight is the entry's**, resolved where the journey was
    // recorded: a pooling link's is `work_per_link_tile_per_turn`, a march's is
    // `work_per_worker_tile × workers`.
    //
    // ⛔ **EVERY PATH IS TRACED BEFORE ANY IS BANKED.** The trace reads the registry (it prefers an
    // existing road among equally good steps) and the banking writes it, so the two cannot be
    // interleaved — and doing so would also let the first journey of a turn lay the road the second
    // one then follows, which is a second producer of this turn's traffic.
    let journeys: Vec<(Vec<UVec2>, f32)> = std::mem::take(&mut traffic.journeys)
        .into_iter()
        .map(|journey| {
            (
                trace_path(journey.from, journey.to, width, height, wrap, &registry),
                journey.work_per_tile,
            )
        })
        .collect();
    for (path, work_per_tile) in journeys {
        for tile in path {
            registry.road_or_trail(tile, &ladder).traffic_work += work_per_tile;
        }
    }

    // **The position IS the accumulator** (§2.8), so the turn's traffic is banked straight onto it
    // and no second meter exists to disagree with it.
    //
    // ## ⛔ AND IT IS CAPPED AT THE TOP OF THE FREE FLOOR (§4.13a rule 1)
    //
    // A road already **above** the cap — one a band's builders graded or paved — is untouched: the
    // `max` is what keeps the cap from dragging a paved road back down to a trail every turn a link
    // runs over it, which would be a second producer of a position the builders' pool owns.
    let ceiling = traffic_ceiling(&ladder);
    for road in registry.iter_mut() {
        if road.traffic_work <= NO_TRAFFIC {
            // **Nothing walked here.** Consecutive idle turns, so a road that carried a journey last
            // turn and none this one starts its count from one.
            road.idle_turns = road.idle_turns.saturating_add(1);
            continue;
        }
        road.idle_turns = NEGLECT_NONE;
        let banked = (road.position() + road.traffic_work).min(ceiling.max(road.position()));
        road.traffic_work = NO_TRAFFIC;
        road.set_position(banked, &ladder);
    }

    // ## ⛔ THE SECOND DECAY TRIGGER — DISUSE, AND IT OWNS THE FREE FLOOR ALONE
    //
    // **A rung that costs nothing to hold cannot be short**, so the shortfall path above can never
    // reach `route:path` or `route:trail`. The loss is **FLAT rather than proportional**,
    // unlike the shortfall bleed: a bill can be partly paid, but traffic is a yes/no.
    //
    // It runs **after** the banking, because whether a road was idle is only known once this turn's
    // journeys have been drained onto it.
    let grace = ladder.route_traffic.disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    for road in registry.iter_mut() {
        if road.position() > ceiling {
            continue;
        }
        if u32::from(road.idle_turns) <= grace {
            continue;
        }
        let bled = road.position() - loss;
        road.set_position(bled, &ladder);
    }

    // ## ⛔ THE PRUNE, AND IT MUST COME AFTER THE BANKING
    //
    // **A path with no work in it is indistinguishable from no road at all** — it buys
    // nothing, lights nothing and owes nothing — so a registry that kept every tile ever walked
    // would grow without bound on reverted trails.
    //
    // **After the banking, because a tile first crossed THIS turn is at `RUNG_UNSTARTED` until its
    // traffic lands.** Pruning before phase 4 would delete every road on the turn it formed.
    //
    // Remembering that animals once walked there is **issue #215's concern, not this registry's**.
    let reverted: Vec<UVec2> = registry
        .iter()
        .filter(|(_, road)| road.position() <= RUNG_UNSTARTED)
        .map(|(tile, _)| tile)
        .collect();
    for tile in reverted {
        registry.remove(tile);
    }
}

/// ⛔ **A CONNECTION THAT STANDS TEACHES ROADBUILDING** — the route branch's earn pass, and what
/// makes the branch climbable at all.
///
/// Before this, nothing in the sim credited a route lesson: the only ladder credit was
/// `systems::labor::credit_rung_lesson`, whose reachable callers are all food-web arms. So
/// `roadbuilding` could not leave `0` by any means, `grade` was permanently refused, and
/// `dirt_road` / `paved_road` were unreachable.
///
/// # THE MODEL — A CONNECTION IS TWO BANDS WITHIN REACH OF EACH OTHER. DISTANCE ONLY.
///
/// Nothing about goods, nothing about factions, nothing about whether they have ever met. **A road
/// is what makes that reach bigger** — the same extended reach the pooling test uses,
/// `max(reach_tiles, path_reach_tiles(..))`.
///
/// ⛔ **THE UNIT OF LEARNING IS THE CONNECTION, NOT THE TILE.** Ray: *"only credit the 'connection',
/// so if a trail (or other road type) makes an unbroken connection between two bands … only then does
/// road building get learned. That fits our distance model perfectly, since the local connections are
/// shorter, they would contribute less."* Three things follow, and each is a reason:
///
/// - **It dissolves the *how often* question rather than answering it.** That question only bit while
///   a caravan was modelled as an occasional *journey* against a neighbour link's every-turn pooling.
///   A connection is a **standing** thing, so both are present every turn and there is no frequency
///   term left.
/// - **It kills a scaling bug a per-tile credit would have shipped**: a per-tile lesson scales with
///   tile count, so a wider map would teach roadbuilding faster for a reason no player caused.
/// - **Credit per turn while the connection stands, never once on completion.** A one-off is a step
///   function and pays twice for a connection that breaks and reforms.
///
/// **The lesson is the connection's WEAKEST tile** ([`path_lesson_rung`]) and the credit is
/// proportional to its **length in tiles** ([`RungDef::route_knowledge_accrual`]).
///
/// # ⛔ IT DOES ITS OWN O(n²) PASS AND MUST NOT BORROW THE SUPPLY LINK LIST
///
/// This is the thing a future reader will "simplify". `balance_supply_networks`' links carry a
/// **same-people** gate (`pools_freely`) and a **live-tie** gate (`tie_is_live`), and neither has
/// anything to do with whether two camps are connected by ground. Band counts are small; a pass of
/// its own is what keeps *"a connection is distance"* true.
///
/// The faction of **each endpoint band** is credited. With one people that is one credit and no
/// branch — `connections.rs`' discipline is that the code never asks, so the arrival of a second
/// people changes nothing here.
///
/// Runs in `TurnStage::Logistics` `.after(advance_roads)`, declared rather than left to the ambiguity
/// gate: it reads the road standing that pass has just produced.
// Every parameter is a distinct world resource or query this pass genuinely reads — the ECS's own
// signature, not a call site anyone types. The repo's convention for that is this allow, as on
// `supply::balance_supply_networks` and `terrain::def`.
#[allow(clippy::too_many_arguments)]
pub fn credit_route_lessons(
    registry: Res<RoadRegistry>,
    ladder: Res<crate::intensification::LadderConfigHandle>,
    supply: Res<crate::supply_network_config::SupplyNetworkConfigHandle>,
    sim_config: Res<crate::resources::SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    mut discovery: ResMut<crate::resources::DiscoveryProgressLedger>,
    bands: Query<&crate::components::PopulationCohort, With<crate::components::ResidentBand>>,
    tiles: Query<&Tile>,
) {
    // **No roads, no lesson** — the shipped turn-1 state, and the whole pass is skipped rather than
    // tracing a path between every pair of camps to be told there is nothing on it.
    if registry.is_empty() {
        return;
    }
    let ladder = ladder.get();
    let cfg = supply.get();
    let (width, height) = (tile_registry.width, tile_registry.height);
    let wrap = sim_config.map_topology.wrap_horizontal;
    // The free reach every pair has without a road, and the widest any rung can hold open — the
    // bound that says whether tracing this pair could possibly answer yes.
    let free_reach = cfg.reach_tiles;
    let widest = max_route_reach_tiles(&ladder).max(free_reach);

    let camps: Vec<(UVec2, FactionId)> = bands
        .iter()
        .filter_map(|cohort| {
            tiles
                .get(cohort.current_tile)
                .ok()
                .map(|tile| (tile.position, cohort.faction))
        })
        .collect();

    for (index, &(from, from_faction)) in camps.iter().enumerate() {
        for &(to, to_faction) in camps.iter().skip(index + 1) {
            let distance = hex_distance_wrapped(from, to, width, wrap);
            if distance > widest {
                continue;
            }
            let path = trace_path(from, to, width, height, wrap, &registry);
            if distance > free_reach.max(path_reach_tiles(&registry, &path)) {
                continue;
            }
            let Some(lesson_rung) = path_lesson_rung(&registry, &path) else {
                continue;
            };
            let Some((lesson, amount)) = ladder
                .rung(lesson_rung)
                .route_knowledge_accrual(path.len() as u32, &ladder.knowledge)
            else {
                continue;
            };
            // A set, so one people at both ends is one credit — without the code ever asking whether
            // the two ends are the same people.
            let taught: BTreeSet<FactionId> = [from_faction, to_faction].into_iter().collect();
            for faction in taught {
                discovery.add_progress(faction, lesson, crate::scalar::scalar_from_f32(amount));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intensification::UpkeepScale;

    /// The four route rungs, bottom to top.
    const ROUTE_RUNGS: [RungKey; 4] = [
        RungKey::RoutePath,
        RungKey::RouteTrail,
        RungKey::RouteDirtRoad,
        RungKey::RoutePavedRoad,
    ];

    fn dirt_road_top(ladder: &LadderConfig) -> f32 {
        let (base, width) = road_rung_span(RungKey::RouteDirtRoad, ladder, NEAR_ENOUGH_TO_KEEP);
        base + width
    }

    /// Seat a kept dirt road on `tile`, with a bill that is met — the state every payoff claim below
    /// is read at, because `grants_sight` gates both of them on the paid bill.
    fn seat_a_kept_dirt_road(registry: &mut RoadRegistry, tile: UVec2, ladder: &LadderConfig) {
        let top = dirt_road_top(ladder);
        let road = registry.road_or_trail(tile, ladder);
        road.set_position(top, ladder);
        road.upkeep_demanded = Some(NO_UPKEEP_DEMAND);
    }

    /// **Seat a fully worn TRAIL on `tile`** — the free floor's top, nobody's job, owing nothing.
    ///
    /// It stamps no bill, and that is the whole of what makes it kept: a rung declaring no `upkeep`
    /// has nothing to be short of, so `keeping_is_met` is `true` by arithmetic rather than by
    /// anybody paying.
    fn seat_a_trail(registry: &mut RoadRegistry, tile: UVec2, ladder: &LadderConfig) {
        let ceiling = traffic_ceiling(ladder);
        let road = registry.road_or_trail(tile, ladder);
        road.set_position(ceiling, ladder);
        assert_eq!(
            road.held_rung(),
            FREE_FLOOR_TOP_RUNG,
            "precondition: the free floor's top rung is what a fully worn trail holds"
        );
    }

    /// What a route rung's record says it buys — read off the ladder, never restated as a literal.
    fn payoff_of(rung: RungKey, ladder: &LadderConfig) -> RungRoutePayoff {
        ladder
            .rung(rung)
            .route_payoff
            .expect("every route rung declares a payoff")
    }

    /// ⛔ **THE LIVENESS CLAIM EVERY OTHER TEST IN THIS FILE RESTS ON.** Without it, a branch that
    /// silently failed to load would leave every assertion below passing over an empty ladder.
    #[test]
    fn the_shipped_ladder_carries_four_route_rungs_and_a_position_climbs_them() {
        let ladder = LadderConfig::builtin();

        for (index, key) in ROUTE_RUNGS.iter().enumerate() {
            let rung = ladder.rung(*key);
            assert_eq!(rung.branch, RungBranch::Route, "{key:?} is a route rung");
            assert_eq!(
                rung.order as usize,
                index + 1,
                "{key:?}'s record sits where the coded climb puts it"
            );
        }

        // The floor is held at zero work, costs nothing, and is nobody's to keep.
        let floor = road_standing_at(&ladder, RUNG_UNSTARTED, NEAR_ENOUGH_TO_KEEP);
        assert_eq!(floor.held, RungKey::RoutePath);
        // ⛔ **THE FLOOR'S WIRE KEY IS PINNED AS A LITERAL, DELIBERATELY.** Every other rung
        // assertion in the workspace compares against `wire_key()` so a hand-typed string cannot
        // drift; here the client reads `"route:path"` as a literal of its own, so one side has to
        // state the spelling or the two can drift apart in silence.
        assert_eq!(RungKey::RoutePath.wire_key(), "route:path");
        assert_eq!(floor.raising, Some(RungKey::RouteTrail));
        assert!(
            ladder.rung(RungKey::RoutePath).build.is_none()
                && ladder.rung(RungKey::RoutePath).upkeep.is_none(),
            "NOBODY MAINTAINS A PATH — that is the whole of what makes the floor free"
        );

        // And a position really climbs: banking every rung's work reaches the top of the branch.
        let all_the_work: f32 = ROUTE_RUNGS
            .iter()
            .filter_map(|key| ladder.rung(*key).build.as_ref())
            .map(|build| build.work_cost)
            .sum();
        let top = road_standing_at(&ladder, all_the_work, NEAR_ENOUGH_TO_KEEP);
        assert_eq!(
            top.held,
            RungKey::RoutePavedRoad,
            "banking every rung's work reaches the top of the branch"
        );
        assert_eq!(top.raising, None, "there is nothing above a paved road");
    }

    /// ⛔ **THE FREE FLOOR AND THE FIRST BUILT RUNG ARE ADJACENT, AND THE FLOOR IS EXACTLY THE RUNGS
    /// THAT COST NOTHING TO HOLD — AND THE ONES NO VERB RAISES.**
    ///
    /// The verb half is what replaced `RungBranch::is_crew_built`: *"does this rung declare a
    /// `verb`"* answers the same question at the correct grain, now that the branch is no longer
    /// uniformly crew-free.
    #[test]
    fn the_free_floor_and_the_first_built_rung_are_adjacent() {
        assert_eq!(
            FREE_FLOOR_TOP_RUNG.above(),
            Some(FIRST_BUILT_RUNG),
            "the first built rung is the one directly above the floor's top — no gap, no overlap"
        );

        let ladder = LadderConfig::builtin();
        let mut rung = RungBranch::Route.root_rung();
        loop {
            let free = !rung.is_at_or_above(FIRST_BUILT_RUNG);
            assert_eq!(
                ladder.rung(rung).upkeep.is_none(),
                free,
                "{} is {} the coded free floor, so it must {} an `upkeep`",
                rung.wire_key(),
                if free { "inside" } else { "above" },
                if free { "declare no" } else { "declare" }
            );
            assert_eq!(
                ladder.rung(rung).verb.is_none(),
                free,
                "{} is {} the coded free floor, so it must {} a verb",
                rung.wire_key(),
                if free { "inside" } else { "above" },
                if free { "declare no" } else { "declare" }
            );
            match rung.above() {
                Some(next) => rung = next,
                None => break,
            }
        }

        // And the cap really is the top of that floor, in the position units traffic banks in.
        let (base, width) = road_rung_span(FREE_FLOOR_TOP_RUNG, &ladder, NEAR_ENOUGH_TO_KEEP);
        assert_eq!(
            traffic_ceiling(&ladder),
            base + width,
            "traffic stops at the top of the free floor's own span, read off the ladder"
        );
    }

    /// ⛔ **THE MEASURE IS THIS TILE'S OWN GROUND, PRICED BY DISTANCE — AND IT IS NOT A LENGTH.**
    ///
    /// The path object's `length × terrain` sum went with the path. What is left is the same shape
    /// the plant web reads: one tile, its own ground, times what distance did to the price.
    #[test]
    fn the_measure_is_the_tiles_own_ground_times_what_distance_did_to_it() {
        let valley = road_upkeep_measure(TerrainType::AlluvialPlain, NEAR_ENOUGH_TO_KEEP);
        let range = road_upkeep_measure(TerrainType::AlpineMountain, NEAR_ENOUGH_TO_KEEP);

        assert!(
            (valley - 0.9).abs() < 1.0e-5,
            "a tile of alluvial plain measures its own infrastructure_cost, got {valley}"
        );
        assert!(
            range > valley,
            "a road over a range costs more to hold than one down a river valley ({range} vs \
             {valley}) — the per-terrain answer terrain.rs carried unread for 37 biomes"
        );

        let remote = road_upkeep_measure(TerrainType::AlluvialPlain, 2.0);
        assert!(
            (remote - 2.0 * valley).abs() < 1.0e-5,
            "and the remoteness quote is a SCALE term on it, never an offset (§2.7)"
        );
    }

    /// ⛔ **THE SCALE PRIMITIVE COLLAPSED**: `RouteSpan` retired into `SourceLoad` with a per-branch
    /// reading, which is §4.11's stated preference. Every rung that owes anything now names one
    /// measure, and the route branch supplies its own reading of it.
    #[test]
    fn every_rung_that_owes_anything_scales_on_the_one_scale_primitive() {
        let ladder = LadderConfig::builtin();
        let mut route_rungs_with_upkeep = 0;
        for rung in &ladder.rungs {
            let Some(upkeep) = rung.upkeep.as_ref() else {
                continue;
            };
            assert_eq!(
                upkeep.scaled_by,
                UpkeepScale::SourceLoad,
                "{}:{} scales on a measure that no longer exists",
                rung.branch.as_str(),
                rung.id
            );
            if rung.branch == RungBranch::Route {
                route_rungs_with_upkeep += 1;
            }
        }
        assert_eq!(
            route_rungs_with_upkeep, 2,
            "the two built route rungs are the ones that owe — the liveness half of the claim above"
        );
    }

    /// The bill is the rung's rate times the tile's own measure — one rule, and the whole reason a
    /// valley road is cheap to keep and a mountain road is dear.
    ///
    /// **It is asked of the DIRT ROAD, because the trail beneath it is free.**
    #[test]
    fn the_keeping_bill_is_the_rungs_rate_times_the_tiles_measure() {
        let ladder = LadderConfig::builtin();
        let road = ladder.rung(RungKey::RouteDirtRoad);
        let rate = road
            .upkeep
            .as_ref()
            .expect("the dirt road is the first rung anybody keeps")
            .work_per_turn;

        let valley = road_upkeep_measure(TerrainType::AlluvialPlain, NEAR_ENOUGH_TO_KEEP);
        let range = road_upkeep_measure(TerrainType::AlpineMountain, NEAR_ENOUGH_TO_KEEP);

        assert!(
            (road.upkeep_demand(valley) - rate * valley).abs() < 1.0e-5,
            "the demand is rate × measure"
        );
        assert!(
            road.upkeep_demand(range) > road.upkeep_demand(valley),
            "the same road costs more to hold over a range than down a valley"
        );
        for free in [RungKey::RoutePath, RungKey::RouteTrail] {
            assert_eq!(
                ladder.rung(free).upkeep_demand(range),
                NO_UPKEEP_DEMAND,
                "{} costs nothing to hold over ANY country — the free floor is formed by use and \
                 nobody keeps it",
                free.wire_key()
            );
        }
    }

    /// ⛔ **DISTANCE IS A COST AND IT IS READ THROUGH THE SEAM.** One config edit moves the seam's
    /// answer, and **both** the upkeep and the build pile move with it — which is what a bare
    /// `cfg.base_tiles` read at one call site could not do.
    #[test]
    fn a_road_beyond_the_range_costs_more_to_build_and_more_to_hold() {
        let ladder = LadderConfig::builtin();
        let base = road_keeping_range(&ladder);

        assert_eq!(
            remoteness_multiplier(base, &ladder),
            NEAR_ENOUGH_TO_KEEP,
            "a road at the edge of the base range costs exactly what the rung says"
        );
        let remote = remoteness_multiplier(base + 1, &ladder);
        assert!(
            remote > NEAR_ENOUGH_TO_KEEP,
            "and one tile past it costs more — a THRESHOLD, not a curve"
        );

        // The upkeep half.
        let near = road_upkeep_measure(
            TerrainType::AlluvialPlain,
            remoteness_multiplier(base, &ladder),
        );
        let far = road_upkeep_measure(
            TerrainType::AlluvialPlain,
            remoteness_multiplier(base + 1, &ladder),
        );
        let dirt = ladder.rung(RungKey::RouteDirtRoad);
        assert!(
            dirt.upkeep_demand(far) > dirt.upkeep_demand(near),
            "the same road is dearer to hold when its keeper is far from it"
        );

        // The build half — the same seam, the same edit.
        let near_cost = road_rung_cost(RungKey::RouteDirtRoad, &ladder, NEAR_ENOUGH_TO_KEEP)
            .expect("a dirt road is something to build");
        let far_cost = road_rung_cost(RungKey::RouteDirtRoad, &ladder, remote)
            .expect("a dirt road is something to build");
        assert!(
            far_cost > near_cost,
            "and slower to build: {far_cost} against {near_cost}"
        );

        // ⛔ **AND THE FREE FLOOR IS NEVER RE-PRICED**, which is what keeps the traffic cap one
        // number for every road on the map.
        assert_eq!(
            road_rung_cost(RungKey::RouteTrail, &ladder, remote),
            road_rung_cost(RungKey::RouteTrail, &ladder, NEAR_ENOUGH_TO_KEEP),
            "traffic wears a trail in and traffic does not care how far anybody's camp is"
        );
    }

    /// ⛔ **A ROAD LIGHTS ITS TILE ONLY WHILE IT IS BUILT *AND* KEPT** — and all three states are
    /// asserted, because any two of them pass with the third condition dropped.
    #[test]
    fn a_road_lights_its_tile_only_while_it_is_built_and_kept() {
        let ladder = LadderConfig::builtin();
        let mut road = Road::worn_in(UVec2::new(2, 2), &ladder);

        // ① The floor. Nobody maintains a path, so it lights nothing.
        assert!(!road.is_built());
        assert!(
            !road.grants_sight(),
            "a PATH grants no sight — it is free precisely because nobody keeps it"
        );

        // ①b **AND NEITHER DOES A FULLY WORN TRAIL** (§4.13a).
        road.set_position(traffic_ceiling(&ladder), &ladder);
        assert_eq!(road.held_rung(), FREE_FLOOR_TOP_RUNG);
        assert!(!road.is_built(), "the whole free floor is unbuilt");
        road.upkeep_demanded = Some(NO_UPKEEP_DEMAND);
        assert!(
            road.keeping_is_met(),
            "precondition: a free trail owes nothing, so its bill is trivially met"
        );
        assert!(
            !road.grants_sight(),
            "a TRAIL grants no sight either — it costs nothing to hold, so nobody is on it keeping \
             it, and the sight grant reasons from the PAID BILL"
        );

        // ② A built rung with its bill met.
        road.set_position(dirt_road_top(&ladder), &ladder);
        assert_eq!(road.held_rung(), RungKey::RouteDirtRoad);
        assert!(road.is_built());
        road.upkeep_demanded = Some(4.0);
        road.upkeep_supplied = 4.0;
        assert!(
            road.grants_sight(),
            "a KEPT dirt road is presence on that ground — paying the upkeep IS the traffic"
        );

        // ③ The same road, short of hands. It goes DARK BEFORE IT DECAYS.
        road.upkeep_supplied = 1.0;
        assert!(
            road.upkeep_shortfall() > 0.0,
            "precondition: this road really is short"
        );
        assert!(
            !road.grants_sight(),
            "a road in SHORTFALL goes dark before it decays — the condition is the PAID BILL, not \
             the held rung"
        );
    }

    /// ⛔ **THE FREE FLOOR HAS NO KEEPER**, and a road that decays back into it loses the one it had:
    /// there is nothing left to keep, and a `Roadwork` row naming it would be a job with no work.
    #[test]
    fn a_road_that_falls_back_into_the_free_floor_loses_its_keeper() {
        let ladder = LadderConfig::builtin();
        let mut road = Road::worn_in(UVec2::new(1, 1), &ladder);
        road.set_position(traffic_ceiling(&ladder), &ladder);
        assert_eq!(road.keeper, None, "a trail is nobody's job");

        road.take_keeper(
            RoadKeeper {
                faction: FactionId(0),
                band: BandId(7),
            },
            NEAR_ENOUGH_TO_KEEP,
            &ladder,
        );
        assert!(
            road.keeper.is_some(),
            "a fresh `grade` leaves the keeper set with no work banked yet — clearing it HERE would \
             undo the command on the turn it was typed"
        );

        road.set_position(traffic_ceiling(&ladder) - 1.0, &ladder);
        assert_eq!(
            road.keeper, None,
            "back inside the free floor, the road is nobody's job again"
        );
    }

    /// ⛔ **FRICTION AVERAGES ALONG THE TILES: A HALF-ROADED RUN PAYS ABOUT HALF.** Not nothing (the
    /// dead *"best road"* rule, which would have called this a whole dirt road) and not full.
    #[test]
    fn friction_averages_along_the_run_so_half_a_road_pays_about_half() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let path: Vec<UVec2> = (0..4).map(|x| UVec2::new(x, 0)).collect();

        assert_eq!(
            path_friction_multiplier(&registry, &path),
            FRICTION_UNCHANGED,
            "bare ground helps nothing"
        );

        // Two of the four tiles kept at a dirt road.
        for tile in &path[..2] {
            seat_a_kept_dirt_road(&mut registry, *tile, &ladder);
        }
        let dirt = ladder
            .rung(RungKey::RouteDirtRoad)
            .route_payoff
            .expect("every route rung declares a payoff")
            .friction_multiplier;
        let half = path_friction_multiplier(&registry, &path);
        let expected = (2.0 * dirt + 2.0 * FRICTION_UNCHANGED) / 4.0;
        assert!(
            (half - expected).abs() < 1.0e-5,
            "half a roaded run is the mean of its tiles: {half} against {expected}"
        );
        assert!(
            half > dirt && half < FRICTION_UNCHANGED,
            "which is strictly between 'roaded all the way' and 'no road at all'"
        );

        // The other half, and the run is worth the whole rung.
        for tile in &path[2..] {
            seat_a_kept_dirt_road(&mut registry, *tile, &ladder);
        }
        assert!(
            (path_friction_multiplier(&registry, &path) - dirt).abs() < 1.0e-5,
            "and a wholly roaded run pays exactly the rung's own multiplier"
        );
    }

    /// ⛔ **REACH TAKES THE WEAKEST TILE — ONE GAP BREAKS THE RUN.** A link goods must get *through*
    /// is not most-of-the-way-there, which is why this is a minimum where the friction is a mean.
    #[test]
    fn reach_takes_the_weakest_tile_so_one_bare_tile_breaks_the_run() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let path: Vec<UVec2> = (0..4).map(|x| UVec2::new(x, 0)).collect();

        for tile in &path {
            seat_a_kept_dirt_road(&mut registry, *tile, &ladder);
        }
        let dirt_reach = ladder
            .rung(RungKey::RouteDirtRoad)
            .route_payoff
            .expect("every route rung declares a payoff")
            .holds_link_to_tiles;
        assert!(
            dirt_reach > NO_REACH_HELD_OPEN,
            "precondition: a dirt road holds something open"
        );
        assert_eq!(
            path_reach_tiles(&registry, &path),
            dirt_reach,
            "a wholly roaded run holds what its rung holds — the liveness half"
        );

        // One tile in the middle taken back to bare ground.
        registry.remove(path[2]);
        assert_eq!(
            path_reach_tiles(&registry, &path),
            NO_REACH_HELD_OPEN,
            "ONE GAP BREAKS THE LINK — the weakest tile is the answer, not the best one"
        );
    }

    /// ⛔ **THE FREE FLOOR HOLDS A LINK OPEN, AND ONE BARE TILE STILL BREAKS IT.** The trail sibling
    /// of the dirt-road reach claim above, and the test that pins the payoff filter.
    ///
    /// Both payoffs filter on `Road::keeping_is_met` and **not** on `Road::grants_sight`. Under
    /// `grants_sight` — which is `is_built() && keeping_is_met()`, and the free floor is by definition
    /// not built — a fully worn trail read as bare ground: `0` tiles held open against a config that
    /// declares `holds_link_to_tiles: 6`. That made the whole branch unclimbable, because `grade` is
    /// gated on `roadbuilding` and `roadbuilding` is learned from a connection only a road holds open.
    #[test]
    fn a_trailed_run_holds_its_own_rungs_reach_open_and_one_bare_tile_breaks_it() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let path: Vec<UVec2> = (0..4).map(|x| UVec2::new(x, 0)).collect();

        for tile in &path {
            seat_a_trail(&mut registry, *tile, &ladder);
        }
        let trail_reach = payoff_of(FREE_FLOOR_TOP_RUNG, &ladder).holds_link_to_tiles;
        assert!(
            trail_reach > NO_REACH_HELD_OPEN,
            "precondition: the config really does say the free floor buys something ({trail_reach})"
        );
        assert_eq!(
            path_reach_tiles(&registry, &path),
            trail_reach,
            "a wholly trailed run holds the TRAIL rung's own reach — the liveness half, and FREE IS              NOT WORTHLESS"
        );

        registry.remove(path[2]);
        assert_eq!(
            path_reach_tiles(&registry, &path),
            NO_REACH_HELD_OPEN,
            "and one bare tile in it still holds nothing — the containment half"
        );
    }

    /// ⛔ **AND THE FREE FLOOR TAKES ITS OWN SLICE OFF THE FRICTION** — the trail sibling of the
    /// friction claim, read off the rung's record rather than restated as a literal.
    #[test]
    fn a_trailed_run_reads_the_trail_rungs_own_friction() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let path: Vec<UVec2> = (0..4).map(|x| UVec2::new(x, 0)).collect();

        assert_eq!(
            path_friction_multiplier(&registry, &path),
            FRICTION_UNCHANGED,
            "precondition: bare ground helps nothing"
        );

        for tile in &path {
            seat_a_trail(&mut registry, *tile, &ladder);
        }
        let trail_friction = payoff_of(FREE_FLOOR_TOP_RUNG, &ladder).friction_multiplier;
        assert!(
            trail_friction < FRICTION_UNCHANGED,
            "precondition: the config really does take something off the friction for a trail              ({trail_friction})"
        );
        assert!(
            (path_friction_multiplier(&registry, &path) - trail_friction).abs() < 1.0e-5,
            "a wholly trailed run pays exactly the trail rung's own multiplier: {} against {}",
            path_friction_multiplier(&registry, &path),
            trail_friction
        );
    }

    /// ⛔ **NEW TRAFFIC PREFERS AN EXISTING ROAD AMONG EQUALLY GOOD STEPS, AND NEVER DETOURS.**
    ///
    /// The walk is run twice over the same two ends: once on bare ground, and once with a road seated
    /// on a tile the *tie-break* would otherwise have passed over. The road must pull the path onto
    /// it — and the path must be exactly as long either way, because only steps already tied for best
    /// are compared.
    #[test]
    fn a_traced_path_takes_the_road_among_equally_good_steps_without_detouring() {
        let ladder = LadderConfig::builtin();
        let (from, to) = (UVec2::new(4, 4), UVec2::new(9, 7));
        let bare = RoadRegistry::default();
        let unrouted = trace_path(from, to, 40, 30, false, &bare);

        // A tile the unrouted walk does NOT take, one step off its second hex but still on a
        // shortest run: found by asking the walk itself rather than by asserting a coordinate.
        let alternative = (0..HEX_DIRECTION_COUNT)
            .filter_map(|dir| hex_neighbor(from.x, from.y, dir, 40, 30, false))
            .map(|(x, y)| UVec2::new(x, y))
            .find(|candidate| {
                hex_distance_wrapped(*candidate, to, 40, false)
                    == hex_distance_wrapped(from, to, 40, false) - 1
                    && !unrouted.contains(candidate)
            })
            .expect("a hex walk between these ends has more than one equally good first step");

        let mut registry = RoadRegistry::default();
        seat_a_trail(&mut registry, alternative, &ladder);
        let routed = trace_path(from, to, 40, 30, false, &registry);

        assert!(
            routed.contains(&alternative),
            "the walk takes the road: {alternative:?} is not on {routed:?}"
        );
        assert_eq!(
            routed.len(),
            unrouted.len(),
            "and it costs nothing to do so — only steps already tied for best are compared, so the              hex distance still bounds the walk exactly"
        );
        assert_eq!(routed.last(), Some(&to), "and it still arrives");
    }

    /// **A road that is not being kept is not carrying anything**, on either payoff — the built half
    /// of the filter, and the negative that the free floor's pass is not simply *"everything passes"*.
    #[test]
    fn a_built_road_in_shortfall_carries_nothing_on_either_payoff() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let path: Vec<UVec2> = (0..3).map(|x| UVec2::new(x, 0)).collect();
        for tile in &path {
            seat_a_kept_dirt_road(&mut registry, *tile, &ladder);
        }
        let dirt = payoff_of(RungKey::RouteDirtRoad, &ladder);
        assert_eq!(
            path_reach_tiles(&registry, &path),
            dirt.holds_link_to_tiles,
            "precondition: kept, the run holds its rung's reach"
        );

        // One tile handed a bill nobody paid.
        let short = registry.road_mut(path[1]).expect("just seated");
        short.upkeep_demanded = Some(1.0);
        short.upkeep_supplied = NO_UPKEEP_DEMAND;
        assert_eq!(
            path_reach_tiles(&registry, &path),
            NO_REACH_HELD_OPEN,
            "an unmaintained road is not carrying anything, so the run is broken"
        );
        assert!(
            path_friction_multiplier(&registry, &path) > dirt.friction_multiplier,
            "and it stops taking its slice off the friction too"
        );
    }

    /// `trace_path` carries both ends, so the tiles a journey wears include the camps it ran between.
    #[test]
    fn a_traced_path_reaches_its_target_and_carries_both_ends() {
        let (from, to) = (UVec2::new(2, 2), UVec2::new(7, 5));
        let path = trace_path(from, to, 40, 30, false, &RoadRegistry::default());

        assert_eq!(path.first(), Some(&from), "the journey starts at the camp");
        assert_eq!(path.last(), Some(&to), "and reaches the other one");
        assert!(
            path.len() >= 2,
            "a journey between two different tiles crosses at least two of them"
        );
        for pair in path.windows(2) {
            assert_eq!(
                hex_distance_wrapped(pair[0], pair[1], 40, false),
                1,
                "every step of a journey is one hex — {:?} to {:?} is not",
                pair[0],
                pair[1]
            );
        }
    }

    /// The registry answers *"which roads are this band's job"* off the keeper and nothing else — the
    /// catchment the `Roadwork` pool pays for.
    #[test]
    fn the_registry_answers_which_roads_are_this_bands_job() {
        let ladder = LadderConfig::builtin();
        let mut registry = RoadRegistry::default();
        let ours = BandId(1);
        let theirs = BandId(2);
        for (x, band) in [(0u32, ours), (1, theirs), (2, ours)] {
            let tile = UVec2::new(x, 0);
            seat_a_kept_dirt_road(&mut registry, tile, &ladder);
            registry.road_mut(tile).expect("just seated").take_keeper(
                RoadKeeper {
                    faction: FactionId(0),
                    band,
                },
                NEAR_ENOUGH_TO_KEEP,
                &ladder,
            );
        }
        // And one tile nobody took on.
        let bare = UVec2::new(3, 0);
        let ceiling = traffic_ceiling(&ladder);
        registry
            .road_or_trail(bare, &ladder)
            .set_position(ceiling, &ladder);

        let mine: Vec<UVec2> = registry.kept_by(ours).map(|(tile, _)| tile).collect();
        assert_eq!(
            mine,
            vec![UVec2::new(0, 0), UVec2::new(2, 0)],
            "one band keeps some of the tiles and another keeps the rest — which is the whole \
             reason a road is a TILE improvement"
        );
        assert_eq!(registry.kept_by(theirs).count(), 1);
        assert!(
            registry
                .road(bare)
                .expect("a trail is still a road")
                .keeper
                .is_none(),
            "and the free floor is nobody's job"
        );
    }
}
